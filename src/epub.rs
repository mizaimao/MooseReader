use roxmltree::{Document, ParsingOptions};
use std::collections::HashMap;
use std::io::{Read, Seek};
use zip::ZipArchive;

use crate::css::{Align, Computed, Stylesheet};
use crate::i18n::{Text, tr};
use crate::images::{self, Layout};
use crate::width;

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

/// The value of attribute `name` in the text of a tag, quoted either way.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(pos) = tag[from..].find(name) {
        let start = from + pos;
        from = start + name.len();
        // Whole attribute names only, so "href" doesn't match inside "xlink:href"
        if !tag[..start].ends_with(char::is_whitespace) {
            continue;
        }
        let Some(value) = tag[from..].strip_prefix('=') else {
            continue;
        };
        let quote = value.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let value = &value[1..];
        return value.find(quote).map(|end| &value[..end]);
    }
    None
}

/// Starts a line to center: headings, or text-align: center.
const CENTER: char = '\x1e';
/// Starts a line to align right.
const RIGHT: char = '\x1c';
/// Starts a paragraph whose first line is indented (text-indent).
const INDENT: char = '\x1a';
const FIRST_LINE_INDENT: &str = "  ";

fn strip_markers(line: &str) -> String {
    line.chars()
        .filter(|c| !matches!(*c, CENTER | RIGHT | INDENT))
        .collect()
}

