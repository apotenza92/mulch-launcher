//! Finds programs on this PC that look like games but aren't in any launcher
//! (e.g. OpenMW, emulators, DRM-free downloads), to suggest adding them.
//!
//! No list of game names: it looks for evidence.
//! - Where: Start menu and desktop shortcuts, and `<drive>:\Games\*` folders.
//! - Evidence in the program's folder: files games ship with, like SDL's
//!   controller database, SDL/OpenAL/FMOD/Bink/PhysX libraries, Unity or
//!   Unreal engine files, Steam/GOG game APIs, or game data archives.
//! - Evidence in the program itself: it loads game-controller or game-audio
//!   libraries (XInput, DirectInput, SDL, OpenAL, XAudio).
//!
//! Suggestions are for the user to review, so a few false positives are fine.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub name: String,
    pub exe: PathBuf,
    /// Why it looks like a game, for showing to the user.
    pub reason: String,
}

/// Files that games ship with (matched case-insensitively in the program's
/// folder or the one above it), with a human description.
const MARKER_FILES: &[(&str, &str)] = &[
    ("gamecontrollerdb.txt", "game controller database"),
    ("sdl2.dll", "SDL"),
    ("sdl3.dll", "SDL"),
    ("openal32.dll", "OpenAL audio"),
    ("soft_oal.dll", "OpenAL audio"),
    ("unityplayer.dll", "Unity engine"),
    ("gameassembly.dll", "Unity engine"),
    ("steam_api.dll", "Steam game API"),
    ("steam_api64.dll", "Steam game API"),
    ("galaxy.dll", "GOG game API"),
    ("galaxy64.dll", "GOG game API"),
    ("eossdk-win64-shipping.dll", "Epic game API"),
    ("discord_game_sdk.dll", "Discord game SDK"),
    ("bink2w64.dll", "Bink video"),
    ("binkw32.dll", "Bink video"),
    ("binkw64.dll", "Bink video"),
    ("fmod.dll", "FMOD audio"),
    ("fmod64.dll", "FMOD audio"),
    ("fmodex.dll", "FMOD audio"),
    ("fmodex64.dll", "FMOD audio"),
    ("fmodstudio.dll", "FMOD audio"),
    ("physx3_x64.dll", "PhysX"),
    ("physxloader.dll", "PhysX"),
    ("openvr_api.dll", "VR"),
];

/// Game data archive extensions.
const MARKER_EXTENSIONS: &[(&str, &str)] = &[
    ("pak", "game data archives"),
    ("bsa", "game data archives"),
    ("ba2", "game data archives"),
    ("esm", "game data files"),
    ("vpk", "game data archives"),
    ("wad", "game data archives"),
    ("pk3", "game data archives"),
    ("upk", "game data archives"),
    ("unity3d", "Unity engine"),
];

/// Files that mark a Chromium/Electron app (Discord, NVIDIA App, Playnite...).
/// These ship `.pak` resources and sometimes SDL, so they need stronger
/// evidence than ordinary programs.
const CHROMIUM_FILES: &[&str] = &["icudtl.dat", "libcef.dll", "chrome_elf.dll"];

/// Evidence strong enough to count even in a Chromium/Electron app.
const STRONG_EVIDENCE: &[&str] = &[
    "Unity engine",
    "Unreal engine",
    "Steam game API",
    "GOG game API",
    "Epic game API",
    "Bink video",
    "PhysX",
    "FMOD audio",
];

/// Libraries a program imports that point to a game.
const GAME_IMPORTS: &[(&str, &str)] = &[
    ("xinput", "uses game controllers"),
    ("dinput8", "uses game controllers"),
    ("sdl2", "uses SDL"),
    ("sdl3", "uses SDL"),
    ("openal32", "uses OpenAL audio"),
    ("xaudio2", "uses XAudio game audio"),
    ("x3daudio", "uses XAudio game audio"),
    ("steam_api", "uses the Steam game API"),
];

/// Executable names that are never the game itself.
const NOT_GAME_EXES: &[&str] =
    &["unins", "uninst", "setup", "install", "update", "crash", "report", "redist", "helper", "dxsetup", "vcredist"];

