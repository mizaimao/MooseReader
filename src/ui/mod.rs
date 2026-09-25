pub mod input;
pub mod render;
pub mod settings;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use std::fs::File;
use std::io::{self, Write};
use zip::ZipArchive;

use crate::config::Config;
use crate::epub::{chapter_starts, load_chapter};
use crate::images::{self, Kitty, Layout};
use crate::state::{BookId, State, save_state};

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
    /// Where each chapter starts as a fraction of the book, plus a final 1.0
    pub chapter_starts: Vec<f64>,
    pub layout: Layout,
    pub kitty: Kitty,
}

/// Lays out the current chapter for the window and settings, first refreshing
/// how pictures are shown, since the cell size or screen height may have changed.
pub fn load_current(
    app: &mut AppState,
    cfg: &Config,
    archive: &mut ZipArchive<File>,
    spine: &[(String, String)],
) -> Vec<String> {
    let background = match render::get_palette(&cfg.theme).bg {
        Color::Rgb { r, g, b } => [r, g, b],
        _ => [0, 0, 0],
    };
    app.layout = Layout {
        mode: images::Mode::from_setting(cfg.images),
        cell: images::cell_size(),
        max_rows: app.lines_per_page.max(1),
        background,
    };
    let path = &spine[app.chapter_index].0;
    load_chapter(
        archive,
        path,
        app.dynamic_width,
        cfg.margin_left,
        &app.layout,
        cfg.plain_styles,
    )
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

fn save_bookmark(state: &mut State, book: &BookId, app: &AppState, lines: &[String]) {
    let progress = if lines.is_empty() {
        0.0
    } else {
        app.offset as f64 / lines.len() as f64
    };
    state.record(book, app.chapter_index, progress);
    save_state(state);
}

pub fn run(
    mut archive: ZipArchive<File>,
    spine: Vec<(String, String)>,
    mut cfg: Config,
    mut state: State,
    book: BookId,
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
        chapter_starts: chapter_starts(&mut archive, &spine),
        layout: Layout::labels(),
        kitty: Kitty::default(),
    };

    let progress = if let Some(bookmark) = state.find(&book) {
        app.chapter_index = std::cmp::min(bookmark.chapter, spine.len().saturating_sub(1));
        bookmark.progress
    } else {
        0.0
    };

    app.toc_cursor = app.chapter_index;
    let mut lines = load_current(&mut app, &cfg, &mut archive, &spine);
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
    let mut redraw = true;
    let mut frame = Vec::new();

    loop {
        // Draw only after something changed, into one buffer written at once,
        // wrapped in a synchronized update so the terminal never shows a half-drawn frame
        if redraw {
            frame.clear();
            // Grab the active color palette and flood-fill the background
            let palette = render::get_palette(&cfg.theme);
            queue!(
                frame,
                BeginSynchronizedUpdate,
                SetBackgroundColor(palette.bg),
                SetForegroundColor(palette.fg)
            )?;
            // Erased row by row: a full-screen erase (ED 2) makes Ghostty free every
            // picture not on screen at that moment, including the ones sent for reuse
            for row in 0..app.term_rows {
                queue!(frame, MoveTo(0, row), Clear(ClearType::CurrentLine))?;
            }

            // Pass the palette into the render functions
            render::draw_reading_view(&mut frame, &app, &cfg, &lines, &spine, &palette)?;
            // Pictures sit above the text, so they stay off while a menu is open
            app.kitty.clear(&mut frame)?;
            if app.layout.mode == images::Mode::Kitty && app.mode == AppMode::Reading {
                let (offset, page) = (app.offset, app.lines_per_page);
                app.kitty.draw(
                    &mut frame,
                    &mut stdout,
                    &lines,
                    offset,
                    page,
                    &app.layout,
                    &mut archive,
                )?;
            }
            match app.mode {
                AppMode::TocMenu => {
                    render::draw_toc_menu(&mut frame, &mut app, &cfg, &spine, &palette)?
                }
                AppMode::SettingsMenu => {
                    render::draw_settings_menu(&mut frame, &app, &cfg, &palette)?
                }
                AppMode::Reading => {}
            }
            queue!(frame, EndSynchronizedUpdate)?;
            stdout.write_all(&frame)?;
            stdout.flush()?;
            redraw = false;
        }

        if event::poll(std::time::Duration::from_millis(500))? {
            redraw = true;
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
                        save_bookmark(&mut state, &book, &app, &lines);
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
            save_bookmark(&mut state, &book, &app, &lines);
            unsaved = false;
        }
    }

    // Frees the pictures the terminal holds; dropping the guard then hands it back
    app.kitty.release(&mut stdout)?;
    Ok(())
}
