use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::fs::File;
use std::io::{self, Write};
use zip::ZipArchive;

use crate::config::{Alignment, Config};
use crate::epub::load_chapter;
use crate::state::{save_state, Bookmark, State};

#[derive(PartialEq)]
enum AppMode {
    Reading,
    TocMenu,
}

pub fn run(
    mut archive: ZipArchive<File>, 
    spine: Vec<(String, String)>, 
    mut cfg: Config, 
    mut state: State, 
    book_path: String
) -> io::Result<()> {
    
    let (mut term_cols, mut term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    
    let mut dynamic_width = std::cmp::max(10, std::cmp::min(
        cfg.max_width, 
        (term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right)
    ));
    
    let mut footer_space = if cfg.show_footer { 2 } else { 0 };
    let mut lines_per_page = (term_rows as usize).saturating_sub(footer_space);

    let (mut chapter_index, progress) = if let Some(bookmark) = state.books.get(&book_path) {
        let safe_chapter = std::cmp::min(bookmark.chapter, spine.len().saturating_sub(1));
        (safe_chapter, bookmark.progress)
    } else {
        (0, 0.0) 
    };

    let mut lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
    let mut offset = (progress * lines.len() as f64).floor() as usize;

    if offset >= lines.len() {
        offset = lines.len().saturating_sub(lines_per_page);
    }

    let mut mode = AppMode::Reading;
    let mut toc_cursor = chapter_index;
    let mut toc_top = 0;

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, Hide, Clear(ClearType::All))?;

    loop {
        // --- 1. DRAW THE BACKGROUND ---
        execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;
        
        let end = std::cmp::min(offset + lines_per_page, lines.len());
        for (row_idx, i) in (offset..end).enumerate() {
            execute!(stdout, MoveTo(0, row_idx as u16))?;
            print!("{}\r", lines[i]);
        }
        
        if cfg.show_footer {
            let footer_text = format!("--- {} ({}/{}) ---", 
                spine[chapter_index].1, 
                chapter_index + 1, 
                spine.len()
            );

            let layout_width = std::cmp::min(cfg.max_width, (term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right));
            let padding_spaces = match cfg.footer_align {
                Alignment::Left => cfg.margin_left,
                Alignment::Center => {
                    if layout_width > footer_text.len() { cfg.margin_left + ((layout_width - footer_text.len()) / 2) } else { cfg.margin_left }
                },
                Alignment::Right => {
                    if layout_width > footer_text.len() { cfg.margin_left + (layout_width - footer_text.len()) } else { cfg.margin_left }
                }
            };

            execute!(stdout, MoveTo(0, term_rows - 1))?;
            print!("{padding}{text}\r", padding = " ".repeat(padding_spaces), text = footer_text);
        }

        // --- 2. DRAW OVERLAYS ---
        if mode == AppMode::TocMenu {
            let box_width_usize = std::cmp::max(30, std::cmp::min(70, dynamic_width.saturating_sub(4)));
            let box_width = box_width_usize as u16;
            let box_height = std::cmp::max(10, std::cmp::min(25, term_rows.saturating_sub(4)));
            
            let text_center_x = cfg.margin_left + (dynamic_width / 2);
            let mut start_x = text_center_x.saturating_sub(box_width_usize / 2) as u16;
            
            if start_x + box_width > term_cols {
                start_x = term_cols.saturating_sub(box_width);
            }
            
            let start_y = term_rows.saturating_sub(box_height) / 2;

            execute!(stdout, MoveTo(start_x, start_y))?;
            let title = " Table of Contents ";
            let dashes = box_width as usize - 2 - title.len();
            
            // FIX: Added \x1b[1m (Bold) and \x1b[0m (Reset) directly to the print macro
            print!("╭\x1b[1m{}\x1b[0m{}╮", title, "─".repeat(dashes));

            let visible_items = box_height as usize - 2;
            if toc_cursor < toc_top {
                toc_top = toc_cursor;
            } else if toc_cursor >= toc_top + visible_items {
                toc_top = toc_cursor - visible_items + 1;
            }

            let max_title_len = box_width as usize - 6;
            for i in 0..visible_items {
                execute!(stdout, MoveTo(start_x, start_y + 1 + i as u16))?;
                let idx = toc_top + i;

                if idx < spine.len() {
                    let mut chap_title = spine[idx].1.clone();
                    
                    if chap_title.chars().count() > max_title_len {
                        chap_title = chap_title.chars().take(max_title_len - 3).collect::<String>() + "...";
                    }

                    let padded_title = format!("{:<width$}", chap_title, width = max_title_len);

                    if idx == toc_cursor {
                        print!("│ \x1b[7m> {}\x1b[0m │", padded_title);
                    } else {
                        print!("│   {} │", padded_title);
                    }
                } else {
                    print!("│{}│", " ".repeat(box_width as usize - 2));
                }
            }

            execute!(stdout, MoveTo(start_x, start_y + box_height - 1))?;
            print!("╰{}╯", "─".repeat(box_width as usize - 2));
        }
        
        stdout.flush()?;

        // --- 3. EVENT ROUTER ---
        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    
                    match mode {
                        AppMode::TocMenu => {
                            match key_event.code {
                                KeyCode::Tab | KeyCode::Esc | KeyCode::Char('q') => {
                                    mode = AppMode::Reading;
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if toc_cursor + 1 < spine.len() {
                                        toc_cursor += 1;
                                    }
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    if toc_cursor > 0 {
                                        toc_cursor -= 1;
                                    }
                                }
                                KeyCode::Enter => {
                                    chapter_index = toc_cursor;
                                    lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                                    offset = 0; 
                                    mode = AppMode::Reading; 
                                }
                                _ => {}
                            }
                        }

                        AppMode::Reading => {
                            match key_event.code {
                                KeyCode::Tab => {
                                    mode = AppMode::TocMenu;
                                    toc_cursor = chapter_index; 
                                }

                                KeyCode::Char('q') => {
                                    let current_progress = if lines.is_empty() { 0.0 } else { offset as f64 / lines.len() as f64 };
                                    state.books.insert(book_path.clone(), Bookmark {
                                        chapter: chapter_index,
                                        progress: current_progress,
                                    });
                                    save_state(&state);
                                    break;
                                }
                                
                                KeyCode::Char('F') | KeyCode::Char('f') => {
                                    cfg.show_footer = !cfg.show_footer;
                                    footer_space = if cfg.show_footer { 2 } else { 0 };
                                    lines_per_page = (term_rows as usize).saturating_sub(footer_space);
                                }
                                
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if offset + lines_per_page < lines.len() {
                                        offset += 1;
                                    } else if chapter_index + 1 < spine.len() {
                                        chapter_index += 1;
                                        lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                                        offset = 0;
                                    }
                                }
                                
                                KeyCode::Char('k') | KeyCode::Up => {
                                    if offset > 0 {
                                        offset -= 1;
                                    } else if chapter_index > 0 {
                                        chapter_index -= 1;
                                        lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                                        offset = if lines.len() > lines_per_page { lines.len() - lines_per_page } else { 0 };
                                    }
                                }

                                KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Right | KeyCode::Char(' ') => {
                                    if offset + lines_per_page < lines.len() {
                                        offset = std::cmp::min(offset + cfg.scroll_by_lines, lines.len().saturating_sub(lines_per_page));
                                    } else if chapter_index + 1 < spine.len() {
                                        chapter_index += 1;
                                        lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                                        offset = 0;
                                    }
                                }

                                KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Left => {
                                    if offset > 0 {
                                        offset = offset.saturating_sub(cfg.scroll_by_lines);
                                    } else if chapter_index > 0 {
                                        chapter_index -= 1;
                                        lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                                        offset = if lines.len() > lines_per_page { lines.len() - lines_per_page } else { 0 };
                                    }
                                }
                                
                                _ => {}
                            }
                        }
                    }
                }
            } else if let Event::Resize(new_cols, new_rows) = event::read()? {
                let current_progress = if lines.is_empty() { 0.0 } else { offset as f64 / lines.len() as f64 };
                term_cols = new_cols;
                term_rows = new_rows;
                
                dynamic_width = std::cmp::max(10, std::cmp::min(
                    cfg.max_width, 
                    (term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right)
                ));
                lines_per_page = (term_rows as usize).saturating_sub(footer_space);
                
                lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                offset = (current_progress * lines.len() as f64).floor() as usize;
                
                if offset >= lines.len() {
                    offset = lines.len().saturating_sub(lines_per_page);
                }
            }
        }
    }

    execute!(stdout, Show)?;
    disable_raw_mode()?;
    Ok(())
}