/// Shortcut names that point to a game's companion tools; the game itself is
/// preferred when a folder has several.
const TOOL_WORDS: &[&str] = &[
    "launcher",
    "editor",
    "construction",
    "wizard",
    "config",
    "settings",
    "setup",
    "tool",
    "server",
    "benchmark",
    "readme",
    "manual",
    "help",
    "website",
    "support",
    "uninstall",
    "dedicated",
    "mod ",
];

/// Suggests games not already known. `known` are folders to ignore: every
/// detected game's folders and every launcher's folder.
pub fn find_games(known: &[PathBuf]) -> Vec<Suggestion> {
    let known: Vec<String> = known.iter().map(|p| lower_dir(p)).collect();
    let is_known = |exe: &Path| {
        let exe = exe.display().to_string().to_lowercase();
        known.iter().any(|dir| exe.starts_with(dir))
    };

    let mut candidates: Vec<(String, PathBuf)> = shortcut_targets();
    candidates.extend(games_folder_exes());

    // Group by folder; keep the most game-like name per folder.
    let mut by_folder: HashMap<String, (String, PathBuf)> = HashMap::new();
    for (name, exe) in candidates {
        let stem = exe.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        if NOT_GAME_EXES.iter().any(|w| stem.contains(w)) || is_known(&exe) || is_system(&exe) {
            continue;
        }
        let Some(folder) = exe.parent().map(lower_dir) else { continue };
        match by_folder.get(&folder) {
            Some((existing, _)) if preference(existing) <= preference(&name) => {}
            _ => {
                by_folder.insert(folder, (name, exe));
            }
        }
    }

    let mut suggestions: Vec<Suggestion> = by_folder
        .into_values()
        .filter_map(|(name, exe)| Some(Suggestion { reason: game_evidence(&exe)?, name: better_name(name, &exe), exe }))
        .collect();
    suggestions.sort_by_key(|s| s.name.to_lowercase());
    suggestions
}

fn lower_dir(path: &Path) -> String {
    format!("{}\\", path.display().to_string().to_lowercase().trim_end_matches('\\'))
}

fn is_system(exe: &Path) -> bool {
    let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
    exe.display().to_string().to_lowercase().starts_with(&windows)
}

/// The name the program gives itself (e.g. "World of Warcraft" for a
/// shortcut called "Warmane - Lordaeron"), unless that looks like a tool's.
fn better_name(name: String, exe: &Path) -> String {
    match mulch_core::exe_info::product_name(exe) {
        Some(product) if product.len() <= 60 && !preference(&product).0 => product,
        _ => name,
    }
}

/// Lower is better: names without tool words first, then shorter names.
fn preference(name: &str) -> (bool, usize) {
    let lower = name.to_lowercase();
    (TOOL_WORDS.iter().any(|w| lower.contains(w)), name.len())
}

/// Why `exe` looks like a game, or `None` if it doesn't.
fn game_evidence(exe: &Path) -> Option<String> {
    let dir = exe.parent()?;
    let mut found: Vec<&str> = Vec::new();
    let chromium = CHROMIUM_FILES.iter().any(|f| dir.join(f).is_file());
    for folder in std::iter::once(dir).chain(dir.parent()) {
        let Ok(entries) = fs::read_dir(folder) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if chromium && name.ends_with(".pak") {
                continue; // Chromium's own resource bundles, not game data
            }
            if let Some((_, what)) = MARKER_FILES.iter().find(|(file, _)| *file == name) {
                found.push(what);
            } else if let Some((_, what)) =
                Path::new(&name).extension().and_then(|e| MARKER_EXTENSIONS.iter().find(|(ext, _)| e == *ext))
            {
                found.push(what);
            } else if entry.path().is_dir() && name.ends_with("_data") && entry.path().join("Managed").is_dir() {
                found.push("Unity engine");
            } else if name == "engine" && entry.path().join("Binaries").is_dir() {
                found.push("Unreal engine");
            }
        }
    }
    if chromium {
        found.retain(|what| STRONG_EVIDENCE.contains(what));
    } else if found.is_empty() {
        found.extend(game_imports(exe));
    }
    found.sort_unstable();
    found.dedup();
    (!found.is_empty()).then(|| found.join(", "))
}

