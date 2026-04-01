use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Alignment { Left, Center, Right }

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ProgressMode { Chapter, Overall }

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Theme { Default, Sepia, Dracula, Hacker, Nord, SolarizedLight, SolarizedDark, Gruvbox, Monokai, Catppuccin, Oceanic }

#[derive(Serialize, Deserialize, Clone)]
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
        }
    }
}

pub fn load_or_create_config() -> Config {
    let config_path = "reader_config.json";
    if let Ok(file_content) = std::fs::read_to_string(config_path) {
        if let Ok(config) = serde_json::from_str(&file_content) { return config; }
    }
    let default_config = Config::default();
    if let Ok(json) = serde_json::to_string_pretty(&default_config) {
        let _ = std::fs::write(config_path, json);
    }
    default_config
}

pub fn save_config(cfg: &Config) {
    let config_path = "reader_config.json";
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(config_path, json);
    }
}