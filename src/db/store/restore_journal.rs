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
const MAX_JOURNAL_BYTES: u64 = 64 * 1_024;
const MAX_STARTUP_INSPECTIONS: usize = 256;
const MAX_STARTUP_CLEANUPS: usize = 128;
const MAX_TEMP_CREATE_ATTEMPTS: usize = 8;
const RECOVERY_SHARDS: u64 = 64;
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
        preflight_shards(&directory)?;
        cleanup_recovery_directory(&directory);
        let expires_unix = now_unix()?.saturating_add(RECOVERY_LIFETIME.as_secs());
        let path = journal_path(&directory, &operation_id);
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
    if !is_operation_id(&operation_id) {
        return None;
    }
    let expires_unix = lines.next()?.strip_prefix("expires_unix=")?.parse().ok()?;
    let generation = lines.next()?.strip_prefix("generation=")?.parse().ok()?;
    if lines.next().is_some() {
        return None;
    }
    Some(GenerationMarker {
        operation_id,
        expires_unix,
        generation,
    })
}

fn marker_path(directory: &Path, generation: i64) -> PathBuf {
    shard_directory(directory, generation as u64 % RECOVERY_SHARDS)
        .join(format!("generation-{generation}.marker"))
}

fn journal_path(directory: &Path, operation_id: &str) -> PathBuf {
    let shard = u64::from_str_radix(&operation_id[..2], 16).unwrap_or(0) % RECOVERY_SHARDS;
    shard_directory(directory, shard).join(format!("{operation_id}.journal"))
}

fn shard_directory(directory: &Path, shard: u64) -> PathBuf {
    directory.join("owned").join(format!("shard-{shard:02}"))
}

fn preflight_shards(directory: &Path) -> Result<()> {
    let owned = directory.join("owned");
    fs::create_dir_all(&owned).context("create restore recovery owned directory")?;
    set_private_directory_permissions(&owned)?;
    for shard in 0..RECOVERY_SHARDS {
        let path = shard_directory(directory, shard);
        fs::create_dir_all(&path).context("create restore recovery shard")?;
        set_private_directory_permissions(&path)?;
    }
    fs::File::open(&owned)?
        .sync_all()
        .context("sync restore recovery owned directory")?;
    sync_parent(&owned)
}

fn atomic_persist(path: &Path, content: &[u8]) -> Result<()> {
    let created_unix = now_unix()?;
    let candidates = (0..MAX_TEMP_CREATE_ATTEMPTS).map(|_| {
        path.with_extension(format!(
            "tmp-{created_unix}-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY_FILE.fetch_add(1, Ordering::Relaxed)
        ))
    });
    atomic_persist_with(path, content, candidates, |temporary, destination| {
        fs::rename(temporary, destination)
    })
}

fn atomic_persist_with<I, Publish>(
    path: &Path,
    content: &[u8],
    candidates: I,
    publish: Publish,
) -> Result<()>
where
    I: IntoIterator<Item = PathBuf>,
    Publish: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    let (mut file, temporary) = create_temporary(candidates)?;
    let mut guard = TemporaryFileGuard::new(temporary);
    file.write_all(content)?;
    file.sync_all().context("sync restore recovery file")?;
    publish(guard.path(), path).context("publish restore recovery file")?;
    guard.disarm();
    sync_parent(path)
}

fn create_temporary<I>(candidates: I) -> Result<(fs::File, PathBuf)>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut last_collision = None;
    for temporary in candidates {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((file, temporary)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => last_collision = Some(error),
            Err(error) => return Err(error).context("open restore recovery temp file"),
        }
    }
    Err(last_collision
        .unwrap_or_else(|| std::io::Error::other("no restore recovery temp candidates")))
    .context("restore recovery temp names exhausted")
}

