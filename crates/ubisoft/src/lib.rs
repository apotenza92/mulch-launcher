//! Ubisoft Connect: installed games are listed in the registry by game id.
//! Ubisoft keeps play history in your online account only, so there's no
//! local last-played.

use mulch_core::registry::{self, HKEY_LOCAL_MACHINE};
use mulch_core::{Action, Game, Launcher, Library, Platform, ScanContext};
use std::path::Path;

const INSTALLS: &str = r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs";
const UNINSTALL: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

pub struct Ubisoft;

impl Library for Ubisoft {
    fn id(&self) -> &'static str {
        "ubisoft"
    }

    fn launcher(&self, cx: &ScanContext) -> Option<Launcher> {
        let exe = registry::launcher_exe(cx.uninstall_entries(), &["uplay"], Some("Ubisoft Connect"))?;
        Some(Launcher::from_exe(Platform::Ubisoft, "Ubisoft Connect", exe))
    }

    fn games(&self, cx: &ScanContext) -> Vec<Game> {
        // Ubisoft has no documented link to a game's page, so "show" opens the app.
        let show = self.launcher(cx).map(|l| l.open);
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

            let mut game = Game::new(
                format!("ubisoft:{game_id}"),
                name,
                Platform::Ubisoft,
                Some(install_dir),
                Action::Uri(format!("uplay://launch/{game_id}/0")),
            );
            game.uninstall = Some(Action::Uri(format!("uplay://uninstall/{game_id}")));
            game.show_in_launcher = show.clone();
            game.icon_source = registry::string(HKEY_LOCAL_MACHINE, &uninstall_key, "DisplayIcon")
                .and_then(|icon| registry::icon_path(&icon));
            games.push(game);
        }
        games
    }
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
