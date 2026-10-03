//! Ubisoft Connect: installed games are listed in the registry by game id.

use super::registry::{self, HKEY_LOCAL_MACHINE};
use super::{Action, Game, Platform};
use std::path::Path;

const INSTALLS: &str = r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs";
const UNINSTALL: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

pub fn scan() -> Vec<Game> {
    let mut games = Vec::new();
    for game_id in registry::subkeys(HKEY_LOCAL_MACHINE, INSTALLS) {
        let Some(raw_dir) = registry::string(HKEY_LOCAL_MACHINE, &format!(r"{INSTALLS}\{game_id}"), "InstallDir")
        else {
            continue;
        };
        let install_dir = registry::clean_path(&raw_dir);
        if !install_dir.is_dir() {
            continue;
        }

        // Ubisoft also writes a normal Windows uninstall entry with the real name.
        let uninstall_key = format!(r"{UNINSTALL}\Uplay Install {game_id}");
        let name = registry::string(HKEY_LOCAL_MACHINE, &uninstall_key, "DisplayName")
            .unwrap_or_else(|| folder_name(&install_dir));

        games.push(Game {
            id: format!("ubisoft:{game_id}"),
            name,
            platform: Platform::Ubisoft,
            install_dir: Some(install_dir),
            launch: Action::Uri(format!("uplay://launch/{game_id}/0")),
            uninstall: Some(Action::Uri(format!("uplay://uninstall/{game_id}"))),
            art: None,
            icon_source: registry::string(HKEY_LOCAL_MACHINE, &uninstall_key, "DisplayIcon")
                .and_then(|icon| super::art::icon_path(&icon)),
        });
    }
    games
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
