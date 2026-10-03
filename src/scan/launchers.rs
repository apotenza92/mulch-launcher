//! Detects which game launchers are installed, so the app can show a button
//! for each. Found through the link handlers and uninstall entries they
//! register with Windows, never by assuming an install folder.

use super::registry::{self, UninstallEntry};
use super::{Action, Platform};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize)]
pub struct Launcher {
    pub platform: Platform,
    pub name: &'static str,
    pub open: Action,
    /// The launcher's app icon (filled in after the scan, like game art).
    pub icon: Option<PathBuf>,
    /// Where to take the icon from: the launcher's executable.
    pub icon_source: Option<PathBuf>,
}

struct Known {
    platform: Platform,
    name: &'static str,
    /// Link handlers registered by the launcher, tried in order.
    schemes: &'static [&'static str],
    /// Fallback: Windows uninstall entries whose DisplayName starts with this.
    uninstall_name: Option<&'static str>,
}

const KNOWN: &[Known] = &[
    Known { platform: Platform::Steam, name: "Steam", schemes: &["steam"], uninstall_name: Some("Steam") },
    Known {
        platform: Platform::Epic,
        name: "Epic Games",
        schemes: &["com.epicgames.launcher"],
        uninstall_name: Some("Epic Games Launcher"),
    },
    Known { platform: Platform::Ubisoft, name: "Ubisoft Connect", schemes: &["uplay"], uninstall_name: Some("Ubisoft Connect") },
    Known { platform: Platform::Ea, name: "EA app", schemes: &["origin2", "link2ea"], uninstall_name: Some("EA app") },
    Known {
        platform: Platform::BattleNet,
        name: "Battle.net",
        schemes: &["battlenet", "blizzard"],
        uninstall_name: Some("Battle.net"),
    },
    Known { platform: Platform::Gog, name: "GOG Galaxy", schemes: &["goggalaxy"], uninstall_name: Some("GOG GALAXY") },
    Known {
        platform: Platform::Rockstar,
        name: "Rockstar Games",
        schemes: &[],
        uninstall_name: Some("Rockstar Games Launcher"),
    },
];

pub fn detect() -> Vec<Launcher> {
    let entries = registry::uninstall_entries();
    let mut found: Vec<Launcher> = KNOWN
        .iter()
        .filter_map(|known| {
            let exe = known
                .schemes
                .iter()
                .find_map(|scheme| registry::protocol_handler_exe(scheme))
                .or_else(|| known.uninstall_name.and_then(|name| exe_from_uninstall_entry(&entries, name)))?;
            Some(Launcher {
                platform: known.platform,
                name: known.name,
                icon: None,
                icon_source: Some(exe.clone()),
                open: Action::Exe { path: exe, args: Vec::new(), working_dir: None },
            })
        })
        .collect();

    if let Some(xbox) = xbox_app() {
        found.push(xbox);
    }
    found
}

/// The launcher's executable from its Windows uninstall entry: `DisplayIcon`
/// usually points straight at it.
fn exe_from_uninstall_entry(entries: &[UninstallEntry], display_name: &str) -> Option<PathBuf> {
    entries
        .iter()
        .filter(|entry| entry.display_name.starts_with(display_name))
        .filter_map(|entry| {
            let icon = entry.display_icon.split(',').next().unwrap_or(&entry.display_icon);
            registry::exe_from_command(icon)
        })
        .find(|exe| !exe.file_name().is_some_and(|f| f.to_string_lossy().to_lowercase().contains("uninst")))
}

fn xbox_app() -> Option<Launcher> {
    use windows::Management::Deployment::PackageManager;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    use windows::core::HSTRING;

    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let manager = PackageManager::new().ok()?;
    let package = manager
        .FindPackagesByUserSecurityIdPackageFamilyName(&HSTRING::new(), &HSTRING::from("Microsoft.GamingApp_8wekyb3d8bbwe"))
        .ok()?
        .into_iter()
        .next()?;
    let app_id = package.GetAppListEntries().ok()?.into_iter().find_map(|e| e.AppUserModelId().ok())?;
    let install_dir = package.InstalledPath().ok().map(|p| PathBuf::from(p.to_string()));
    Some(Launcher {
        platform: Platform::Xbox,
        name: "Xbox",
        open: Action::StoreApp(app_id.to_string()),
        icon: install_dir.as_deref().and_then(super::xbox::manifest_icon),
        icon_source: None,
    })
}
