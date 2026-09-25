use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Attribute, Color, SetAttribute, SetBackgroundColor, SetForegroundColor},
};
use std::io::{self, Write};

use super::AppState;
use super::settings::{MAIN_ROWS, ROWS};
use crate::config::{Alignment, Config, ProgressMode, Theme};
use crate::i18n::{Text, tr};
use crate::images;
use crate::width;

pub struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub dim: Color,
}

pub fn get_palette(theme: &Theme) -> Palette {
    match theme {
        Theme::Default => Palette {
            bg: Color::Reset,
            fg: Color::Reset,
            accent: Color::Rgb {
                r: 90,
                g: 160,
                b: 250,
            },
            dim: Color::DarkGrey,
        },
        Theme::Sepia => Palette {
            bg: Color::Rgb {
                r: 244,
                g: 236,
                b: 216,
            },
            fg: Color::Rgb {
                r: 91,
                g: 70,
                b: 54,
            },
            accent: Color::Rgb {
                r: 217,
                g: 108,
                b: 6,
            },
            dim: Color::Rgb {
                r: 180,
                g: 160,
                b: 140,
            },
        },
        Theme::Dracula => Palette {
            bg: Color::Rgb {
                r: 40,
                g: 42,
                b: 54,
            },
            fg: Color::Rgb {
                r: 248,
                g: 248,
                b: 242,
            },
            accent: Color::Rgb {
                r: 189,
                g: 147,
                b: 249,
            },
            dim: Color::Rgb {
                r: 98,
                g: 114,
                b: 164,
            },
        },
        Theme::Hacker => Palette {
            bg: Color::Rgb { r: 0, g: 0, b: 0 },
            fg: Color::Rgb { r: 0, g: 255, b: 0 },
            accent: Color::Rgb { r: 0, g: 180, b: 0 },
            dim: Color::Rgb { r: 0, g: 100, b: 0 },
        },
        Theme::Nord => Palette {
            bg: Color::Rgb {
                r: 46,
                g: 52,
                b: 64,
            },
            fg: Color::Rgb {
                r: 236,
                g: 239,
                b: 244,
            },
            accent: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            dim: Color::Rgb {
                r: 76,
                g: 86,
                b: 106,
            },
        },
        Theme::SolarizedLight => Palette {
            bg: Color::Rgb {
                r: 253,
                g: 246,
                b: 227,
            },
            fg: Color::Rgb {
                r: 101,
                g: 123,
                b: 131,
            },
            accent: Color::Rgb {
                r: 38,
                g: 139,
                b: 210,
            },
            dim: Color::Rgb {
                r: 147,
                g: 161,
                b: 161,
            },
        },
        Theme::SolarizedDark => Palette {
            bg: Color::Rgb { r: 0, g: 43, b: 54 },
            fg: Color::Rgb {
                r: 131,
                g: 148,
                b: 150,
            },
            accent: Color::Rgb {
                r: 42,
                g: 161,
                b: 152,
            },
            dim: Color::Rgb {
                r: 88,
                g: 110,
                b: 117,
            },
        },
        Theme::Gruvbox => Palette {
            bg: Color::Rgb {
                r: 40,
                g: 40,
                b: 40,
            },
            fg: Color::Rgb {
                r: 235,
                g: 219,
                b: 178,
            },
            accent: Color::Rgb {
                r: 254,
                g: 128,
                b: 25,
            },
            dim: Color::Rgb {
                r: 146,
                g: 131,
                b: 116,
            },
        },
        Theme::Monokai => Palette {
            bg: Color::Rgb {
                r: 39,
                g: 40,
                b: 34,
            },
            fg: Color::Rgb {
                r: 248,
                g: 248,
                b: 242,
            },
            accent: Color::Rgb {
                r: 249,
                g: 38,
                b: 114,
            },
            dim: Color::Rgb {
                r: 117,
                g: 113,
                b: 94,
            },
        },
        Theme::Catppuccin => Palette {
            bg: Color::Rgb {
                r: 30,
                g: 30,
                b: 46,
            },
            fg: Color::Rgb {
                r: 205,
                g: 214,
                b: 244,
            },
            accent: Color::Rgb {
                r: 203,
                g: 166,
                b: 247,
            },
            dim: Color::Rgb {
                r: 147,
                g: 153,
                b: 178,
            },
        },
        Theme::Oceanic => Palette {
            bg: Color::Rgb {
                r: 27,
                g: 43,
                b: 52,
            },
            fg: Color::Rgb {
                r: 216,
                g: 222,
                b: 233,
            },
            accent: Color::Rgb {
                r: 102,
                g: 153,
                b: 204,
            },
            dim: Color::Rgb {
                r: 101,
                g: 115,
                b: 126,
            },
        },
    }
}

