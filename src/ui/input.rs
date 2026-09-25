use crossterm::event::KeyCode;
use std::io::{Read, Seek};
use zip::ZipArchive;

use super::settings::ROWS;
use super::{AppMode, AppState, load_current, page_height, text_width};
use crate::config::{Config, save_config};

pub fn handle_reading_input<R: Read + Seek>(
    code: KeyCode,
    app: &mut AppState,
    cfg: &mut Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<R>,
    spine: &[(String, String)],
) -> bool {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return true,
        KeyCode::Tab => {
            app.mode = AppMode::TocMenu;
            app.toc_cursor = app.chapter_index;
        }
        KeyCode::Char('S') | KeyCode::Char('s') => {
            app.mode = AppMode::SettingsMenu;
            app.settings_cursor = 0;
        }
        KeyCode::Char('F') | KeyCode::Char('f') => {
            cfg.show_footer = !cfg.show_footer;
            app.lines_per_page = page_height(app.term_rows, cfg);
            // Kept like the same switch in the settings menu
            save_config(cfg);
        }
        KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Down => {
            if app.offset + app.lines_per_page < lines.len() {
                app.offset += 1;
            } else if app.chapter_index + 1 < spine.len() {
                app.chapter_index += 1;
                *lines = load_current(app, cfg, archive, spine);
                app.offset = 0;
            }
        }
        KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Up => {
            if app.offset > 0 {
                app.offset -= 1;
            } else if app.chapter_index > 0 {
                app.chapter_index -= 1;
                *lines = load_current(app, cfg, archive, spine);
                app.offset = if lines.len() > app.lines_per_page {
                    lines.len() - app.lines_per_page
                } else {
                    0
                };
            }
        }
        KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Right | KeyCode::Char(' ') => {
            if app.offset + app.lines_per_page < lines.len() {
                app.offset = std::cmp::min(
                    app.offset + cfg.scroll_by_lines,
                    lines.len().saturating_sub(app.lines_per_page),
                );
            } else if app.chapter_index + 1 < spine.len() {
                app.chapter_index += 1;
                *lines = load_current(app, cfg, archive, spine);
                app.offset = 0;
            }
        }
        KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Left => {
            if app.offset > 0 {
                app.offset = app.offset.saturating_sub(cfg.scroll_by_lines);
            } else if app.chapter_index > 0 {
                app.chapter_index -= 1;
                *lines = load_current(app, cfg, archive, spine);
                app.offset = if lines.len() > app.lines_per_page {
                    lines.len() - app.lines_per_page
                } else {
                    0
                };
            }
        }
        _ => {}
    }
    false
}

