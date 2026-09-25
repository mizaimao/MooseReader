pub mod input;
pub mod render;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    style::{ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use std::fs::File;
use std::io::{self, Write};
use zip::ZipArchive;

use crate::config::Config;
use crate::epub::load_chapter;
use crate::state::{Bookmark, State, save_state};

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

/// Raw mode on the alternate screen, undone on drop so an early return
/// never leaves the shell unusable or the book in the scrollback.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = execute!(io::stdout(), ResetColor, Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

fn save_bookmark(state: &mut State, book_path: &str, app: &AppState, lines: &[String]) {
    let progress = if lines.is_empty() {
        0.0
    } else {
        app.offset as f64 / lines.len() as f64
    };
    state.books.insert(
        book_path.to_string(),
        Bookmark {
            chapter: app.chapter_index,
            progress,
        },
    );
    save_state(state);
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
        dynamic_width: std::cmp::max(
            10,
            std::cmp::min(
                cfg.max_width,
                (term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right),
            ),
        ),
        lines_per_page: (term_rows as usize).saturating_sub(if cfg.show_footer { 2 } else { 0 }),
        toc_cursor: 0,
        toc_top: 0,
        settings_cursor: 0,
        term_cols,
        term_rows,
    };

    let progress = if let Some(bookmark) = state.books.get(&book_path) {
        app.chapter_index = std::cmp::min(bookmark.chapter, spine.len().saturating_sub(1));
        bookmark.progress
    } else {
        0.0
    };

    app.toc_cursor = app.chapter_index;
    let mut lines = load_chapter(
        &mut archive,
        &spine[app.chapter_index].0,
        app.dynamic_width,
        cfg.margin_left,
    );
    app.offset = (progress * lines.len() as f64).floor() as usize;
    if app.offset >= lines.len() {
        app.offset = lines.len().saturating_sub(app.lines_per_page);
    }

    // Restore the terminal before the panic message prints, or it lands on the alternate screen
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    let mut stdout = io::stdout();
    let _terminal = TerminalGuard::enter()?;
    let mut unsaved = false;

    loop {
        // Grab the active color palette and flood-fill the background
        let palette = render::get_palette(&cfg.theme);
        execute!(
            stdout,
            MoveTo(0, 0),
            SetBackgroundColor(palette.bg),
            SetForegroundColor(palette.fg),
            Clear(ClearType::All)
        )?;

        // Pass the palette into the render functions
        render::draw_reading_view(&mut stdout, &app, &cfg, &lines, &spine, &palette)?;
        match app.mode {
            AppMode::TocMenu => {
                render::draw_toc_menu(&mut stdout, &mut app, &cfg, &spine, &palette)?
            }
            AppMode::SettingsMenu => render::draw_settings_menu(&mut stdout, &app, &cfg, &palette)?,
            AppMode::Reading => {}
        }
        stdout.flush()?;

        if event::poll(std::time::Duration::from_millis(500))? {
            let position = (app.chapter_index, app.offset);
            // Read exactly one event per poll; a second read() would block and drop this one
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    let ctrl_c = key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && key_event.code == KeyCode::Char('c');
                    let quit_requested = ctrl_c
                        || match app.mode {
                            AppMode::Reading => input::handle_reading_input(
                                key_event.code,
                                &mut app,
                                &mut cfg,
                                &mut lines,
                                &mut archive,
                                &spine,
                            ),
                            AppMode::TocMenu => input::handle_toc_input(
                                key_event.code,
                                &mut app,
                                &cfg,
                                &mut lines,
                                &mut archive,
                                &spine,
                            ),
                            AppMode::SettingsMenu => input::handle_settings_input(
                                key_event.code,
                                &mut app,
                                &mut cfg,
                                &mut lines,
                                &mut archive,
                                &spine,
                            ),
                        };
                    if quit_requested {
                        save_bookmark(&mut state, &book_path, &app, &lines);
                        break;
                    }
                }
                Event::Resize(new_cols, new_rows) => {
                    input::handle_resize(
                        new_cols,
                        new_rows,
                        &mut app,
                        &cfg,
                        &mut lines,
                        &mut archive,
                        &spine,
                    );
                }
                _ => {}
            }
            unsaved |= (app.chapter_index, app.offset) != position;
        } else if unsaved {
            // Saves once reading pauses, so closing the window or a crash loses at most half a second
            save_bookmark(&mut state, &book_path, &app, &lines);
            unsaved = false;
        }
    }

    // Dropping the guard resets colors and hands the terminal back
    Ok(())
}
