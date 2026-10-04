//! Rockstar Games Launcher: each installed title has a Windows uninstall entry
//! whose command ends in `uninstall=<title id>`. Title ids from Playnite's
//! Rockstar library.

use mulch_core::registry::{self, UninstallEntry};
use mulch_core::{Action, Game, Launcher, Library, Platform, ScanContext};
use std::path::PathBuf;

const TITLES: &[(&str, &str)] = &[
    ("gta5", "Grand Theft Auto V Legacy"),
    ("gta5_gen9", "Grand Theft Auto V Enhanced"),
    ("rdr", "Red Dead Redemption"),
    ("rdr2", "Red Dead Redemption 2"),
    ("lanoire", "L.A. Noire"),
    ("lanoirevr", "L.A. Noire: The VR Case Files"),
    ("mp3", "Max Payne 3"),
    ("gta3", "Grand Theft Auto III"),
    ("gtavc", "Grand Theft Auto: Vice City"),
    ("gtasa", "Grand Theft Auto: San Andreas"),
    ("gta3unreal", "Grand Theft Auto III – The Definitive Edition"),
    ("gtavcunreal", "Grand Theft Auto: Vice City – The Definitive Edition"),
    ("gtasaunreal", "Grand Theft Auto: San Andreas – The Definitive Edition"),
    ("bully", "Bully: Scholarship Edition"),
    ("gta4", "Grand Theft Auto IV"),
];

pub struct Rockstar;

impl Library for Rockstar {
    fn id(&self) -> &'static str {
        "rockstar"
    }

    fn launcher(&self, cx: &ScanContext) -> Option<Launcher> {
        Some(Launcher::from_exe(Platform::Rockstar, "Rockstar Games", launcher_exe(cx.uninstall_entries())?))
    }

    fn games(&self, cx: &ScanContext) -> Vec<Game> {
        scan(cx.uninstall_entries())
    }
}

fn scan(entries: &[UninstallEntry]) -> Vec<Game> {
    let Some(launcher) = launcher_exe(entries) else { return Vec::new() };
    let mut games = Vec::new();

    for entry in entries {
        let Some(title_id) = title_id(&entry.uninstall_string) else { continue };
        let Some(&(_, name)) = TITLES.iter().find(|(id, _)| id.eq_ignore_ascii_case(&title_id)) else { continue };
        let install_dir = registry::clean_path(&entry.install_location);
        if !install_dir.is_dir() {
            continue;
        }
        let launch = Action::Exe {
            path: launcher.clone(),
            args: vec!["-launchTitleInFolder".into(), install_dir.display().to_string()],
            working_dir: None,
        };
        let mut game = Game::new(format!("rockstar:{title_id}"), name.to_string(), Platform::Rockstar, Some(install_dir), launch);
        game.uninstall = Some(Action::Exe {
            path: launcher.clone(),
            args: vec!["-enableFullMode".into(), format!("-uninstall={title_id}")],
            working_dir: None,
        });
        // No documented link to a game's page, so this opens the launcher.
        game.show_in_launcher = Some(Action::Exe { path: launcher.clone(), args: Vec::new(), working_dir: None });
        game.icon_source = registry::icon_path(&entry.display_icon);
        games.push(game);
    }
    games
}

/// `<title id>` from an uninstall command ending in `uninstall=<title id>`.
fn title_id(uninstall_string: &str) -> Option<String> {
    let lower = uninstall_string.to_ascii_lowercase();
    if !(lower.contains("launcher.exe") || lower.contains("uninstall.exe")) {
        return None;
    }
    let start = lower.rfind("uninstall=")? + "uninstall=".len();
    let id = uninstall_string[start..].trim().trim_matches('"');
    (!id.is_empty()).then(|| id.to_string())
}

fn launcher_exe(entries: &[UninstallEntry]) -> Option<PathBuf> {
    entries
        .iter()
        .find(|e| e.display_name == "Rockstar Games Launcher")
        .map(|e| registry::clean_path(&e.install_location).join("Launcher.exe"))
        .filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_title_id() {
        let cmd = r#""C:\Program Files\Rockstar Games\Launcher\uninstall.exe" -uninstall=rdr2"#;
        assert_eq!(title_id(cmd).as_deref(), Some("rdr2"));
    }
}