struct TemporaryFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TemporaryFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
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
    let shard = read_cleanup_cursor(directory) % RECOVERY_SHARDS;
    let shard_path = shard_directory(directory, shard);
    let Ok(entries) = fs::read_dir(&shard_path) else {
        return;
    };
    let paths = entries.map(|entry| entry.map(|value| value.path()));
    let (_, cleaned) = cleanup_recovery_paths(paths, now);
    if cleaned != 0 {
        let _ = fs::File::open(&shard_path).and_then(|file| file.sync_all());
    }
    write_cleanup_cursor(directory, (shard + 1) % RECOVERY_SHARDS);
}

fn read_cleanup_cursor(directory: &Path) -> u64 {
    let path = directory.join("cleanup-cursor");
    read_regular_bounded(&path, 32)
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

fn write_cleanup_cursor(directory: &Path, shard: u64) {
    if let Err(error) = atomic_persist(
        &directory.join("cleanup-cursor"),
        format!("{shard}\n").as_bytes(),
    ) {
        journal_diagnostic("cannot advance recovery cleanup cursor", &error);
    }
}

fn cleanup_recovery_paths<I>(paths: I, now: u64) -> (usize, usize)
where
    I: IntoIterator<Item = std::io::Result<PathBuf>>,
{
    let mut inspected = 0;
    let mut cleaned = 0;
    for entry in paths.into_iter().take(MAX_STARTUP_INSPECTIONS) {
        inspected += 1;
        if cleaned >= MAX_STARTUP_CLEANUPS {
            break;
        }
        let Ok(path) = entry else {
            continue;
        };
        let Some(kind) = recovery_artifact_kind(&path) else {
            continue;
        };
        let remove = match fs::symlink_metadata(&path) {
            // Preserve symlinks and other non-files; removal must never follow
            // or reinterpret an attacker-controlled path.
            Ok(metadata) if !metadata.file_type().is_file() => false,
            Ok(metadata) => artifact_is_stale(&path, kind, metadata.len(), now),
            Err(_) => false,
        };
        if remove {
            cleaned += 1;
            if let Err(error) = remove_file_if_present(&path) {
                journal_diagnostic("cannot clean recovery marker", &error);
            }
        }
    }
    (inspected, cleaned)
}

#[derive(Clone, Copy)]
enum RecoveryArtifactKind {
    Marker,
    Journal,
    Temporary { created_unix: u64 },
}

fn recovery_artifact_kind(path: &Path) -> Option<RecoveryArtifactKind> {
    let name = path.file_name()?.to_str()?;
    if let Some(generation) = name
        .strip_prefix("generation-")
        .and_then(|value| value.strip_suffix(".marker"))
    {
        return is_canonical_nonnegative_decimal(generation)
            .then_some(RecoveryArtifactKind::Marker);
    }
    if let Some(operation_id) = name.strip_suffix(".journal") {
        return is_operation_id(operation_id).then_some(RecoveryArtifactKind::Journal);
    }
    let (stem, suffix) = name.rsplit_once(".tmp-")?;
    if !is_owned_destination_stem(stem) {
        return None;
    }
    let mut parts = suffix.split('-');
    let created_unix = parts.next()?.parse().ok()?;
    parts.next()?.parse::<u32>().ok()?;
    parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(RecoveryArtifactKind::Temporary { created_unix })
}

fn artifact_is_stale(path: &Path, kind: RecoveryArtifactKind, len: u64, now: u64) -> bool {
    match kind {
        RecoveryArtifactKind::Marker => {
            len > MAX_MARKER_BYTES
                || read_regular_bounded(path, MAX_MARKER_BYTES)
                    .ok()
                    .flatten()
                    .and_then(|content| parse_marker(&content))
                    .is_none_or(|marker| marker.expires_unix < now)
        }
        RecoveryArtifactKind::Journal => {
            len > MAX_JOURNAL_BYTES
                || read_regular_bounded(path, MAX_JOURNAL_BYTES)
                    .ok()
                    .flatten()
                    .and_then(|content| journal_expiry(&content))
                    .is_none_or(|expires| expires < now)
        }
        RecoveryArtifactKind::Temporary { created_unix } => {
            created_unix.saturating_add(RECOVERY_LIFETIME.as_secs()) < now
        }
    }
}

fn journal_expiry(content: &str) -> Option<u64> {
    let mut lines = content.lines();
    if lines.next()? != JOURNAL_VERSION {
        return None;
    }
    let operation_id = lines.next()?.strip_prefix("operation_id=")?;
    if !is_operation_id(operation_id) {
        return None;
    }
    lines.next()?.strip_prefix("expires_unix=")?.parse().ok()
}

fn is_owned_destination_stem(stem: &str) -> bool {
    is_operation_id(stem)
        || stem
            .strip_prefix("generation-")
            .is_some_and(is_canonical_nonnegative_decimal)
}

fn is_operation_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_canonical_nonnegative_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
        && value.parse::<i64>().is_ok()
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
#[path = "restore_journal_fairness_tests.rs"]
mod fairness_tests;

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
        let mut journal =
            RestoreRecoveryJournal::create(&db_path.0, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into())?;
        journal.register_generation(41)?;
        journal.register_generation(42)?;

        assert_eq!(
            operation_for_generation(&db_path.0, 41).as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            operation_for_generation(&db_path.0, 42).as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(operation_for_generation(&db_path.0, 43), None);
        Ok(())
    }

    #[test]
    fn unrelated_junk_does_not_affect_constant_work_exact_lookup() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal =
            RestoreRecoveryJournal::create(&db_path.0, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into())?;
        journal.register_generation(7)?;
        let directory = journal_directory(&db_path.0);
        for index in 0..1_100 {
            fs::write(directory.join(format!("junk-{index}.artifact")), b"junk")?;
        }

        assert_eq!(
            operation_for_generation(&db_path.0, 7).as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(operation_for_generation(&db_path.0, 8), None);
        Ok(())
    }

    #[test]
    fn malformed_expired_and_non_file_exact_markers_fail_open() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        preflight_shards(&directory)?;
        fs::write(marker_path(&directory, 1), b"malformed")?;
        fs::create_dir(marker_path(&directory, 2))?;
        fs::write(
            marker_path(&directory, 3),
            format!(
                "{MARKER_VERSION}\noperation_id={}\nexpires_unix=0\ngeneration=3\n",
                "a".repeat(32)
            ),
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
    fn invalid_operation_id_and_trailing_marker_content_fail_open() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        preflight_shards(&directory)?;
        let expires = now_unix()?.saturating_add(30);
        fs::write(
            marker_path(&directory, 20),
            format!("{MARKER_VERSION}\noperation_id=x\nexpires_unix={expires}\ngeneration=20\n"),
        )?;
        fs::write(
            marker_path(&directory, 21),
            format!(
                "{MARKER_VERSION}\noperation_id={}\nexpires_unix={expires}\ngeneration=21\ntrailing=true\n",
                "a".repeat(32)
            ),
        )?;

        assert_eq!(operation_for_generation(&db_path.0, 20), None);
        assert_eq!(operation_for_generation(&db_path.0, 21), None);
        Ok(())
    }

    #[test]
    fn bulk_cleanup_is_bounded_and_stays_out_of_exact_lookup() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        preflight_shards(&directory)?;
        let shard = shard_directory(&directory, 0);
        for generation in 0..300 {
            fs::write(
                shard.join(format!("generation-{generation}.marker")),
                b"malformed",
            )?;
        }

        cleanup_recovery_directory(&directory);
        let markers_after_startup = fs::read_dir(&shard)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("marker")
            })
            .count();
        assert_eq!(markers_after_startup, 300 - MAX_STARTUP_CLEANUPS);

        assert_eq!(operation_for_generation(&db_path.0, 999), None);
        let markers_after_lookup = fs::read_dir(&shard)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("marker")
            })
            .count();
        assert_eq!(markers_after_lookup, markers_after_startup);
        Ok(())
    }

    #[test]
    fn maintenance_inspection_bound_counts_ignored_artifacts() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        preflight_shards(&directory)?;
        let mut paths = Vec::new();
        for index in 0..MAX_STARTUP_INSPECTIONS {
            let path = directory.join(format!("unrelated-{index}.txt"));
            fs::write(&path, b"unrelated")?;
            paths.push(Ok(path));
        }
        let stale = marker_path(&directory, 99);
        fs::write(&stale, b"malformed")?;
        paths.push(Ok(stale.clone()));

        let (inspected, cleaned) = cleanup_recovery_paths(paths, now_unix()?);

        assert_eq!(inspected, MAX_STARTUP_INSPECTIONS);
        assert_eq!(cleaned, 0);
        assert!(stale.exists());
        Ok(())
    }

    #[test]
    fn startup_cleanup_removes_owned_expired_journal_and_stale_temp_only() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        preflight_shards(&directory)?;
        let operation_id = "a".repeat(32);
        let journal = journal_path(&directory, &operation_id);
        fs::write(
            &journal,
            format!("{JOURNAL_VERSION}\noperation_id={operation_id}\nexpires_unix=0\n"),
        )?;
        let temporary = journal
            .parent()
            .unwrap()
            .join(format!("{operation_id}.tmp-0-123-0"));
        fs::write(&temporary, b"partial")?;
        let unrelated = journal.parent().unwrap().join("notes.tmp-0-123-0");
        fs::write(&unrelated, b"keep")?;
        fs::write(directory.join("cleanup-cursor"), b"42\n")?;

        cleanup_recovery_directory(&directory);

        assert!(!journal.exists());
        assert!(!temporary.exists());
        assert!(unrelated.exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn startup_cleanup_preserves_symlinks_even_with_owned_names() -> Result<()> {
        use std::os::unix::fs::symlink;

        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        preflight_shards(&directory)?;
        let target = directory.join("target");
        fs::write(&target, b"unrelated")?;
        let link = marker_path(&directory, 9);
        symlink(&target, &link)?;
        fs::write(directory.join("cleanup-cursor"), b"9\n")?;

        cleanup_recovery_directory(&directory);

        assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert_eq!(fs::read(&target)?, b"unrelated");
        Ok(())
    }

    #[test]
    fn failed_publication_removes_create_new_temporary_file() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        let destination = directory.join("destination.journal");
        let temporary = directory.join("temporary");

        atomic_persist_with(&destination, b"content", [temporary.clone()], |_, _| {
            Err(std::io::Error::other("injected publish failure"))
        })
        .expect_err("injected publication failure is reported");

        assert!(!temporary.exists());
        assert!(!destination.exists());
        Ok(())
    }

    #[test]
    fn stale_temporary_name_collision_retries_next_candidate() -> Result<()> {
        let db_path = TestPath::new();
        let directory = journal_directory(&db_path.0);
        fs::create_dir_all(&directory)?;
        let destination = directory.join("destination.journal");
        let collision = directory.join("collision");
        let retry = directory.join("retry");
        fs::write(&collision, b"stale")?;

        atomic_persist_with(
            &destination,
            b"published",
            [collision.clone(), retry.clone()],
            |temporary, destination| fs::rename(temporary, destination),
        )?;

        assert_eq!(fs::read(&collision)?, b"stale");
        assert!(!retry.exists());
        assert_eq!(fs::read(&destination)?, b"published");
        Ok(())
    }

    #[test]
    fn successful_removal_deletes_every_owned_marker() -> Result<()> {
        let db_path = TestPath::new();
        let mut journal =
            RestoreRecoveryJournal::create(&db_path.0, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into())?;
        journal.register_generation(10)?;
        journal.register_generation(11)?;
        journal.remove()?;

        assert_eq!(operation_for_generation(&db_path.0, 10), None);
        assert_eq!(operation_for_generation(&db_path.0, 11), None);
        Ok(())
    }
}
