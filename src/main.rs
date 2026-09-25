mod config;
mod epub;
mod i18n;
mod paths;
mod state; // <-- Register our new state module
mod ui;
mod width;

use std::env;
use std::fs::File;
use std::process::ExitCode;
use zip::ZipArchive;

use i18n::{Text, tr};

fn main() -> ExitCode {
    // Messages follow the language setting even before a book is open
    i18n::set(config::peek_language());

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("{}", tr(Text::Usage));
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

    let file =
        File::open(raw_path).map_err(|e| message(Text::CannotOpen, raw_path, &e.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(|_| message(Text::NotZip, raw_path, ""))?;

    let spine =
        epub::get_epub_spine(&mut archive).ok_or_else(|| message(Text::NotEpub, raw_path, ""))?;
    if spine.is_empty() {
        return Err(message(Text::NoChapters, raw_path, ""));
    }

    // Settings and bookmarks are only touched once there is a book to read
    let cfg = config::load_or_create_config();
    i18n::set(cfg.language);
    let state = state::load_state(); // <-- Load all bookmarks from disk

    // Hand off to the terminal UI loop, passing the state and how to find this book's bookmark
    ui::run(
        archive,
        spine,
        cfg,
        state,
        state::BookId::new(absolute_path),
    )
    .map_err(|e| message(Text::TerminalError, raw_path, &e.to_string()))
}

fn message(text: Text, path: &str, error: &str) -> String {
    tr(text).replace("{path}", path).replace("{error}", error)
}
