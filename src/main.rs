mod config;
mod epub;
mod paths;
mod state; // <-- Register our new state module
mod ui;

use std::env;
use std::fs::File;
use std::process::ExitCode;
use zip::ZipArchive;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run -- <path_to_epub>");
        return ExitCode::FAILURE;
    }

    match open_and_read(&args[1]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("MooseReader: {}", message);
            ExitCode::FAILURE
        }
    }
}

fn open_and_read(raw_path: &str) -> Result<(), String> {
    // Get the absolute path so we can uniquely identify this specific book
    let absolute_path = std::fs::canonicalize(raw_path)
        .unwrap_or_else(|_| std::path::PathBuf::from(raw_path))
        .to_string_lossy()
        .to_string();

    let file = File::open(raw_path).map_err(|e| format!("cannot open {}: {}", raw_path, e))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|_| format!("{} is not an EPUB (not a zip archive)", raw_path))?;

    let spine = epub::get_epub_spine(&mut archive)
        .ok_or_else(|| format!("{} is not a readable EPUB (no package or spine)", raw_path))?;
    if spine.is_empty() {
        return Err(format!("no chapters found in {}", raw_path));
    }

    // Settings and bookmarks are only touched once there is a book to read
    let cfg = config::load_or_create_config();
    let state = state::load_state(); // <-- Load all bookmarks from disk

    // Hand off to the terminal UI loop, passing the state and the book's unique path
    ui::run(archive, spine, cfg, state, absolute_path).map_err(|e| format!("terminal error: {}", e))
}