pub fn draw_reading_view(
    stdout: &mut impl Write,
    app: &AppState,
    cfg: &Config,
    lines: &[String],
    spine: &[(String, String)],
    pal: &Palette,
) -> io::Result<()> {
    let end = std::cmp::min(app.offset + app.lines_per_page, lines.len());
    for (row_idx, i) in (app.offset..end).enumerate() {
        // Colors are set per line, since a half-block picture line ends on the terminal defaults
        queue!(
            stdout,
            MoveTo(0, row_idx as u16),
            SetForegroundColor(pal.fg),
            SetBackgroundColor(pal.bg)
        )?;
        // Rows of a picture the terminal draws stay empty here
        if !lines[i].starts_with(images::MARKER) {
            write!(stdout, "{}\r", lines[i])?;
        }
    }

    if cfg.show_footer {
        let mut footer_parts = Vec::new();
        if cfg.show_chapter_title {
            footer_parts.push(spine[app.chapter_index].1.clone());
        }

        if cfg.show_progress_bar || cfg.show_progress_percentage {
            let chap_prog = if lines.is_empty() {
                0.0
            } else {
                app.offset as f64 / lines.len() as f64
            };
            let prog_val = match cfg.progress_mode {
                ProgressMode::Chapter => chap_prog * 100.0,
                ProgressMode::Overall => {
                    let start = app.chapter_starts[app.chapter_index];
                    let end = app.chapter_starts[app.chapter_index + 1];
                    (start + chap_prog * (end - start)) * 100.0
                }
            }
            .clamp(0.0, 100.0);

            if cfg.show_progress_bar {
                let length = cfg.progress_bar_length as f64;
                let filled = ((prog_val / 100.0) * length).round() as usize;
                let filled = std::cmp::min(filled, cfg.progress_bar_length);
                let empty = cfg.progress_bar_length.saturating_sub(filled);
                footer_parts.push(format!("[{}{}]", "█".repeat(filled), "░".repeat(empty)));
            }
            if cfg.show_progress_percentage {
                footer_parts.push(format!("{:.0}%", prog_val));
            }
        }

        if cfg.show_chapter_location {
            footer_parts.push(format!("({}/{})", app.chapter_index + 1, spine.len()));
        }

        if !footer_parts.is_empty() {
            // The footer must fit on its row; a longer one wraps and scrolls the page up
            let max_len = (app.term_cols as usize).saturating_sub(cfg.margin_left);
            let compose = |parts: &[String]| format!("--- {} ---", parts.join(" "));
            let mut footer_text = compose(&footer_parts);
            if width::width(&footer_text) > max_len && cfg.show_chapter_title {
                // Shorten the chapter title first; the numbers are what the footer is for
                let overflow = width::width(&footer_text) - max_len;
                let room = width::width(&footer_parts[0]).saturating_sub(overflow);
                footer_parts[0] = width::truncate(&footer_parts[0], room);
                if footer_parts[0].is_empty() {
                    footer_parts.remove(0);
                }
                footer_text = compose(&footer_parts);
            }
            footer_text = width::truncate(&footer_text, max_len);
            let footer_len = width::width(&footer_text);

            let layout_width = std::cmp::min(
                cfg.max_width,
                (app.term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right),
            );
            let padding_spaces = match cfg.footer_align {
                Alignment::Left => cfg.margin_left,
                Alignment::Center => {
                    if layout_width > footer_len {
                        cfg.margin_left + ((layout_width - footer_len) / 2)
                    } else {
                        cfg.margin_left
                    }
                }
                Alignment::Right => {
                    if layout_width > footer_len {
                        cfg.margin_left + (layout_width - footer_len)
                    } else {
                        cfg.margin_left
                    }
                }
            };

            let footer_color = if cfg.dim_footer { pal.dim } else { pal.fg };

            queue!(
                stdout,
                MoveTo(0, app.term_rows - 1),
                SetForegroundColor(footer_color)
            )?;
            write!(
                stdout,
                "{padding}{text}\r",
                padding = " ".repeat(padding_spaces),
                text = footer_text
            )?;
            queue!(stdout, SetForegroundColor(pal.fg))?;
        }
    }
    Ok(())
}

