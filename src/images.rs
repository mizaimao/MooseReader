//! Pictures in the book: drawn by the terminal through the kitty graphics
//! protocol where it has one (Ghostty, kitty), drawn with colored half-block
//! characters elsewhere, or shown as an "[Image]" label.

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::{self, Cursor, Read, Seek, Write};
use zip::ZipArchive;

use crate::epub::read_zip_bytes;
use crate::i18n::{Text, tr};

/// Starts a line that holds one row of a picture the terminal draws.
pub const MARKER: char = '\x1d';
const SEPARATOR: char = '\x1f';

/// The Images setting.
#[derive(Serialize, Deserialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Setting {
    Auto,
    Kitty,
    Blocks,
    Off,
}

impl Setting {
    pub fn next(self) -> Self {
        match self {
            Setting::Auto => Setting::Kitty,
            Setting::Kitty => Setting::Blocks,
            Setting::Blocks => Setting::Off,
            Setting::Off => Setting::Auto,
        }
    }

    pub fn prev(self) -> Self {
        self.next().next().next()
    }

    pub fn name(self) -> &'static str {
        match self {
            Setting::Auto => tr(Text::Auto),
            Setting::Kitty => "Kitty",
            Setting::Blocks => tr(Text::Blocks),
            Setting::Off => tr(Text::Off),
        }
    }
}

/// How pictures are shown on this terminal.
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Mode {
    Labels,
    Kitty,
    Blocks,
}

impl Mode {
    pub fn from_setting(setting: Setting) -> Self {
        match setting {
            Setting::Kitty => Mode::Kitty,
            Setting::Blocks => Mode::Blocks,
            Setting::Off => Mode::Labels,
            Setting::Auto if kitty_terminal() => Mode::Kitty,
            Setting::Auto if truecolor_terminal() => Mode::Blocks,
            Setting::Auto => Mode::Labels,
        }
    }
}

/// Terminals known to implement the kitty graphics protocol, going by the
/// variables they set. Inside tmux or screen the protocol needs passthrough,
/// so it is left off there.
fn kitty_terminal() -> bool {
    let var = |name| std::env::var(name).unwrap_or_default();
    let term = var("TERM");
    if !var("TMUX").is_empty() || term.starts_with("screen") || term.starts_with("tmux") {
        return false;
    }
    term == "xterm-kitty"
        || term == "xterm-ghostty"
        || var("TERM_PROGRAM") == "ghostty"
        || !var("KITTY_WINDOW_ID").is_empty()
}

fn truecolor_terminal() -> bool {
    matches!(
        std::env::var("COLORTERM").as_deref(),
        Ok("truecolor" | "24bit")
    )
}

/// What laying out a picture needs to know about the terminal.
#[derive(Clone, Copy)]
pub struct Layout {
    pub mode: Mode,
    /// Cell size in pixels, which sets a picture's rows for its columns
    pub cell: (u32, u32),
    /// Tallest picture in rows, so each fits on one screen
    pub max_rows: usize,
    /// Color that transparent pixels blend into (half-blocks only)
    pub background: [u8; 3],
}

impl Layout {
    pub fn labels() -> Self {
        Layout {
            mode: Mode::Labels,
            cell: (10, 20),
            max_rows: 24,
            background: [0, 0, 0],
        }
    }
}

/// The cell size in pixels from the window size the terminal reports; a
/// terminal that reports none gets a typical 1:2 cell.
pub fn cell_size() -> (u32, u32) {
    match crossterm::terminal::window_size() {
        Ok(w) if w.width > 0 && w.height > 0 && w.columns > 0 && w.rows > 0 => (
            (w.width / w.columns).max(1) as u32,
            (w.height / w.rows).max(1) as u32,
        ),
        _ => (10, 20),
    }
}

/// The lines one picture takes: rows reserved for the terminal to draw into,
/// rows of half-blocks, or a label when the picture can't be shown.
pub fn picture_lines<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    wrap_width: usize,
    indent: usize,
    layout: &Layout,
) -> Vec<String> {
    let label = || {
        let text = format!("[{}]", tr(Text::Image));
        vec![format!("{}\x1b[2m{}\x1b[22m", " ".repeat(indent), text)]
    };
    if layout.mode == Mode::Labels {
        return label();
    }
    let Some(bytes) = read_zip_bytes(archive, path) else {
        return label();
    };
    let Some((width, height)) = dimensions(&bytes) else {
        return label();
    };
    let (cols, rows) = fit(width, height, wrap_width, layout);
    let col = indent + (wrap_width.saturating_sub(cols)) / 2;
    match layout.mode {
        Mode::Kitty => (0..rows)
            .map(|row| {
                let s = SEPARATOR;
                format!("{MARKER}{row}{s}{rows}{s}{col}{s}{cols}{s}{path}")
            })
            .collect(),
        // Each character cell shows two pixels, one above the other
        _ => match canvas(&bytes, cols as u32, rows as u32 * 2) {
            Some(canvas) => block_lines(&canvas, col, layout.background),
            None => label(),
        },
    }
}

fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// Columns and rows for a picture: its natural size, shrunk to the text width
/// and the screen height, keeping its shape.
fn fit(width: u32, height: u32, wrap_width: usize, layout: &Layout) -> (usize, usize) {
    let (cell_w, cell_h) = (layout.cell.0 as f64, layout.cell.1 as f64);
    let (width, height) = (width.max(1) as f64, height.max(1) as f64);
    let scale = (wrap_width as f64 * cell_w / width)
        .min(layout.max_rows as f64 * cell_h / height)
        .min(1.0);
    // Rounded up, so a picture shown at its natural size fits its cells without resizing
    let cols = (width * scale / cell_w - 1e-9).ceil() as usize;
    let rows = (height * scale / cell_h - 1e-9).ceil() as usize;
    (
        cols.clamp(1, wrap_width.max(1)),
        rows.clamp(1, layout.max_rows.max(1)),
    )
}

/// The picture centered on a transparent `width` × `height` canvas, shrunk
/// first if it doesn't fit. Shrinking uses the integer box filter, which needs
/// no floating-point copy of the picture; pictures are never enlarged.
fn canvas(bytes: &[u8], width: u32, height: u32) -> Option<RgbaImage> {
    let mut picture = image::load_from_memory(bytes).ok()?;
    if picture.width() > width || picture.height() > height {
        picture = picture.thumbnail(width, height);
    }
    let fitted = picture.to_rgba8();
    let mut canvas = RgbaImage::new(width, height);
    let x = width.saturating_sub(fitted.width()) / 2;
    let y = height.saturating_sub(fitted.height()) / 2;
    image::imageops::replace(&mut canvas, &fitted, x as i64, y as i64);
    Some(canvas)
}

/// Text lines drawing a picture with "▀": the foreground paints the top pixel
/// and the background the bottom one.
fn block_lines(canvas: &RgbaImage, col: usize, background: [u8; 3]) -> Vec<String> {
    let blend = |x: u32, y: u32| {
        let pixel = canvas.get_pixel(x, y);
        let alpha = pixel[3] as u16;
        [0, 1, 2]
            .map(|i| ((pixel[i] as u16 * alpha + background[i] as u16 * (255 - alpha)) / 255) as u8)
    };
    (0..canvas.height() / 2)
        .map(|row| {
            let mut line = " ".repeat(col);
            let mut last = None;
            for x in 0..canvas.width() {
                let colors = (blend(x, row * 2), blend(x, row * 2 + 1));
                if last != Some(colors) {
                    let ([r, g, b], [br, bg, bb]) = colors;
                    let _ = write!(line, "\x1b[38;2;{r};{g};{b}m\x1b[48;2;{br};{bg};{bb}m");
                    last = Some(colors);
                }
                line.push('▀');
            }
            line.push_str("\x1b[39;49m");
            line
        })
        .collect()
}

/// One row of a picture, read back from its marker line.
struct Row<'a> {
    row: usize,
    rows: usize,
    col: usize,
    cols: usize,
    path: &'a str,
}

fn parse_row(line: &str) -> Option<Row<'_>> {
    let mut parts = line.strip_prefix(MARKER)?.splitn(5, SEPARATOR);
    let mut number = || parts.next()?.parse().ok();
    let (row, rows, col, cols) = (number()?, number()?, number()?, number()?);
    Some(Row {
        row,
        rows,
        col,
        cols,
        path: parts.next()?,
    })
}

/// Pictures on a terminal with the kitty graphics protocol. Each picture is
/// sent once and kept by the terminal under its id; every frame then places
/// the visible ones, cropped to the rows on screen.
#[derive(Default)]
pub struct Kitty {
    sent: HashSet<u32>,
}

impl Kitty {
    /// Removes every picture from the screen; run before each frame.
    pub fn clear(&self, frame: &mut impl Write) -> io::Result<()> {
        if !self.sent.is_empty() {
            frame.write_all(b"\x1b_Ga=d,d=a,q=2\x1b\\")?;
        }
        Ok(())
    }

