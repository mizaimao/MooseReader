//! The reader run as a real program: arguments, error messages and exit codes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh folder for one test, inside Cargo's target directory.
fn workdir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("cli")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs the reader in `dir`, with its settings and bookmarks kept there too,
/// under the given LANG. Returns the exit code and what it printed as errors.
fn run(dir: &Path, args: &[&str], lang: &str) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_moose_reader"))
        .args(args)
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env("LANG", lang)
        .output()
        .unwrap();
    let errors = String::from_utf8_lossy(&out.stderr).into_owned();
    (out.status.code().unwrap_or(-1), errors)
}

#[test]
fn no_arguments_prints_usage() {
    let dir = workdir("usage");
    let (code, errors) = run(&dir, &[], "C");
    assert_eq!(code, 1);
    assert_eq!(errors.trim(), "Usage: cargo run -- <path_to_epub>");
}

#[test]
fn a_missing_file_is_reported() {
    let dir = workdir("missing");
    let (code, errors) = run(&dir, &["missing.epub"], "C");
    assert_eq!(code, 1);
    assert!(
        errors.starts_with("MooseReader: cannot open missing.epub:"),
        "{errors}"
    );
}

#[test]
fn a_file_that_is_not_a_zip_is_reported() {
    let dir = workdir("not_zip");
    std::fs::write(dir.join("fake.epub"), "plain text").unwrap();
    let (code, errors) = run(&dir, &["fake.epub"], "C");
    assert_eq!(code, 1);
    assert_eq!(
        errors.trim(),
        "MooseReader: fake.epub is not an EPUB (not a zip archive)"
    );
}

#[test]
fn a_zip_without_a_book_inside_is_reported() {
    let dir = workdir("not_epub");
    let file = std::fs::File::create(dir.join("other.epub")).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file("hello.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"hi").unwrap();
    zip.finish().unwrap();
    let (code, errors) = run(&dir, &["other.epub"], "C");
    assert_eq!(code, 1);
    assert_eq!(
        errors.trim(),
        "MooseReader: other.epub is not a readable EPUB (no package or spine)"
    );
}

#[test]
fn messages_follow_the_system_language() {
    let dir = workdir("languages");
    let (_, japanese) = run(&dir, &["missing.epub"], "ja_JP.UTF-8");
    assert!(japanese.contains("missing.epub を開けません"), "{japanese}");
    let (_, german) = run(&dir, &["missing.epub"], "de_DE.UTF-8");
    assert!(
        german.contains("missing.epub kann nicht geöffnet werden"),
        "{german}"
    );
    let (_, chinese) = run(&dir, &[], "zh_TW.UTF-8");
    assert!(chinese.starts_with("用法："), "{chinese}");
}

#[test]
fn failed_runs_leave_no_files_behind() {
    let dir = workdir("no_files");
    std::fs::write(dir.join("fake.epub"), "plain text").unwrap();
    run(&dir, &[], "C");
    run(&dir, &["missing.epub"], "C");
    run(&dir, &["fake.epub"], "C");
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, ["fake.epub"]);
}
