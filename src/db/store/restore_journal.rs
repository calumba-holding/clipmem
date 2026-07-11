use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

const JOURNAL_VERSION: &str = "clipmem-restore-journal-v1";
const RECOVERY_LIFETIME: Duration = Duration::from_secs(30);

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
        if !self.generations.contains(&change_count) {
            self.generations.push(change_count);
            self.persist()?;
        }
        Ok(())
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

pub(super) fn operation_for_generation(
    db_path: &Path,
    change_count: i64,
) -> Result<Option<String>> {
    let directory = journal_directory(db_path);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read restore recovery journals"),
    };
    let now = now_unix()?;
    for entry in entries {
        let entry = entry.context("read restore recovery journal entry")?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("journal") {
            continue;
        }
        let content = fs::read_to_string(entry.path()).context("read restore recovery journal")?;
        if let Some(operation_id) = matching_operation(&content, change_count, now) {
            return Ok(Some(operation_id));
        }
    }
    Ok(None)
}

fn matching_operation(content: &str, change_count: i64, now: u64) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()? != JOURNAL_VERSION {
        return None;
    }
    let operation_id = lines.next()?.strip_prefix("operation_id=")?.to_string();
    let expires = lines
        .next()?
        .strip_prefix("expires_unix=")?
        .parse::<u64>()
        .ok()?;
    if expires < now {
        return None;
    }
    lines
        .filter_map(|line| line.strip_prefix("generation="))
        .filter_map(|value| value.parse::<i64>().ok())
        .any(|generation| generation == change_count)
        .then_some(operation_id)
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
