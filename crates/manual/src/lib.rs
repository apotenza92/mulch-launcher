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

fn store_in(data_dir: &Path) -> PathBuf {
    data_dir.join("manual-games.json")
}

fn data_dir() -> io::Result<PathBuf> {
    mulch_core::paths::data_dir().ok_or_else(|| io::Error::other("can't find MulchLauncher's folder"))
}

pub fn load() -> Vec<ManualGame> {
    data_dir().map(|dir| load_from(&dir)).unwrap_or_default()
}

fn load_from(data_dir: &Path) -> Vec<ManualGame> {
    fs::read_to_string(store_in(data_dir))
        .ok()
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

fn save(games: &[ManualGame]) -> io::Result<()> {
    save_in(&data_dir()?, games)
}

fn save_in(data_dir: &Path, games: &[ManualGame]) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    fs::write(store_in(data_dir), serde_json::to_string_pretty(games)?)
}

/// Adds games (exe, name) to the store in another copy's `data_dir`: used by
/// the installer, which runs from Downloads, for the installed copy.
pub fn add_all_in(data_dir: &Path, new: impl IntoIterator<Item = (PathBuf, String)>) -> io::Result<()> {
    let mut games = load_from(data_dir);
    for (exe, name) in new {
        if !games.iter().any(|g| g.exe == exe) {
            games.push(ManualGame { name, exe, args: Vec::new() });
        }
    }
    save_in(data_dir, &games)
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
