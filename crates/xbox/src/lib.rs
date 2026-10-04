//! Xbox app and Microsoft Store games, from Windows' package database. That
//! knows each package's real folder, so games on any drive (e.g. the
//! `XboxGames` folder the Xbox app creates per drive) are found.
//!
//! A package counts as a game when it ships an Xbox game config: GDK games
//! have `MicrosoftGame.config`, older Xbox Live UWP games `xboxservices.config`.
//! DLC and add-on "stub" packages also ship that config but declare no
//! launchable app, so requiring an app entry filters them out.

use mulch_core::{Action, Art, Game, Launcher, Library, Platform, ScanContext};
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

pub struct Xbox;

impl Library for Xbox {
    fn id(&self) -> &'static str {
        "xbox"
    }

    /// The Xbox app, found as a package (it registers no link handler).
    fn launcher(&self, _: &ScanContext) -> Option<Launcher> {
        let mut launcher = store_app("Microsoft.GamingApp_8wekyb3d8bbwe", "Xbox")?;
        launcher.platform = Some(Platform::Xbox);
        Some(launcher)
    }

    fn games(&self, _: &ScanContext) -> Vec<Game> {
        init_winrt();
        scan_packages().unwrap_or_default()
    }
}

/// An installed Microsoft Store app as a button, by package family name:
/// opened by its AppUserModelId, with its app-list icon.
pub fn store_app(family_name: &str, name: &'static str) -> Option<Launcher> {
    init_winrt();
    let manager = PackageManager::new().ok()?;
    let package = manager
        .FindPackagesByUserSecurityIdPackageFamilyName(&HSTRING::new(), &HSTRING::from(family_name))
        .ok()?
        .into_iter()
        .next()?;
    let app_id = package.GetAppListEntries().ok()?.into_iter().find_map(|e| e.AppUserModelId().ok())?;
    let install_dir = package.InstalledPath().ok().map(|p| PathBuf::from(p.to_string()));
    Some(Launcher {
        platform: None,
        name,
        open: Action::StoreApp(app_id.to_string()),
        icon: install_dir.as_deref().and_then(manifest_icon),
        icon_source: None,
    })
}

/// WinRT needs the thread initialised; already-initialised is fine.
fn init_winrt() {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
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

        let family_name = package.Id()?.FamilyName()?.to_string();
        let mut game = Game::new(
            format!("xbox:{family_name}"),
            package.DisplayName()?.to_string(),
            Platform::Xbox,
            Some(visible_dir.clone()),
            Action::StoreApp(app_id),
        );
        // The game's processes can run from either folder.
        game.process_dirs = vec![visible_dir, install_dir.clone()];
        game.show_in_launcher = Some(Action::Uri(format!("ms-windows-store://pdp/?PFN={family_name}")));
        // Square logos, not posters: shown as icons until a real poster is
        // fetched (see the posters crate).
        game.art = logo(&install_dir, &config).or_else(|| manifest_logo(&install_dir)).map(Art::Icon);
        games.push(game);
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

/// The app's tile logo from its package manifest, for packages whose game
/// config has no artwork (e.g. older Xbox Live UWP games like Solitaire).
pub fn manifest_logo(install_dir: &Path) -> Option<PathBuf> {
    manifest_visual(install_dir, &["Square150x150Logo", "Square44x44Logo"])
}

/// The app's small app-list icon, which is drawn edge to edge (good for
/// launcher buttons, where the padded tile logo looks tiny).
pub fn manifest_icon(install_dir: &Path) -> Option<PathBuf> {
    manifest_visual(install_dir, &["Square44x44Logo", "Square150x150Logo"])
}

fn manifest_visual(install_dir: &Path, attributes: &[&str]) -> Option<PathBuf> {
    let manifest = fs::read_to_string(install_dir.join("AppxManifest.xml")).ok()?;
    let visuals = &manifest[manifest.find("VisualElements")?..];
    attributes.iter().find_map(|attr| resolve_asset(install_dir, attribute(visuals, attr)?))
}

/// Package assets are referenced without their scale qualifier
/// (`Assets\Tile.png`) but stored with one (`Assets\Tile.scale-200.png`).
/// Picks the largest file matching the reference.
fn resolve_asset(install_dir: &Path, reference: &str) -> Option<PathBuf> {
    let exact = install_dir.join(reference.replace('/', "\\"));
    if exact.is_file() {
        return Some(exact);
    }
    let dir = exact.parent()?;
    let stem = exact.file_stem()?.to_string_lossy().to_lowercase();
    let ext = exact.extension()?.to_string_lossy().to_lowercase();
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            name.starts_with(&format!("{stem}.")) && name.ends_with(&format!(".{ext}")) && !name.contains("contrast")
        })
        .max_by_key(|entry| entry.metadata().map(|m| m.len()).unwrap_or(0))
        .map(|entry| entry.path())
}

/// Value of `name="..."` in an XML document (enough for these configs).
fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.find(&format!("{name}=\""))? + name.len() + 2;
    let len = text[start..].find('"')?;
    Some(&text[start..start + len])
}
