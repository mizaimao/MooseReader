use roxmltree::{Document, ParsingOptions};
use std::collections::HashMap;
use std::io::{Read, Seek};
use zip::ZipArchive;

const EPUB_OPS_NS: &str = "http://www.idpf.org/2007/ops";
const NCX_MEDIA_TYPE: &str = "application/x-dtbncx+xml";

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

/// The escape sequence at the start of `s`, if any: CSI ("\x1b[1m")
/// or OSC ("\x1b]8;;uri\x1b\\", as used for hyperlinks).
fn leading_escape(s: &str) -> Option<&str> {
    let rest = s.strip_prefix('\x1b')?;
    if let Some(csi) = rest.strip_prefix('[') {
        let end = csi.find(|c: char| ('\x40'..='\x7e').contains(&c))?;
        Some(&s[..end + 3])
    } else if rest.starts_with(']') {
        let end = rest.find("\x1b\\")?;
        Some(&s[..end + 3])
    } else {
        None
    }
}

/// Calls `f` with each escape sequence in `s`, in order.
fn for_each_escape(s: &str, mut f: impl FnMut(&str)) {
    let mut rest = s;
    while let Some(pos) = rest.find('\x1b') {
        rest = &rest[pos..];
        match leading_escape(rest) {
            Some(esc) => {
                f(esc);
                rest = &rest[esc.len()..];
            }
            None => rest = &rest[1..],
        }
    }
}

fn strip_ansi(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        match leading_escape(rest) {
            Some(esc) => rest = &rest[esc.len()..],
            None => {
                res.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    res
}

/// Text styles still open at a point in a chapter.
#[derive(Default)]
struct OpenStyles {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    link: Option<String>,
}

impl OpenStyles {
    fn apply(&mut self, esc: &str) {
        match esc {
            "\x1b[1m" => self.bold = true,
            "\x1b[2m" => self.dim = true,
            "\x1b[22m" => {
                self.bold = false;
                self.dim = false;
            }
            "\x1b[3m" => self.italic = true,
            "\x1b[23m" => self.italic = false,
            "\x1b[4m" => self.underline = true,
            "\x1b[24m" => self.underline = false,
            osc if osc.starts_with("\x1b]8;") => {
                // OSC 8 is "\x1b]8;params;uri\x1b\\"; an empty uri ends the link
                let uri = osc.trim_end_matches("\x1b\\").splitn(3, ';').nth(2);
                self.link = uri.filter(|u| !u.is_empty()).map(str::to_string);
            }
            _ => {}
        }
    }

    /// Wraps a line so it reopens the styles it inherits and closes whatever it
    /// leaves open. Any line can then be drawn first on screen without losing
    /// its style, and no style leaks into the next line or the footer.
    fn seal(&mut self, line: &str) -> String {
        let mut out = String::with_capacity(line.len() + 16);
        if let Some(uri) = &self.link {
            out.push_str(&format!("\x1b]8;;{}\x1b\\", uri));
        }
        if self.bold {
            out.push_str("\x1b[1m");
        }
        if self.dim {
            out.push_str("\x1b[2m");
        }
        if self.italic {
            out.push_str("\x1b[3m");
        }
        if self.underline {
            out.push_str("\x1b[4m");
        }
        out.push_str(line);
        for_each_escape(line, |esc| self.apply(esc));
        if self.bold || self.dim {
            out.push_str("\x1b[22m");
        }
        if self.italic {
            out.push_str("\x1b[23m");
        }
        if self.underline {
            out.push_str("\x1b[24m");
        }
        if self.link.is_some() {
            out.push_str("\x1b]8;;\x1b\\");
        }
        out
    }
}

fn read_zip_file<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut content = String::new();
    file.read_to_string(&mut content).ok()?;
    Some(content)
}

fn parse_xml(xml: &str) -> Option<Document<'_>> {
    // EPUB 2 NCX and XHTML files usually carry a DOCTYPE, which roxmltree rejects by default
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(xml, options).ok()
}

