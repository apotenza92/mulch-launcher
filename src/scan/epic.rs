//! Epic Games Store: one JSON `.item` manifest per installed app.

use super::registry::{self, HKEY_LOCAL_MACHINE};
use super::{Action, Game, Platform};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    display_name: String,
    install_location: String,
    app_name: String,
    #[serde(default)]
    main_game_app_name: Option<String>,
    #[serde(default)]
    catalog_namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    #[serde(default)]
    app_categories: Vec<String>,
    #[serde(default)]
    compatible_apps: Vec<String>,
    #[serde(default)]
    launch_executable: String,
    #[serde(default, rename = "bIsIncompleteInstall")]
    is_incomplete_install: bool,
}

fn manifests_dir() -> Option<PathBuf> {
    // Epic records its data folder in the registry; ProgramData is the fallback.
    let from_registry = registry::string_any(
        &[
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Epic Games\EpicGamesLauncher"),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Epic Games\EpicGamesLauncher"),
        ],
        "AppDataPath",
    )
    .map(|p| registry::clean_path(&p).join("Manifests"));
    let fallback = std::env::var_os("ProgramData")
        .map(|p| PathBuf::from(p).join(r"Epic\EpicGamesLauncher\Data\Manifests"));
    [from_registry, fallback].into_iter().flatten().find(|p| p.is_dir())
}

pub fn scan() -> Vec<Game> {
    let Some(dir) = manifests_dir() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut games = Vec::new();

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("item")) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(manifest) = serde_json::from_str::<Manifest>(&text) else { continue };

        let has = |category: &str| manifest.app_categories.iter().any(|c| c == category);
        // Same rules as Playnite: launchable add-ons count, other add-ons and
        // Unreal Engine plugins don't.
        let is_game = has("games")
            && !(has("addons") && !has("addons/launchable"))
            && !has("plugins")
            && !has("plugins/engine")
            && !manifest.compatible_apps.iter().any(|a| a.starts_with("UE_"));
        // DLC and add-ons have their own manifest pointing at the main game.
        // Games themselves leave this empty or point at themselves.
        let is_addon = manifest
            .main_game_app_name
            .as_deref()
            .is_some_and(|main| !main.is_empty() && main != manifest.app_name);
        let install_dir = registry::clean_path(&manifest.install_location);
        if !is_game || is_addon || manifest.is_incomplete_install || !install_dir.is_dir() {
            continue;
        }

        let launch_id = format!(
            "{}%3A{}%3A{}",
            manifest.catalog_namespace, manifest.catalog_item_id, manifest.app_name
        );
        games.push(Game {
            id: format!("epic:{}", manifest.app_name),
            name: manifest.display_name,
            platform: Platform::Epic,
            install_dir: Some(install_dir.clone()),
            launch: Action::Uri(format!("com.epicgames.launcher://apps/{launch_id}?action=launch&silent=true")),
            uninstall: None,
            art: None,
            icon_source: Some(install_dir.join(&manifest.launch_executable)).filter(|p| p.is_file()),
        });
    }
    games
}
