use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use zip::ZipArchive;

// --- CONFIGURATION ENGINE ---

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Serialize, Deserialize)]
struct Config {
    max_width: usize,       // The maximum comfortable reading width (e.g., 80)
    margin_left: usize,     // Left padding
    footer_align: Alignment,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_width: 80,
            margin_left: 4,
            footer_align: Alignment::Center,
        }
    }
}

fn load_or_create_config() -> Config {
    let config_path = "reader_config.json";
    
    if let Ok(file_content) = std::fs::read_to_string(config_path) {
        if let Ok(config) = serde_json::from_str(&file_content) {
            return config;
        }
    }
    
    let default_config = Config::default();
    if let Ok(json) = serde_json::to_string_pretty(&default_config) {
        let _ = std::fs::write(config_path, json);
    }
    default_config
}

// --- UTILITIES & PARSING ---

fn format_html_for_terminal(input: &str) -> String {
    let mut in_tag = false;
    let mut current_tag = String::new();
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars();

    let mut ignore_mode = false;
    let mut expected_closing_tag = String::new();

    while let Some(c) = chars.next() {
        if c == '<' {
            in_tag = true;
            current_tag.clear();
            continue;
        }
        if c == '>' {
            in_tag = false;
            let tag_lower = current_tag.to_lowercase();
            let base_tag = tag_lower.split_whitespace().next().unwrap_or("");

            if ignore_mode {
                if base_tag == expected_closing_tag {
                    ignore_mode = false;
                }
                continue; 
            }

            match base_tag {
                "head" | "style" | "script" => {
                    ignore_mode = true;
                    expected_closing_tag = format!("/{}", base_tag);
                    continue;
                }
                _ => {}
            }

            match base_tag {
                "h1" | "h2" | "h3" | "b" | "strong" => output.push_str("\x1b[1m"),
                "/h1" | "/h2" | "/h3" => output.push_str("\x1b[0m\n\n"),
                "/b" | "/strong" => output.push_str("\x1b[22m"),
                "i" | "em" => output.push_str("\x1b[3m"),
                "/i" | "/em" => output.push_str("\x1b[23m"),
                "p" | "div" | "/p" | "/div" | "br" | "br/" => output.push('\n'),
                _ => {}
            }
            continue;
        }

        if in_tag {
            current_tag.push(c);
        } else if !ignore_mode { 
            output.push(c); 
        }
    }

    output.replace("&nbsp;", " ").replace("&rsquo;", "'").replace("&lsquo;", "'")
          .replace("&rdquo;", "\"").replace("&ldquo;", "\"").replace("&mdash;", "—")
          .replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
}

fn read_zip_file(archive: &mut ZipArchive<File>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut content = String::new();
    file.read_to_string(&mut content).ok()?;
    Some(content)
}

fn get_epub_spine(archive: &mut ZipArchive<File>) -> Option<Vec<(String, String)>> {
    let container_xml = read_zip_file(archive, "META-INF/container.xml")?;
    let doc = Document::parse(&container_xml).ok()?;
    let rootfile = doc.descendants().find(|n| n.tag_name().name() == "rootfile")?;
    let opf_path = rootfile.attribute("full-path")?;

    let opf_xml = read_zip_file(archive, opf_path)?;
    let opf_doc = Document::parse(&opf_xml).ok()?;

    let mut manifest = HashMap::new();
    let spine_node = opf_doc.descendants().find(|n| n.tag_name().name() == "spine")?;
    let toc_id = spine_node.attribute("toc");
    let mut ncx_href = None;

    for node in opf_doc.descendants().filter(|n| n.tag_name().name() == "item") {
        if let (Some(id), Some(href)) = (node.attribute("id"), node.attribute("href")) {
            manifest.insert(id, href);
            if Some(id) == toc_id {
                ncx_href = Some(href);
            }
        }
    }

    let mut titles_map = HashMap::new();
    if let Some(ncx_rel_path) = ncx_href {
        let ncx_full_path = if opf_path.contains('/') {
            let parts: Vec<&str> = opf_path.rsplitn(2, '/').collect();
            format!("{}/{}", parts[1], ncx_rel_path)
        } else {
            ncx_rel_path.to_string()
        };

        if let Some(ncx_xml) = read_zip_file(archive, &ncx_full_path) {
            if let Ok(ncx_doc) = Document::parse(&ncx_xml) {
                for nav_point in ncx_doc.descendants().filter(|n| n.tag_name().name() == "navPoint") {
                    let text_node = nav_point.descendants().find(|n| n.tag_name().name() == "text");
                    let content_node = nav_point.descendants().find(|n| n.tag_name().name() == "content");
                    
                    if let (Some(t), Some(c)) = (text_node, content_node) {
                        if let (Some(text), Some(src)) = (t.text(), c.attribute("src")) {
                            let clean_src = src.split('#').next().unwrap_or(src);
                            titles_map.insert(clean_src.to_string(), text.trim().to_string());
                        }
                    }
                }
            }
        }
    }

    let mut spine = Vec::new();
    for node in spine_node.descendants().filter(|n| n.tag_name().name() == "itemref") {
        if let Some(idref) = node.attribute("idref") {
            if let Some(href) = manifest.get(idref) {
                let full_path = if opf_path.contains('/') {
                    let parts: Vec<&str> = opf_path.rsplitn(2, '/').collect();
                    format!("{}/{}", parts[1], href)
                } else {
                    href.to_string()
                };
                let title = titles_map.get(*href).cloned().unwrap_or_else(|| "Section".to_string());
                spine.push((full_path, title));
            }
        }
    }
    Some(spine)
}

