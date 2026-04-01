use crossterm::{cursor::MoveTo, execute};
use std::io::{self, Write};

use crate::config::{Alignment, Config, ProgressMode};
use super::AppState;

pub fn draw_reading_view(stdout: &mut io::Stdout, app: &AppState, cfg: &Config, lines: &[String], spine: &[(String, String)]) -> io::Result<()> {
    let end = std::cmp::min(app.offset + app.lines_per_page, lines.len());
    for (row_idx, i) in (app.offset..end).enumerate() {
        execute!(stdout, MoveTo(0, row_idx as u16))?;
        print!("{}\r", lines[i]);
    }

    if cfg.show_footer {
        let mut footer_parts = Vec::new();

        if cfg.show_chapter_title {
            footer_parts.push(spine[app.chapter_index].1.clone());
        }

        if cfg.show_progress_bar || cfg.show_progress_percentage {
            let chap_prog = if lines.is_empty() { 0.0 } else { app.offset as f64 / lines.len() as f64 };
            let prog_val = match cfg.progress_mode {
                ProgressMode::Chapter => chap_prog * 100.0,
                ProgressMode::Overall => ((app.chapter_index as f64 + chap_prog) / spine.len() as f64) * 100.0,
            }.clamp(0.0, 100.0);

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
            let footer_text = format!("--- {} ---", footer_parts.join(" "));

            let layout_width = std::cmp::min(cfg.max_width, (app.term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right));
            let padding_spaces = match cfg.footer_align {
                Alignment::Left => cfg.margin_left,
                Alignment::Center => if layout_width > footer_text.len() { cfg.margin_left + ((layout_width - footer_text.len()) / 2) } else { cfg.margin_left },
                Alignment::Right => if layout_width > footer_text.len() { cfg.margin_left + (layout_width - footer_text.len()) } else { cfg.margin_left },
            };
            execute!(stdout, MoveTo(0, app.term_rows - 1))?;
            print!("{padding}{text}\r", padding = " ".repeat(padding_spaces), text = footer_text);
        }
    }
    Ok(())
}

pub fn draw_toc_menu(stdout: &mut io::Stdout, app: &mut AppState, cfg: &Config, spine: &[(String, String)]) -> io::Result<()> {
    let box_width_usize = std::cmp::max(30, std::cmp::min(70, app.dynamic_width.saturating_sub(4)));
    let box_width = box_width_usize as u16;
    let box_height = std::cmp::max(10, std::cmp::min(25, app.term_rows.saturating_sub(4)));

    let text_center_x = cfg.margin_left + (app.dynamic_width / 2);
    let mut start_x = text_center_x.saturating_sub(box_width_usize / 2) as u16;
    if start_x + box_width > app.term_cols { start_x = app.term_cols.saturating_sub(box_width); }
    
    let start_y = app.term_rows.saturating_sub(box_height) / 2;

    execute!(stdout, MoveTo(start_x, start_y))?;
    let title = " Table of Contents ";
    let dashes = box_width as usize - 2 - title.len();
    print!("╭\x1b[1m{}\x1b[0m{}╮", title, "─".repeat(dashes));

    let visible_items = box_height as usize - 2;
    if app.toc_cursor < app.toc_top { app.toc_top = app.toc_cursor; } 
    else if app.toc_cursor >= app.toc_top + visible_items { app.toc_top = app.toc_cursor - visible_items + 1; }

    let max_title_len = box_width as usize - 6;
    for i in 0..visible_items {
        execute!(stdout, MoveTo(start_x, start_y + 1 + i as u16))?;
        let idx = app.toc_top + i;
        if idx < spine.len() {
            let mut chap_title = spine[idx].1.clone();
            if chap_title.chars().count() > max_title_len { chap_title = chap_title.chars().take(max_title_len - 3).collect::<String>() + "..."; }
            let padded_title = format!("{:<width$}", chap_title, width = max_title_len);

            if idx == app.toc_cursor { print!("│ \x1b[7m> {}\x1b[0m │", padded_title); } 
            else { print!("│   {} │", padded_title); }
        } else {
            print!("│{}│", " ".repeat(box_width as usize - 2));
        }
    }
    execute!(stdout, MoveTo(start_x, start_y + box_height - 1))?;
    print!("╰{}╯", "─".repeat(box_width as usize - 2));
    Ok(())
}

