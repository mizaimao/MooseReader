use roxmltree::Document;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use zip::ZipArchive;

/// Tags that start a new line of their own, open or closing.
fn is_block(tag: &str) -> bool {
    matches!(
        tag.trim_start_matches('/'),
        "p" | "div"
            | "br"
            | "hr"
            | "blockquote"
            | "section"
            | "article"
            | "aside"
            | "header"
            | "footer"
            | "nav"
            | "figure"
            | "figcaption"
            | "table"
            | "tr"
            | "dl"
            | "dt"
            | "dd"
            | "address"
            | "center"
    )
}

fn format_html_for_terminal(input: &str) -> String {
    let mut in_tag = false;
    let mut current_tag = String::new();
    let mut output = String::with_capacity(input.len());

    let mut ignore_mode = false;
    let mut expected_closing_tag = String::new();

    // Open lists, innermost last: None for <ul>, Some(next number) for <ol>
    let mut lists: Vec<Option<usize>> = Vec::new();
    let mut in_pre = false;
    // Source line breaks are plain whitespace in HTML; only block tags start new lines
    let mut at_space = true;

    for c in input.chars() {
        if c == '<' {
            in_tag = true;
            current_tag.clear();
            continue;
        }
        if c == '>' {
            in_tag = false;
            let tag_lower = current_tag.to_lowercase();
            let self_closing = tag_lower.ends_with('/');
            let base_tag = tag_lower
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/');

            if ignore_mode {
                if base_tag == expected_closing_tag {
                    ignore_mode = false;
                }
                continue;
            }

            // An empty element such as <a id="c05"/> must not switch on a style that never closes
            if self_closing && !is_block(base_tag) && !matches!(base_tag, "img" | "image") {
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
                "h1" | "h2" | "h3" => {
                    output.push_str("\n\x1b[1m\x1e");
                    at_space = true;
                }

                // FIX: Changed from \x1b[0m to \x1b[22m to stop wiping the background color
                "/h1" | "/h2" | "/h3" => {
                    output.push_str("\x1b[22m\n\n");
                    at_space = true;
                }

                "h4" | "h5" | "h6" => {
                    output.push_str("\n\x1b[1m");
                    at_space = true;
                }
                "/h4" | "/h5" | "/h6" => {
                    output.push_str("\x1b[22m\n");
                    at_space = true;
                }

                "b" | "strong" => output.push_str("\x1b[1m"),
                "/b" | "/strong" => output.push_str("\x1b[22m"),
                "i" | "em" => output.push_str("\x1b[3m"),
                "/i" | "/em" => output.push_str("\x1b[23m"),

                "ul" | "ol" | "/ul" | "/ol" => {
                    match base_tag {
                        "ul" => lists.push(None),
                        "ol" => lists.push(Some(1)),
                        _ => {
                            lists.pop();
                        }
                    }
                    output.push('\n');
                    at_space = true;
                }
                "li" => {
                    output.push('\n');
                    match lists.last_mut() {
                        Some(Some(n)) => {
                            output.push_str(&format!("{}. ", n));
                            *n += 1;
                        }
                        _ => output.push_str("• "),
                    }
                    at_space = true;
                }
                "/li" => {
                    output.push('\n');
                    at_space = true;
                }

                "pre" | "/pre" => {
                    in_pre = base_tag == "pre";
                    output.push('\n');
                    at_space = true;
                }

                "img" | "image" => {
                    output.push_str("\n\x1b[2m[Image]\x1b[22m\n");
                    at_space = true;
                }

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

                tag if is_block(tag) => {
                    output.push('\n');
                    at_space = true;
                }

                _ => {}
            }
            continue;
        }

        if in_tag {
            current_tag.push(c);
        } else if ignore_mode {
            continue;
        } else if in_pre {
            output.push(c);
        } else if c.is_ascii_whitespace() {
            // Collapses runs of spaces and source line breaks; leaves &nbsp; (U+00A0) alone
            if !at_space {
                output.push(' ');
                at_space = true;
            }
        } else {
            output.push(c);
            at_space = false;
        }
    }

    decode_entities(&output)
}

/// Decodes character references in a single pass, so "&amp;lt;" stays "&lt;".
/// Unknown or malformed references are kept as written.
fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        // Entity names are short; a ';' further away means this '&' is plain text
        let end = rest.bytes().skip(1).take(32).position(|b| b == b';');
        match end {
            Some(end) if push_entity(&mut out, &rest[1..1 + end]) => rest = &rest[end + 2..],
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn push_entity(out: &mut String, name: &str) -> bool {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok(),
            None => num.parse().ok(),
        };
        return match code.and_then(char::from_u32) {
            Some(ch) => {
                out.push(ch);
                true
            }
            None => false,
        };
    }
    let text = match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        "shy" => "",
        "ensp" | "emsp" | "thinsp" => " ",
        "lsquo" => "‘",
        "rsquo" => "’",
        "sbquo" => "‚",
        "ldquo" => "“",
        "rdquo" => "”",
        "bdquo" => "„",
        "laquo" => "«",
        "raquo" => "»",
        "lsaquo" => "‹",
        "rsaquo" => "›",
        "ndash" => "–",
        "mdash" => "—",
        "hellip" => "…",
        "bull" => "•",
        "middot" => "·",
        "prime" => "′",
        "Prime" => "″",
        "deg" => "°",
        "times" => "×",
        "divide" => "÷",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "sect" => "§",
        "para" => "¶",
        "dagger" => "†",
        "Dagger" => "‡",
        "iexcl" => "¡",
        "iquest" => "¿",
        _ => return false,
    };
    out.push_str(text);
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Visible text lines of formatted HTML, without styling or blank lines.
    fn text_lines(html: &str) -> Vec<String> {
        strip_ansi(&format_html_for_terminal(html).replace('\x1e', ""))
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }

    #[test]
    fn self_closing_anchor_does_not_underline() {
        let out = format_html_for_terminal(r#"<p><a id="c05"/></p><div>Text</div>"#);
        assert!(!out.contains("\x1b[4m"), "{:?}", out);
    }

    #[test]
    fn source_line_breaks_are_spaces() {
        assert_eq!(text_lines("<p>Hello\n    world</p>"), ["Hello world"]);
    }

    #[test]
    fn list_items_get_their_own_lines() {
        let html = "<ul>\n  <li>\n    One\n  </li>\n  <li>Two</li>\n</ul>";
        assert_eq!(text_lines(html), ["• One", "• Two"]);
        let html = "<ol><li>First</li><li>Second</li></ol>";
        assert_eq!(text_lines(html), ["1. First", "2. Second"]);
    }

    #[test]
    fn minor_headings_are_bold_lines() {
        let out = format_html_for_terminal("text<h4>Notes</h4>more");
        assert!(out.contains("\n\x1b[1mNotes\x1b[22m\n"), "{:?}", out);
    }

    #[test]
    fn pre_keeps_line_breaks() {
        assert_eq!(text_lines("<pre>a\nb</pre>"), ["a", "b"]);
    }

    #[test]
    fn entities_decode_once() {
        assert_eq!(decode_entities("don&#8217;t &#x2014; &rsquo;"), "don’t — ’");
        assert_eq!(decode_entities("&amp;lt;"), "&lt;");
        assert_eq!(
            decode_entities("AT&T &unknown; &#xZZ; &"),
            "AT&T &unknown; &#xZZ; &"
        );
        assert_eq!(decode_entities("a&nbsp;b"), "a\u{a0}b");
    }
}