fn load_chapter(archive: &mut ZipArchive<File>, path: &str, wrap_width: usize, margin_left: usize) -> Vec<String> {
    let raw_html = read_zip_file(archive, path).unwrap_or_default();
    let clean = format_html_for_terminal(&raw_html);
    
    let mut wrapped_lines = Vec::new();
    let indent = " ".repeat(margin_left);

    for line in clean.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        
        let wrapped = textwrap::wrap(trimmed, wrap_width);
        for w in wrapped {
            wrapped_lines.push(format!("{}{}", indent, w));
        }
    }
    wrapped_lines
}

// --- MAIN LOOP ---

fn main() -> io::Result<()> {
    let cfg = load_or_create_config();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: cargo run -- <path_to_epub>");
        return Ok(());
    }

    let file = File::open(&args[1])?;
    let mut archive = ZipArchive::new(file)?;
    
    let spine = get_epub_spine(&mut archive).expect("Failed to parse EPUB spine.");
    if spine.is_empty() {
        println!("No chapters found in EPUB.");
        return Ok(());
    }

    // 1. Get initial terminal dimensions
    let (mut term_cols, mut term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    
    // 2. Calculate dynamic wrap width (capped by config.max_width)
    let mut dynamic_width = std::cmp::max(10, std::cmp::min(
        cfg.max_width, 
        (term_cols as usize).saturating_sub(cfg.margin_left + 2)
    ));
    
    // 3. Calculate dynamic page height (leaving 2 lines for spacing and footer)
    let mut lines_per_page = (term_rows as usize).saturating_sub(2);

    let mut chapter_index = 0;
    let mut lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
    let mut offset = 0;

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, Hide, Clear(ClearType::All))?;

    loop {
        execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;
        
        let end = std::cmp::min(offset + lines_per_page, lines.len());
        for i in offset..end {
            println!("{}\r", lines[i]);
        }
        
        let footer_text = format!("--- {} ({}/{}) ---", 
            spine[chapter_index].1, 
            chapter_index + 1, 
            spine.len()
        );

        let layout_width = std::cmp::min(cfg.max_width, term_cols as usize);
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

        // Move to the bottom of the terminal dynamically
        execute!(stdout, MoveTo(0, term_rows - 1))?;
        print!("{padding}{text}\r", 
            padding = " ".repeat(padding_spaces), 
            text = footer_text
        );
        io::stdout().flush()?;

        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    match key_event.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('j') | KeyCode::Down | KeyCode::Char(' ') => {
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
                        _ => {}
                    }
                }
            } else if let Event::Resize(new_cols, new_rows) = event::read()? {
                // Instantly react to the user resizing the terminal window
                term_cols = new_cols;
                term_rows = new_rows;
                
                dynamic_width = std::cmp::max(10, std::cmp::min(
                    cfg.max_width, 
                    (term_cols as usize).saturating_sub(cfg.margin_left + 2)
                ));
                lines_per_page = (term_rows as usize).saturating_sub(2);
                
                // Reload and re-wrap the chapter to fit the new width
                lines = load_chapter(&mut archive, &spine[chapter_index].0, dynamic_width, cfg.margin_left);
                offset = 0; 
            }
        }
    }

    execute!(stdout, Show)?;
    disable_raw_mode()?;
    Ok(())
}