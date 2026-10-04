//! Per-user install with no installer and no admin rights. Run from anywhere
//! (e.g. Downloads), `MulchLauncher.exe` silently copies itself to
//! `%LOCALAPPDATA%\Programs\MulchLauncher`, adds a Start menu shortcut and an
//! "Installed apps" entry, then relaunches from there.
//!
//! Everything MulchLauncher saves lives in that folder (see
//! `mulch_core::paths`), so uninstalling removes, in full:
//! - the install folder (program, settings, play history, caches),
//! - the Start menu shortcut, and a taskbar pin if the user made one,
//! - the "Installed apps" entry,
//! - folders older versions used in `%APPDATA%` and `%LOCALAPPDATA%`.
//!
//! Windows itself keeps a few anonymous notes about every program ever run
//! (jump lists, the recently-run cache); those belong to Windows, not to us.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{HSTRING, Interface};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

pub const APP_NAME: &str = "MulchLauncher";
const EXE_NAME: &str = "MulchLauncher.exe";
/// Run helper commands without a console window flashing up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\MulchLauncher";

pub fn install_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs").join(APP_NAME))
}

fn start_menu_shortcut() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs").join(format!("{APP_NAME}.lnk")))
}

/// Where Windows keeps a taskbar pin the user made (named after the shortcut).
fn taskbar_pin() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|p| {
        PathBuf::from(p)
            .join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar")
            .join(format!("{APP_NAME}.lnk"))
    })
}

/// Folders older versions saved data in, outside the install folder.
fn legacy_data_dirs() -> Vec<PathBuf> {
    ["APPDATA", "LOCALAPPDATA"].iter().filter_map(|var| env::var_os(var)).map(|p| PathBuf::from(p).join(APP_NAME)).collect()
}

/// Whether this process is the installed copy.
pub fn is_installed_copy() -> bool {
    let (Ok(exe), Some(dir)) = (env::current_exe(), install_dir()) else { return false };
    exe.parent().is_some_and(|parent| same_path(parent, &dir))
}

/// Development builds run from Cargo's `target` folder and never install.
pub fn is_dev_build() -> bool {
    env::current_exe()
        .map(|exe| exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target")))
        .unwrap_or(false)
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

/// Copies this executable into place and registers it. Returns the installed exe.
pub fn install() -> io::Result<PathBuf> {
    let dir = install_dir().ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))?;
    fs::create_dir_all(&dir)?;
    let installed = dir.join(EXE_NAME);
    let current = env::current_exe()?;
    if !same_path(&current, &installed) {
        fs::copy(&current, &installed)?;
    }
    adopt_legacy_data(&dir.join("data"), true);
    if let Some(shortcut) = start_menu_shortcut() {
        create_shortcut(&installed, &shortcut)?;
    }
    register_uninstall_entry(&installed)?;
    Ok(installed)
}

/// Moves (or for development builds, copies) data that older versions kept in
/// `%APPDATA%` / `%LOCALAPPDATA%` into `data`, so nothing is lost and, once
/// moved, nothing is left behind there. Caches go into `data\cache`.
pub fn adopt_legacy_data(data: &Path, move_it: bool) {
    for legacy in legacy_data_dirs() {
        let Ok(entries) = fs::read_dir(&legacy) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let is_cache = matches!(name.to_str(), Some("posters" | "icons"));
            let target = if is_cache { data.join("cache").join(&name) } else { data.join(&name) };
            if target.exists() {
                continue;
            }
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let moved = move_it && fs::rename(entry.path(), &target).is_ok();
            if !moved {
                let _ = copy_recursive(&entry.path(), &target);
            }
        }
        if move_it {
            remove_dir_with_retry(&legacy);
        }
    }
}