fn is_heading(tag: &str) -> bool {
    matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// Elements that have no content and no closing tag.
fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "image"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Output text that switches bold, italic and underline codes only where the
/// text's style changes.
struct Writer {
    output: String,
    written: Computed,
    // Source line breaks are plain whitespace in HTML; only block tags start new lines
    at_space: bool,
    // No text on this line yet, so the next text may start with a line marker
    line_start: bool,
    // Inside an entity such as &amp;, which small caps must not upper-case
    in_entity: bool,
}

impl Writer {
    fn restyle(&mut self, style: &Computed) {
        let switches = [
            (self.written.bold, style.bold, "\x1b[1m", "\x1b[22m"),
            (self.written.italic, style.italic, "\x1b[3m", "\x1b[23m"),
            (
                self.written.underline,
                style.underline,
                "\x1b[4m",
                "\x1b[24m",
            ),
        ];
        for (was, now, on, off) in switches {
            if was != now {
                self.output.push_str(if now { on } else { off });
            }
        }
        self.written = *style;
    }

    fn newline(&mut self, style: &Computed) {
        self.restyle(style);
        self.output.push('\n');
        self.at_space = true;
        self.line_start = true;
    }

    fn text(&mut self, c: char, style: &Computed) {
        if c.is_ascii_whitespace() {
            // Collapses runs of spaces and source line breaks; leaves &nbsp; (U+00A0) alone
            if !self.at_space {
                self.restyle(style);
                self.output.push(' ');
                self.at_space = true;
            }
            self.in_entity = false;
            return;
        }
        if self.line_start {
            match style.align {
                Align::Center => self.output.push(CENTER),
                Align::Right => self.output.push(RIGHT),
                Align::Left if style.indent => self.output.push(INDENT),
                Align::Left => {}
            }
            self.line_start = false;
        }
        self.restyle(style);
        match c {
            '&' => self.in_entity = true,
            ';' => self.in_entity = false,
            _ => {}
        }
        if style.small_caps && !self.in_entity {
            self.output.extend(c.to_uppercase());
        } else {
            self.output.push(c);
        }
        self.at_space = false;
    }
}

/// Formats with the reader's default styles only.
fn format_html_for_terminal(input: &str) -> String {
    format_html(input, &Stylesheet::new())
}

fn format_html(input: &str, sheet: &Stylesheet) -> String {
    let mut in_tag = false;
    let mut current_tag = String::new();
    let mut w = Writer {
        output: String::with_capacity(input.len()),
        written: Computed::default(),
        at_space: true,
        line_start: true,
        in_entity: false,
    };

    let mut ignore_mode = false;
    let mut expected_closing_tag = String::new();

    // Open elements, innermost last: tag, style, and whether it opened a hyperlink
    let mut open: Vec<(String, Computed, bool)> = Vec::new();
    let style_of =
        |open: &[(String, Computed, bool)]| open.last().map_or(Computed::default(), |f| f.1);
    // Open lists, innermost last: None for <ul>, Some(next number) for <ol>
    let mut lists: Vec<Option<usize>> = Vec::new();
    let mut in_pre = false;

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

            if let Some(name) = base_tag.strip_prefix('/') {
                // Closes the innermost element of this name and anything left open inside it
                if let Some(pos) = open.iter().rposition(|f| f.0 == name) {
                    let closes_link = open.drain(pos..).any(|f| f.2);
                    if closes_link {
                        w.restyle(&style_of(&open));
                        w.output.push_str("\x1b]8;;\x1b\\");
                    }
                }
                let parent = style_of(&open);
                match name {
                    "ul" | "ol" => {
                        lists.pop();
                        w.newline(&parent);
                    }
                    "pre" => {
                        in_pre = false;
                        w.newline(&parent);
                    }
                    "h1" | "h2" | "h3" => {
                        w.newline(&parent);
                        w.newline(&parent);
                    }
                    tag if is_block(tag) || is_heading(tag) || tag == "li" => w.newline(&parent),
                    _ => {}
                }
                continue;
            }

            let name = base_tag;
            if matches!(name, "head" | "style" | "script") && !self_closing {
                ignore_mode = true;
                expected_closing_tag = format!("/{}", name);
                continue;
            }

            // Line breaks come before the element's own style starts
            let parent = style_of(&open);
            match name {
                "ul" | "ol" => {
                    w.newline(&parent);
                    if !self_closing {
                        lists.push((name == "ol").then_some(1));
                    }
                }
                "li" => {
                    w.newline(&parent);
                    match lists.last_mut() {
                        Some(Some(n)) => {
                            w.output.push_str(&format!("{}. ", n));
                            *n += 1;
                        }
                        _ => w.output.push_str("• "),
                    }
                    w.line_start = false;
                }
                "pre" => {
                    w.newline(&parent);
                    in_pre = !self_closing;
                }
                "img" | "image" => {
                    // Kept as a marker line; the chapter layout decides how to show it
                    let src = attribute(&current_tag, "src")
                        .or_else(|| attribute(&current_tag, "xlink:href"))
                        .or_else(|| attribute(&current_tag, "href"))
                        .unwrap_or("");
                    w.newline(&parent);
                    w.output.push(images::MARKER);
                    w.output.push_str(src);
                    w.newline(&parent);
                }
                tag if is_block(tag) || is_heading(tag) => w.newline(&parent),
                _ => {}
            }

            // Empty elements such as <a id="c05"/> and <br> have no text to style
            if self_closing || is_void(name) {
                continue;
            }
            let class = attribute(&current_tag, "class").unwrap_or("");
            let inline = attribute(&current_tag, "style").unwrap_or("");
            let mut style = sheet.compute(&parent, name, class, inline);
            let mut link = false;
            if name == "a" {
                let href = attribute(&current_tag, "href").unwrap_or("");
                // Links into the book's own files and bare anchors have nowhere
                // for the terminal to go, so only web and mail links stay links
                if ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|scheme| href.starts_with(scheme))
                {
                    w.output.push_str(&format!("\x1b]8;;{}\x1b\\", href));
                    style.underline = true;
                    link = true;
                }
            }
            open.push((name.to_string(), style, link));
            continue;
        }

        if in_tag {
            current_tag.push(c);
        } else if ignore_mode {
            continue;
        } else if in_pre {
            w.restyle(&style_of(&open));
            w.output.push(c);
        } else {
            w.text(c, &style_of(&open));
        }
    }

    w.restyle(&Computed::default());
    if open.iter().any(|f| f.2) {
        w.output.push_str("\x1b]8;;\x1b\\");
    }
    decode_entities(&w.output)
}