pub fn draw_toc_menu(
    stdout: &mut impl Write,
    app: &mut AppState,
    cfg: &Config,
    spine: &[(String, String)],
    pal: &Palette,
) -> io::Result<()> {
    let box_width_usize = app.dynamic_width.saturating_sub(4).clamp(30, 70);
    let box_width = box_width_usize as u16;
    let box_height = app.term_rows.saturating_sub(4).clamp(10, 25);

    let text_center_x = cfg.margin_left + (app.dynamic_width / 2);
    let mut start_x = text_center_x.saturating_sub(box_width_usize / 2) as u16;
    if start_x + box_width > app.term_cols {
        start_x = app.term_cols.saturating_sub(box_width);
    }
    let start_y = app.term_rows.saturating_sub(box_height) / 2;

    // FIX: Replaced ANSI \x1b[0m with explicit SetAttribute to stop color leaks
    queue!(
        stdout,
        MoveTo(start_x, start_y),
        SetBackgroundColor(pal.bg),
        SetForegroundColor(pal.accent)
    )?;
    let title = width::truncate(
        &format!(" {} ", tr(Text::TableOfContents)),
        box_width_usize - 2,
    );
    let dashes = (box_width_usize - 2).saturating_sub(width::width(&title));
    write!(stdout, "╭")?;
    queue!(stdout, SetAttribute(Attribute::Bold))?;
    write!(stdout, "{}", title)?;
    queue!(
        stdout,
        SetAttribute(Attribute::Reset),
        SetBackgroundColor(pal.bg),
        SetForegroundColor(pal.accent)
    )?;
    write!(stdout, "{}╮", "─".repeat(dashes))?;

    let visible_items = box_height as usize - 2;
    if app.toc_cursor < app.toc_top {
        app.toc_top = app.toc_cursor;
    } else if app.toc_cursor >= app.toc_top + visible_items {
        app.toc_top = app.toc_cursor - visible_items + 1;
    }

    let max_title_len = box_width as usize - 6;
    for i in 0..visible_items {
        queue!(stdout, MoveTo(start_x, start_y + 1 + i as u16))?;
        let idx = app.toc_top + i;

        if idx < spine.len() {
            let chap_title = width::truncate(&spine[idx].1, max_title_len);
            let padded = width::pad_right(&chap_title, max_title_len);

            if idx == app.toc_cursor {
                queue!(
                    stdout,
                    SetBackgroundColor(pal.accent),
                    SetForegroundColor(pal.bg)
                )?;
                write!(stdout, "│ > {} │", padded)?;
                queue!(
                    stdout,
                    SetBackgroundColor(pal.bg),
                    SetForegroundColor(pal.accent)
                )?;
            } else {
                write!(stdout, "│   {} │", padded)?;
            }
        } else {
            write!(stdout, "│{}│", " ".repeat(box_width as usize - 2))?;
        }
    }
    queue!(stdout, MoveTo(start_x, start_y + box_height - 1))?;
    write!(stdout, "╰{}╯", "─".repeat(box_width as usize - 2))?;
    queue!(stdout, SetForegroundColor(pal.fg))?;
    Ok(())
}

