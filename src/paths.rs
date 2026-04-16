use std::env;
use std::path::PathBuf;

#[must_use]
pub fn default_db_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        expand_tilde("~/Library/Application Support/clipmem/clipmem.sqlite3")
    } else {
        expand_tilde("~/.local/state/clipmem/clipmem.sqlite3")
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::default_db_path;

    #[test]
    fn default_db_path_ends_with_clipmem_database_name() {
        let path = default_db_path();

        assert!(path.ends_with("clipmem.sqlite3"));
    }
}
