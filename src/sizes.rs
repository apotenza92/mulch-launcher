//! How much disk each game takes: its own folder, measured in the
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

/// The folder whose size is the game's: its install folder, or for a game
/// added by hand, the folder its program is in (see `game_root`). None when
/// that folder is shared: a drive, a Windows folder, or one holding another
/// game.
pub fn folder(game: &Game, games: &[Game]) -> Option<PathBuf> {
    let dir = game.install_dir.as_deref()?;
    if game.platform != Platform::Manual {
        return Some(dir.to_path_buf());
    }
    let root = game_root(dir);
    let shared = root.parent().is_none()
        || shared_folders().iter().any(|shared| same_folder(shared, &root))
        || games
            .iter()
            .filter(|g| g.id != game.id)
            .filter_map(|g| g.install_dir.as_deref())
            .any(|other| lowercase(other).starts_with(lowercase(&root)));
    (!shared).then_some(root)
}

/// A game's own folder, from the folder its program is in: above the folders
/// games keep programs in (`bin`, `Binaries\Win64`, ...), and for Unreal
/// games, above the project folder too (to the one that has `Engine`).
pub fn game_root(program_dir: &Path) -> PathBuf {
    const PROGRAM_FOLDERS: [&str; 9] = ["bin", "bin32", "bin64", "binaries", "win32", "win64", "x64", "x86", "x86_64"];
    let mut root = program_dir;
    while let (Some(name), Some(parent)) = (root.file_name(), root.parent()) {
        if !PROGRAM_FOLDERS.contains(&name.to_string_lossy().to_lowercase().as_str()) {
            break;
        }
        root = parent;
    }
    if root != program_dir
        && let Some(parent) = root.parent()
        && parent.join("Engine").is_dir()
        && parent.parent().is_some()
    {
        root = parent;
    }
    root.to_path_buf()
}

/// Windows' own folders, which hold much more than one game.
fn shared_folders() -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut folders: Vec<PathBuf> = [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "SystemRoot",
        "USERPROFILE",
        "PUBLIC",
        "LOCALAPPDATA",
        "APPDATA",
    ]
    .into_iter()
    .filter_map(var)
    .collect();
    if let Some(home) = var("USERPROFILE") {
        folders.extend(["Desktop", "Downloads", "Documents", "OneDrive"].map(|f| home.join(f)));
    }
    folders
}

fn lowercase(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

/// Windows paths ignore case.
fn same_folder(a: &Path, b: &Path) -> bool {
    lowercase(a) == lowercase(b)
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

    fn manual(id: &str, dir: &Path) -> Game {
        let launch = mulch_core::Action::Uri(String::new());
        Game::new(id.into(), id.into(), Platform::Manual, Some(dir.to_path_buf()), launch)
    }

    #[test]
    fn finds_a_games_own_folder() {
        let dir = std::env::temp_dir().join(format!("mulch-roots-{}", std::process::id()));
        let unreal = dir.join("Unreal Game");
        fs::create_dir_all(unreal.join("Engine")).unwrap();
        fs::create_dir_all(unreal.join("Project/Binaries/Win64")).unwrap();
        assert_eq!(game_root(&unreal.join("Project/Binaries/Win64")), unreal);
        assert_eq!(game_root(&dir.join("Other/bin/x64")), dir.join("Other"));
        assert_eq!(game_root(&dir.join("Plain")), dir.join("Plain"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn skips_shared_folders() {
        let games = vec![manual("a", Path::new(r"D:\Games")), manual("b", Path::new(r"D:\Games\Other"))];
        assert_eq!(folder(&games[0], &games), None);
        assert_eq!(folder(&games[1], &games), Some(PathBuf::from(r"D:\Games\Other")));
        assert_eq!(folder(&manual("c", Path::new(r"D:\")), &games), None);
        let downloads = PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join("downloads");
        assert_eq!(folder(&manual("d", &downloads), &games), None);
        let steam = Game::new(
            "s".into(),
            "s".into(),
            Platform::Steam,
            Some(r"D:\Games".into()),
            mulch_core::Action::Uri(String::new()),
        );
        assert_eq!(folder(&steam, &games), Some(PathBuf::from(r"D:\Games")));
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