    /// Places the pictures among the visible lines. Pixel data goes straight
    /// to `terminal`, once per picture; placements go into `frame`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<R: Read + Seek>(
        &mut self,
        frame: &mut impl Write,
        terminal: &mut impl Write,
        lines: &[String],
        offset: usize,
        page: usize,
        layout: &Layout,
        archive: &mut ZipArchive<R>,
    ) -> io::Result<()> {
        let (cell_w, cell_h) = layout.cell;
        let end = (offset + page).min(lines.len());
        let mut i = offset;
        while i < end {
            let Some(row) = parse_row(&lines[i]) else {
                i += 1;
                continue;
            };
            let visible = (row.rows - row.row).min(end - i);
            let (width, height) = (row.cols as u32 * cell_w, row.rows as u32 * cell_h);
            let id = picture_id(row.path, width, height);
            if !self.sent.contains(&id) {
                let canvas = read_zip_bytes(archive, row.path)
                    .and_then(|bytes| canvas(&bytes, width, height));
                match canvas {
                    Some(canvas) => {
                        transmit(terminal, id, &canvas)?;
                        self.sent.insert(id);
                    }
                    None => {
                        i += visible;
                        continue;
                    }
                }
            }
            // Cursor to the picture's first visible cell, then show just the rows on screen
            write!(frame, "\x1b[{};{}H", i - offset + 1, row.col + 1)?;
            write!(
                frame,
                "\x1b_Ga=p,i={id},x=0,y={},w={width},h={},c={},r={visible},C=1,q=2\x1b\\",
                row.row as u32 * cell_h,
                visible as u32 * cell_h,
                row.cols,
            )?;
            i += visible;
        }
        Ok(())
    }

    /// Frees every picture the terminal holds for us; run on exit.
    pub fn release(&mut self, terminal: &mut impl Write) -> io::Result<()> {
        for id in self.sent.drain() {
            write!(terminal, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\")?;
        }
        terminal.flush()
    }
}

/// Picture ids are shared by everything in the terminal window, so they come
/// from the picture and its size rather than counting up from 1.
fn picture_id(path: &str, width: u32, height: u32) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    let bytes = path
        .bytes()
        .chain(width.to_le_bytes())
        .chain(height.to_le_bytes());
    for byte in bytes {
        hash = (hash ^ byte as u32).wrapping_mul(0x0100_0193);
    }
    (hash & 0x7fff_ffff).max(1)
}

