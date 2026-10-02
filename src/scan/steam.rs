//! Steam: library folders from `libraryfolders.vdf` (covers every drive),
//! installed games from each library's `appmanifest_*.acf`.

use super::registry::{self, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use super::{Action, Game, Platform, vdf};
use std::fs;
use std::path::{Path, PathBuf};

/// Steam's own redistributables and runtimes, which aren't games.
const NOT_GAMES: &[&str] = &[
    "228980",  // Steamworks Common Redistributables
    "1070560", // Steam Linux Runtime
    "1391110", // Steam Linux Runtime - Soldier
    "1628350", // Steam Linux Runtime - Sniper
];

/// `StateFlags` bit meaning the app's files are fully installed.
const FULLY_INSTALLED: u32 = 4;

pub fn steam_root() -> Option<PathBuf> {
    let raw = registry::string_any(
        &[
            (HKEY_CURRENT_USER, r"Software\Valve\Steam"),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam"),
        ],
        "SteamPath",
    )
    .or_else(|| {
        registry::string_any(
            &[
                (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
                (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam"),
            ],
            "InstallPath",
        )
    })?;
    let root = registry::clean_path(&raw);
    root.is_dir().then_some(root)
}

fn library_folders(root: &Path) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    // libraryfolders.vdf lists every library, including the main one, with the
    // path's real capitalisation (the registry often stores it lowercased).
    if let Ok(text) = fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")) {
        let parsed = vdf::parse(&text);
        if let Some(libraries) = parsed.obj("libraryfolders") {
            folders.extend(libraries.objects().filter_map(|(_, library)| library.str("path")).map(registry::clean_path));
        }
    }
    folders.push(root.to_path_buf());

    let mut unique: Vec<PathBuf> = Vec::new();
    for folder in folders {
        if !unique.iter().any(|f| f.as_os_str().eq_ignore_ascii_case(folder.as_os_str())) {
            unique.push(folder);
        }
    }
    unique
}

/// Portrait cover art file names, best first. Older games use the first;
/// newer ones use `library_capsule.jpg`.
const COVER_NAMES: &[&str] = &["library_600x900.jpg", "library_capsule.jpg"];

/// Steam keeps cover art locally, either directly in the app's cache folder or
/// one level down in a hashed subfolder (newer clients).
fn cover_art(root: &Path, app_id: &str) -> Option<PathBuf> {
    let dir = root.join(r"appcache\librarycache").join(app_id);
    let subfolders: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    COVER_NAMES.iter().find_map(|name| {
        std::iter::once(&dir).chain(&subfolders).map(|folder| folder.join(name)).find(|path| path.is_file())
    })
}

pub fn scan() -> Vec<Game> {
    let Some(root) = steam_root() else { return Vec::new() };
    let mut games = Vec::new();

    for library in library_folders(&root) {
        let steamapps = library.join("steamapps");
        let Ok(entries) = fs::read_dir(&steamapps) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            let parsed = vdf::parse(&text);
            let Some(app) = parsed.obj("AppState") else { continue };
            let (Some(app_id), Some(title)) = (app.str("appid"), app.str("name")) else { continue };
            if NOT_GAMES.contains(&app_id) {
                continue;
            }
            let flags: u32 = app.str("StateFlags").and_then(|f| f.parse().ok()).unwrap_or(0);
            let install_dir = app.str("installdir").map(|d| steamapps.join("common").join(d));
            let installed = flags & FULLY_INSTALLED != 0 && install_dir.as_ref().is_some_and(|d| d.is_dir());
            if !installed {
                continue;
            }

            games.push(Game {
                id: format!("steam:{app_id}"),
                name: title.to_string(),
                platform: Platform::Steam,
                install_dir,
                launch: Action::Uri(format!("steam://rungameid/{app_id}")),
                uninstall: Some(Action::Uri(format!("steam://uninstall/{app_id}"))),
                art: cover_art(&root, app_id),
            });
        }
    }
    games
}