/// Decodes %XX escapes: package hrefs are URLs, zip entry names are not.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = s
            .get(i + 1..i + 3)
            .filter(|hex| bytes[i] == b'%' && hex.bytes().all(|b| b.is_ascii_hexdigit()));
        if let Some(hex) = escaped {
            out.push(u8::from_str_radix(hex, 16).unwrap_or(b'%'));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolves an href against the zip path of the file it appears in,
/// the way a browser resolves a relative URL, dropping any #fragment.
fn resolve_href(base_file: &str, href: &str) -> String {
    let href = percent_decode(href.split('#').next().unwrap_or(href));
    let mut parts: Vec<&str> = base_file.split('/').collect();
    parts.pop();
    if href.starts_with('/') {
        parts.clear();
    }
    for segment in href.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            segment => parts.push(segment),
        }
    }
    parts.join("/")
}

/// Adds titles from one list of an EPUB 3 navigation document:
/// "toc" for the table of contents, "landmarks" for pages like the cover.
fn nav_titles<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    nav_path: &str,
    kind: &str,
    titles: &mut HashMap<String, String>,
) -> Option<()> {
    let xml = read_zip_file(archive, nav_path)?;
    let doc = parse_xml(&xml)?;
    let navs: Vec<_> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "nav")
        .collect();
    let list = navs
        .iter()
        .find(|n| {
            n.attribute((EPUB_OPS_NS, "type"))
                .is_some_and(|t| t.split_whitespace().any(|t| t == kind))
        })
        // Some books leave epub:type off their table of contents
        .or(if kind == "toc" { navs.first() } else { None })?;

    for link in list.descendants().filter(|n| n.tag_name().name() == "a") {
        if let Some(href) = link.attribute("href") {
            let text: String = link
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                titles.entry(resolve_href(nav_path, href)).or_insert(text);
            }
        }
    }
    Some(())
}

/// Adds titles from an EPUB 2 NCX file, keeping any already found.
fn ncx_titles<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    ncx_path: &str,
    titles: &mut HashMap<String, String>,
) -> Option<()> {
    let xml = read_zip_file(archive, ncx_path)?;
    let doc = parse_xml(&xml)?;
    for nav_point in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "navPoint")
    {
        let text_node = nav_point
            .descendants()
            .find(|n| n.tag_name().name() == "text");
        let content_node = nav_point
            .descendants()
            .find(|n| n.tag_name().name() == "content");

        if let (Some(t), Some(c)) = (text_node, content_node)
            && let (Some(text), Some(src)) = (t.text(), c.attribute("src"))
        {
            titles
                .entry(resolve_href(ncx_path, src))
                .or_insert_with(|| text.trim().to_string());
        }
    }
    Some(())
}

pub fn get_epub_spine<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Option<Vec<(String, String)>> {
    let container_xml = read_zip_file(archive, "META-INF/container.xml")?;
    let doc = parse_xml(&container_xml)?;
    let rootfile = doc
        .descendants()
        .find(|n| n.tag_name().name() == "rootfile")?;
    let opf_path = rootfile.attribute("full-path")?;

    let opf_xml = read_zip_file(archive, opf_path)?;
    let opf_doc = parse_xml(&opf_xml)?;

    // Manifest items by id, with each href resolved to its full zip path
    let mut manifest = HashMap::new();
    for node in opf_doc
        .descendants()
        .filter(|n| n.tag_name().name() == "item")
    {
        if let (Some(id), Some(href)) = (node.attribute("id"), node.attribute("href")) {
            manifest.insert(id, (resolve_href(opf_path, href), node));
        }
    }
    let spine_node = opf_doc
        .descendants()
        .find(|n| n.tag_name().name() == "spine")?;

    let nav_path = manifest
        .values()
        .find(|(_, item)| {
            item.attribute("properties")
                .is_some_and(|p| p.split_whitespace().any(|p| p == "nav"))
        })
        .map(|(path, _)| path.clone());
    let ncx_path = spine_node
        .attribute("toc")
        .and_then(|id| manifest.get(id))
        .or_else(|| {
            manifest
                .values()
                .find(|(_, item)| item.attribute("media-type") == Some(NCX_MEDIA_TYPE))
        })
        .map(|(path, _)| path.clone());

    // First found wins: the table of contents (EPUB 3, then EPUB 2), then the
    // landmarks or guide entries that name pages such as the cover
    let mut titles = HashMap::new();
    if let Some(nav_path) = &nav_path {
        nav_titles(archive, nav_path, "toc", &mut titles);
    }
    if let Some(ncx_path) = &ncx_path {
        ncx_titles(archive, ncx_path, &mut titles);
    }
    if let Some(nav_path) = &nav_path {
        nav_titles(archive, nav_path, "landmarks", &mut titles);
    }
    for reference in opf_doc
        .descendants()
        .filter(|n| n.tag_name().name() == "reference")
    {
        if let (Some(href), Some(title)) =
            (reference.attribute("href"), reference.attribute("title"))
        {
            titles
                .entry(resolve_href(opf_path, href))
                .or_insert_with(|| title.trim().to_string());
        }
    }

    let mut spine = Vec::new();
    for itemref in spine_node
        .descendants()
        .filter(|n| n.tag_name().name() == "itemref")
    {
        let Some((path, _)) = itemref.attribute("idref").and_then(|id| manifest.get(id)) else {
            continue;
        };
        let title = match titles.get(path) {
            Some(title) => title.clone(),
            None => first_line(archive, path).unwrap_or_else(|| "Section".to_string()),
        };
        spine.push((path.clone(), title));
    }
    Some(spine)
}

