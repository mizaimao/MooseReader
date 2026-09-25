//! The rows of the settings menu: what each shows and how it changes.

use crate::config::{Alignment, Config, ProgressMode};
use crate::i18n::{self, Text, tr};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Row {
    MaxWidth,
    MarginLeft,
    MarginRight,
    ScrollLines,
    Theme,
    Language,
    Images,
    PlainStyles,
    ShowFooter,
    DimFooter,
    FooterAlign,
    ChapterTitle,
    ProgressMode,
    ProgressBar,
    BarLength,
    ProgressPercent,
    ChapterLoc,
}

/// Menu order: the first MAIN_ROWS form the main group, the rest the footer group.
pub const ROWS: [Row; 17] = [
    Row::MaxWidth,
    Row::MarginLeft,
    Row::MarginRight,
    Row::ScrollLines,
    Row::Theme,
    Row::Language,
    Row::Images,
    Row::PlainStyles,
    Row::ShowFooter,
    Row::DimFooter,
    Row::FooterAlign,
    Row::ChapterTitle,
    Row::ProgressMode,
    Row::ProgressBar,
    Row::BarLength,
    Row::ProgressPercent,
    Row::ChapterLoc,
];
pub const MAIN_ROWS: usize = 8;

impl Row {
    pub fn label(self) -> &'static str {
        tr(match self {
            Row::MaxWidth => Text::MaxWidth,
            Row::MarginLeft => Text::MarginLeft,
            Row::MarginRight => Text::MarginRight,
            Row::ScrollLines => Text::ScrollLines,
            Row::Theme => Text::Theme,
            Row::Language => Text::Language,
            Row::Images => Text::Images,
            Row::PlainStyles => Text::PlainStyles,
            Row::ShowFooter => Text::ShowFooter,
            Row::DimFooter => Text::DimFooter,
            Row::FooterAlign => Text::FooterAlign,
            Row::ChapterTitle => Text::ChapterTitle,
            Row::ProgressMode => Text::ProgressMode,
            Row::ProgressBar => Text::ProgressBar,
            Row::BarLength => Text::BarLength,
            Row::ProgressPercent => Text::ProgressPercent,
            Row::ChapterLoc => Text::ChapterLoc,
        })
    }

    pub fn value(self, cfg: &Config) -> String {
        let on_off = |on: bool| tr(if on { Text::On } else { Text::Off }).to_string();
        match self {
            Row::MaxWidth => cfg.max_width.to_string(),
            Row::MarginLeft => cfg.margin_left.to_string(),
            Row::MarginRight => cfg.margin_right.to_string(),
            Row::ScrollLines => cfg.scroll_by_lines.to_string(),
            Row::Theme => cfg.theme.name().to_string(),
            Row::Language => cfg.language.name().to_string(),
            Row::Images => cfg.images.name().to_string(),
            Row::PlainStyles => on_off(cfg.plain_styles),
            Row::ShowFooter => on_off(cfg.show_footer),
            Row::DimFooter => on_off(cfg.dim_footer),
            Row::FooterAlign => tr(match cfg.footer_align {
                Alignment::Left => Text::Left,
                Alignment::Center => Text::Center,
                Alignment::Right => Text::Right,
            })
            .to_string(),
            Row::ChapterTitle => on_off(cfg.show_chapter_title),
            Row::ProgressMode => tr(match cfg.progress_mode {
                ProgressMode::Chapter => Text::Chapter,
                ProgressMode::Overall => Text::Overall,
            })
            .to_string(),
            Row::ProgressBar => on_off(cfg.show_progress_bar),
            Row::BarLength => cfg.progress_bar_length.to_string(),
            Row::ProgressPercent => on_off(cfg.show_progress_percentage),
            Row::ChapterLoc => on_off(cfg.show_chapter_location),
        }
    }

    /// Moves the setting one step, forward (l, Right) or back (h, Left).
    /// Returns true when the chapter has to be laid out again.
    pub fn step(self, cfg: &mut Config, forward: bool) -> bool {
        // Numbers move by one within their range; true if the value changed
        let nudge = |value: &mut usize, min: usize, max: usize| {
            let old = *value;
            *value = if forward {
                old + 1
            } else {
                old.saturating_sub(1)
            }
            .clamp(min, max);
            *value != old
        };
        match self {
            Row::MaxWidth => nudge(&mut cfg.max_width, 20, 200),
            Row::MarginLeft => nudge(&mut cfg.margin_left, 0, 40),
            Row::MarginRight => nudge(&mut cfg.margin_right, 0, 40),
            Row::ScrollLines => {
                nudge(&mut cfg.scroll_by_lines, 1, 50);
                false
            }
            // Half-block pictures blend into the theme's background
            Row::Theme => {
                cfg.theme = if forward {
                    cfg.theme.next()
                } else {
                    cfg.theme.prev()
                };
                true
            }
            // [Image] labels are part of the laid-out text
            Row::Language => {
                cfg.language = if forward {
                    cfg.language.next()
                } else {
                    cfg.language.prev()
                };
                i18n::set(cfg.language);
                true
            }
            Row::Images => {
                cfg.images = if forward {
                    cfg.images.next()
                } else {
                    cfg.images.prev()
                };
                true
            }
            Row::PlainStyles => {
                cfg.plain_styles = !cfg.plain_styles;
                true
            }
            Row::ShowFooter => {
                cfg.show_footer = !cfg.show_footer;
                false
            }
            Row::DimFooter => {
                cfg.dim_footer = !cfg.dim_footer;
                false
            }
            Row::FooterAlign => {
                cfg.footer_align = match (cfg.footer_align, forward) {
                    (Alignment::Left, true) | (Alignment::Right, false) => Alignment::Center,
                    (Alignment::Center, true) | (Alignment::Left, false) => Alignment::Right,
                    (Alignment::Right, true) | (Alignment::Center, false) => Alignment::Left,
                };
                false
            }
            Row::ChapterTitle => {
                cfg.show_chapter_title = !cfg.show_chapter_title;
                false
            }
            Row::ProgressMode => {
                cfg.progress_mode = match cfg.progress_mode {
                    ProgressMode::Chapter => ProgressMode::Overall,
                    ProgressMode::Overall => ProgressMode::Chapter,
                };
                false
            }
            Row::ProgressBar => {
                cfg.show_progress_bar = !cfg.show_progress_bar;
                false
            }
            Row::BarLength => {
                nudge(&mut cfg.progress_bar_length, 5, 100);
                false
            }
            Row::ProgressPercent => {
                cfg.show_progress_percentage = !cfg.show_progress_percentage;
                false
            }
            Row::ChapterLoc => {
                cfg.show_chapter_location = !cfg.show_chapter_location;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    /// Picks one numeric setting out of the config.
    type Field = fn(&mut Config) -> &mut usize;

    #[test]
    fn numbers_stay_in_their_range() {
        let mut cfg = Config::default();
        let cases: [(Row, Field, usize, usize); 5] = [
            (Row::MaxWidth, |c| &mut c.max_width, 20, 200),
            (Row::MarginLeft, |c| &mut c.margin_left, 0, 40),
            (Row::MarginRight, |c| &mut c.margin_right, 0, 40),
            (Row::ScrollLines, |c| &mut c.scroll_by_lines, 1, 50),
            (Row::BarLength, |c| &mut c.progress_bar_length, 5, 100),
        ];
        for (row, field, min, max) in cases {
            *field(&mut cfg) = min;
            row.step(&mut cfg, false);
            assert_eq!(*field(&mut cfg), min, "{row:?}");
            *field(&mut cfg) = max;
            row.step(&mut cfg, true);
            assert_eq!(*field(&mut cfg), max, "{row:?}");
            row.step(&mut cfg, false);
            assert_eq!(*field(&mut cfg), max - 1, "{row:?}");
        }
    }

    #[test]
    fn choices_come_back_around() {
        let mut cfg = Config::default();
        let start = cfg.clone();
        let same = |a: &Config, b: &Config| {
            serde_json::to_string(a).unwrap() == serde_json::to_string(b).unwrap()
        };
        for (row, choices) in [
            (Row::Theme, 11),
            (Row::Language, 10),
            (Row::Images, 4),
            (Row::FooterAlign, 3),
            (Row::PlainStyles, 2),
            (Row::ProgressMode, 2),
            (Row::ShowFooter, 2),
        ] {
            for _ in 0..choices {
                row.step(&mut cfg, true);
            }
            assert!(same(&cfg, &start), "{row:?} forward");
            row.step(&mut cfg, true);
            row.step(&mut cfg, false);
            assert!(same(&cfg, &start), "{row:?} forward then back");
        }
        i18n::set(Language::English);
    }

    #[test]
    fn only_layout_changes_ask_for_a_new_layout() {
        for row in ROWS {
            let mut cfg = Config::default();
            let expected = matches!(
                row,
                Row::MaxWidth
                    | Row::MarginLeft
                    | Row::MarginRight
                    | Row::Theme
                    | Row::Language
                    | Row::Images
                    | Row::PlainStyles
            );
            assert_eq!(row.step(&mut cfg, true), expected, "{row:?}");
        }
        i18n::set(Language::English);
    }

    #[test]
    fn rows_show_their_settings() {
        let mut cfg = Config::default();
        for row in ROWS {
            assert!(
                !row.label().is_empty() && !row.value(&cfg).is_empty(),
                "{row:?}"
            );
        }
        cfg.plain_styles = true;
        cfg.footer_align = Alignment::Right;
        assert_eq!(Row::PlainStyles.value(&cfg), "On");
        assert_eq!(Row::FooterAlign.value(&cfg), "Right");
        assert_eq!(Row::Theme.value(&cfg), "Terminal");
    }
}
