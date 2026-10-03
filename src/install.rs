//! Per-user self-install: no installer, no admin rights. The downloaded
//! `MulchLauncher.exe` copies itself to `%LOCALAPPDATA%\Programs\MulchLauncher`,
//! adds a Start menu shortcut and an "Installed apps" entry (so it uninstalls
//! like any other app), then relaunches from there.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
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
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\MulchLauncher";

pub fn install_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs").join(APP_NAME))
}

fn start_menu_shortcut() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs").join(format!("{APP_NAME}.lnk")))
}

/// Whether this process is the installed copy.
pub fn is_installed_copy() -> bool {
    let (Ok(exe), Some(dir)) = (env::current_exe(), install_dir()) else { return false };
    exe.parent().is_some_and(|parent| same_path(parent, &dir))
}

/// Development builds run from Cargo's `target` folder and never self-install.
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

    if let Some(shortcut) = start_menu_shortcut() {
        create_shortcut(&installed, &shortcut)?;
    }
    register_uninstall_entry(&installed)?;
    Ok(installed)
}

/// Starts the installed copy (with extra arguments) so this one can exit.
pub fn relaunch(installed: &Path, args: &[&str]) -> io::Result<()> {
    Command::new(installed).args(args).current_dir(installed.parent().unwrap_or(Path::new("."))).spawn().map(|_| ())
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

/// Removes the shortcut, the Installed apps entry, settings and the program
/// itself. The running exe can't delete itself, so a short-lived `cmd`
/// removes the folder once this process has exited.
pub fn uninstall() -> io::Result<()> {
    if let Some(shortcut) = start_menu_shortcut() {
        let _ = fs::remove_file(shortcut);
    }
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);
    if let Some(data) = crate::settings::data_dir() {
        remove_dir_with_retry(&data);
    }
    if let Some(cache) = env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join(APP_NAME)) {
        remove_dir_with_retry(&cache);
    }
    if let Some(dir) = install_dir() {
        // `ping` is the classic console-free delay (`timeout` needs a console).
        Command::new("cmd")
            .args(["/C", "ping", "-n", "3", "127.0.0.1", ">NUL", "&", "rmdir", "/S", "/Q"])
            .arg(&dir)
            .spawn()?;
    }
    Ok(())
}

/// Deletes a folder, retrying briefly: right after the app closes, Windows
/// can still be holding its files open for a moment.
fn remove_dir_with_retry(dir: &Path) {
    for _ in 0..10 {
        if !dir.exists() || fs::remove_dir_all(dir).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Launchers the user has pinned to the taskbar (pinned shortcuts live in a
/// known folder). Used to suggest unpinning them, since Mulch has buttons for
/// them; Windows offers no supported way for an app to unpin others.
pub fn pinned_launchers(launcher_names: &[&str]) -> Vec<String> {
    let Some(folder) = env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"))
    else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    let pinned: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .filter(|name| name.ends_with(".lnk"))
        .collect();

    launcher_names
        .iter()
        .filter(|name| {
            let key = pin_keyword(name);
            pinned.iter().any(|file| file.contains(&key))
        })
        .map(|name| name.to_string())
        .collect()
}

/// The word a launcher's taskbar shortcut name is likely to contain.
fn pin_keyword(launcher: &str) -> String {
    match launcher {
        "EA app" => "ea app".into(),
        "Epic Games" => "epic games".into(),
        "Ubisoft Connect" => "ubisoft".into(),
        "Rockstar Games" => "rockstar".into(),
        "GOG Galaxy" => "gog".into(),
        other => other.to_lowercase(),
    }
}

/// Asks Windows to pin this app to the taskbar. Windows shows its own
/// confirmation. Unpackaged apps need Microsoft-issued access for this, so it
/// usually reports `false` and the app tells the user how to pin by hand.
pub fn request_taskbar_pin() -> bool {
    use windows::UI::Shell::TaskbarManager;
    let Ok(manager) = TaskbarManager::GetDefault() else { return false };
    if !manager.IsSupported().unwrap_or(false) || !manager.IsPinningAllowed().unwrap_or(false) {
        return false;
    }
    manager.RequestPinCurrentAppAsync().and_then(|op| op.get()).unwrap_or(false)
}