/// Names a page that no table of contents lists by its first line of text.
fn first_line<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str) -> Option<String> {
    const MAX_CHARS: usize = 40;
    let html = read_zip_file(archive, path)?;
    let text = strip_ansi(&format_html_for_terminal(&html).replace('\x1e', ""));
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    if line.chars().count() <= MAX_CHARS {
        return Some(line.to_string());
    }
    let cut: String = line.chars().take(MAX_CHARS - 1).collect();
    // Break at a word boundary when there is one
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    Some(format!("{}…", cut.trim_end()))
}

pub fn load_chapter<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
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
    let mut styles = OpenStyles::default();

    for line in clean.lines() {
        let trimmed = line.trim();

        // A line of bare style codes shows nothing, but its codes still count
        if strip_ansi(trimmed).trim().is_empty() {
            for_each_escape(trimmed, |esc| styles.apply(esc));
            // Only push a single empty line, and only if we haven't just pushed one
            if !last_was_empty {
                wrapped_lines.push(String::new());
                last_was_empty = true;
            }
            continue;
        }

        last_was_empty = false;

        if trimmed.contains('\x1e') {
            let clean_line = styles.seal(&trimmed.replace('\x1e', ""));
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
                wrapped_lines.push(format!("{}{}", indent, styles.seal(&w)));
            }
        }
    }

    // Clean up trailing empty lines at the very bottom of the chapter
    while wrapped_lines.last().is_some_and(|l| l.is_empty()) {
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

    fn epub(files: &[(&str, &str)]) -> ZipArchive<std::io::Cursor<Vec<u8>>> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        ZipArchive::new(zip.finish().unwrap()).unwrap()
    }

    const CONTAINER: &str = r#"<?xml version="1.0"?>
        <container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles>
          <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
        </rootfiles></container>"#;

    #[test]
    fn epub3_nav_titles_and_encoded_paths() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><manifest>
            <item id="nav" href="nav/toc.xhtml" media-type="application/xhtml+xml" properties="nav"/>
            <item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/>
            <item id="c1" href="Text/Chapter%20One.xhtml" media-type="application/xhtml+xml"/>
            <item id="c2" href="Text/two.xhtml" media-type="application/xhtml+xml"/>
          </manifest><spine><itemref idref="cover"/><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#;
        let nav = r#"<!DOCTYPE html>
          <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
            <nav epub:type="landmarks"><ol>
              <li><a epub:type="cover" href="../cover.xhtml">Cover</a></li>
              <li><a href="../Text/two.xhtml">Wrong list</a></li>
            </ol></nav>
            <nav epub:type="toc"><ol>
              <li><a href="../Text/Chapter%20One.xhtml#start"><span>Chapter</span> One</a></li>
              <li><a href="../Text/two.xhtml">Chapter Two</a></li>
            </ol></nav>
          </body></html>"#;
        let mut archive = epub(&[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", opf),
            ("OEBPS/nav/toc.xhtml", nav),
            ("OEBPS/cover.xhtml", r#"<img src="cover.jpg"/>"#),
            (
                "OEBPS/Text/Chapter One.xhtml",
                "<html><body><p>Hello there</p></body></html>",
            ),
            ("OEBPS/Text/two.xhtml", "<p>Second</p>"),
        ]);

        let spine = get_epub_spine(&mut archive).unwrap();
        let expected = [
            ("OEBPS/cover.xhtml", "Cover"),
            ("OEBPS/Text/Chapter One.xhtml", "Chapter One"),
            ("OEBPS/Text/two.xhtml", "Chapter Two"),
        ]
        .map(|(p, t)| (p.to_string(), t.to_string()));
        assert_eq!(spine, expected);
        assert_eq!(
            load_chapter(&mut archive, &spine[1].0, 40, 0),
            ["Hello there"]
        );
    }

    #[test]
    fn ncx_with_doctype_gives_titles() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0"><manifest>
            <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
            <item id="c1" href="c1.html" media-type="application/xhtml+xml"/>
          </manifest><spine toc="ncx"><itemref idref="c1"/></spine></package>"#;
        let ncx = r#"<?xml version="1.0"?>
          <!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">
          <ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap>
            <navPoint id="p1"><navLabel><text>CHAPTER 1</text></navLabel><content src="c1.html#c01"/></navPoint>
          </navMap></ncx>"#;
        let mut archive = epub(&[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", opf),
            ("OEBPS/toc.ncx", ncx),
            ("OEBPS/c1.html", "<p>Text</p>"),
        ]);

        let spine = get_epub_spine(&mut archive).unwrap();
        assert_eq!(
            spine,
            [("OEBPS/c1.html".to_string(), "CHAPTER 1".to_string())]
        );
    }

    #[test]
    fn hrefs_resolve_like_urls() {
        assert_eq!(
            resolve_href("OEBPS/nav/toc.xhtml", "../Text/a%20b.xhtml#x"),
            "OEBPS/Text/a b.xhtml"
        );
        assert_eq!(resolve_href("content.opf", "./ch1.html"), "ch1.html");
        assert_eq!(resolve_href("OEBPS/content.opf", "/root.html"), "root.html");
        assert_eq!(percent_decode("caf%C3%A9 100%"), "café 100%");
    }

    #[test]
    fn strip_ansi_skips_hyperlinks() {
        let s = "\x1b]8;;https://x.org/a\x1b\\\x1b[4mlink\x1b[24m\x1b]8;;\x1b\\ text";
        assert_eq!(strip_ansi(s), "link text");
    }

    #[test]
    fn styles_carry_across_wrapped_lines() {
        let html =
            "<p><i>one two three four five six</i> plain <a href=\"n.html\">a long link</a></p>";
        let mut archive = epub(&[("c.html", html)]);
        let lines = load_chapter(&mut archive, "c.html", 10, 0);
        assert!(lines.len() >= 5, "{:?}", lines);
        for line in &lines {
            let text = strip_ansi(line);
            if text.contains("one") || text.contains("three") || text.contains("five") {
                assert!(
                    line.starts_with("\x1b[3m") && line.ends_with("\x1b[23m"),
                    "{:?}",
                    line
                );
            }
            if text.contains("link") {
                assert!(line.contains("\x1b]8;;n.html\x1b\\"), "{:?}", line);
                assert!(line.ends_with("\x1b]8;;\x1b\\"), "{:?}", line);
            }
        }
        assert!(lines.iter().any(|l| l.starts_with("plain")), "{:?}", lines);
    }

    #[test]
    fn untitled_pages_use_guide_then_first_line() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0"><manifest>
            <item id="cover" href="cover.html" media-type="application/xhtml+xml"/>
            <item id="letter" href="letter.html" media-type="application/xhtml+xml"/>
            <item id="long" href="long.html" media-type="application/xhtml+xml"/>
            <item id="blank" href="blank.html" media-type="application/xhtml+xml"/>
          </manifest><spine>
            <itemref idref="cover"/><itemref idref="letter"/><itemref idref="long"/><itemref idref="blank"/>
          </spine><guide><reference href="cover.html" title="Cover Image" type="cover"/></guide></package>"#;
        let mut archive = epub(&[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", opf),
            ("OEBPS/cover.html", r#"<img src="c.jpg"/>"#),
            (
                "OEBPS/letter.html",
                "<p>\n  Dear   Muscovites!</p><p>More</p>",
            ),
            (
                "OEBPS/long.html",
                "<p>A first line that runs on far past forty characters</p>",
            ),
            ("OEBPS/blank.html", "<p> </p>"),
        ]);

        let titles: Vec<String> = get_epub_spine(&mut archive)
            .unwrap()
            .into_iter()
            .map(|(_, title)| title)
            .collect();
        assert_eq!(
            titles,
            [
                "Cover Image",
                "Dear Muscovites!",
                "A first line that runs on far past…",
                "Section"
            ]
        );
    }
}