pub fn draw_settings_menu(stdout: &mut io::Stdout, app: &AppState, cfg: &Config) -> io::Result<()> {
    let box_width: u16 = 36;
    let box_height: u16 = 19; // Expanded to fit 12 settings + 2 dividers + spacing

    let text_center_x = cfg.margin_left + (app.dynamic_width / 2);
    let mut start_x = text_center_x.saturating_sub((box_width / 2) as usize) as u16;
    if start_x + box_width > app.term_cols { start_x = app.term_cols.saturating_sub(box_width); }
    
    let start_y = app.term_rows.saturating_sub(box_height) / 2;

    execute!(stdout, MoveTo(start_x, start_y))?;
    print!("╭\x1b[1m Settings \x1b[0m{}╮", "─".repeat(box_width as usize - 12));

    // Reordered to match visual layout
    let labels = [
        "Max Width", "Margin Left", "Margin Right", "Scroll Lines", 
        "Show Footer", "Footer Align", "Chapter Title", "Progress Mode", 
        "Progress Bar", "Bar Length", "Progress %", "Chapter Loc"
    ];

    let align_str = match cfg.footer_align { Alignment::Left => "Left", Alignment::Center => "Center", Alignment::Right => "Right" };
    let prog_mode_str = match cfg.progress_mode { ProgressMode::Chapter => "Chapter", ProgressMode::Overall => "Overall" };
    
    let values = [
        cfg.max_width.to_string(),
        cfg.margin_left.to_string(),
        cfg.margin_right.to_string(),
        cfg.scroll_by_lines.to_string(),
        if cfg.show_footer { "On".to_string() } else { "Off".to_string() },
        align_str.to_string(),
        if cfg.show_chapter_title { "On".to_string() } else { "Off".to_string() },
        prog_mode_str.to_string(),
        if cfg.show_progress_bar { "On".to_string() } else { "Off".to_string() },
        cfg.progress_bar_length.to_string(),
        if cfg.show_progress_percentage { "On".to_string() } else { "Off".to_string() },
        if cfg.show_chapter_location { "On".to_string() } else { "Off".to_string() },
    ];

    let inner_pad = " ".repeat(box_width as usize - 2);

    // TOP SPACING & MAIN UI SECTION
    execute!(stdout, MoveTo(start_x, start_y + 1))?;
    print!("│{}│", inner_pad);
    execute!(stdout, MoveTo(start_x, start_y + 2))?;
    print!("│\x1b[2m{:^34}\x1b[0m│", "--- Main UI ---");

    for i in 0..4 {
        execute!(stdout, MoveTo(start_x, start_y + 3 + i as u16))?;
        if app.settings_cursor == i {
            let content = format!("{:<15} < {:>7} >", labels[i], values[i]);
            print!("│\x1b[7m{:^34}\x1b[0m│", content);
        } else {
            let content = format!("{:<15}   {:>7}  ", labels[i], values[i]);
            print!("│{:^34}│", content);
        }
    }

    // DIVIDER & FOOTER SECTION
    execute!(stdout, MoveTo(start_x, start_y + 7))?;
    print!("│{}│", inner_pad);
    execute!(stdout, MoveTo(start_x, start_y + 8))?;
    print!("│\x1b[2m{:^34}\x1b[0m│", "--- Footer ---");

    for i in 4..12 {
        execute!(stdout, MoveTo(start_x, start_y + 5 + i as u16))?; // Shifted down past the headers
        if app.settings_cursor == i {
            let content = format!("{:<15} < {:>7} >", labels[i], values[i]);
            print!("│\x1b[7m{:^34}\x1b[0m│", content);
        } else {
            let content = format!("{:<15}   {:>7}  ", labels[i], values[i]);
            print!("│{:^34}│", content);
        }
    }

    // BOTTOM SPACING
    execute!(stdout, MoveTo(start_x, start_y + 17))?;
    print!("│{}│", inner_pad);
    execute!(stdout, MoveTo(start_x, start_y + 18))?;
    print!("╰{}╯", "─".repeat(box_width as usize - 2));
    Ok(())
}