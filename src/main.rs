mod config;
mod epub;
mod state; // <-- Register our new state module
mod ui;

use std::env;
use std::fs::File;
use zip::ZipArchive;

fn main() -> std::io::Result<()> {
    let cfg = config::load_or_create_config();
    let state = state::load_state(); // <-- Load all bookmarks from disk

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: cargo run -- <path_to_epub>");
        return Ok(());
    }

    // Get the absolute path so we can uniquely identify this specific book
    let raw_path = &args[1];
    let absolute_path = std::fs::canonicalize(raw_path)
        .unwrap_or_else(|_| std::path::PathBuf::from(raw_path))
        .to_string_lossy()
        .to_string();

    let file = File::open(raw_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    let spine = epub::get_epub_spine(&mut archive).expect("Failed to parse EPUB spine.");
    if spine.is_empty() {
        println!("No chapters found in EPUB.");
        return Ok(());
    }

    // Hand off to the terminal UI loop, passing the state and the book's unique path
    ui::run(archive, spine, cfg, state, absolute_path)
}