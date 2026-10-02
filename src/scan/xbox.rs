//! Xbox app and Microsoft Store games, from Windows' package database. That
//! knows each package's real folder, so games on any drive (e.g. the
//! `XboxGames` folder the Xbox app creates per drive) are found.
//!
//! A package counts as a game when it ships an Xbox game config: GDK games
//! have `MicrosoftGame.config`, older Xbox Live UWP games `xboxservices.config`.
//! DLC and add-on "stub" packages also ship that config but declare no
//! launchable app, so requiring an app entry filters them out.

use super::{Action, Game, Platform};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use windows::Management::Deployment::PackageManager;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::HSTRING;

const GAME_CONFIGS: &[&str] = &["MicrosoftGame.config", "xboxservices.config"];

/// Xbox's own apps also ship Xbox Live configs but aren't games.
const NOT_GAMES: &[&str] = &[
    "Microsoft.GamingApp",
    "Microsoft.XboxApp",
    "Microsoft.XboxGamingOverlay",
    "Microsoft.XboxGameOverlay",
    "Microsoft.XboxIdentityProvider",
    "Microsoft.XboxSpeechToTextOverlay",
    "Microsoft.Xbox.TCUI",
    "Microsoft.GamingServices",
];

pub fn scan() -> Vec<Game> {
    // WinRT needs the thread initialised; already-initialised is fine.
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    scan_packages().unwrap_or_default()
}

/// The Xbox app installs games to `<drive>:\XboxGames\<game>\Content`, then
/// mounts them into the protected WindowsApps folder, which is what the
/// package API reports. Map package names to those visible folders by reading
/// each folder's manifest. Games in a custom folder keep their WindowsApps path.
fn xbox_games_folders() -> HashMap<String, PathBuf> {
    let mut folders = HashMap::new();
    for drive in 'A'..='Z' {
        let root = PathBuf::from(format!(r"{drive}:\XboxGames"));
        let Ok(entries) = fs::read_dir(&root) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let content = entry.path().join("Content");
            let Ok(manifest) = fs::read_to_string(content.join("appxmanifest.xml")) else { continue };
            if let Some(name) = manifest.find("<Identity").and_then(|i| attribute(&manifest[i..], "Name")) {
                folders.insert(name.to_string(), content);
            }
        }
    }
    folders
}

fn scan_packages() -> windows::core::Result<Vec<Game>> {
    let manager = PackageManager::new()?;
    let visible_folders = xbox_games_folders();
    let mut games = Vec::new();

    // An empty user id means "the current user".
    for package in manager.FindPackagesByUserSecurityId(&HSTRING::new())? {
        if package.IsFramework()? || package.IsResourcePackage()? || package.IsBundle()? || package.IsOptional()? {
            continue;
        }
        let name = package.Id()?.Name()?.to_string();
        if NOT_GAMES.contains(&name.as_str()) {
            continue;
        }
        let Ok(location) = package.InstalledPath() else { continue };
        let install_dir = PathBuf::from(location.to_string());
        let Some(config) = GAME_CONFIGS.iter().map(|c| install_dir.join(c)).find(|p| p.is_file()) else {
            continue;
        };

        let Some(app_id) = package
            .GetAppListEntries()?
            .into_iter()
            .find_map(|entry| entry.AppUserModelId().ok().map(|id| id.to_string()).filter(|id| !id.is_empty()))
        else {
            continue;
        };

        let visible_dir = visible_folders.get(&name).cloned().unwrap_or_else(|| install_dir.clone());

        games.push(Game {
            id: format!("xbox:{}", package.Id()?.FamilyName()?),
            name: package.DisplayName()?.to_string(),
            platform: Platform::Xbox,
            install_dir: Some(visible_dir),
            launch: Action::StoreApp(app_id),
            uninstall: None,
            art: logo(&install_dir, &config),
        });
    }
    Ok(games)
}

/// The largest square logo the game's config points at.
fn logo(install_dir: &Path, config: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(config).ok()?;
    ["Square480x480Logo", "Square150x150Logo", "StoreLogo"].iter().find_map(|attr| {
        let value = attribute(&text, attr)?;
        let path = install_dir.join(value.replace('/', "\\"));
        path.is_file().then_some(path)
    })
}

/// Value of `name="..."` in an XML document (enough for these configs).
fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.find(&format!("{name}=\""))? + name.len() + 2;
    let len = text[start..].find('"')?;
    Some(&text[start..start + len])
}