pub fn handle_toc_input<R: Read + Seek>(
    code: KeyCode,
    app: &mut AppState,
    cfg: &Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<R>,
    spine: &[(String, String)],
) -> bool {
    match code {
        KeyCode::Tab | KeyCode::Esc | KeyCode::Char('q') => app.mode = AppMode::Reading,
        KeyCode::Char('j') | KeyCode::Down => {
            if app.toc_cursor + 1 < spine.len() {
                app.toc_cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.toc_cursor > 0 {
                app.toc_cursor -= 1;
            }
        }
        KeyCode::Enter => {
            app.chapter_index = app.toc_cursor;
            *lines = load_current(app, cfg, archive, spine);
            app.offset = 0;
            app.mode = AppMode::Reading;
        }
        _ => {}
    }
    false
}

pub fn handle_settings_input<R: Read + Seek>(
    code: KeyCode,
    app: &mut AppState,
    cfg: &mut Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<R>,
    spine: &[(String, String)],
) -> bool {
    match code {
        KeyCode::Tab
        | KeyCode::Esc
        | KeyCode::Char('q')
        | KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Enter => {
            save_config(cfg);
            app.lines_per_page = page_height(app.term_rows, cfg);
            app.mode = AppMode::Reading;
        }

        KeyCode::Char('j') | KeyCode::Down => {
            app.settings_cursor = (app.settings_cursor + 1) % ROWS.len()
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.settings_cursor = (app.settings_cursor + ROWS.len() - 1) % ROWS.len()
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('l') | KeyCode::Right => {
            let forward = matches!(code, KeyCode::Char('l') | KeyCode::Right);
            if ROWS[app.settings_cursor].step(cfg, forward) {
                update_layout_live(app, cfg, lines, archive, spine);
            }
        }
        _ => {}
    }
    false
}

pub fn update_layout_live<R: Read + Seek>(
    app: &mut AppState,
    cfg: &Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<R>,
    spine: &[(String, String)],
) {
    let current_progress = if lines.is_empty() {
        0.0
    } else {
        app.offset as f64 / lines.len() as f64
    };
    app.dynamic_width = text_width(app.term_cols, cfg);
    *lines = load_current(app, cfg, archive, spine);
    app.offset = (current_progress * lines.len() as f64).floor() as usize;
    if app.offset >= lines.len() {
        app.offset = lines.len().saturating_sub(app.lines_per_page);
    }
}

pub fn handle_resize<R: Read + Seek>(
    new_cols: u16,
    new_rows: u16,
    app: &mut AppState,
    cfg: &Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<R>,
    spine: &[(String, String)],
) {
    let current_progress = if lines.is_empty() {
        0.0
    } else {
        app.offset as f64 / lines.len() as f64
    };
    app.term_cols = new_cols;
    app.term_rows = new_rows;
    app.dynamic_width = text_width(app.term_cols, cfg);
    app.lines_per_page = page_height(app.term_rows, cfg);

    *lines = load_current(app, cfg, archive, spine);
    app.offset = (current_progress * lines.len() as f64).floor() as usize;
    if app.offset >= lines.len() {
        app.offset = lines.len().saturating_sub(app.lines_per_page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::chapter_starts;
    use crate::images::Setting;
    use crate::paths;
    use crate::test_support::{Book, Spine, book, paragraphs};

    struct Reader {
        app: AppState,
        cfg: Config,
        archive: Book,
        spine: Spine,
        lines: Vec<String>,
    }

    /// Two chapters of the same length in a 60 x 20 window.
    fn reader() -> Reader {
        let cfg = Config {
            images: Setting::Off,
            ..Config::default()
        };
        let body = paragraphs(12, 30);
        let (mut archive, spine) = book(&[("One", &body), ("Two", &body)], &[]);
        let starts = chapter_starts(&mut archive, &spine);
        let mut app = AppState::new(60, 20, &cfg, starts);
        let lines = load_current(&mut app, &cfg, &mut archive, &spine);
        Reader {
            app,
            cfg,
            archive,
            spine,
            lines,
        }
    }

    impl Reader {
        fn read(&mut self, code: KeyCode) -> bool {
            let r = self;
            handle_reading_input(
                code,
                &mut r.app,
                &mut r.cfg,
                &mut r.lines,
                &mut r.archive,
                &r.spine,
            )
        }

        fn toc(&mut self, code: KeyCode) -> bool {
            let r = self;
            handle_toc_input(
                code,
                &mut r.app,
                &r.cfg,
                &mut r.lines,
                &mut r.archive,
                &r.spine,
            )
        }

        fn settings(&mut self, code: KeyCode) -> bool {
            let r = self;
            handle_settings_input(
                code,
                &mut r.app,
                &mut r.cfg,
                &mut r.lines,
                &mut r.archive,
                &r.spine,
            )
        }

        fn at(&self) -> (usize, usize) {
            (self.app.chapter_index, self.app.offset)
        }

        fn last_page(&self) -> usize {
            self.lines.len() - self.app.lines_per_page
        }
    }

    #[test]
    fn j_and_k_scroll_a_line_and_cross_chapters() {
        let mut r = reader();
        r.read(KeyCode::Char('j'));
        assert_eq!(r.at(), (0, 1));
        r.read(KeyCode::Char('k'));
        assert_eq!(r.at(), (0, 0));
        r.read(KeyCode::Char('k'));
        assert_eq!(r.at(), (0, 0), "nothing comes before the first chapter");

        r.app.offset = r.last_page();
        r.read(KeyCode::Down);
        assert_eq!(
            r.at(),
            (1, 0),
            "past the last page is the next chapter's start"
        );
        r.read(KeyCode::Up);
        assert_eq!(
            r.at(),
            (0, r.last_page()),
            "and back is the previous chapter's end"
        );
        // With caps lock on
        r.read(KeyCode::Char('K'));
        assert_eq!(r.at(), (0, r.last_page() - 1));
        r.read(KeyCode::Char('J'));
        assert_eq!(r.at(), (0, r.last_page()));
    }

    #[test]
    fn l_and_h_move_by_the_scroll_setting() {
        let mut r = reader();
        r.cfg.scroll_by_lines = 5;
        r.read(KeyCode::Char('l'));
        r.read(KeyCode::Char(' '));
        assert_eq!(r.at(), (0, 10));
        r.read(KeyCode::Char('h'));
        r.read(KeyCode::Left);
        assert_eq!(r.at(), (0, 0));
        // A step never runs past the last page
        r.app.offset = r.last_page() - 2;
        r.read(KeyCode::Right);
        assert_eq!(r.at(), (0, r.last_page()));
    }

    #[test]
    fn q_quits_and_menus_open_and_close() {
        let mut r = reader();
        assert!(r.read(KeyCode::Char('q')));
        assert!(r.read(KeyCode::Char('Q')));
        assert!(!r.read(KeyCode::Char('j')));

        r.read(KeyCode::Tab);
        assert!(r.app.mode == AppMode::TocMenu);
        assert!(
            !r.toc(KeyCode::Char('q')),
            "q closes the menu instead of quitting"
        );
        assert!(r.app.mode == AppMode::Reading);

        r.read(KeyCode::Char('s'));
        assert!(r.app.mode == AppMode::SettingsMenu);
        r.settings(KeyCode::Esc);
        assert!(r.app.mode == AppMode::Reading);
    }

    #[test]
    fn the_contents_list_moves_within_bounds_and_jumps() {
        let mut r = reader();
        r.read(KeyCode::Char('j'));
        r.read(KeyCode::Tab);
        assert_eq!(r.app.toc_cursor, 0, "opens on the current chapter");
        r.toc(KeyCode::Char('k'));
        assert_eq!(r.app.toc_cursor, 0);
        r.toc(KeyCode::Char('j'));
        r.toc(KeyCode::Down);
        assert_eq!(r.app.toc_cursor, 1, "stops at the last chapter");
        r.toc(KeyCode::Enter);
        assert_eq!(r.at(), (1, 0));
        assert!(r.app.mode == AppMode::Reading);
    }

    #[test]
    fn the_settings_cursor_wraps_both_ways() {
        let mut r = reader();
        r.read(KeyCode::Char('s'));
        r.settings(KeyCode::Char('k'));
        assert_eq!(r.app.settings_cursor, ROWS.len() - 1);
        r.settings(KeyCode::Down);
        assert_eq!(r.app.settings_cursor, 0);
    }

    #[test]
    fn settings_changes_lay_the_page_out_again_and_are_saved() {
        paths::reset_test_dir();
        let mut r = reader();
        assert!(r.lines[0].starts_with("    p0w0"));
        r.read(KeyCode::Char('s'));
        r.settings(KeyCode::Char('j')); // Margin Left
        r.settings(KeyCode::Char('l'));
        assert_eq!(r.cfg.margin_left, 5);
        assert_eq!(r.app.dynamic_width, 60 - 5 - 4);
        assert!(r.lines[0].starts_with("     p0w0"), "{:?}", r.lines[0]);

        r.settings(KeyCode::Enter);
        let saved = std::fs::read_to_string(paths::config_file()).unwrap();
        assert!(saved.contains("\"margin_left\": 5"), "{saved}");
    }

    #[test]
    fn f_toggles_the_footer_and_the_page_height() {
        let mut r = reader();
        assert_eq!(r.app.lines_per_page, 18);
        r.read(KeyCode::Char('f'));
        assert!(!r.cfg.show_footer);
        assert_eq!(r.app.lines_per_page, 20);
        r.read(KeyCode::Char('F'));
        assert_eq!(r.app.lines_per_page, 18);
    }

    #[test]
    fn resizing_keeps_the_reading_position() {
        let mut r = reader();
        r.app.offset = r.lines.len() / 2;
        let before = r.app.offset as f64 / r.lines.len() as f64;
        let old_len = r.lines.len();
        let (app, cfg, lines, archive) = (&mut r.app, &r.cfg, &mut r.lines, &mut r.archive);
        handle_resize(100, 30, app, cfg, lines, archive, &r.spine);
        assert_eq!((r.app.dynamic_width, r.app.lines_per_page), (80, 28));
        assert!(r.lines.len() < old_len, "wider lines, fewer of them");
        let after = r.app.offset as f64 / r.lines.len() as f64;
        assert!((before - after).abs() < 0.02, "{before} vs {after}");
    }
}
