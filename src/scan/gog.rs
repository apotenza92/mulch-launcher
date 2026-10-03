//! GOG: every installed game (Galaxy or offline installer) registers itself
//! under `GOG.com\Games\<id>` with its folder and executable.

use super::registry::{self, HKEY_LOCAL_MACHINE};
use super::{Action, Game, Platform};
use std::path::PathBuf;

const GAMES: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";

pub fn scan() -> Vec<Game> {
    let mut games = Vec::new();
    for game_id in registry::subkeys(HKEY_LOCAL_MACHINE, GAMES) {
        let key = format!(r"{GAMES}\{game_id}");
        let value = |name: &str| registry::string(HKEY_LOCAL_MACHINE, &key, name);
        let (Some(name), Some(path)) = (value("gameName"), value("path")) else { continue };
        let install_dir = registry::clean_path(&path);
        if !install_dir.is_dir() {
            continue;
        }

        // Prefer starting through GOG Galaxy when it's installed (keeps
        // overlay and cloud saves working); otherwise run the game directly.
        let launch = if registry::protocol_handler_exe("goggalaxy").is_some() {
            Action::Uri(format!("goggalaxy://openGameView/{game_id}"))
        } else if let Some(exe) = value("exe").map(|e| PathBuf::from(e)).filter(|e| e.is_file()) {
            Action::Exe {
                path: exe,
                args: value("launchParam").map(|p| vec![p]).unwrap_or_default(),
                working_dir: value("workingDir").map(PathBuf::from),
            }
        } else {
            continue;
        };

        games.push(Game {
            id: format!("gog:{game_id}"),
            name,
            platform: Platform::Gog,
            install_dir: Some(install_dir),
            launch,
            uninstall: None,
            art: None,
            icon_source: value("exe").map(PathBuf::from),
        });
    }
    games
}
