//! The few things Mulch remembers, in `data\settings.json` next to MulchLauncher.exe.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Chosen tile size (an index into `layout::TILE_SIZES`); the middle one if unset.
    pub tile_size: Option<usize>,
}

fn path() -> Option<PathBuf> {
    mulch_core::paths::data_dir().map(|d| d.join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }
}
