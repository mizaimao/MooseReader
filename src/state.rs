use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::paths;

#[derive(Serialize, Deserialize, Clone)]
pub struct Bookmark {
    pub chapter: usize,
    pub progress: f64, // Changed from offset to a percentage
}

#[derive(Serialize, Deserialize, Default)]
pub struct State {
    pub books: HashMap<String, Bookmark>,
}

pub fn load_state() -> State {
    let state_path = paths::bookmarks_file();

    if let Some(file_content) = paths::read_with_legacy(&state_path, "bookmarks.json")
        && let Ok(state) = serde_json::from_str(&file_content)
    {
        return state;
    }

    State::default()
}

pub fn save_state(state: &State) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = paths::write_atomic(&paths::bookmarks_file(), &json);
    }
}
