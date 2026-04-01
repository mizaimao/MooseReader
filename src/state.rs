use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct Bookmark {
    pub chapter: usize,
    pub offset: usize,
}

#[derive(Serialize, Deserialize, Default)]
pub struct State {
    // Maps the absolute file path of a book to its saved bookmark
    pub books: HashMap<String, Bookmark>,
}

pub fn load_state() -> State {
    let state_path = "bookmarks.json";
    
    if let Ok(file_content) = fs::read_to_string(state_path) {
        if let Ok(state) = serde_json::from_str(&file_content) {
            return state;
        }
    }
    
    State::default()
}

pub fn save_state(state: &State) {
    let state_path = "bookmarks.json";
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(state_path, json);
    }
}
