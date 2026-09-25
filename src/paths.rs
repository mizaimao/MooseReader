use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "moosereader";

/// Resolves an XDG base directory: the variable's value when it is an
/// absolute path, otherwise `home/<fallback>` (XDG Base Directory Specification).
fn xdg_dir(value: Option<OsString>, home: Option<OsString>, fallback: &str) -> Option<PathBuf> {
    value
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|home| PathBuf::from(home).join(fallback)))
}

#[cfg(not(test))]
fn base_dir(var: &str, fallback: &str) -> Option<PathBuf> {
    xdg_dir(std::env::var_os(var), std::env::var_os("HOME"), fallback)
}

/// Where the settings and bookmarks of earlier versions sat: the working directory.
#[cfg(not(test))]
fn legacy(name: &str) -> PathBuf {
    PathBuf::from(name)
}

// Under test, each test thread gets its own folder in target/, so tests never
// touch the user's settings, bookmarks or old files, nor each other's.
#[cfg(test)]
fn base_dir(var: &str, _fallback: &str) -> Option<PathBuf> {
    Some(test_dir().join(var))
}

#[cfg(test)]
fn legacy(name: &str) -> PathBuf {
    test_dir().join("legacy").join(name)
}

#[cfg(test)]
pub fn test_dir() -> PathBuf {
    let thread = std::thread::current();
    let name: String = (thread.name().unwrap_or("main").chars())
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-files")
        .join(name)
}

/// Empties this test's folder, for a test that needs a clean start.
#[cfg(test)]
pub fn reset_test_dir() {
    let _ = fs::remove_dir_all(test_dir());
}

/// Settings live in `$XDG_CONFIG_HOME/moosereader/config.json` (default `~/.config`).
pub fn config_file() -> PathBuf {
    base_dir("XDG_CONFIG_HOME", ".config")
        .map(|dir| dir.join(APP_DIR).join("config.json"))
        .unwrap_or_else(|| legacy("reader_config.json"))
}

/// Bookmarks live in `$XDG_STATE_HOME/moosereader/bookmarks.json` (default `~/.local/state`).
pub fn bookmarks_file() -> PathBuf {
    base_dir("XDG_STATE_HOME", ".local/state")
        .map(|dir| dir.join(APP_DIR).join("bookmarks.json"))
        .unwrap_or_else(|| legacy("bookmarks.json"))
}

/// Reads `path`, falling back to the file of the same role that earlier
/// versions kept in the working directory, so old settings and bookmarks carry over.
pub fn read_with_legacy(path: &Path, legacy_name: &str) -> Option<String> {
    fs::read_to_string(path)
        .or_else(|_| fs::read_to_string(legacy(legacy_name)))
        .ok()
}

/// Writes through a temporary file and a rename, so a crash mid-save
/// never leaves a truncated file behind.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_variables_win_when_absolute() {
        let home = Some(OsString::from("/home/reader"));
        let dir = |value: Option<&str>| xdg_dir(value.map(OsString::from), home.clone(), ".config");
        assert_eq!(dir(Some("/custom")), Some(PathBuf::from("/custom")));
        // The spec says relative values are invalid and must be ignored
        assert_eq!(
            dir(Some("relative/dir")),
            Some(PathBuf::from("/home/reader/.config"))
        );
        assert_eq!(dir(None), Some(PathBuf::from("/home/reader/.config")));
        assert_eq!(xdg_dir(None, None, ".config"), None);
    }

    #[test]
    fn atomic_writes_replace_and_leave_no_temp_file() {
        reset_test_dir();
        let path = test_dir().join("deep/down/file.json");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn old_files_are_read_until_new_ones_exist() {
        reset_test_dir();
        let path = test_dir().join("new.json");
        assert_eq!(read_with_legacy(&path, "old.json"), None);
        write_atomic(&legacy("old.json"), "old").unwrap();
        assert_eq!(read_with_legacy(&path, "old.json").as_deref(), Some("old"));
        write_atomic(&path, "new").unwrap();
        assert_eq!(read_with_legacy(&path, "old.json").as_deref(), Some("new"));
    }

    #[test]
    fn tests_never_see_the_real_files() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-files");
        assert!(config_file().starts_with(&root));
        assert!(bookmarks_file().starts_with(&root));
    }
}
