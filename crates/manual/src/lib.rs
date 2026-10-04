//! Games the user added by pointing at an executable. Stored as JSON in
//! `data\manual-games.json` next to MulchLauncher.exe.

use mulch_core::{Action, Game, Launcher, Library, Platform, ScanContext};
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
    mulch_core::paths::data_dir().map(|d| d.join("manual-games.json"))
}

pub fn load() -> Vec<ManualGame> {
    store_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(games: &[ManualGame]) -> io::Result<()> {
    let path = store_path().ok_or_else(|| io::Error::other("can't find MulchLauncher's folder"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string_pretty(games)?)
}

/// Adds an executable, named after its file. Returns false if it was already there.
pub fn add(exe: &Path) -> io::Result<bool> {
    let name = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Game".into());
    add_named(exe, name)
}

/// Adds an executable under the given name. Returns false if it was already there.
pub fn add_named(exe: &Path, name: String) -> io::Result<bool> {
    let mut games = load();
    if games.iter().any(|g| g.exe == exe) {
        return Ok(false);
    }
    games.push(ManualGame { name, exe: exe.to_path_buf(), args: Vec::new() });
    save(&games)?;
    Ok(true)
}

pub fn remove(exe: &Path) -> io::Result<()> {
    let mut games = load();
    games.retain(|g| g.exe != exe);
    save(&games)
}

/// Games the user added. Executables that are missing (e.g. on an unplugged
/// drive) are skipped, not deleted, so they come back when the drive does.
pub struct Manual;

impl Library for Manual {
    fn id(&self) -> &'static str {
        "manual"
    }

    fn launcher(&self, _: &ScanContext) -> Option<Launcher> {
        None
    }

    fn games(&self, _: &ScanContext) -> Vec<Game> {
        load()
            .into_iter()
            .filter(|g| g.exe.is_file())
            .map(|g| {
                let dir = g.exe.parent().map(Path::to_path_buf);
                let launch = Action::Exe { working_dir: dir.clone(), path: g.exe.clone(), args: g.args };
                let mut game = Game::new(format!("manual:{}", g.exe.display()), g.name, Platform::Manual, dir, launch);
                game.icon_source = Some(g.exe);
                game
            })
            .collect()
    }
}