/// The reader's defaults plus the stylesheets a chapter links to or embeds,
/// in document order.
fn chapter_stylesheet<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    html: &str,
) -> Stylesheet {
    let mut sheet = Stylesheet::new();
    // ASCII lower-casing keeps every byte offset the same as in `html`
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find('<') {
        let start = from + pos;
        let Some(len) = lower[start..].find('>') else {
            break;
        };
        let (tag, tag_lower) = (
            &html[start + 1..start + len],
            &lower[start + 1..start + len],
        );
        from = start + len + 1;
        if tag_lower.starts_with("link") {
            let is_stylesheet = attribute(tag_lower, "rel")
                .is_some_and(|rel| rel.split_whitespace().any(|r| r == "stylesheet"));
            if let (true, Some(href)) = (is_stylesheet, attribute(tag, "href"))
                && let Some(css) = read_zip_file(archive, &resolve_href(path, href))
            {
                sheet.add(&css);
            }
        } else if tag_lower.starts_with("style") {
            if let Some(end) = lower[from..].find("</style") {
                sheet.add(&html[from..from + end]);
                from += end;
            }
        } else if tag_lower.starts_with("body") {
            // Stylesheets belong in the head
            break;
        }
    }
    sheet
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
    read_zip_bytes(archive, name).map(|bytes| decode_text(&bytes))
}

pub fn read_zip_bytes<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Decodes a text file from the book. EPUB allows UTF-8 and UTF-16 (told apart
/// by the byte order mark); anything else that isn't valid UTF-8 is read as
/// Windows-1252, as browsers read files labelled latin1 (WHATWG Encoding).
fn decode_text(bytes: &[u8]) -> String {
    fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| unit([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    }
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_string(),
            Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
        },
    }
}

fn windows_1252(byte: u8) -> char {
    // 0x80-0x9F are the only bytes where Windows-1252 differs from Latin-1
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        _ => byte as char,
    }
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
            None => first_line(archive, path).unwrap_or_else(|| tr(Text::Section).to_string()),
        };
        spine.push((path.clone(), title));
    }
    Some(spine)
}

/// Names a page that no table of contents lists by its first line of text.
fn first_line<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str) -> Option<String> {
    const MAX_COLUMNS: usize = 40;
    let html = read_zip_file(archive, path)?;
    let text = strip_ansi(&strip_markers(&format_html_for_terminal(&html)));
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    if line.starts_with(images::MARKER) {
        return Some(format!("[{}]", tr(Text::Image)));
    }
    if width::width(line) <= MAX_COLUMNS {
        return Some(line.to_string());
    }
    let cut = width::truncate(line, MAX_COLUMNS);
    let cut = cut.trim_end_matches('…');
    // Break at a word boundary when there is one
    let cut = cut.rsplit_once(' ').map_or(cut, |(head, _)| head);
    Some(format!("{}…", cut.trim_end()))
}

/// Where each chapter starts as a fraction of the whole book, weighted by each
/// file's uncompressed size, as Readium's positions list is, so image-only pages
/// barely move the overall percentage. Ends with an extra 1.0.
pub fn chapter_starts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    spine: &[(String, String)],
) -> Vec<f64> {
    let sizes: Vec<u64> = spine
        .iter()
        .map(|(path, _)| archive.by_name(path).map(|f| f.size()).unwrap_or(0))
        .collect();
    let total: u64 = sizes.iter().sum();
    if total == 0 {
        let count = spine.len().max(1) as f64;
        return (0..=spine.len()).map(|i| i as f64 / count).collect();
    }
    let mut starts = Vec::with_capacity(sizes.len() + 1);
    let mut before = 0;
    for size in sizes {
        starts.push(before as f64 / total as f64);
        before += size;
    }
    starts.push(1.0);
    starts
}

