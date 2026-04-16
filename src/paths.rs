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
    expand_tilde_with_home(path, home_dir())
}

fn expand_tilde_with_home(path: &str, home: Option<PathBuf>) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = home {
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
    use std::path::PathBuf;

    use super::{default_db_path, expand_tilde_with_home};

    #[test]
    fn default_db_path_ends_with_clipmem_database_name() {
        let path = default_db_path();

        assert!(path.ends_with("clipmem.sqlite3"));
    }

    #[test]
    fn expand_tilde_uses_supplied_home_directory() {
        let path = expand_tilde_with_home(
            "~/Library/Application Support/clipmem/clipmem.sqlite3",
            Some(PathBuf::from("/tmp/test-home")),
        );

        assert_eq!(
            path,
            PathBuf::from("/tmp/test-home/Library/Application Support/clipmem/clipmem.sqlite3")
        );
    }

    #[test]
    fn expand_tilde_preserves_input_when_home_is_missing() {
        let path = expand_tilde_with_home("~/clipmem.sqlite3", None);

        assert_eq!(path, PathBuf::from("~/clipmem.sqlite3"));
    }
}