/// Game-related libraries the executable imports.
fn game_imports(exe: &Path) -> Vec<&'static str> {
    const MAX_SIZE: u64 = 512 * 1024 * 1024;
    if fs::metadata(exe).map(|m| m.len() > MAX_SIZE).unwrap_or(true) {
        return Vec::new();
    }
    let Ok(bytes) = fs::read(exe) else { return Vec::new() };
    let Ok(pe) = goblin::pe::PE::parse(&bytes) else { return Vec::new() };
    pe.libraries
        .iter()
        .filter_map(|library| {
            let library = library.to_lowercase();
            GAME_IMPORTS.iter().find(|(prefix, _)| library.starts_with(prefix)).map(|(_, what)| *what)
        })
        .collect()
}

/// (shortcut name, target exe) for every shortcut in the Start menu (user
/// and all users) and on the desktop that points at an existing .exe.
fn shortcut_targets() -> Vec<(String, PathBuf)> {
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let roots: Vec<PathBuf> = [
        env("APPDATA").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs")),
        env("ProgramData").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs")),
        env("USERPROFILE").map(|p| p.join("Desktop")),
        env("OneDrive").map(|p| p.join("Desktop")),
        env("PUBLIC").map(|p| p.join("Desktop")),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut shortcuts = Vec::new();
    for root in roots {
        collect_shortcuts(&root, 0, &mut shortcuts);
    }
    shortcuts
        .into_iter()
        .filter_map(|lnk| {
            let target = resolve_shortcut(&lnk)?;
            let is_exe = target.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
            let name = lnk.file_stem()?.to_string_lossy().into_owned();
            (is_exe && target.is_file()).then_some((name, target))
        })
        .collect()
}

fn collect_shortcuts(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                collect_shortcuts(&path, depth + 1, out);
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
            out.push(path);
        }
    }
}

/// The target of a `.lnk` file, via Windows' shell link API.
fn resolve_shortcut(lnk: &Path) -> Option<PathBuf> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
    use windows::core::{HSTRING, Interface};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        link.cast::<IPersistFile>().ok()?.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ).ok()?;
        let mut buffer = [0u16; 1024];
        link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).ok()?;
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let raw = String::from_utf16_lossy(&buffer[..len]);
        let expanded = expand_env(&raw);
        (!expanded.is_empty()).then(|| PathBuf::from(expanded))
    }
}

/// Expands `%VAR%` references in a raw shortcut path.
fn expand_env(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                out.push_str(&std::env::var(name).unwrap_or_else(|_| format!("%{name}%")));
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Programs in `<drive>:\Games\<folder>` (one level), named after the folder,
/// for portable games that have no shortcut.
fn games_folder_exes() -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    for drive in 'A'..='Z' {
        let Ok(folders) = fs::read_dir(format!(r"{drive}:\Games")) else { continue };
        for folder in folders.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()) {
            let Some(name) = folder.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if let Some(exe) = main_exe(&folder, &name) {
                found.push((name, exe));
            }
        }
    }
    found
}

/// The folder's main program: the .exe whose name best matches the folder,
/// otherwise the largest.
fn main_exe(folder: &Path, folder_name: &str) -> Option<PathBuf> {
    let wanted: String = folder_name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let exes: Vec<(PathBuf, u64)> = fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        .filter(|p| {
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            !NOT_GAME_EXES.iter().any(|w| stem.contains(w))
        })
        .map(|p| {
            let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            (p, size)
        })
        .collect();
    let matching = exes.iter().find(|(p, _)| {
        let stem: String = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let stem: String = stem.chars().filter(|c| c.is_alphanumeric()).collect();
        !stem.is_empty() && (stem == wanted || wanted.starts_with(&stem) || stem.starts_with(&wanted))
    });
    matching.or_else(|| exes.iter().max_by_key(|(_, size)| *size)).map(|(p, _)| p.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_game_over_its_tools() {
        assert!(preference("OpenMW") < preference("OpenMW Launcher"));
        assert!(preference("OpenMW") < preference("OpenMW Construction Set"));
        assert!(preference("Doom") < preference("Doom Dedicated Server"));
    }

    #[test]
    fn expands_environment_variables() {
        // SAFETY: tests in this crate don't read this variable concurrently.
        unsafe { std::env::set_var("MULCH_TEST_DIR", r"C:\Games") };
        assert_eq!(expand_env(r"%MULCH_TEST_DIR%\x.exe"), r"C:\Games\x.exe");
        assert_eq!(expand_env("no vars"), "no vars");
    }
}