/// Sends RGBA pixels in the protocol's chunks: 3072 bytes encode to the
/// largest allowed payload of 4096 base64 characters.
fn transmit(out: &mut impl Write, id: u32, canvas: &RgbaImage) -> io::Result<()> {
    let chunks: Vec<&[u8]> = canvas.as_raw().chunks(3072).collect();
    for (n, chunk) in chunks.iter().enumerate() {
        let more = u8::from(n + 1 < chunks.len());
        if n == 0 {
            let (w, h) = canvas.dimensions();
            write!(out, "\x1b_Ga=t,f=32,s={w},v={h},i={id},q=2,m={more};")?;
        } else {
            write!(out, "\x1b_Gm={more};")?;
        }
        out.write_all(base64(chunk).as_bytes())?;
        out.write_all(b"\x1b\\")?;
    }
    out.flush()
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for group in data.chunks(3) {
        let bytes = [
            group[0],
            *group.get(1).unwrap_or(&0),
            *group.get(2).unwrap_or(&0),
        ];
        let n = (bytes[0] as u32) << 16 | (bytes[1] as u32) << 8 | bytes[2] as u32;
        for i in 0..4 {
            if i <= group.len() {
                out.push(TABLE[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(mode: Mode, max_rows: usize) -> Layout {
        Layout {
            mode,
            cell: (10, 20),
            max_rows,
            background: [0, 0, 0],
        }
    }

    #[test]
    fn base64_matches_rfc_4648() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
    }

    #[test]
    fn pictures_fit_the_text_width_and_the_screen() {
        // Natural size when it fits: 400 px over 10 px cells
        assert_eq!(fit(400, 600, 60, &layout(Mode::Kitty, 100)), (40, 30));
        // Shrunk to the screen height, keeping its shape
        assert_eq!(fit(400, 600, 60, &layout(Mode::Kitty, 20)), (27, 20));
        // Shrunk to the text width
        assert_eq!(fit(2000, 1000, 50, &layout(Mode::Kitty, 100)), (50, 13));
    }

    #[test]
    fn kitty_transmission_is_chunked() {
        let canvas = RgbaImage::new(32, 32); // 4096 bytes: one full chunk and a partial one
        let mut out = Vec::new();
        transmit(&mut out, 7, &canvas).unwrap();
        let text = String::from_utf8(out).unwrap();
        let commands: Vec<&str> = text.split("\x1b\\").filter(|c| !c.is_empty()).collect();
        assert_eq!(commands.len(), 2);
        assert!(commands[0].starts_with("\x1b_Ga=t,f=32,s=32,v=32,i=7,q=2,m=1;"));
        assert_eq!(commands[0].split(';').nth(1).unwrap().len(), 4096);
        assert!(commands[1].starts_with("\x1b_Gm=0;"));
    }

    #[test]
    fn marker_rows_round_trip() {
        let line = format!("{MARKER}3\x1f10\x1f4\x1f40\x1fOEBPS/images/a b.jpg");
        let row = parse_row(&line).unwrap();
        assert_eq!((row.row, row.rows, row.col, row.cols), (3, 10, 4, 40));
        assert_eq!(row.path, "OEBPS/images/a b.jpg");
        assert!(parse_row("plain text").is_none());
    }

    #[test]
    fn half_blocks_paint_two_pixels_per_cell() {
        let mut canvas = RgbaImage::new(2, 2);
        canvas.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        canvas.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        // The right column is transparent and takes the background
        let lines = block_lines(&canvas, 1, [9, 9, 9]);
        assert_eq!(
            lines,
            [
                "\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀\x1b[38;2;9;9;9m\x1b[48;2;9;9;9m▀\x1b[39;49m"
                    .to_string()
            ]
            .map(|l| format!(" {}", l))
        );
    }

    #[test]
    fn settings_cycle_through_every_mode() {
        let mut setting = Setting::Auto;
        for _ in 0..4 {
            setting = setting.next();
        }
        assert_eq!(setting, Setting::Auto);
        assert_eq!(Setting::Auto.prev(), Setting::Off);
        assert_eq!(Mode::from_setting(Setting::Kitty), Mode::Kitty);
        assert_eq!(Mode::from_setting(Setting::Blocks), Mode::Blocks);
        assert_eq!(Mode::from_setting(Setting::Off), Mode::Labels);
    }

    #[test]
    fn kitty_places_the_visible_rows_and_sends_each_picture_once() {
        let picture = crate::test_support::png(100, 200, [0, 120, 200, 255]);
        let mut archive = crate::test_support::archive(&[("p.png", &picture)]);
        let layout = layout(Mode::Kitty, 30);
        // 100 x 200 px on 10 x 20 px cells: 10 columns by 10 rows
        let mut lines = vec!["text".to_string()];
        lines.extend(picture_lines(&mut archive, "p.png", 40, 0, &layout));
        lines.push("text".to_string());
        assert_eq!(lines.len(), 12);

        let mut kitty = Kitty::default();
        let (mut frame, mut sent) = (Vec::new(), Vec::new());
        // Scrolled so the picture's first three rows are off the top
        kitty
            .draw(&mut frame, &mut sent, &lines, 4, 20, &layout, &mut archive)
            .unwrap();
        let frame = String::from_utf8(frame).unwrap();
        assert!(frame.contains("\x1b[1;16H"), "top row, centered: {frame:?}");
        assert!(frame.contains("y=60,w=100,h=140,c=10,r=7"), "{frame:?}");
        assert_eq!(String::from_utf8(sent).unwrap().matches("a=t,").count(), 1);

        let mut sent = Vec::new();
        kitty
            .draw(
                &mut Vec::new(),
                &mut sent,
                &lines,
                0,
                20,
                &layout,
                &mut archive,
            )
            .unwrap();
        assert!(sent.is_empty(), "the terminal already has it");
    }

    #[test]
    fn nothing_is_cleared_or_freed_until_a_picture_is_sent() {
        let mut kitty = Kitty::default();
        let mut out = Vec::new();
        kitty.clear(&mut out).unwrap();
        assert!(out.is_empty());
        kitty.sent.insert(42);
        kitty.clear(&mut out).unwrap();
        assert_eq!(out, b"\x1b_Ga=d,d=a,q=2\x1b\\");
        let mut released = Vec::new();
        kitty.release(&mut released).unwrap();
        assert_eq!(released, b"\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
        assert!(kitty.sent.is_empty());
    }

    #[test]
    fn broken_or_missing_pictures_get_a_label() {
        let mut archive = crate::test_support::archive(&[("bad.png", b"not a picture")]);
        for mode in [Mode::Kitty, Mode::Blocks, Mode::Labels] {
            for path in ["bad.png", "missing.png"] {
                let lines = picture_lines(&mut archive, path, 40, 2, &layout(mode, 30));
                assert_eq!(lines, ["  \x1b[2m[Image]\x1b[22m"], "{mode:?} {path}");
            }
        }
    }
}