fn copy_recursive(from: &Path, to: &Path) -> io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)?.filter_map(Result::ok) {
            copy_recursive(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// Starts the installed copy so this one can exit.
pub fn relaunch(installed: &Path) -> io::Result<()> {
    Command::new(installed).current_dir(installed.parent().unwrap_or(Path::new("."))).spawn().map(|_| ())
}

fn create_shortcut(target: &Path, shortcut: &Path) -> io::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(io::Error::other)?;
        link.SetPath(&HSTRING::from(target.as_os_str())).map_err(io::Error::other)?;
        if let Some(dir) = target.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(io::Error::other)?;
        }
        link.SetDescription(&HSTRING::from("Every installed game, from every launcher")).map_err(io::Error::other)?;
        let file: IPersistFile = link.cast().map_err(io::Error::other)?;
        if let Some(parent) = shortcut.parent() {
            fs::create_dir_all(parent)?;
        }
        file.Save(&HSTRING::from(shortcut.as_os_str()), true).map_err(io::Error::other)?;
    }
    Ok(())
}

fn register_uninstall_entry(installed: &Path) -> io::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY)?;
    let exe = installed.display().to_string();
    key.set_value("DisplayName", &APP_NAME)?;
    key.set_value("DisplayIcon", &format!("{exe},0"))?;
    key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
    key.set_value("Publisher", &"MulchLauncher")?;
    key.set_value("InstallLocation", &installed.parent().map(|p| p.display().to_string()).unwrap_or_default())?;
    key.set_value("UninstallString", &format!("\"{exe}\" --uninstall"))?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    if let Ok(meta) = fs::metadata(installed) {
        key.set_value("EstimatedSize", &((meta.len() / 1024) as u32))?;
    }
    Ok(())
}

/// Removes everything listed at the top of this file. Any other open copy of
/// MulchLauncher is closed first. The running exe can't delete its own
/// folder, so a short-lived `cmd` removes it once this process has exited,
/// retrying while Windows still holds the files.
pub fn uninstall() -> io::Result<()> {
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", EXE_NAME, "/FI"])
        .arg(format!("PID ne {}", std::process::id()))
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    for file in [start_menu_shortcut(), taskbar_pin()].into_iter().flatten() {
        let _ = fs::remove_file(file);
    }
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);
    if let Some(dir) = install_dir() {
        forget_in_windows_caches(&dir.join(EXE_NAME));
    }
    for legacy in legacy_data_dirs() {
        remove_dir_with_retry(&legacy);
    }
    if let Some(dir) = install_dir() {
        let dir = dir.display().to_string();
        // `ping` is the console-free delay (`timeout` needs a console).
        let script = format!(
            "for /L %i in (1,1,15) do @(if exist \"{dir}\" (ping -n 2 127.0.0.1 >NUL & rmdir /S /Q \"{dir}\"))"
        );
        // Passed verbatim: cmd doesn't understand Rust's argument quoting.
        Command::new("cmd").raw_arg(format!("/D /S /C \"{script}\"")).creation_flags(CREATE_NO_WINDOW).spawn()?;
    }
    Ok(())
}

/// Removes the notes Windows keeps on any program that has run (its display
/// name, and the compatibility assistant's record) for the installed exe.
/// Windows recreates these for any program it runs, so removing them is safe.
fn forget_in_windows_caches(exe: &Path) {
    let exe = exe.display().to_string().to_lowercase();
    let caches = [
        r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache",
        r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Compatibility Assistant\Store",
    ];
    for cache in caches {
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(cache, winreg::enums::KEY_ALL_ACCESS)
        else {
            continue;
        };
        let ours: Vec<String> =
            key.enum_values().filter_map(Result::ok).map(|(name, _)| name).filter(|n| n.to_lowercase().starts_with(&exe)).collect();
        for name in ours {
            let _ = key.delete_value(name);
        }
    }
}

/// Deletes a folder, retrying briefly: right after an app closes, Windows
/// can still be holding its files open for a moment.
fn remove_dir_with_retry(dir: &Path) {
    for _ in 0..10 {
        if !dir.exists() || fs::remove_dir_all(dir).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
