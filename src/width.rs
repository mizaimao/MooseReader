//! Text measured in terminal columns rather than characters: wide characters
//! (Chinese, Japanese, Korean) take two columns and escape sequences none.

use textwrap::core::display_width;

pub fn width(s: &str) -> usize {
    display_width(s)
}

/// Cuts plain text to at most `max` columns, ending with "…" when shortened.
pub fn truncate(s: &str, max: usize) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    let mut buf = [0u8; 4];
    for c in s.chars() {
        let w = width(c.encode_utf8(&mut buf));
        if used + w + 1 > max {
            break;
        }
        out.push(c);
        used += w;
    }
    if max > 0 {
        out.push('…');
    }
    out
}

pub fn pad_right(s: &str, columns: usize) -> String {
    format!("{}{}", s, " ".repeat(columns.saturating_sub(width(s))))
}

pub fn pad_left(s: &str, columns: usize) -> String {
    format!("{}{}", " ".repeat(columns.saturating_sub(width(s))), s)
}

pub fn center(s: &str, columns: usize) -> String {
    let room = columns.saturating_sub(width(s));
    format!(
        "{}{}{}",
        " ".repeat(room / 2),
        s,
        " ".repeat(room - room / 2)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_characters_take_two_columns() {
        assert_eq!(width("地铁 Metro"), 10);
        assert_eq!(width("\x1b[1m地铁\x1b[22m"), 4);
        assert_eq!(truncate("地铁站的生活", 7), "地铁站…");
        assert_eq!(truncate("Metro", 5), "Metro");
        assert_eq!(pad_right("地铁", 6), "地铁  ");
        assert_eq!(pad_left("地铁", 6), "  地铁");
        assert_eq!(center("地铁", 7), " 地铁  ");
    }
}
