use serde::{Deserialize, Serialize};

use crate::i18n::{Language, Text, tr};
use crate::images;
use crate::paths;

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ProgressMode {
    Chapter,
    Overall,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Default,
    Sepia,
    Dracula,
    Hacker,
    Nord,
    SolarizedLight,
    SolarizedDark,
    Gruvbox,
    Monokai,
    Catppuccin,
    Oceanic,
}

const THEMES: [Theme; 11] = [
    Theme::Default,
    Theme::Sepia,
    Theme::Dracula,
    Theme::Hacker,
    Theme::Nord,
    Theme::SolarizedLight,
    Theme::SolarizedDark,
    Theme::Gruvbox,
    Theme::Monokai,
    Theme::Catppuccin,
    Theme::Oceanic,
];

impl Theme {
    pub fn next(self) -> Self {
        THEMES[(self.index() + 1) % THEMES.len()]
    }

    pub fn prev(self) -> Self {
        THEMES[(self.index() + THEMES.len() - 1) % THEMES.len()]
    }

    fn index(self) -> usize {
        THEMES.iter().position(|&t| t == self).unwrap_or(0)
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::Default => tr(Text::TerminalTheme),
            Theme::Sepia => "Sepia",
            Theme::Dracula => "Dracula",
            Theme::Hacker => "Hacker",
            Theme::Nord => "Nord",
            Theme::SolarizedLight => "Sol Light",
            Theme::SolarizedDark => "Sol Dark",
            Theme::Gruvbox => "Gruvbox",
            Theme::Monokai => "Monokai",
            Theme::Catppuccin => "Catppuccin",
            Theme::Oceanic => "Oceanic",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
// Settings missing from an older file take their defaults instead of discarding the file
#[serde(default)]
pub struct Config {
    pub max_width: usize,
    pub margin_left: usize,
    pub margin_right: usize,
    pub scroll_by_lines: usize,
    pub theme: Theme,
    pub show_footer: bool,
    pub dim_footer: bool,
    pub footer_align: Alignment,
    pub show_chapter_title: bool,
    pub show_chapter_location: bool,
    pub show_progress_bar: bool,
    pub show_progress_percentage: bool,
    pub progress_bar_length: usize,
    pub progress_mode: ProgressMode,
    pub language: Language,
    pub images: images::Setting,
    /// Ignore the book's stylesheets and use the reader's own plain formatting
    pub plain_styles: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_width: 80,
            margin_left: 4,
            margin_right: 4,
            scroll_by_lines: 2,
            theme: Theme::Default,
            show_footer: true,
            dim_footer: true,
            footer_align: Alignment::Center,
            show_chapter_title: true,
            show_chapter_location: true,
            show_progress_bar: true,
            show_progress_percentage: true,
            progress_bar_length: 10,
            progress_mode: ProgressMode::Overall,
            language: Language::Auto,
            images: images::Setting::Auto,
            plain_styles: false,
        }
    }
}

pub fn load_or_create_config() -> Config {
    let config_path = paths::config_file();
    if let Some(file_content) = paths::read_with_legacy(&config_path, "reader_config.json")
        && let Ok(config) = serde_json::from_str(&file_content)
    {
        // Rewrites a config carried over from the legacy location into the new one
        if !config_path.exists() {
            save_config(&config);
        }
        return config;
    }
    let default_config = Config::default();
    save_config(&default_config);
    default_config
}

/// The configured interface language, read without creating a settings file.
pub fn peek_language() -> Language {
    paths::read_with_legacy(&paths::config_file(), "reader_config.json")
        .and_then(|text| serde_json::from_str::<Config>(&text).ok())
        .map_or(Language::Auto, |cfg| cfg.language)
}

pub fn save_config(cfg: &Config) {
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = paths::write_atomic(&paths::config_file(), &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_keep_the_rest() {
        let cfg: Config =
            serde_json::from_str(r#"{ "max_width": 108, "theme": "dracula" }"#).unwrap();
        assert_eq!(cfg.max_width, 108);
        assert!(cfg.theme == Theme::Dracula);
        assert_eq!(cfg.margin_left, Config::default().margin_left);
    }
}
