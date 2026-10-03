//! Finds installed games and launchers.
//!
//! Every location comes from the launcher's own records (registry, manifests,
//! Windows' package database) so games on any drive or non-default install
//! folder are found. Default paths are only ever a last-resort fallback.

pub mod art;
mod battlenet;
mod epic;
mod gog;
mod launchers;
pub mod manual;
mod registry;
mod rockstar;
mod steam;
mod ubisoft;
mod vdf;
mod xbox;

use serde::Serialize;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

pub use launchers::Launcher;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Platform {
    Steam,
    Epic,
    Ubisoft,
    Gog,
    Xbox,
    Ea,
    BattleNet,
    Rockstar,
    Manual,
}

impl Platform {
    pub fn label(self) -> &'static str {
        match self {
            Platform::Steam => "Steam",
            Platform::Epic => "Epic",
            Platform::Ubisoft => "Ubisoft",
            Platform::Gog => "GOG",
            Platform::Xbox => "Xbox",
            Platform::Ea => "EA",
            Platform::BattleNet => "Battle.net",
            Platform::Rockstar => "Rockstar",
            Platform::Manual => "Added by you",
        }
    }
}

/// How to start (or uninstall) something.
#[derive(Clone, Debug, Serialize)]
pub enum Action {
    /// A URI handled by a launcher, e.g. `steam://rungameid/730`.
    Uri(String),
    /// An executable started directly.
    Exe { path: PathBuf, args: Vec<String>, working_dir: Option<PathBuf> },
    /// A Microsoft Store / Xbox app, by its AppUserModelId.
    StoreApp(String),
}

#[derive(Clone, Debug, Serialize)]
pub struct Game {
    /// Stable id, unique across platforms, e.g. `steam:730`.
    pub id: String,
    pub name: String,
    pub platform: Platform,
    pub install_dir: Option<PathBuf>,
    pub launch: Action,
    pub uninstall: Option<Action>,
    pub art: Option<Art>,
    /// An executable or .ico to take an icon from when there's no art.
    pub icon_source: Option<PathBuf>,
}

/// Local artwork for a tile.
#[derive(Clone, Debug, Serialize)]
pub enum Art {
    /// Portrait cover art that fills the tile.
    Cover(PathBuf),
    /// A square icon or logo, shown centred on the tile.
    Icon(PathBuf),
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub games: Vec<Game>,
    pub launchers: Vec<Launcher>,
    /// How long each scanner took, for keeping startup fast.
    pub timings: Vec<(String, Duration)>,
    pub total: Duration,
}

type Scanner = fn() -> Vec<Game>;

const SCANNERS: &[(&str, Scanner)] = &[
    ("steam", steam::scan),
    ("epic", epic::scan),
    ("ubisoft", ubisoft::scan),
    ("gog", gog::scan),
    ("xbox", xbox::scan),
    ("battlenet+rockstar", scan_uninstall_entries),
    ("manual", manual::scan),
];

/// Battle.net and Rockstar games are both found from Windows' uninstall
/// entries, so read those once for both.
fn scan_uninstall_entries() -> Vec<Game> {
    let entries = registry::uninstall_entries();
    let mut games = battlenet::scan(&entries);
    games.extend(rockstar::scan(&entries));
    games
}

/// Runs every scanner in parallel and merges the results.
pub fn scan_all() -> ScanResult {
    let start = Instant::now();

    let handles: Vec<_> = SCANNERS
        .iter()
        .map(|&(name, scanner)| {
            thread::spawn(move || {
                let started = Instant::now();
                let games = scanner();
                (name.to_string(), games, started.elapsed())
            })
        })
        .collect();
    let launchers_handle = thread::spawn(launchers::detect);

    let mut games = Vec::new();
    let mut timings = Vec::new();
    for handle in handles {
        if let Ok((name, found, took)) = handle.join() {
            games.extend(found);
            timings.push((name, took));
        }
    }
    let launchers = launchers_handle.join().unwrap_or_default();

    games.sort_by_key(|g| g.name.to_lowercase());
    ScanResult { games, launchers, timings, total: start.elapsed() }
}
