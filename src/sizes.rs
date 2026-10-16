//! How much disk each game takes: its install folder, measured in the
//! background and remembered between runs in `data\sizes.json`, so a size
//! shows at once and is brought up to date (after updates) each run.

use mulch_core::{Game, Platform};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Sizes {
    /// Game id -> bytes.
    bytes: HashMap<String, u64>,
}

fn path() -> Option<PathBuf> {
    mulch_core::paths::data_dir().map(|d| d.join("sizes.json"))
}

impl Sizes {
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
        if let Ok(text) = serde_json::to_string(self) {
            let _ = fs::write(path, text);
        }
    }

    pub fn get(&self, game_id: &str) -> Option<u64> {
        self.bytes.get(game_id).copied()
    }

    pub fn set(&mut self, game_id: &str, bytes: Option<u64>) {
        match bytes {
            Some(bytes) => self.bytes.insert(game_id.to_string(), bytes),
            None => self.bytes.remove(game_id),
        };
    }
}

/// The folder whose size is the game's. Games added by hand have only the
/// folder their program is in, which may hold much more than the game.
pub fn folder(game: &Game) -> Option<&Path> {
    if game.platform == Platform::Manual {
        return None;
    }
    game.install_dir.as_deref()
}

/// The total size of the files in a folder and everything in it, or None if
/// it can't be read (or is empty). Links and junctions aren't followed, so
/// nothing is counted twice.
pub fn measure(dir: &Path) -> Option<u64> {
    let mut total = 0;
    let mut pending = vec![fs::read_dir(dir).ok()?];
    while let Some(entries) = pending.pop() {
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                pending.extend(fs::read_dir(entry.path()).ok());
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    (total > 0).then_some(total)
}

/// A size as Windows shows it (1 KB = 1024 bytes): "812 KB", "4.2 GB",
/// "128 GB". One decimal below 100, and never "1010 MB" (that's "1 GB").
pub fn format(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    let mut value = (bytes as f64 / 1024.).max(1.);
    let mut unit = 0;
    while value >= 1000. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    let text = if unit == 0 || value >= 100. { format!("{value:.0}") } else { format!("{value:.1}") };
    format!("{} {}", text.trim_end_matches(".0"), UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_readably() {
        assert_eq!(format(200), "1 KB");
        assert_eq!(format(812 * 1024), "812 KB");
        assert_eq!(format(4_404_019), "4.2 MB");
        assert_eq!(format(1000 * 1024 * 1024), "1 GB");
        assert_eq!(format(41_339_633_664), "38.5 GB");
        assert_eq!(format(137_438_953_472), "128 GB");
        assert_eq!(format(1_319_413_953_331), "1.2 TB");
    }

    #[test]
    fn measures_nested_files() {
        let dir = std::env::temp_dir().join(format!("mulch-sizes-{}", std::process::id()));
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("one"), vec![0; 100]).unwrap();
        fs::write(dir.join("a/b/two"), vec![0; 50]).unwrap();
        assert_eq!(measure(&dir), Some(150));
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(measure(&dir), None);
    }
}
