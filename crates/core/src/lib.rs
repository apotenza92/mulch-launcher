//! Shared types for MulchLauncher's libraries.
//!
//! Each launcher (Steam, Epic, Xbox, ...) lives in its own crate and implements
//! [`Library`]: detect its launcher, list its installed games, and say how to
//! launch and show each one. The app just runs every library.
//!
//! Every location must come from the launcher's own records (registry,
//! manifests, Windows' package database) so games on any drive or custom
//! folder are found. Default paths are only ever a last-resort fallback.

pub mod registry;

use registry::UninstallEntry;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Platform {
    Steam,
    Epic,
    Ubisoft,
    Gog,
    Xbox,
    Ea,
    BattleNet,
    Rockstar,
    Manual,
}

impl Platform {
    pub fn label(self) -> &'static str {
        match self {
            Platform::Steam => "Steam",
            Platform::Epic => "Epic",
            Platform::Ubisoft => "Ubisoft",
            Platform::Gog => "GOG",
            Platform::Xbox => "Xbox",
            Platform::Ea => "EA",
            Platform::BattleNet => "Battle.net",
            Platform::Rockstar => "Rockstar",
            Platform::Manual => "Added by you",
        }
    }
}

/// How to start (or show) something.
#[derive(Clone, Debug, Serialize)]
pub enum Action {
    /// A URI handled by a launcher, e.g. `steam://rungameid/730`.
    Uri(String),
    /// An executable started directly.
    Exe { path: PathBuf, args: Vec<String>, working_dir: Option<PathBuf> },
    /// A Microsoft Store / Xbox app, by its AppUserModelId.
    StoreApp(String),
    /// A complete command line, run exactly as written, with its quoting
    /// kept intact.
    CommandLine(String),
}

#[derive(Clone, Debug, Serialize)]
pub struct Game {
    /// Stable id, unique across platforms, e.g. `steam:730`.
    pub id: String,
    pub name: String,
    pub platform: Platform,
    /// The folder shown to the user ("Open folder").
    pub install_dir: Option<PathBuf>,
    /// Folders the game's processes run from, for play tracking. Usually just
    /// the install folder; Xbox games also run from their WindowsApps folder.
    pub process_dirs: Vec<PathBuf>,
    pub launch: Action,
    /// Opens the game's page (or at least the library) in its own launcher.
    pub show_in_launcher: Option<Action>,
    pub art: Option<Art>,
    /// An executable or .ico to take an icon from when there's no art.
    pub icon_source: Option<PathBuf>,
    /// When the launcher itself last saw it played (Unix seconds), if it records that.
    pub last_played: Option<u64>,
}

impl Game {
    /// A game with only the required fields; libraries fill in the rest.
    pub fn new(id: String, name: String, platform: Platform, install_dir: Option<PathBuf>, launch: Action) -> Self {
        Self {
            process_dirs: install_dir.iter().cloned().collect(),
            id,
            name,
            platform,
            install_dir,
            launch,
            show_in_launcher: None,
            art: None,
            icon_source: None,
            last_played: None,
        }
    }
}

/// Local artwork for a tile.
#[derive(Clone, Debug, Serialize)]
pub enum Art {
    /// Portrait cover art.
    Cover(PathBuf),
    /// A square icon or logo, shown centred on the tile.
    Icon(PathBuf),
}

/// An installed app shown as a button: a game launcher (with its platform)
/// or a chat app like Discord (no platform).
#[derive(Clone, Debug, Serialize)]
pub struct Launcher {
    pub platform: Option<Platform>,
    pub name: &'static str,
    pub open: Action,
    /// The launcher's app icon (filled in after the scan, like game art).
    pub icon: Option<PathBuf>,
    /// Where to take the icon from: the launcher's executable.
    pub icon_source: Option<PathBuf>,
}

impl Launcher {
    /// A launcher opened by running its executable.
    pub fn from_exe(platform: Platform, name: &'static str, exe: PathBuf) -> Self {
        Self {
            platform: Some(platform),
            name,
            icon: None,
            icon_source: Some(exe.clone()),
            open: Action::Exe { path: exe, args: Vec::new(), working_dir: None },
        }
    }
}

/// Shared, lazily-read system state that several libraries need, so a scan
/// reads it once however many libraries ask.
#[derive(Default)]
pub struct ScanContext {
    uninstall_entries: OnceLock<Vec<UninstallEntry>>,
}

impl ScanContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Windows' "Installed apps" entries (the Uninstall registry).
    pub fn uninstall_entries(&self) -> &[UninstallEntry] {
        self.uninstall_entries.get_or_init(registry::uninstall_entries)
    }
}

/// One game library (usually one launcher).
pub trait Library: Send + Sync {
    /// Short name, e.g. `"steam"`, for timings and `--scan <id>`.
    fn id(&self) -> &'static str;
    /// The launcher app, if it's installed.
    fn launcher(&self, cx: &ScanContext) -> Option<Launcher>;
    /// Installed games. Must be fast: read local records only.
    fn games(&self, cx: &ScanContext) -> Vec<Game>;
}
