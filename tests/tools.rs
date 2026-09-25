//! The helper scripts: tools/pack-epub and the moose launcher.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A fresh folder for one test, inside Cargo's target directory.
fn workdir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("tools")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn has_zip() -> bool {
    Command::new("zip")
        .arg("-v")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// An unpacked book: container, one chapter, and the .DS_Store Finder leaves behind.
fn book_folder(dir: &Path, name: &str, with_mimetype: bool) {
    let book = dir.join(name);
    std::fs::create_dir_all(book.join("META-INF")).unwrap();
    std::fs::create_dir_all(book.join("OEBPS")).unwrap();
    if with_mimetype {
        std::fs::write(book.join("mimetype"), "application/epub+zip").unwrap();
    }
    std::fs::write(book.join("META-INF/container.xml"), "<container/>").unwrap();
    std::fs::write(book.join("OEBPS/c.html"), "<p>text</p>").unwrap();
    std::fs::write(book.join(".DS_Store"), "junk").unwrap();
}

fn pack(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(root().join("tools/pack-epub"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    let errors = String::from_utf8_lossy(&out.stderr).into_owned();
    (out.status.code().unwrap_or(-1), errors)
}

/// Each entry's name and whether it is stored uncompressed.
fn entries(path: &Path) -> Vec<(String, bool)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let entry = zip.by_index(i).unwrap();
            let stored = entry.compression() == zip::CompressionMethod::Stored;
            (entry.name().to_string(), stored)
        })
        .collect()
}

#[test]
fn packs_an_apple_books_folder_the_way_epub_requires() {
    if !has_zip() {
        return eprintln!("skipped: no zip command");
    }
    let dir = workdir("apple_folder");
    book_folder(&dir, "My Book.epub", true);
    assert_eq!(pack(&dir, &["-q", "My Book.epub"]).0, 0);
    let entries = entries(&dir.join("My Book (packed).epub"));
    assert_eq!(
        entries[0],
        ("mimetype".into(), true),
        "first and uncompressed"
    );
    let names: Vec<&str> = entries.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"META-INF/container.xml") && names.contains(&"OEBPS/c.html"));
    assert!(
        !names.iter().any(|name| name.contains(".DS_Store")),
        "{names:?}"
    );
}

#[test]
fn keeps_an_existing_book_unless_told_to_replace_it() {
    if !has_zip() {
        return eprintln!("skipped: no zip command");
    }
    let dir = workdir("existing");
    book_folder(&dir, "Book.epub", true);
    assert_eq!(pack(&dir, &["-q", "Book.epub"]).0, 0);
    let (code, errors) = pack(&dir, &["-q", "Book.epub"]);
    assert_eq!(code, 1);
    assert!(errors.contains("already exists"), "{errors}");
    assert_eq!(pack(&dir, &["-q", "-f", "Book.epub"]).0, 0);
}

#[test]
fn adds_a_missing_mimetype() {
    if !has_zip() {
        return eprintln!("skipped: no zip command");
    }
    let dir = workdir("no_mimetype");
    book_folder(&dir, "Loose", false);
    assert_eq!(pack(&dir, &["-q", "-o", "out.epub", "Loose"]).0, 0);
    let path = dir.join("out.epub");
    assert_eq!(entries(&path)[0], ("mimetype".into(), true));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut mimetype = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("mimetype").unwrap(), &mut mimetype).unwrap();
    assert_eq!(mimetype, "application/epub+zip");
    let leftovers = std::fs::read_dir(&dir).unwrap().filter(|e| {
        e.as_ref()
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pack-epub")
    });
    assert_eq!(leftovers.count(), 0);
}

#[test]
fn refuses_folders_that_are_not_books() {
    let dir = workdir("not_a_book");
    std::fs::create_dir_all(dir.join("notes")).unwrap();
    std::fs::write(dir.join("notes/todo.txt"), "milk").unwrap();
    let (code, errors) = pack(&dir, &["notes"]);
    assert_eq!(code, 1);
    assert!(errors.contains("not an EPUB folder"), "{errors}");
    assert!(!dir.join("notes.epub").exists());
}

#[test]
fn the_launcher_explains_itself_without_building() {
    let run = |args: &[&str]| {
        Command::new(root().join("moose"))
            .args(args)
            .output()
            .unwrap()
    };
    let bare = run(&[]);
    assert_eq!(bare.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bare.stderr).starts_with("Usage: moose BOOK"));
    assert_eq!(run(&["--help"]).status.code(), Some(0));
}
