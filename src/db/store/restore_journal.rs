use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

const JOURNAL_VERSION: &str = "clipmem-restore-journal-v1";
const MARKER_VERSION: &str = "clipmem-restore-generation-v1";
const RECOVERY_LIFETIME: Duration = Duration::from_secs(30);
const MAX_MARKER_BYTES: u64 = 4 * 1_024;
const MAX_STARTUP_CLEANUPS: usize = 256;
static NEXT_TEMPORARY_FILE: AtomicU64 = AtomicU64::new(0);

pub(super) struct RestoreRecoveryJournal {
    path: PathBuf,
    directory: PathBuf,
    operation_id: String,
    expires_unix: u64,
    generations: Vec<i64>,
}

impl RestoreRecoveryJournal {
    pub(super) fn create(db_path: &Path, operation_id: String) -> Result<Self> {
        let directory = journal_directory(db_path);
        fs::create_dir_all(&directory).context("create restore recovery journal directory")?;
        set_private_directory_permissions(&directory)?;
        cleanup_recovery_directory(&directory);
        let expires_unix = now_unix()?.saturating_add(RECOVERY_LIFETIME.as_secs());
        let path = directory.join(format!("{operation_id}.journal"));
        let journal = Self {
            path,
            directory,
            operation_id,
            expires_unix,
            generations: Vec::new(),
        };
        journal.persist_journal()?;
        Ok(journal)
    }

    pub(super) fn register_generation(&mut self, change_count: i64) -> Result<()> {
        anyhow::ensure!(
            change_count >= 0,
            "pasteboard generation must be nonnegative"
        );
        self.expires_unix = now_unix()?.saturating_add(RECOVERY_LIFETIME.as_secs());
        if !self.generations.contains(&change_count) {
            self.generations.push(change_count);
        }
        self.persist_journal()?;
        for generation in &self.generations {
            self.persist_marker(*generation)?;
        }
        Ok(())
    }

    pub(super) fn remove(self) -> Result<()> {
        for generation in &self.generations {
            let marker_path = marker_path(&self.directory, *generation);
            if marker_operation(&marker_path, *generation, 0).as_deref()
                == Some(self.operation_id.as_str())
            {
                remove_file_if_present(&marker_path)?;
            }
        }
        remove_file_if_present(&self.path)?;
        sync_parent(&self.path)
    }

    fn persist_journal(&self) -> Result<()> {
        let mut content = format!(
            "{JOURNAL_VERSION}\noperation_id={}\nexpires_unix={}\n",
            self.operation_id, self.expires_unix
        );
        for generation in &self.generations {
            content.push_str(&format!("generation={generation}\n"));
        }
        atomic_persist(&self.path, content.as_bytes())
    }

    fn persist_marker(&self, generation: i64) -> Result<()> {
        let content = format!(
            "{MARKER_VERSION}\noperation_id={}\nexpires_unix={}\ngeneration={generation}\n",
            self.operation_id, self.expires_unix
        );
        atomic_persist(
            &marker_path(&self.directory, generation),
            content.as_bytes(),
        )
    }
}

pub(super) fn operation_for_generation(db_path: &Path, change_count: i64) -> Option<String> {
    if change_count < 0 {
        return None;
    }
    let directory = journal_directory(db_path);
    let path = marker_path(&directory, change_count);
    let now = match now_unix() {
        Ok(now) => now,
        Err(error) => {
            journal_diagnostic("cannot read system time", &error);
            return None;
        }
    };
    let operation = marker_operation(&path, change_count, now);
    if operation.is_none() && marker_is_expired(&path, change_count, now) {
        if let Err(error) = remove_file_if_present(&path).and_then(|()| sync_parent_io(&path)) {
            journal_diagnostic("cannot remove expired recovery marker", &error);
        }
    }
    operation
}

fn marker_operation(path: &Path, expected_generation: i64, now: u64) -> Option<String> {
    let content = match read_regular_bounded(path, MAX_MARKER_BYTES) {
        Ok(Some(content)) => content,
        Ok(None) => return None,
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            journal_diagnostic("cannot read recovery marker", &error);
            return None;
        }
    };
    let marker = parse_marker(&content)?;
    (marker.generation == expected_generation && marker.expires_unix >= now)
        .then_some(marker.operation_id)
}

fn marker_is_expired(path: &Path, expected_generation: i64, now: u64) -> bool {
    read_regular_bounded(path, MAX_MARKER_BYTES)
        .ok()
        .flatten()
        .and_then(|content| parse_marker(&content))
        .is_some_and(|marker| marker.generation == expected_generation && marker.expires_unix < now)
}

struct GenerationMarker {
    operation_id: String,
    expires_unix: u64,
    generation: i64,
}

fn parse_marker(content: &str) -> Option<GenerationMarker> {
    let mut lines = content.lines();
    if lines.next()? != MARKER_VERSION {
        return None;
    }
    let operation_id = lines.next()?.strip_prefix("operation_id=")?.to_string();
    if operation_id.is_empty() {
        return None;
    }
    let expires_unix = lines.next()?.strip_prefix("expires_unix=")?.parse().ok()?;
    let generation = lines.next()?.strip_prefix("generation=")?.parse().ok()?;
    Some(GenerationMarker {
        operation_id,
        expires_unix,
        generation,
    })
}

fn marker_path(directory: &Path, generation: i64) -> PathBuf {
    directory.join(format!("generation-{generation}.marker"))
}

