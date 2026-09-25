use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use crate::paths;

#[derive(Serialize, Deserialize, Clone)]
pub struct Bookmark {
    pub chapter: usize,
    pub progress: f64, // Changed from offset to a percentage
    /// Where the book was last opened from, to find it again if its content changes
    #[serde(default)]
    pub path: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct State {
    pub books: HashMap<String, Bookmark>,
}

/// How a book is looked up in the bookmarks: by a fingerprint of its content,
/// so moving or renaming the file keeps its place, with its path as a fallback.
pub struct BookId {
    pub key: String,
    pub path: String,
}

impl BookId {
    pub fn new(path: String) -> Self {
        let key = std::fs::File::open(&path)
            .ok()
            .and_then(fingerprint)
            .unwrap_or_else(|| path.clone());
        Self { key, path }
    }
}

/// Hashes 1 KiB samples at 256 B, 1 KiB, 4 KiB, ... up to 1 GiB into the file:
/// the sampling KOReader uses for its partial-MD5 document fingerprint,
/// hashed here with 64-bit FNV-1a.
fn fingerprint<R: Read + Seek>(mut book: R) -> Option<String> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut sample = Vec::with_capacity(1024);
    for step in -1..=10 {
        let offset = if step < 0 { 256 } else { 1024u64 << (2 * step) };
        book.seek(SeekFrom::Start(offset)).ok()?;
        sample.clear();
        (&mut book).take(1024).read_to_end(&mut sample).ok()?;
        if sample.is_empty() {
            break;
        }
        for &byte in &sample {
            hash = (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    Some(format!("fingerprint:{:016x}", hash))
}

impl State {
    pub fn find(&self, book: &BookId) -> Option<&Bookmark> {
        self.books
            .get(&book.key)
            // Saved by path, before fingerprints
            .or_else(|| self.books.get(&book.path))
            // Same file, changed content (e.g. edited metadata)
            .or_else(|| self.books.values().find(|b| b.path == book.path))
    }

    /// Saves a position under the book's fingerprint, replacing older entries for the same file.
    pub fn record(&mut self, book: &BookId, chapter: usize, progress: f64) {
        self.books
            .retain(|key, b| *key == book.key || (*key != book.path && b.path != book.path));
        self.books.insert(
            book.key.clone(),
            Bookmark {
                chapter,
                progress,
                path: book.path.clone(),
            },
        );
    }
}

pub fn load_state() -> State {
    let state_path = paths::bookmarks_file();

    if let Some(file_content) = paths::read_with_legacy(&state_path, "bookmarks.json")
        && let Ok(state) = serde_json::from_str(&file_content)
    {
        return state;
    }

    State::default()
}

pub fn save_state(state: &State) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = paths::write_atomic(&paths::bookmarks_file(), &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn id(key: &str, path: &str) -> BookId {
        BookId {
            key: key.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn fingerprint_depends_on_content_only() {
        let a: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        let mut b = a.clone();
        b[5_000] ^= 1; // inside the sample taken at 4 KiB
        let print = |bytes: &[u8]| fingerprint(Cursor::new(bytes)).unwrap();
        assert_eq!(print(&a), print(&a.clone()));
        assert_ne!(print(&a), print(&b));
    }

    #[test]
    fn bookmarks_follow_moves_edits_and_old_path_keys() {
        let mut state = State::default();
        let old = Bookmark {
            chapter: 3,
            progress: 0.5,
            path: String::new(),
        };
        state.books.insert("/old/book.epub".to_string(), old);

        // First open after upgrading: found by its path key, then saved by fingerprint
        let at_old = id("fingerprint:1", "/old/book.epub");
        assert_eq!(state.find(&at_old).map(|b| b.chapter), Some(3));
        state.record(&at_old, 4, 0.1);
        assert_eq!(state.books.len(), 1);

        // Moved or renamed: same fingerprint
        let moved = id("fingerprint:1", "/new/renamed.epub");
        assert_eq!(state.find(&moved).map(|b| b.chapter), Some(4));

        // Edited in place: new fingerprint, same path
        let edited = id("fingerprint:2", "/old/book.epub");
        assert_eq!(state.find(&edited).map(|b| b.chapter), Some(4));
        state.record(&edited, 5, 0.0);
        assert_eq!(state.books.len(), 1);
        assert!(state.books.contains_key("fingerprint:2"));
    }

    #[test]
    fn bookmarks_round_trip_through_the_file() {
        paths::reset_test_dir();
        let mut state = State::default();
        let book = id("fingerprint:7", "/books/a.epub");
        state.record(&book, 3, 0.25);
        save_state(&state);
        let loaded = load_state();
        let mark = loaded.find(&book).unwrap();
        assert_eq!((mark.chapter, mark.progress), (3, 0.25));
        assert_eq!(mark.path, "/books/a.epub");
    }

    #[test]
    fn a_copied_or_renamed_book_has_the_same_key() {
        paths::reset_test_dir();
        let dir = paths::test_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let bytes: Vec<u8> = (0..50_000u32).map(|i| (i * 7 % 256) as u8).collect();
        let mut edited = bytes.clone();
        edited[17_000] ^= 1; // inside the sample taken at 16 KiB
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        std::fs::write(path("a.epub"), &bytes).unwrap();
        std::fs::write(path("renamed.epub"), &bytes).unwrap();
        std::fs::write(path("edited.epub"), &edited).unwrap();
        let key = |name: &str| BookId::new(path(name)).key;
        assert_eq!(key("a.epub"), key("renamed.epub"));
        assert_ne!(key("a.epub"), key("edited.epub"));
        // A file that can't be read is known by its path
        assert_eq!(key("missing.epub"), path("missing.epub"));
    }
}
