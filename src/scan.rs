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
    /// Installed chat apps (Discord, ...).
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
    let mut games = merge_copies(games);
    games.sort_by_key(|g| g.name.to_lowercase());
    launchers.sort_by_key(|l| l.name.to_lowercase());
    ScanResult { games, launchers, social, timings, total: start.elapsed() }
}

/// One tile per game, even when two launchers both installed it: a Steam or
/// Epic copy of a Ubisoft or EA game, say, which starts through that
/// publisher's launcher, which lists the game as its own as well. The copy
/// kept is the one its launcher says was played last, or else the one from
/// the store it was most likely bought in (`store_rank`). Playing either copy
/// counts, as the kept one watches both folders, and the tile shows each
/// launcher's button.
pub fn merge_copies(games: Vec<Game>) -> Vec<Game> {
    let mut kept: Vec<Game> = Vec::new();
    for game in games {
        let same = (game.platform != Platform::Manual)
            .then(|| kept.iter().position(|k| k.platform != Platform::Manual && same_title(&k.name, &game.name)))
            .flatten();
        let Some(ix) = same else {
            kept.push(game);
            continue;
        };
        let other = &kept[ix];
        let prefer_new = match (game.last_played, other.last_played) {
            (Some(a), Some(b)) if a != b => a > b,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            _ => store_rank(game.platform) < store_rank(other.platform),
        };
        let (mut keep, drop) = if prefer_new { (game, kept.remove(ix)) } else { (kept.remove(ix), game) };
        keep.last_played = keep.last_played.max(drop.last_played);
        keep.other_copies.push((drop.platform, drop.show_in_launcher));
        keep.other_copies.extend(drop.other_copies);
        for dir in drop.process_dirs.into_iter().chain(drop.install_dir) {
            if !keep.process_dirs.contains(&dir) {
                keep.process_dirs.push(dir);
            }
        }
        if keep.art.is_none() {
            keep.art = drop.art;
        }
        kept.insert(ix.min(kept.len()), keep);
    }
    kept
}

/// Stores first, then the publishers' own launchers that other stores start
/// their games through.
fn store_rank(platform: Platform) -> u8 {
    match platform {
        Platform::Steam => 0,
        Platform::Epic => 1,
        Platform::Gog => 2,
        Platform::Xbox => 3,
        Platform::BattleNet => 4,
        Platform::Ea => 5,
        Platform::Rockstar => 6,
        Platform::Ubisoft => 7,
        Platform::Manual => 8,
    }
}

/// The same title, ignoring case, symbols like ™ and spacing.
fn same_title(a: &str, b: &str) -> bool {
    let key = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let (a, b) = (key(a), key(b));
    !a.is_empty() && a == b
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

#[cfg(test)]
mod tests {
    use super::*;

    fn game(platform: Platform, name: &str, dir: &str, last_played: Option<u64>) -> Game {
        let mut game = Game::new(
            format!("{platform:?}:{name}"),
            name.into(),
            platform,
            Some(dir.into()),
            Action::Uri("x:".into()),
        );
        game.last_played = last_played;
        game
    }

    #[test]
    fn one_tile_for_a_game_two_launchers_installed() {
        let games = vec![
            game(Platform::Ubisoft, "Trackmania", r"C:\Ubisoft\Trackmania", None),
            game(Platform::Steam, "Trackmania", r"C:\Steam\Trackmania", None),
            game(Platform::Steam, "Halo", r"C:\Steam\Halo", None),
        ];
        let merged = merge_copies(games);
        assert_eq!(merged.len(), 2);
        let tm = merged.iter().find(|g| g.name == "Trackmania").unwrap();
        // Neither played: the store's copy, watching both folders.
        assert_eq!(tm.platform, Platform::Steam);
        assert_eq!(tm.process_dirs.len(), 2);
        assert_eq!(tm.other_copies.len(), 1);
        assert_eq!(tm.other_copies[0].0, Platform::Ubisoft);
    }

    #[test]
    fn keeps_the_copy_played_last_and_ignores_symbols() {
        let games = vec![
            game(Platform::Steam, "Rocket League", r"C:\a", Some(10)),
            game(Platform::Epic, "Rocket League®", r"C:\b", Some(20)),
        ];
        let merged = merge_copies(games);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].platform, Platform::Epic);
        assert_eq!(merged[0].last_played, Some(20));
    }

    #[test]
    fn games_added_by_hand_are_never_merged() {
        let games = vec![
            game(Platform::Manual, "Trackmania", r"C:\a", None),
            game(Platform::Steam, "Trackmania", r"C:\b", None),
        ];
        assert_eq!(merge_copies(games).len(), 2);
    }
}
