use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "moosereader";

/// Resolves an XDG base directory: `$var` when it holds an absolute path,
/// otherwise `$HOME/<fallback>` (XDG Base Directory Specification).
fn xdg_dir(var: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
}

/// Settings live in `$XDG_CONFIG_HOME/moosereader/config.json` (default `~/.config`).
pub fn config_file() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config")
        .map(|dir| dir.join(APP_DIR).join("config.json"))
        .unwrap_or_else(|| PathBuf::from("reader_config.json"))
}

/// Bookmarks live in `$XDG_STATE_HOME/moosereader/bookmarks.json` (default `~/.local/state`).
pub fn bookmarks_file() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state")
        .map(|dir| dir.join(APP_DIR).join("bookmarks.json"))
        .unwrap_or_else(|| PathBuf::from("bookmarks.json"))
}

/// Reads `path`, falling back to `legacy` (the old working-directory location)
/// so earlier settings and bookmarks carry over.
pub fn read_with_legacy(path: &Path, legacy: &str) -> Option<String> {
    fs::read_to_string(path)
        .or_else(|_| fs::read_to_string(legacy))
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
