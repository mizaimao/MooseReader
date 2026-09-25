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
use std::io::{self, Read, Seek, Write};
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

impl AppState {
    pub fn new(term_cols: u16, term_rows: u16, cfg: &Config, chapter_starts: Vec<f64>) -> Self {
        AppState {
            mode: AppMode::Reading,
            chapter_index: 0,
            offset: 0,
            dynamic_width: text_width(term_cols, cfg),
            lines_per_page: page_height(term_rows, cfg),
            toc_cursor: 0,
            toc_top: 0,
            settings_cursor: 0,
            term_cols,
            term_rows,
            chapter_starts,
            layout: Layout::labels(),
            kitty: Kitty::default(),
        }
    }
}

/// Columns for text: the configured width, as far as the window and margins allow.
pub fn text_width(term_cols: u16, cfg: &Config) -> usize {
    let room = (term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right);
    cfg.max_width.min(room).max(10)
}

/// Rows for text: the window less the footer's two rows when it shows.
pub fn page_height(term_rows: u16, cfg: &Config) -> usize {
    (term_rows as usize).saturating_sub(if cfg.show_footer { 2 } else { 0 })
}

/// Lays out the current chapter for the window and settings, first refreshing
/// how pictures are shown, since the cell size or screen height may have changed.
pub fn load_current<R: Read + Seek>(
    app: &mut AppState,
    cfg: &Config,
    archive: &mut ZipArchive<R>,
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

/// Draws one frame into `frame`: erase, text, pictures, then any open menu,
/// wrapped in a synchronized update. Picture data goes straight to `terminal`.
pub fn draw_frame<R: Read + Seek>(
    frame: &mut Vec<u8>,
    terminal: &mut impl Write,
    app: &mut AppState,
    cfg: &Config,
    lines: &[String],
    spine: &[(String, String)],
    archive: &mut ZipArchive<R>,
) -> io::Result<()> {
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
    render::draw_reading_view(frame, app, cfg, lines, spine, &palette)?;
    // Pictures sit above the text, so they stay off while a menu is open
    app.kitty.clear(frame)?;
    if app.layout.mode == images::Mode::Kitty && app.mode == AppMode::Reading {
        let (offset, page) = (app.offset, app.lines_per_page);
        app.kitty
            .draw(frame, terminal, lines, offset, page, &app.layout, archive)?;
    }
    match app.mode {
        AppMode::TocMenu => render::draw_toc_menu(frame, app, cfg, spine, &palette)?,
        AppMode::SettingsMenu => render::draw_settings_menu(frame, app, cfg, &palette)?,
        AppMode::Reading => {}
    }
    queue!(frame, EndSynchronizedUpdate)?;
    Ok(())
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
    let starts = chapter_starts(&mut archive, &spine);
    let mut app = AppState::new(term_cols, term_rows, &cfg, starts);

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
            draw_frame(
                &mut frame,
                &mut stdout,
                &mut app,
                &cfg,
                &lines,
                &spine,
                &mut archive,
            )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{self, Language};
    use crate::images::Setting;
    use crate::test_support::{Book, Screen, Spine, book, paragraphs, png};

    fn plain_config() -> Config {
        Config {
            images: Setting::Off,
            ..Config::default()
        }
    }

    fn open(
        cfg: &Config,
        size: (u16, u16),
        chapters: &[(&str, &str)],
        extra: &[(&str, &[u8])],
    ) -> (AppState, Book, Spine, Vec<String>) {
        let (mut archive, spine) = book(chapters, extra);
        let starts = chapter_starts(&mut archive, &spine);
        let mut app = AppState::new(size.0, size.1, cfg, starts);
        let lines = load_current(&mut app, cfg, &mut archive, &spine);
        (app, archive, spine, lines)
    }

    /// Draws one frame and replays it; also returns what went straight to the terminal.
    fn draw(
        app: &mut AppState,
        cfg: &Config,
        lines: &[String],
        spine: &Spine,
        archive: &mut Book,
    ) -> (String, Screen, String) {
        let (mut frame, mut terminal) = (Vec::new(), Vec::new());
        draw_frame(&mut frame, &mut terminal, app, cfg, lines, spine, archive).unwrap();
        let screen = Screen::replay(app.term_cols as usize, app.term_rows as usize, &frame);
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();
        (text(frame), screen, text(terminal))
    }

    #[test]
    fn frames_update_in_place_without_erasing_the_screen() {
        let cfg = plain_config();
        let (mut app, mut archive, spine, lines) =
            open(&cfg, (60, 20), &[("One", &paragraphs(3, 20))], &[]);
        let (frame, _, _) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
        assert!(
            frame.starts_with("\x1b[?2026h"),
            "synchronized update starts the frame"
        );
        assert!(frame.ends_with("\x1b[?2026l"), "and ends it");
        // A full-screen erase makes Ghostty free the pictures sent for reuse
        assert!(!frame.contains("\x1b[2J"));
    }

    #[test]
    fn reading_view_shows_the_page_and_the_footer() {
        let cfg = plain_config();
        let chapters = [("One", paragraphs(4, 20)), ("Two", paragraphs(4, 20))];
        let chapters: Vec<(&str, &str)> = chapters.iter().map(|(t, b)| (*t, b.as_str())).collect();
        let (mut app, mut archive, spine, lines) = open(&cfg, (60, 20), &chapters, &[]);

        let (_, screen, _) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
        assert!(
            screen.row(0).starts_with("    p0w0 p0w1"),
            "{}",
            screen.text()
        );
        let footer = screen.row(19);
        assert!(
            footer.contains("--- One") && footer.contains("0%") && footer.contains("(1/2) ---")
        );
        assert!(!screen.overflowed);

        app.offset = 1;
        let (_, screen, _) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
        assert_eq!(screen.row(0), lines[1]);
    }

    #[test]
    fn footer_fits_a_narrow_window_with_a_wide_title() {
        let cfg = plain_config();
        let title = "第一章 一个专门用来测试页脚宽度的很长的标题";
        let body = paragraphs(2, 10);
        for (cols, shortened_title) in [(50, true), (30, false)] {
            let (mut app, mut archive, spine, lines) =
                open(&cfg, (cols, 12), &[(title, &body)], &[]);
            let (_, screen, _) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
            let footer = screen.row(11);
            // The title gives way first, then the dashes; the numbers stay
            assert!(
                footer.contains("0%") && footer.contains("(1/1)"),
                "{cols}: {footer}"
            );
            assert_eq!(
                footer.contains("第一章"),
                shortened_title,
                "{cols}: {footer}"
            );
            assert_eq!(footer.contains('…'), shortened_title, "{cols}: {footer}");
            assert!(
                !screen.overflowed,
                "a footer wider than the window would scroll the page"
            );
        }
    }

    #[test]
    fn contents_box_lines_up_with_wide_titles() {
        let cfg = plain_config();
        let body = paragraphs(3, 10);
        let titles = [
            "第一章 地铁",
            "Chapter Two",
            "第三章 一个很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的标题",
        ];
        let chapters: Vec<(&str, &str)> = titles.iter().map(|t| (*t, body.as_str())).collect();
        let (mut app, mut archive, spine, lines) = open(&cfg, (70, 20), &chapters, &[]);
        app.mode = AppMode::TocMenu;
        app.toc_cursor = 1;

        let (_, screen, _) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
        let borders: Vec<usize> = (0..screen.rows())
            .filter_map(|y| screen.last_column_of(y, '│'))
            .collect();
        assert!(!borders.is_empty());
        assert!(
            borders.iter().all(|&x| x == borders[0]),
            "{}",
            screen.text()
        );
        assert!(screen.text().contains("> Chapter Two"));
        assert!(screen.text().contains('…'), "the long title is shortened");
        assert!(!screen.overflowed);
    }

    #[test]
    fn settings_box_fits_in_every_language() {
        let cfg = plain_config();
        let body = paragraphs(3, 10);
        let (mut app, mut archive, spine, lines) = open(&cfg, (80, 30), &[("One", &body)], &[]);
        app.mode = AppMode::SettingsMenu;
        for language in [
            Language::English,
            Language::SimplifiedChinese,
            Language::TraditionalChinese,
            Language::Japanese,
            Language::Korean,
            Language::Russian,
            Language::Spanish,
            Language::French,
            Language::German,
        ] {
            i18n::set(language);
            let (_, screen, _) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
            let borders: Vec<usize> = (0..screen.rows())
                .filter_map(|y| screen.last_column_of(y, '│'))
                .collect();
            // Every row between the top and bottom borders
            assert_eq!(borders.len(), settings::ROWS.len() + 5, "{language:?}");
            assert!(
                borders.iter().all(|&x| x == borders[0]),
                "{language:?}\n{}",
                screen.text()
            );
            assert!(!screen.overflowed, "{language:?}");
        }
        i18n::set(Language::English);
    }

    #[test]
    fn pictures_are_sent_once_and_hidden_under_menus() {
        let cfg = Config {
            images: Setting::Kitty,
            ..Config::default()
        };
        let picture = png(40, 40, [200, 30, 30, 255]);
        let body = r#"<p>before</p><img src="pic.png"/><p>after</p>"#;
        let (mut app, mut archive, spine, lines) =
            open(&cfg, (60, 20), &[("One", body)], &[("pic.png", &picture)]);

        let (_, screen, sent) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
        assert_eq!(sent.matches("a=t,").count(), 1);
        assert!(screen.kitty.iter().any(|c| c.contains("a=p,")));

        let (_, screen, sent) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
        assert!(sent.is_empty(), "already sent");
        assert!(screen.kitty.iter().any(|c| c.contains("a=d,d=a")));
        assert!(screen.kitty.iter().any(|c| c.contains("a=p,")));

        app.mode = AppMode::TocMenu;
        let (_, screen, _) = draw(&mut app, &cfg, &lines, &spine, &mut archive);
        assert!(screen.kitty.iter().any(|c| c.contains("a=d,d=a")));
        assert!(
            !screen.kitty.iter().any(|c| c.contains("a=p,")),
            "hidden under the menu"
        );
    }
}
