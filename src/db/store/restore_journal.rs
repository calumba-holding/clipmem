use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

const JOURNAL_VERSION: &str = "clipmem-restore-journal-v1";
const RECOVERY_LIFETIME: Duration = Duration::from_secs(30);
const MAX_CLEANUPS_PER_LOOKUP: usize = 1_024;
const MAX_JOURNAL_BYTES: u64 = 64 * 1_024;

pub(super) struct RestoreRecoveryJournal {
    path: PathBuf,
    operation_id: String,
    expires_unix: u64,
    generations: Vec<i64>,
}

impl RestoreRecoveryJournal {
    pub(super) fn create(db_path: &Path, operation_id: String) -> Result<Self> {
        let directory = journal_directory(db_path);
        fs::create_dir_all(&directory).context("create restore recovery journal directory")?;
        set_private_directory_permissions(&directory)?;
        let expires_unix = now_unix()?.saturating_add(RECOVERY_LIFETIME.as_secs());
        let path = directory.join(format!("{operation_id}.journal"));
        let journal = Self {
            path,
            operation_id,
            expires_unix,
            generations: Vec::new(),
        };
        journal.persist()?;
        Ok(journal)
    }

    pub(super) fn register_generation(&mut self, change_count: i64) -> Result<()> {
        self.expires_unix = now_unix()?.saturating_add(RECOVERY_LIFETIME.as_secs());
        if !self.generations.contains(&change_count) {
            self.generations.push(change_count);
        }
        self.persist()
    }

    pub(super) fn remove(self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => sync_parent(&self.path),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("remove restore recovery journal"),
        }
    }

    fn persist(&self) -> Result<()> {
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .context("open restore recovery journal")?;
        writeln!(file, "{JOURNAL_VERSION}")?;
        writeln!(file, "operation_id={}", self.operation_id)?;
        writeln!(file, "expires_unix={}", self.expires_unix)?;
        for generation in &self.generations {
            writeln!(file, "generation={generation}")?;
        }
        file.sync_all().context("sync restore recovery journal")?;
        fs::rename(&temporary, &self.path).context("publish restore recovery journal")?;
        sync_parent(&self.path)
    }
}

pub(super) fn operation_for_generation(db_path: &Path, change_count: i64) -> Option<String> {
    operation_for_generation_with_cleanup(db_path, change_count, remove_journal_file)
}

fn operation_for_generation_with_cleanup<Cleanup>(
    db_path: &Path,
    change_count: i64,
    cleanup: Cleanup,
) -> Option<String>
where
    Cleanup: Fn(&Path) -> std::io::Result<()>,
{
    let directory = journal_directory(db_path);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            journal_diagnostic("cannot enumerate recovery directory", &error);
            return None;
        }
    };
    let now = match now_unix() {
        Ok(now) => now,
        Err(error) => {
            journal_diagnostic("cannot read system time", &error);
            return None;
        }
    };
    let mut candidates = Vec::new();
    let mut cleanup_count = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                journal_diagnostic("cannot enumerate recovery entry", &error);
                continue;
            }
        };
        if entry.path().extension().and_then(|value| value.to_str()) != Some("journal") {
            continue;
        }
        match entry.file_type() {
            Ok(file_type) if file_type.is_file() => {}
            Ok(_) => {
                eprintln!("clipmem: ignoring non-file restore recovery journal entry");
                continue;
            }
            Err(error) => {
                journal_diagnostic("cannot inspect recovery entry", &error);
                continue;
            }
        }
        let metadata = match entry.metadata() {
            Ok(metadata) if metadata.len() <= MAX_JOURNAL_BYTES => metadata,
            Ok(_) => {
                eprintln!("clipmem: ignoring oversized restore recovery journal");
                cleanup_if_budgeted(&entry.path(), &cleanup, &mut cleanup_count);
                continue;
            }
            Err(error) => {
                journal_diagnostic("cannot inspect recovery journal", &error);
                continue;
            }
        };
        candidates.push((metadata.modified().unwrap_or(UNIX_EPOCH), entry.path()));
    }
    candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));

    for (_, path) in candidates {
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                journal_diagnostic("cannot read recovery journal", &error);
                continue;
            }
        };
        match inspect_journal(&content, change_count, now) {
            JournalInspection::Match(operation_id) => return Some(operation_id),
            JournalInspection::Expired => {
                cleanup_if_budgeted(&path, &cleanup, &mut cleanup_count);
            }
            JournalInspection::NoMatch => {}
            JournalInspection::Malformed => {
                eprintln!("clipmem: ignoring malformed restore recovery journal");
                cleanup_if_budgeted(&path, &cleanup, &mut cleanup_count);
            }
        }
    }
    None
}

fn cleanup_if_budgeted<Cleanup>(path: &Path, cleanup: &Cleanup, cleanup_count: &mut usize)
where
    Cleanup: Fn(&Path) -> std::io::Result<()>,
{
    if *cleanup_count >= MAX_CLEANUPS_PER_LOOKUP {
        return;
    }
    *cleanup_count += 1;
    if let Err(error) = cleanup(path) {
        journal_diagnostic("cannot remove invalid recovery journal", &error);
    }
}

enum JournalInspection {
    Match(String),
    Expired,
    NoMatch,
    Malformed,
}