fn atomic_persist(path: &Path, content: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT_TEMPORARY_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .context("open restore recovery file")?;
    file.write_all(content)?;
    file.sync_all().context("sync restore recovery file")?;
    fs::rename(&temporary, path).context("publish restore recovery file")?;
    sync_parent(path)
}

fn read_regular_bounded(path: &Path, max_bytes: u64) -> std::io::Result<Option<String>> {
    let path_metadata = fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() || path_metadata.len() > max_bytes {
        return Ok(None);
    }
    let file = fs::File::open(path)?;
    let file_metadata = file.metadata()?;
    if !file_metadata.file_type().is_file() || file_metadata.len() > max_bytes {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if path_metadata.dev() != file_metadata.dev() || path_metadata.ino() != file_metadata.ino()
        {
            return Ok(None);
        }
    }
    let mut bytes = Vec::with_capacity(file_metadata.len() as usize);
    file.take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Ok(None);
    }
    Ok(String::from_utf8(bytes).ok())
}

fn cleanup_recovery_directory(directory: &Path) {
    let now = now_unix().unwrap_or(0);
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut cleaned = 0;
    for entry in entries.flatten() {
        if cleaned >= MAX_STARTUP_CLEANUPS {
            break;
        }
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("marker") {
            continue;
        }
        let remove = match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_file() => true,
            Ok(metadata) if metadata.len() > MAX_MARKER_BYTES => true,
            Ok(_) => fs::read_to_string(&path)
                .ok()
                .and_then(|content| parse_marker(&content))
                .is_none_or(|marker| marker.expires_unix < now),
            Err(_) => false,
        };
        if remove {
            cleaned += 1;
            if let Err(error) = remove_file_if_present(&path) {
                journal_diagnostic("cannot clean recovery marker", &error);
            }
        }
    }
    if cleaned != 0 {
        let _ = fs::File::open(directory).and_then(|file| file.sync_all());
    }
}

fn remove_file_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
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
    sync_parent_io(path).context("sync restore recovery directory")
}

fn sync_parent_io(path: &Path) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("restore recovery file has no parent"))?;
    fs::File::open(parent)?.sync_all()
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
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEST_PATH: AtomicU64 = AtomicU64::new(0);

    struct TestPath(PathBuf);

    impl TestPath {
        fn new() -> Self {
            let suffix = NEXT_TEST_PATH.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!(
                "clipmem-restore-marker-{}-{suffix}.db",
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
    fn exact_markers_support_restore_rollback_and_external_generation() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "operation".into())?;
        journal.register_generation(41)?;
        journal.register_generation(42)?;

        assert_eq!(
            operation_for_generation(&db_path.0, 41).as_deref(),
            Some("operation")
        );
        assert_eq!(
            operation_for_generation(&db_path.0, 42).as_deref(),
            Some("operation")
        );
        assert_eq!(operation_for_generation(&db_path.0, 43), None);
        Ok(())
    }

    #[test]
    fn unrelated_junk_does_not_affect_constant_work_exact_lookup() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "operation".into())?;
        journal.register_generation(7)?;
        let directory = journal_directory(&db_path.0);
        for index in 0..1_100 {
            fs::write(directory.join(format!("junk-{index}.artifact")), b"junk")?;
        }

        assert_eq!(
            operation_for_generation(&db_path.0, 7).as_deref(),
            Some("operation")
        );
        assert_eq!(operation_for_generation(&db_path.0, 8), None);
        Ok(())
    }

    #[test]
    fn malformed_expired_and_non_file_exact_markers_fail_open() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        fs::write(marker_path(&directory, 1), b"malformed")?;
        fs::create_dir(marker_path(&directory, 2))?;
        fs::write(
            marker_path(&directory, 3),
            format!("{MARKER_VERSION}\noperation_id=expired\nexpires_unix=0\ngeneration=3\n"),
        )?;
        fs::write(marker_path(&directory, 4), [0xff, 0xfe, 0xfd])?;

        assert_eq!(operation_for_generation(&db_path.0, 1), None);
        assert_eq!(operation_for_generation(&db_path.0, 2), None);
        assert_eq!(operation_for_generation(&db_path.0, 3), None);
        assert_eq!(operation_for_generation(&db_path.0, 4), None);
        assert!(!marker_path(&directory, 3).exists());
        Ok(())
    }

    #[test]
    fn bulk_cleanup_is_bounded_and_stays_out_of_exact_lookup() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        for generation in 0..300 {
            fs::write(marker_path(&directory, generation), b"malformed")?;
        }

        let _journal = RestoreRecoveryJournal::create(&db_path.0, "operation".into())?;
        let markers_after_startup = fs::read_dir(&directory)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("marker")
            })
            .count();
        assert_eq!(markers_after_startup, 300 - MAX_STARTUP_CLEANUPS);

        assert_eq!(operation_for_generation(&db_path.0, 999), None);
        let markers_after_lookup = fs::read_dir(&directory)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("marker")
            })
            .count();
        assert_eq!(markers_after_lookup, markers_after_startup);
        Ok(())
    }

    #[test]
    fn successful_removal_deletes_every_owned_marker() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal = RestoreRecoveryJournal::create(&db_path.0, "operation".into())?;
        journal.register_generation(10)?;
        journal.register_generation(11)?;
        journal.remove()?;

        assert_eq!(operation_for_generation(&db_path.0, 10), None);
        assert_eq!(operation_for_generation(&db_path.0, 11), None);
        Ok(())
    }
}