pub fn load_chapter<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    wrap_width: usize,
    margin_left: usize,
    layout: &Layout,
    plain_styles: bool,
) -> Vec<String> {
    let raw_html = read_zip_file(archive, path).unwrap_or_default();
    let sheet = if plain_styles {
        Stylesheet::plain()
    } else {
        chapter_stylesheet(archive, path, &raw_html)
    };
    let clean = format_html(&raw_html, &sheet);

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

        if let Some(src) = trimmed.strip_prefix(images::MARKER) {
            let picture = resolve_href(path, src);

            let lines = images::picture_lines(archive, &picture, wrap_width, margin_left, layout);

            wrapped_lines.extend(lines);

            continue;
        }

        // A line starts with at most one marker: centered, right-aligned or first-line indent
        let align = if trimmed.contains(CENTER) {
            Align::Center
        } else if trimmed.contains(RIGHT) {
            Align::Right
        } else {
            Align::Left
        };
        let first = if trimmed.contains(INDENT) {
            FIRST_LINE_INDENT
        } else {
            ""
        };
        let text = strip_markers(trimmed);
        let options = textwrap::Options::new(wrap_width).initial_indent(first);
        for part in textwrap::wrap(&text, options) {
            let sealed = styles.seal(&part);
            let room = wrap_width.saturating_sub(width::width(&sealed));
            let pad = match align {
                Align::Center => room / 2,
                Align::Right => room,
                Align::Left => 0,
            };
            wrapped_lines.push(format!("{}{}{}", indent, " ".repeat(pad), sealed));
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
        strip_ansi(&strip_markers(&format_html_for_terminal(html)))
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
        let files = files.iter().map(|(name, body)| (*name, body.as_bytes()));
        epub_bytes(&files.collect::<Vec<_>>())
    }

    fn epub_bytes(files: &[(&str, &[u8])]) -> ZipArchive<std::io::Cursor<Vec<u8>>> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body).unwrap();
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
            load_chapter(&mut archive, &spine[1].0, 40, 0, &Layout::labels(), false),
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
        let html = "<p><i>one two three four five six</i> plain <a href=\"https://x.org/n\">a long link</a></p>";
        let mut archive = epub(&[("c.html", html)]);
        let lines = load_chapter(&mut archive, "c.html", 10, 0, &Layout::labels(), false);
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
                assert!(line.contains("\x1b]8;;https://x.org/n\x1b\\"), "{:?}", line);
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

    #[test]
    fn only_web_links_become_hyperlinks() {
        let out = format_html_for_terminal(
            r#"<a href="c01.html#x">Chapter</a> <a name="n1">anchor</a> <a href="https://a.org/?q=1&amp;r=2">web</a>"#,
        );
        assert_eq!(
            out,
            "Chapter anchor \x1b]8;;https://a.org/?q=1&r=2\x1b\\\x1b[4mweb\x1b[24m\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn chapter_starts_follow_file_size() {
        let spine = [("a.html", "A"), ("b.html", "B")].map(|(p, t)| (p.to_string(), t.to_string()));
        let mut archive = epub(&[("a.html", &"x".repeat(100)), ("b.html", &"x".repeat(300))]);
        assert_eq!(chapter_starts(&mut archive, &spine), [0.0, 0.25, 1.0]);
        let mut empty = epub(&[("a.html", ""), ("b.html", "")]);
        assert_eq!(chapter_starts(&mut empty, &spine), [0.0, 0.5, 1.0]);
    }

    #[test]
    fn chapters_decode_utf16_and_windows_1252() {
        let mut utf16le = vec![0xFF, 0xFE];
        utf16le.extend(
            "<p>Здравствуй</p>"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        assert_eq!(decode_text(&utf16le), "<p>Здравствуй</p>");
        let mut utf16be = vec![0xFE, 0xFF];
        utf16be.extend("<p>Hi</p>".encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(decode_text(&utf16be), "<p>Hi</p>");
        assert_eq!(decode_text(b"\xEF\xBB\xBFplain"), "plain");
        assert_eq!(
            decode_text(b"caf\xE9 \x93quoted\x94 \x80"),
            "café “quoted” €"
        );
    }

    #[test]
    fn attributes_match_whole_names() {
        let tag = r#"image width="10" xlink:href='a.jpg'"#;
        assert_eq!(attribute(tag, "xlink:href"), Some("a.jpg"));
        assert_eq!(attribute(tag, "href"), None);
        assert_eq!(
            attribute(r#"img alt="x" src="b.png""#, "src"),
            Some("b.png")
        );
    }

    #[test]
    fn pictures_follow_the_layout() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(40, 40, image::Rgba([200, 30, 30, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let html = r#"<p>Before</p><img src="../img/red%20dot.png"/>
            <svg><image xlink:href="../img/red%20dot.png"/></svg><img src="missing.png"/><p>After</p>"#;
        let mut archive = epub_bytes(&[
            ("OEBPS/text/c.html", html.as_bytes()),
            ("OEBPS/img/red dot.png", png.get_ref()),
        ]);
        let chapter = |archive: &mut ZipArchive<_>, mode| {
            let layout = Layout {
                mode,
                cell: (10, 20),
                max_rows: 30,
                background: [0, 0, 0],
            };
            load_chapter(archive, "OEBPS/text/c.html", 20, 2, &layout, false)
        };
        let labels = |lines: &[String]| {
            let is_label = |l: &&String| strip_ansi(l).trim() == "[Image]";
            lines.iter().filter(is_label).count()
        };

        // 40 px square on 10x20 px cells: 4 columns by 2 rows, centered in 20 columns
        let lines = chapter(&mut archive, images::Mode::Kitty);
        let rows: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with(images::MARKER))
            .collect();
        assert_eq!(rows.len(), 4, "{:?}", lines);
        let first = format!(
            "{}0\x1f2\x1f10\x1f4\x1fOEBPS/img/red dot.png",
            images::MARKER
        );
        assert_eq!(rows[0], &first);
        assert_eq!(labels(&lines), 1, "the missing picture gets a label");

        let lines = chapter(&mut archive, images::Mode::Blocks);
        let blocks: Vec<&String> = lines.iter().filter(|l| l.contains('▀')).collect();
        assert_eq!(blocks.len(), 4);
        assert!(blocks[0].starts_with(&format!("{}\x1b[38;2;200;30;30m", " ".repeat(10))));
        assert_eq!(strip_ansi(blocks[0]).matches('▀').count(), 4);
        assert_eq!(labels(&lines), 1);

        let lines = chapter(&mut archive, images::Mode::Labels);
        assert_eq!(labels(&lines), 3);
    }

    #[test]
    fn book_stylesheets_format_the_text() {
        let css = ".c { text-align: center } .r { text-align: right } .tx { text-indent: 1.5em }
            .it { font-style: italic } .sc { font-variant: small-caps }";
        let html = r#"<html><head><link rel="stylesheet" type="text/css" href="../styles/book.css"/>
            <style>.b { font-weight: bold }</style></head><body>
            <p class="c">Centered line</p><p class="r">Right</p>
            <p class="tx">An indented paragraph that is long enough to wrap onto a second line.</p>
            <p>Plain <span class="it">slanted</span> and <span class="b">heavy</span> words.</p>
            <p class="sc">Small &amp; caps</p></body></html>"#;
        let mut archive = epub(&[("OEBPS/styles/book.css", css), ("OEBPS/text/c.html", html)]);
        let lines = load_chapter(
            &mut archive,
            "OEBPS/text/c.html",
            30,
            2,
            &Layout::labels(),
            false,
        );
        let find = |needle: &str| {
            let line = lines.iter().find(|l| strip_ansi(l).contains(needle));
            line.unwrap_or_else(|| panic!("{:?} not in {:?}", needle, lines))
        };

        // Centered and right-aligned by the text's width in the 30 columns after the margin
        assert_eq!(
            strip_ansi(find("Centered")),
            format!("  {}Centered line", " ".repeat(8))
        );
        assert_eq!(
            strip_ansi(find("Right")),
            format!("  {}Right", " ".repeat(25))
        );
        // Only the first line of the indented paragraph starts further in
        assert!(strip_ansi(find("An indented")).starts_with("    An indented"));
        let second = lines
            .iter()
            .position(|l| strip_ansi(l).contains("An indented"))
            .unwrap()
            + 1;
        assert!(
            strip_ansi(&lines[second]).starts_with("  ")
                && !strip_ansi(&lines[second]).starts_with("   ")
        );
        // Styles from a linked file and from a <style> block
        assert!(find("slanted").contains("\x1b[3mslanted\x1b[23m"));
        assert!(find("heavy").contains("\x1b[1mheavy\x1b[22m"));
        // Small caps upper-case the text but not the entity
        assert!(strip_ansi(find("SMALL")).ends_with("SMALL & CAPS"));
    }

    #[test]
    fn plain_styles_ignore_the_books_stylesheets() {
        let css = ".c { text-align: center } .it { font-style: italic }";
        let html = r#"<html><head><link rel="stylesheet" href="s.css"/></head><body>
            <p class="c">Centered</p><p><span class="it">slanted</span> <i>tagged</i></p></body></html>"#;
        let mut archive = epub(&[("s.css", css), ("c.html", html)]);
        let lines = load_chapter(&mut archive, "c.html", 30, 2, &Layout::labels(), true);
        assert_eq!(strip_ansi(&lines[0]), "  Centered");
        let words = lines.iter().find(|l| l.contains("slanted")).unwrap();
        assert!(!words.contains("\x1b[3mslanted"), "{words:?}");
        assert!(
            words.contains("\x1b[3mtagged"),
            "the reader's own <i> still counts"
        );
    }

    #[test]
    fn nested_lists_number_each_level() {
        let html =
            "<ol><li>A<ol><li>x</li><li>y</li></ol></li><li>B</li></ol><ul><li>dot</li></ul>";
        assert_eq!(text_lines(html), ["1. A", "1. x", "2. y", "2. B", "• dot"]);
    }

    #[test]
    fn every_line_of_a_centered_paragraph_is_centered() {
        let out = format_html_for_terminal(r#"<p style="text-align: center">one<br/>three</p>"#);
        let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines, [format!("{CENTER}one"), format!("{CENTER}three")]);
    }

    #[test]
    fn main_headings_are_bold_centered_and_followed_by_a_blank_line() {
        let out = format_html_for_terminal("<h2>Title</h2><p>Text</p>");
        assert!(
            out.contains(&format!("{CENTER}\x1b[1mTitle\x1b[22m\n\n")),
            "{out:?}"
        );
    }

    #[test]
    fn stray_closing_tags_leave_open_styles_alone() {
        let out = format_html_for_terminal("<i>a</p>b</i>c");
        assert!(out.contains("\x1b[3ma\nb\x1b[23mc"), "{out:?}");
    }

    #[test]
    fn closing_a_block_closes_what_was_left_open_inside_it() {
        let out = format_html_for_terminal("<div><span><i>open</div>after");
        assert!(out.contains("open\x1b[23m\nafter"), "{out:?}");
    }

    #[test]
    fn scripts_and_style_blocks_are_not_text() {
        let html = "<head><title>T</title></head><script>var x = 1;</script><style>p { color: red }</style><p>shown</p>";
        assert_eq!(text_lines(html), ["shown"]);
    }

    #[test]
    fn empty_and_missing_chapters_have_no_lines() {
        let mut archive = epub(&[("c.html", "<html><body>  </body></html>")]);
        for path in ["c.html", "missing.html"] {
            assert!(load_chapter(&mut archive, path, 30, 2, &Layout::labels(), false).is_empty());
        }
    }
}
