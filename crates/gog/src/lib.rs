//! GOG: every installed game (Galaxy or offline installer) registers itself
//! under `GOG.com\Games\<id>` with its folder and executable.

use mulch_core::registry::{self, HKEY_LOCAL_MACHINE};
use mulch_core::{Action, Game, Launcher, Library, Platform, ScanContext};

const GAMES: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";

pub struct Gog;

impl Library for Gog {
    fn id(&self) -> &'static str {
        "gog"
    }

    fn launcher(&self, cx: &ScanContext) -> Option<Launcher> {
        let exe = registry::launcher_exe(cx.uninstall_entries(), &["goggalaxy"], Some("GOG GALAXY"))?;
        Some(Launcher::from_exe(Platform::Gog, "GOG Galaxy", exe))
    }

    fn games(&self, cx: &ScanContext) -> Vec<Game> {
        let galaxy = registry::launcher_exe(cx.uninstall_entries(), &["goggalaxy"], Some("GOG GALAXY"));
        let mut games = Vec::new();
        for game_id in registry::subkeys(HKEY_LOCAL_MACHINE, GAMES) {
            let key = format!(r"{GAMES}\{game_id}");
            let value = |name: &str| registry::string(HKEY_LOCAL_MACHINE, &key, name);
            let (Some(name), Some(path)) = (value("gameName"), value("path")) else { continue };
            let install_dir = registry::clean_path(&path);
            if !install_dir.is_dir() {
                continue;
            }
            let exe = value("exe").map(|e| registry::clean_path(&e)).filter(|e| e.is_file());

            // Start through GOG Galaxy when it's installed (keeps overlay and
            // cloud saves working; same command Playnite uses), otherwise run
            // the game directly.
            let launch = match (&galaxy, &exe) {
                (Some(galaxy), _) => Action::Exe {
                    path: galaxy.clone(),
                    args: vec![
                        "/launchViaAutostart".into(),
                        format!("/gameId={game_id}"),
                        "/command=runGame".into(),
                        format!("/path={}", install_dir.display()),
                    ],
                    working_dir: None,
                },
                (None, Some(exe)) => Action::Exe {
                    path: exe.clone(),
                    args: value("launchParam").map(|p| vec![p]).unwrap_or_default(),
                    working_dir: value("workingDir").map(|d| registry::clean_path(&d)),
                },
                (None, None) => continue,
            };

            let mut game = Game::new(format!("gog:{game_id}"), name, Platform::Gog, Some(install_dir), launch);
            game.icon_source = exe;
            if galaxy.is_some() {
                game.show_in_launcher = Some(Action::Uri(format!("goggalaxy://openGameView/{game_id}")));
            }
            games.push(game);
        }
        games
    }
}
