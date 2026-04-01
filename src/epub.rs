use roxmltree::Document;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use zip::ZipArchive;

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
                "h1" | "h2" | "h3" => output.push_str("\x1b[1m\x1e"),
                "/h1" | "/h2" | "/h3" => output.push_str("\x1b[0m\n\n"),

                "b" | "strong" => output.push_str("\x1b[1m"),
                "/b" | "/strong" => output.push_str("\x1b[22m"),
                "i" | "em" => output.push_str("\x1b[3m"),
                "/i" | "/em" => output.push_str("\x1b[23m"),

                // Emitting newlines for block elements
                "p" | "div" | "/p" | "/div" | "br" | "br/" => output.push('\n'),

                "img" | "image" => output.push_str("\n\x1b[2m[Image]\x1b[22m\n"),

                "a" => {
                    let mut href = "";
                    if let Some(start) = current_tag.find("href=\"") {
                        let rest = &current_tag[start + 6..];
                        if let Some(end) = rest.find('"') {
                            href = &rest[..end];
                        }
                    } else if let Some(start) = current_tag.find("href='") {
                        let rest = &current_tag[start + 6..];
                        if let Some(end) = rest.find('\'') {
                            href = &rest[..end];
                        }
                    }
                    if !href.is_empty() {
                        output.push_str(&format!("\x1b]8;;{}\x1b\\\x1b[4m", href));
                    } else {
                        output.push_str("\x1b[4m");
                    }
                }
                "/a" => output.push_str("\x1b[24m\x1b]8;;\x1b\\"),

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

    output
        .replace("&nbsp;", " ")
        .replace("&rsquo;", "'")
        .replace("&lsquo;", "'")
        .replace("&rdquo;", "\"")
        .replace("&ldquo;", "\"")
        .replace("&mdash;", "—")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn strip_ansi(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c.is_ascii_alphabetic() {
                in_esc = false;
            }
        } else if c == '\x1b' {
            in_esc = true;
        } else {
            res.push(c);
        }
    }
    res
}

fn read_zip_file(archive: &mut ZipArchive<File>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut content = String::new();
    file.read_to_string(&mut content).ok()?;
    Some(content)
}

pub fn get_epub_spine(archive: &mut ZipArchive<File>) -> Option<Vec<(String, String)>> {
    let container_xml = read_zip_file(archive, "META-INF/container.xml")?;
    let doc = Document::parse(&container_xml).ok()?;
    let rootfile = doc
        .descendants()
        .find(|n| n.tag_name().name() == "rootfile")?;
    let opf_path = rootfile.attribute("full-path")?;

    let opf_xml = read_zip_file(archive, opf_path)?;
    let opf_doc = Document::parse(&opf_xml).ok()?;

    let mut manifest = HashMap::new();
    let spine_node = opf_doc
        .descendants()
        .find(|n| n.tag_name().name() == "spine")?;
    let toc_id = spine_node.attribute("toc");
    let mut ncx_href = None;

    for node in opf_doc
        .descendants()
        .filter(|n| n.tag_name().name() == "item")
    {
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
                for nav_point in ncx_doc
                    .descendants()
                    .filter(|n| n.tag_name().name() == "navPoint")
                {
                    let text_node = nav_point
                        .descendants()
                        .find(|n| n.tag_name().name() == "text");
                    let content_node = nav_point
                        .descendants()
                        .find(|n| n.tag_name().name() == "content");

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
    for node in spine_node
        .descendants()
        .filter(|n| n.tag_name().name() == "itemref")
    {
        if let Some(idref) = node.attribute("idref") {
            if let Some(href) = manifest.get(idref) {
                let full_path = if opf_path.contains('/') {
                    let parts: Vec<&str> = opf_path.rsplitn(2, '/').collect();
                    format!("{}/{}", parts[1], href)
                } else {
                    href.to_string()
                };
                let title = titles_map
                    .get(*href)
                    .cloned()
                    .unwrap_or_else(|| "Section".to_string());
                spine.push((full_path, title));
            }
        }
    }
    Some(spine)
}

pub fn load_chapter(
    archive: &mut ZipArchive<File>,
    path: &str,
    wrap_width: usize,
    margin_left: usize,
) -> Vec<String> {
    let raw_html = read_zip_file(archive, path).unwrap_or_default();
    let clean = format_html_for_terminal(&raw_html);

    let mut wrapped_lines = Vec::new();
    let indent = " ".repeat(margin_left);

    // Track empty lines to prevent spamming the terminal with gaps
    let mut last_was_empty = true;

    for line in clean.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // Only push a single empty line, and only if we haven't just pushed one
            if !last_was_empty {
                wrapped_lines.push(String::new());
                last_was_empty = true;
            }
            continue;
        }

        last_was_empty = false;

        if trimmed.contains('\x1e') {
            let clean_line = trimmed.replace('\x1e', "");
            let visible_len = strip_ansi(&clean_line).chars().count();

            let pad = if wrap_width > visible_len {
                (wrap_width - visible_len) / 2
            } else {
                0
            };
            wrapped_lines.push(format!("{}{}{}", indent, " ".repeat(pad), clean_line));
        } else {
            let wrapped = textwrap::wrap(trimmed, wrap_width);
            for w in wrapped {
                wrapped_lines.push(format!("{}{}", indent, w));
            }
        }
    }

    // Clean up trailing empty lines at the very bottom of the chapter
    while wrapped_lines.last().map_or(false, |l| l.is_empty()) {
        wrapped_lines.pop();
    }

    wrapped_lines
}
