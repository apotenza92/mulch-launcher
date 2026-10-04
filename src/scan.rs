//! Runs every game library (one crate each, see `crates/`) in parallel and
//! merges what they find.

pub use mulch_core::{Action, Art, Game, Launcher, Library, Platform, ScanContext};
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Every library, one per launcher. Adding a launcher means adding a crate
/// that implements [`Library`] and listing it here.
pub fn libraries() -> Vec<Box<dyn Library>> {
    vec![
        Box::new(mulch_steam::Steam),
        Box::new(mulch_epic::Epic),
        Box::new(mulch_ubisoft::Ubisoft),
        Box::new(mulch_gog::Gog),
        Box::new(mulch_xbox::Xbox),
        Box::new(mulch_battlenet::BattleNet),
        Box::new(mulch_rockstar::Rockstar),
        Box::new(mulch_ea::Ea),
        Box::new(mulch_manual::Manual),
    ]
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub games: Vec<Game>,
    pub launchers: Vec<Launcher>,
    /// Installed chat apps (Discord, WhatsApp, ...).
    pub social: Vec<Launcher>,
    /// How long each library took, for keeping startup fast.
    pub timings: Vec<(&'static str, Duration)>,
    pub total: Duration,
}

/// Scans every library (or just those whose id is in `only`) in parallel.
/// Games come back alphabetical; the app re-sorts by recency.
pub fn scan_all(only: &[String]) -> ScanResult {
    let start = Instant::now();
    let cx = ScanContext::new();
    let libraries: Vec<Box<dyn Library>> =
        libraries().into_iter().filter(|l| only.is_empty() || only.iter().any(|id| id == l.id())).collect();

    let (results, social): (Vec<_>, Vec<Launcher>) = std::thread::scope(|scope| {
        let social = scope.spawn(|| mulch_social::detect(&cx));
        let handles: Vec<_> = libraries
            .iter()
            .map(|library| {
                let cx = &cx;
                scope.spawn(move || {
                    let started = Instant::now();
                    let launcher = library.launcher(cx);
                    let games = library.games(cx);
                    (library.id(), launcher, games, started.elapsed())
                })
            })
            .collect();
        let results = handles.into_iter().filter_map(|h| h.join().ok()).collect();
        (results, social.join().unwrap_or_default())
    });

    let mut games = Vec::new();
    let mut launchers = Vec::new();
    let mut timings = Vec::new();
    for (id, launcher, found, took) in results {
        launchers.extend(launcher);
        games.extend(found);
        timings.push((id, took));
    }
    games.sort_by_key(|g| g.name.to_lowercase());
    launchers.sort_by_key(|l| l.name.to_lowercase());
    ScanResult { games, launchers, social, timings, total: start.elapsed() }
}

/// Folders the game finder should ignore: every known game's folders and
/// every launcher's and chat app's folder.
pub fn known_folders<'a>(games: &[Game], apps: impl IntoIterator<Item = &'a Launcher>) -> Vec<PathBuf> {
    let app_dirs = apps.into_iter().filter_map(|l| match &l.open {
        Action::Exe { path, .. } => path.parent().map(PathBuf::from),
        _ => None,
    });
    games.iter().flat_map(|g| g.process_dirs.iter().cloned()).chain(app_dirs).collect()
}
