use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub max_width: usize,
    pub margin_left: usize,
    pub margin_right: usize,
    pub show_footer: bool,
    pub footer_align: Alignment,
    pub scroll_by_lines: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_width: 80,
            margin_left: 4,
            margin_right: 4,
            show_footer: true,
            footer_align: Alignment::Center,
            scroll_by_lines: 2, // Changed to 2 as requested
        }
    }
}

pub fn load_or_create_config() -> Config {
    let config_path = "reader_config.json";
    
    if let Ok(file_content) = std::fs::read_to_string(config_path) {
        if let Ok(config) = serde_json::from_str(&file_content) {
            return config;
        }
    }
    
    let default_config = Config::default();
    if let Ok(json) = serde_json::to_string_pretty(&default_config) {
        let _ = std::fs::write(config_path, json);
    }
    default_config
}