fn inspect_journal(content: &str, change_count: i64, now: u64) -> JournalInspection {
    let mut lines = content.lines();
    if lines.next() != Some(JOURNAL_VERSION) {
        return JournalInspection::Malformed;
    }
    let Some(operation_id) = lines
        .next()
        .and_then(|line| line.strip_prefix("operation_id="))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return JournalInspection::Malformed;
    };
    let Some(expires) = lines
        .next()
        .and_then(|line| line.strip_prefix("expires_unix="))
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return JournalInspection::Malformed;
    };
    if expires < now {
        return JournalInspection::Expired;
    }
    if lines
        .filter_map(|line| line.strip_prefix("generation="))
        .filter_map(|value| value.parse::<i64>().ok())
        .any(|generation| generation == change_count)
    {
        JournalInspection::Match(operation_id)
    } else {
        JournalInspection::NoMatch
    }
}

fn remove_journal_file(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)?;
    path.parent()
        .ok_or_else(|| std::io::Error::other("recovery journal has no parent"))
        .and_then(|parent| fs::File::open(parent)?.sync_all())
}

fn journal_diagnostic(context: &str, error: &dyn std::fmt::Display) {
    eprintln!("clipmem: {context}: {error}");
}

fn journal_directory(db_path: &Path) -> PathBuf {
    let mut value: OsString = db_path.as_os_str().to_owned();
    value.push(".restore-suppression");
    PathBuf::from(value)
}

fn now_unix() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path.parent().context("restore journal has no parent")?;
    fs::File::open(parent)?
        .sync_all()
        .context("sync restore recovery journal directory")
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .context("set restore recovery journal permissions")
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEST_PATH: AtomicU64 = AtomicU64::new(0);

    struct TestPath(PathBuf);

    impl TestPath {
        fn new() -> Self {
            let suffix = NEXT_TEST_PATH.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!(
                "clipmem-restore-journal-{}-{suffix}.db",
                std::process::id()
            )))
        }
    }

    impl Drop for TestPath {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
            let _ = fs::remove_dir_all(journal_directory(&self.0));
        }
    }

    #[test]
    fn malformed_entry_does_not_hide_readable_exact_match() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "matching-op".into())?;
        journal.register_generation(41)?;
        fs::write(
            journal_directory(&db_path.0).join("malformed.journal"),
            "not a restore journal\n",
        )?;

        assert_eq!(
            operation_for_generation(&db_path.0, 41).as_deref(),
            Some("matching-op")
        );
        assert_eq!(operation_for_generation(&db_path.0, 42), None);
        Ok(())
    }

    #[test]
    fn non_file_entry_does_not_block_external_generation() -> Result<()> {
        let db_path = TestPath::new();
        fs::create_dir_all(journal_directory(&db_path.0).join("unreadable.journal"))?;

        assert_eq!(operation_for_generation(&db_path.0, 77), None);
        Ok(())
    }

    #[test]
    fn recovery_directory_io_failure_does_not_block_external_generation() -> Result<()> {
        let db_path = TestPath::new();
        fs::write(journal_directory(&db_path.0), b"not a directory")?;

        assert_eq!(operation_for_generation(&db_path.0, 99), None);
        Ok(())
    }

    #[test]
    fn registration_refreshes_expiry_before_persisting() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "refresh-op".into())?;
        journal.generations.push(7);
        journal.expires_unix = 0;
        let before = now_unix()?;

        journal.register_generation(7)?;

        assert!(journal.expires_unix >= before + RECOVERY_LIFETIME.as_secs());
        let content = fs::read_to_string(&journal.path)?;
        assert!(matches!(
            inspect_journal(&content, 7, before),
            JournalInspection::Match(operation_id) if operation_id == "refresh-op"
        ));
        Ok(())
    }

    #[test]
    fn expired_valid_journal_is_removed_without_blocking_capture() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "expired-op".into())?;
        journal.generations.push(5);
        journal.expires_unix = 0;
        journal.persist()?;
        let journal_path = journal.path.clone();

        assert_eq!(operation_for_generation(&db_path.0, 5), None);
        assert!(!journal_path.exists());
        Ok(())
    }

    #[test]
    fn expired_journal_cleanup_failure_is_non_fatal() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "expired-op".into())?;
        journal.generations.push(5);
        journal.expires_unix = 0;
        journal.persist()?;

        let result = operation_for_generation_with_cleanup(&db_path.0, 5, |_| {
            Err(std::io::Error::new(
                ErrorKind::PermissionDenied,
                "injected cleanup failure",
            ))
        });

        assert_eq!(result, None);
        assert!(journal.path.exists());
        Ok(())
    }

    #[test]
    fn junk_beyond_cleanup_bound_cannot_starve_oldest_exact_match() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "000-match".into())?;
        journal.register_generation(4242)?;
        let directory = journal_directory(&db_path.0);
        for index in 0..=MAX_CLEANUPS_PER_LOOKUP {
            fs::write(
                directory.join(format!("zzz-junk-{index:04}.journal")),
                b"malformed\n",
            )?;
        }

        let match_cleanups = Cell::new(0);
        let found = operation_for_generation_with_cleanup(&db_path.0, 4242, |_| {
            match_cleanups.set(match_cleanups.get() + 1);
            Ok(())
        });
        assert_eq!(found.as_deref(), Some("000-match"));
        assert_eq!(match_cleanups.get(), MAX_CLEANUPS_PER_LOOKUP);

        let external_cleanups = Cell::new(0);
        let external = operation_for_generation_with_cleanup(&db_path.0, 4243, |_| {
            external_cleanups.set(external_cleanups.get() + 1);
            Ok(())
        });
        assert_eq!(external, None);
        assert_eq!(external_cleanups.get(), MAX_CLEANUPS_PER_LOOKUP);
        Ok(())
    }
}
