use crossterm::event::KeyCode;
use std::fs::File;
use zip::ZipArchive;

use super::{AppMode, AppState, SETTINGS_ITEMS, load_current};
use crate::config::{Alignment, Config, ProgressMode, Theme, save_config};
use crate::i18n;

pub fn handle_reading_input(
    code: KeyCode,
    app: &mut AppState,
    cfg: &mut Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<File>,
    spine: &[(String, String)],
) -> bool {
    match code {
        KeyCode::Char('q') => return true,
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
            app.lines_per_page =
                (app.term_rows as usize).saturating_sub(if cfg.show_footer { 2 } else { 0 });
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

pub fn handle_toc_input(
    code: KeyCode,
    app: &mut AppState,
    cfg: &Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<File>,
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

pub fn handle_settings_input(
    code: KeyCode,
    app: &mut AppState,
    cfg: &mut Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<File>,
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
            app.lines_per_page =
                (app.term_rows as usize).saturating_sub(if cfg.show_footer { 2 } else { 0 });
            app.mode = AppMode::Reading;
        }

        KeyCode::Char('j') | KeyCode::Down => {
            app.settings_cursor = (app.settings_cursor + 1) % SETTINGS_ITEMS
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.settings_cursor = if app.settings_cursor == 0 {
                SETTINGS_ITEMS - 1
            } else {
                app.settings_cursor - 1
            }
        }

        KeyCode::Char('h') | KeyCode::Left => {
            let mut text_changed = false;
            match app.settings_cursor {
                0 => {
                    if cfg.max_width > 20 {
                        cfg.max_width -= 1;
                        text_changed = true;
                    }
                }
                1 => {
                    if cfg.margin_left > 0 {
                        cfg.margin_left -= 1;
                        text_changed = true;
                    }
                }
                2 => {
                    if cfg.margin_right > 0 {
                        cfg.margin_right -= 1;
                        text_changed = true;
                    }
                }
                3 => {
                    if cfg.scroll_by_lines > 1 {
                        cfg.scroll_by_lines -= 1;
                    }
                }
                4 => {
                    cfg.theme = match cfg.theme {
                        Theme::Default => Theme::Oceanic,
                        Theme::Sepia => Theme::Default,
                        Theme::Dracula => Theme::Sepia,
                        Theme::Hacker => Theme::Dracula,
                        Theme::Nord => Theme::Hacker,
                        Theme::SolarizedLight => Theme::Nord,
                        Theme::SolarizedDark => Theme::SolarizedLight,
                        Theme::Gruvbox => Theme::SolarizedDark,
                        Theme::Monokai => Theme::Gruvbox,
                        Theme::Catppuccin => Theme::Monokai,
                        Theme::Oceanic => Theme::Catppuccin,
                    };
                    // Half-block pictures blend into the theme's background
                    text_changed = true;
                }
                5 => {
                    cfg.language = cfg.language.prev();
                    i18n::set(cfg.language);
                    // Reloads the chapter so its [Image] labels change language too
                    text_changed = true;
                }
                6 => {
                    cfg.images = cfg.images.prev();
                    text_changed = true;
                }
                7 => cfg.show_footer = !cfg.show_footer,
                8 => cfg.dim_footer = !cfg.dim_footer,
                9 => {
                    cfg.footer_align = match cfg.footer_align {
                        Alignment::Left => Alignment::Right,
                        Alignment::Center => Alignment::Left,
                        Alignment::Right => Alignment::Center,
                    }
                }
                10 => cfg.show_chapter_title = !cfg.show_chapter_title,
                11 => {
                    cfg.progress_mode = match cfg.progress_mode {
                        ProgressMode::Chapter => ProgressMode::Overall,
                        ProgressMode::Overall => ProgressMode::Chapter,
                    }
                }
                12 => cfg.show_progress_bar = !cfg.show_progress_bar,
                13 => {
                    if cfg.progress_bar_length > 5 {
                        cfg.progress_bar_length -= 1;
                    }
                }
                14 => cfg.show_progress_percentage = !cfg.show_progress_percentage,
                15 => cfg.show_chapter_location = !cfg.show_chapter_location,
                _ => {}
            }
            if text_changed {
                update_layout_live(app, cfg, lines, archive, spine);
            }
        }

        KeyCode::Char('l') | KeyCode::Right => {
            let mut text_changed = false;
            match app.settings_cursor {
                0 => {
                    if cfg.max_width < 200 {
                        cfg.max_width += 1;
                        text_changed = true;
                    }
                }
                1 => {
                    if cfg.margin_left < 40 {
                        cfg.margin_left += 1;
                        text_changed = true;
                    }
                }
                2 => {
                    if cfg.margin_right < 40 {
                        cfg.margin_right += 1;
                        text_changed = true;
                    }
                }
                3 => {
                    if cfg.scroll_by_lines < 50 {
                        cfg.scroll_by_lines += 1;
                    }
                }
                4 => {
                    cfg.theme = match cfg.theme {
                        Theme::Default => Theme::Sepia,
                        Theme::Sepia => Theme::Dracula,
                        Theme::Dracula => Theme::Hacker,
                        Theme::Hacker => Theme::Nord,
                        Theme::Nord => Theme::SolarizedLight,
                        Theme::SolarizedLight => Theme::SolarizedDark,
                        Theme::SolarizedDark => Theme::Gruvbox,
                        Theme::Gruvbox => Theme::Monokai,
                        Theme::Monokai => Theme::Catppuccin,
                        Theme::Catppuccin => Theme::Oceanic,
                        Theme::Oceanic => Theme::Default,
                    };
                    // Half-block pictures blend into the theme's background
                    text_changed = true;
                }
                5 => {
                    cfg.language = cfg.language.next();
                    i18n::set(cfg.language);
                    // Reloads the chapter so its [Image] labels change language too
                    text_changed = true;
                }
                6 => {
                    cfg.images = cfg.images.next();
                    text_changed = true;
                }
                7 => cfg.show_footer = !cfg.show_footer,
                8 => cfg.dim_footer = !cfg.dim_footer,
                9 => {
                    cfg.footer_align = match cfg.footer_align {
                        Alignment::Left => Alignment::Center,
                        Alignment::Center => Alignment::Right,
                        Alignment::Right => Alignment::Left,
                    }
                }
                10 => cfg.show_chapter_title = !cfg.show_chapter_title,
                11 => {
                    cfg.progress_mode = match cfg.progress_mode {
                        ProgressMode::Chapter => ProgressMode::Overall,
                        ProgressMode::Overall => ProgressMode::Chapter,
                    }
                }
                12 => cfg.show_progress_bar = !cfg.show_progress_bar,
                13 => {
                    if cfg.progress_bar_length < 100 {
                        cfg.progress_bar_length += 1;
                    }
                }
                14 => cfg.show_progress_percentage = !cfg.show_progress_percentage,
                15 => cfg.show_chapter_location = !cfg.show_chapter_location,
                _ => {}
            }
            if text_changed {
                update_layout_live(app, cfg, lines, archive, spine);
            }
        }
        _ => {}
    }
    false
}

pub fn update_layout_live(
    app: &mut AppState,
    cfg: &Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<File>,
    spine: &[(String, String)],
) {
    let current_progress = if lines.is_empty() {
        0.0
    } else {
        app.offset as f64 / lines.len() as f64
    };
    app.dynamic_width = std::cmp::max(
        10,
        std::cmp::min(
            cfg.max_width,
            (app.term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right),
        ),
    );
    *lines = load_current(app, cfg, archive, spine);
    app.offset = (current_progress * lines.len() as f64).floor() as usize;
    if app.offset >= lines.len() {
        app.offset = lines.len().saturating_sub(app.lines_per_page);
    }
}

pub fn handle_resize(
    new_cols: u16,
    new_rows: u16,
    app: &mut AppState,
    cfg: &Config,
    lines: &mut Vec<String>,
    archive: &mut ZipArchive<File>,
    spine: &[(String, String)],
) {
    let current_progress = if lines.is_empty() {
        0.0
    } else {
        app.offset as f64 / lines.len() as f64
    };
    app.term_cols = new_cols;
    app.term_rows = new_rows;
    app.dynamic_width = std::cmp::max(
        10,
        std::cmp::min(
            cfg.max_width,
            (app.term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right),
        ),
    );
    app.lines_per_page =
        (app.term_rows as usize).saturating_sub(if cfg.show_footer { 2 } else { 0 });

    *lines = load_current(app, cfg, archive, spine);
    app.offset = (current_progress * lines.len() as f64).floor() as usize;
    if app.offset >= lines.len() {
        app.offset = lines.len().saturating_sub(app.lines_per_page);
    }
}
