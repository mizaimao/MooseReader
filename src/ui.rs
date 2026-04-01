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

    // --- STATE LOADING ---
    // Check if we have a saved bookmark for this specific book
    let (mut chapter_index, mut offset) = if let Some(bookmark) = state.books.get(&book_path) {
        // Ensure we don't accidentally load a chapter that doesn't exist anymore
        let safe_chapter = std::cmp::min(bookmark.chapter, spine.len().saturating_sub(1));
        (safe_chapter, bookmark.offset)
    } else {
        (0, 0) // First time opening this book
    };

    let mut lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
    
    // Safety check: if the terminal was resized while the app was closed, 
    // the saved offset might be past the end of the newly wrapped text.
    if offset >= lines.len() {
        offset = lines.len().saturating_sub(lines_per_page);
    }
    // ---------------------

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, Hide, Clear(ClearType::All))?;

    loop {
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
                    if layout_width > footer_text.len() {
                        cfg.margin_left + ((layout_width - footer_text.len()) / 2)
                    } else {
                        cfg.margin_left
                    }
                },
                Alignment::Right => {
                    if layout_width > footer_text.len() {
                        cfg.margin_left + (layout_width - footer_text.len())
                    } else {
                        cfg.margin_left
                    }
                }
            };

            execute!(stdout, MoveTo(0, term_rows - 1))?;
            print!("{padding}{text}\r", 
                padding = " ".repeat(padding_spaces), 
                text = footer_text
            );
        }
        
        stdout.flush()?;

        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    match key_event.code {
                        
                        KeyCode::Char('q') => {
                            // --- STATE SAVING ---
                            // Update the dictionary with our current location
                            state.books.insert(book_path.clone(), Bookmark {
                                chapter: chapter_index,
                                offset,
                            });
                            // Write the dictionary to the disk
                            save_state(&state);
                            // --------------------
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
                                offset = if lines.len() > lines_per_page {
                                    lines.len() - lines_per_page
                                } else {
                                    0
                                };
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
                                offset = if lines.len() > lines_per_page {
                                    lines.len() - lines_per_page
                                } else {
                                    0
                                };
                            }
                        }
                        
                        _ => {}
                    }
                }
            } else if let Event::Resize(new_cols, new_rows) = event::read()? {
                term_cols = new_cols;
                term_rows = new_rows;
                
                dynamic_width = std::cmp::max(10, std::cmp::min(
                    cfg.max_width, 
                    (term_cols as usize).saturating_sub(cfg.margin_left + cfg.margin_right)
                ));
                lines_per_page = (term_rows as usize).saturating_sub(footer_space);
                
                lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                
                // Keep offset in bounds if the terminal was resized to be much wider (meaning fewer lines total)
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