pub fn draw_settings_menu(
    stdout: &mut impl Write,
    app: &AppState,
    cfg: &Config,
    pal: &Palette,
) -> io::Result<()> {
    let box_width: u16 = 36;
    // Title, blank and header rows, both groups, a header between them, blank and bottom
    let box_height = ROWS.len() as u16 + 7;

    let text_center_x = cfg.margin_left + (app.dynamic_width / 2);
    let mut start_x = text_center_x.saturating_sub((box_width / 2) as usize) as u16;
    if start_x + box_width > app.term_cols {
        start_x = app.term_cols.saturating_sub(box_width);
    }
    let start_y = app.term_rows.saturating_sub(box_height) / 2;

    // FIX: Replaced ANSI \x1b[0m with explicit SetAttribute to stop background/foreground wipes
    queue!(
        stdout,
        MoveTo(start_x, start_y),
        SetBackgroundColor(pal.bg),
        SetForegroundColor(pal.accent)
    )?;
    write!(stdout, "╭")?;
    queue!(stdout, SetAttribute(Attribute::Bold))?;
    let title = width::truncate(&format!(" {} ", tr(Text::Settings)), box_width as usize - 2);
    write!(stdout, "{}", title)?;
    queue!(
        stdout,
        SetAttribute(Attribute::Reset),
        SetBackgroundColor(pal.bg),
        SetForegroundColor(pal.accent)
    )?;
    write!(
        stdout,
        "{}╮",
        "─".repeat((box_width as usize - 2).saturating_sub(width::width(&title)))
    )?;

    let inner_pad = " ".repeat(box_width as usize - 2);
    blank_row(stdout, start_x, start_y + 1, &inner_pad)?;
    header_row(stdout, start_x, start_y + 2, Text::MainUi, pal)?;
    blank_row(stdout, start_x, start_y + 3 + MAIN_ROWS as u16, &inner_pad)?;
    header_row(
        stdout,
        start_x,
        start_y + 4 + MAIN_ROWS as u16,
        Text::Footer,
        pal,
    )?;

    for (i, row) in ROWS.iter().enumerate() {
        // The footer group sits below the main group and its own header
        let y = start_y + 3 + i as u16 + if i < MAIN_ROWS { 0 } else { 2 };
        let label = width::pad_right(&width::truncate(row.label(), 15), 15);
        let value = width::pad_left(&width::truncate(&row.value(cfg), 10), 7);
        queue!(stdout, MoveTo(start_x, y))?;
        write!(stdout, "│")?;
        if app.settings_cursor == i {
            queue!(
                stdout,
                SetBackgroundColor(pal.accent),
                SetForegroundColor(pal.bg)
            )?;
            let content = format!("{} < {} >", label, value);
            write!(stdout, "{}", width::center(&content, 34))?;
            queue!(
                stdout,
                SetBackgroundColor(pal.bg),
                SetForegroundColor(pal.accent)
            )?;
        } else {
            queue!(stdout, SetForegroundColor(pal.fg))?;
            let content = format!("{}   {}  ", label, value);
            write!(stdout, "{}", width::center(&content, 34))?;
            queue!(stdout, SetForegroundColor(pal.accent))?;
        }
        write!(stdout, "│")?;
    }

    blank_row(stdout, start_x, start_y + box_height - 2, &inner_pad)?;
    queue!(stdout, MoveTo(start_x, start_y + box_height - 1))?;
    write!(stdout, "╰{}╯", "─".repeat(box_width as usize - 2))?;

    queue!(stdout, SetForegroundColor(pal.fg))?;
    Ok(())
}

fn blank_row(stdout: &mut impl Write, x: u16, y: u16, inner_pad: &str) -> io::Result<()> {
    queue!(stdout, MoveTo(x, y))?;
    write!(stdout, "│{}│", inner_pad)
}

fn header_row(
    stdout: &mut impl Write,
    x: u16,
    y: u16,
    text: Text,
    pal: &Palette,
) -> io::Result<()> {
    queue!(stdout, MoveTo(x, y))?;
    write!(stdout, "│")?;
    queue!(stdout, SetForegroundColor(pal.dim))?;
    write!(
        stdout,
        "{}",
        width::center(&format!("--- {} ---", tr(text)), 34)
    )?;
    queue!(stdout, SetForegroundColor(pal.accent))?;
    write!(stdout, "│")
}
