pub mod input;
pub mod render;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::fs::File;
use std::io::{self, Write};
use zip::ZipArchive;

use crate::config::Config;
use crate::epub::load_chapter;
use crate::state::{save_state, Bookmark, State};

#[derive(PartialEq)]
pub enum AppMode {
    Reading,
    TocMenu,
    SettingsMenu,
}

pub struct AppState {
    pub mode: AppMode,
    pub chapter_index: usize,
    pub offset: usize,
    pub dynamic_width: usize,
    pub lines_per_page: usize,
    pub toc_cursor: usize,
    pub toc_top: usize,
    pub settings_cursor: usize,
    pub term_cols: u16,
    pub term_rows: u16,
}

pub fn run(
    mut archive: ZipArchive<File>,
    spine: Vec<(String, String)>,
    mut cfg: Config,
    mut state: State,
    book_path: String,
) -> io::Result<()> {
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));

    let mut app = AppState {
        mode: AppMode::Reading,
        chapter_index: 0,
        offset: 0,
        dynamic_width: std::cmp::max(10, std::cmp::min(cfg.max_width, (term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right))),
        lines_per_page: (term_rows as usize).saturating_sub(if cfg.show_footer { 2 } else { 0 }),
        toc_cursor: 0,
        toc_top: 0,
        settings_cursor: 0,
        term_cols,
        term_rows,
    };

    // Load State
    let progress = if let Some(bookmark) = state.books.get(&book_path) {
        app.chapter_index = std::cmp::min(bookmark.chapter, spine.len().saturating_sub(1));
        bookmark.progress
    } else {
        0.0
    };

    app.toc_cursor = app.chapter_index;
    let mut lines = load_chapter(&mut archive, &spine[app.chapter_index].0, app.dynamic_width, cfg.margin_left);
    app.offset = (progress * lines.len() as f64).floor() as usize;
    if app.offset >= lines.len() {
        app.offset = lines.len().saturating_sub(app.lines_per_page);
    }

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, Hide, Clear(ClearType::All))?;

    // --- MAIN LOOP ---
    loop {
        execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;

        // 1. Render Background
        render::draw_reading_view(&mut stdout, &app, &cfg, &lines, &spine)?;

        // 2. Render Overlays
        match app.mode {
            AppMode::TocMenu => render::draw_toc_menu(&mut stdout, &mut app, &cfg, &spine)?,
            AppMode::SettingsMenu => render::draw_settings_menu(&mut stdout, &app, &cfg)?,
            AppMode::Reading => {}
        }

        stdout.flush()?;

        // 3. Handle Events
        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    let quit_requested = match app.mode {
                        AppMode::Reading => input::handle_reading_input(key_event.code, &mut app, &mut cfg, &mut lines, &mut archive, &spine),
                        AppMode::TocMenu => input::handle_toc_input(key_event.code, &mut app, &cfg, &mut lines, &mut archive, &spine),
                        AppMode::SettingsMenu => input::handle_settings_input(key_event.code, &mut app, &mut cfg, &mut lines, &mut archive, &spine),
                    };

                    if quit_requested {
                        let current_progress = if lines.is_empty() { 0.0 } else { app.offset as f64 / lines.len() as f64 };
                        state.books.insert(book_path.clone(), Bookmark { chapter: app.chapter_index, progress: current_progress });
                        save_state(&state);
                        break;
                    }
                }
            } else if let Event::Resize(new_cols, new_rows) = event::read()? {
                input::handle_resize(new_cols, new_rows, &mut app, &cfg, &mut lines, &mut archive, &spine);
            }
        }
    }

    execute!(stdout, Show)?;
    disable_raw_mode()?;
    Ok(())
}
