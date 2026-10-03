//! Games the user added by pointing at an executable. Stored as JSON in
//! `%APPDATA%\MulchLauncher\manual-games.json`.

use super::{Action, Game, Platform};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManualGame {
    pub name: String,
    pub exe: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}

fn store_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("MulchLauncher").join("manual-games.json"))
}

pub fn load() -> Vec<ManualGame> {
    store_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(games: &[ManualGame]) -> io::Result<()> {
    let path = store_path().ok_or_else(|| io::Error::other("APPDATA is not set"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string_pretty(games)?)
}

/// Adds an executable, named after its file. Returns false if it was already there.
pub fn add(exe: &Path) -> io::Result<bool> {
    let mut games = load();
    if games.iter().any(|g| g.exe == exe) {
        return Ok(false);
    }
    let name = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Game".into());
    games.push(ManualGame { name, exe: exe.to_path_buf(), args: Vec::new() });
    save(&games)?;
    Ok(true)
}

pub fn remove(exe: &Path) -> io::Result<()> {
    let mut games = load();
    games.retain(|g| g.exe != exe);
    save(&games)
}

/// Manual games whose executable still exists. Missing ones are skipped (not
/// deleted), so a game on an unplugged drive comes back when the drive does.
pub fn scan() -> Vec<Game> {
    load()
        .into_iter()
        .filter(|g| g.exe.is_file())
        .map(|g| Game {
            id: format!("manual:{}", g.exe.display()),
            name: g.name,
            platform: Platform::Manual,
            install_dir: g.exe.parent().map(Path::to_path_buf),
            launch: Action::Exe {
                working_dir: g.exe.parent().map(Path::to_path_buf),
                path: g.exe.clone(),
                args: g.args,
            },
            uninstall: None,
            art: None,
            icon_source: Some(g.exe.clone()),
        })
        .collect()
}
