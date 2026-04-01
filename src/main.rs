mod config;
mod epub;
mod ui;

use std::env;
use std::fs::File;
use zip::ZipArchive;

fn main() -> std::io::Result<()> {
    // 1. Load configuration
    let cfg = config::load_or_create_config();

    // 2. Parse arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: cargo run -- <path_to_epub>");
        return Ok(());
    }

    // 3. Open the EPUB archive
    let file = File::open(&args[1])?;
    let mut archive = ZipArchive::new(file)?;
    
    // 4. Parse the spine (Table of Contents)
    let spine = epub::get_epub_spine(&mut archive).expect("Failed to parse EPUB spine.");
    if spine.is_empty() {
        println!("No chapters found in EPUB.");
        return Ok(());
    }

    // 5. Hand off to the terminal UI loop
    ui::run(archive, spine, cfg)
}