//! Helpers for tests: books built in memory, and a small terminal screen that
//! replays what the reader draws.

use std::io::{Cursor, Write};
use zip::ZipArchive;

use crate::width::width;

pub type Book = ZipArchive<Cursor<Vec<u8>>>;
pub type Spine = Vec<(String, String)>;

/// An archive holding the given files.
pub fn archive(files: &[(&str, &[u8])]) -> Book {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body).unwrap();
    }
    ZipArchive::new(zip.finish().unwrap()).unwrap()
}

/// A complete EPUB 2 book, one chapter per (title, body HTML), with its
/// spine read back through the reader. `extra` files go under OEBPS/.
pub fn book(chapters: &[(&str, &str)], extra: &[(&str, &[u8])]) -> (Book, Spine) {
    let mut manifest =
        String::from(r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#);
    let (mut spine, mut nav) = (String::new(), String::new());
    let mut files: Vec<(String, Vec<u8>)> = vec![(
        "META-INF/container.xml".into(),
        br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#.to_vec(),
    )];
    for (n, (title, body)) in chapters.iter().enumerate() {
        manifest +=
            &format!(r#"<item id="c{n}" href="c{n}.html" media-type="application/xhtml+xml"/>"#);
        spine += &format!(r#"<itemref idref="c{n}"/>"#);
        nav += &format!(
            r#"<navPoint id="p{n}"><navLabel><text>{title}</text></navLabel><content src="c{n}.html"/></navPoint>"#
        );
        let html = format!("<html><head></head><body>{body}</body></html>");
        files.push((format!("OEBPS/c{n}.html"), html.into_bytes()));
    }
    let opf = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf"><manifest>{manifest}</manifest><spine toc="ncx">{spine}</spine></package>"#
    );
    let ncx = format!(
        r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>{nav}</navMap></ncx>"#
    );
    files.push(("OEBPS/content.opf".into(), opf.into_bytes()));
    files.push(("OEBPS/toc.ncx".into(), ncx.into_bytes()));
    for (name, body) in extra {
        files.push((format!("OEBPS/{name}"), body.to_vec()));
    }
    let named: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let mut book = archive(&named);
    let spine = crate::epub::get_epub_spine(&mut book).unwrap();
    (book, spine)
}

/// `count` paragraphs of `words` numbered filler words each ("p0w0 p0w1 …"),
/// so a test can tell exactly which text is on screen.
pub fn paragraphs(count: usize, words: usize) -> String {
    (0..count)
        .map(|p| {
            let words: Vec<String> = (0..words).map(|w| format!("p{p}w{w}")).collect();
            format!("<p>{}</p>", words.join(" "))
        })
        .collect()
}

/// A PNG of one color.
pub fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(width, height, image::Rgba(rgba))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

/// What a terminal would show after some output: the characters on each row,
/// plus the kitty graphics commands seen. Knows cursor moves, line erases and
/// wide characters; skips colors and other escape sequences.
pub struct Screen {
    cells: Vec<Vec<char>>,
    pub kitty: Vec<String>,
    /// Text was written past the right edge, which would wrap on a real terminal
    pub overflowed: bool,
}

// Marks the second cell of a wide character
const WIDE: char = '\0';

impl Screen {
    pub fn replay(cols: usize, rows: usize, bytes: &[u8]) -> Self {
        let mut screen = Screen {
            cells: vec![vec![' '; cols]; rows],
            kitty: Vec::new(),
            overflowed: false,
        };
        let text = String::from_utf8_lossy(bytes);
        let mut chars = text.chars().peekable();
        let (mut row, mut col) = (0usize, 0usize);
        while let Some(c) = chars.next() {
            match c {
                '\x1b' => match chars.next() {
                    Some('[') => {
                        let mut params = String::new();
                        let mut last = ' ';
                        for c in chars.by_ref() {
                            if ('\x40'..='\x7e').contains(&c) {
                                last = c;
                                break;
                            }
                            params.push(c);
                        }
                        match last {
                            'H' => {
                                let mut at = params.split(';').map(|p| p.parse().unwrap_or(1));
                                row = at.next().unwrap_or(1).max(1) - 1;
                                col = at.next().unwrap_or(1).max(1) - 1;
                            }
                            'K' if row < rows => {
                                let from = if params == "2" { 0 } else { col.min(cols) };
                                screen.cells[row][from..].fill(' ');
                            }
                            _ => {}
                        }
                    }
                    // OSC (hyperlinks) and APC (kitty graphics) run to the string terminator
                    Some(kind @ (']' | '_')) => {
                        let mut body = String::new();
                        while let Some(c) = chars.next() {
                            if c == '\x07' || (c == '\x1b' && chars.peek() == Some(&'\\')) {
                                chars.next_if_eq(&'\\');
                                break;
                            }
                            body.push(c);
                        }
                        if kind == '_' && body.starts_with('G') {
                            screen.kitty.push(body);
                        }
                    }
                    _ => {}
                },
                '\r' => col = 0,
                '\n' => row += 1,
                c => {
                    let w = width(c.encode_utf8(&mut [0; 4]));
                    if w == 0 {
                        continue;
                    }
                    if row >= rows || col + w > cols {
                        screen.overflowed = true;
                    } else {
                        screen.cells[row][col] = c;
                        if w == 2 {
                            screen.cells[row][col + 1] = WIDE;
                        }
                    }
                    col += w;
                }
            }
        }
        screen
    }

    /// A row's text, trailing spaces trimmed.
    pub fn row(&self, y: usize) -> String {
        let text: String = self.cells[y].iter().filter(|c| **c != WIDE).collect();
        text.trim_end().to_string()
    }

    /// Every row, joined with line breaks.
    pub fn text(&self) -> String {
        (0..self.cells.len())
            .map(|y| self.row(y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The screen column of the last `c` in row `y`.
    pub fn last_column_of(&self, y: usize, c: char) -> Option<usize> {
        self.cells[y].iter().rposition(|&cell| cell == c)
    }

    pub fn rows(&self) -> usize {
        self.cells.len()
    }
}
