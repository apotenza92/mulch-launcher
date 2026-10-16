//! Desktop shortcut creation for Store installs and legacy data helpers.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
};
use windows::Win32::UI::Shell::{FOLDERID_Desktop, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink};
use windows::core::{HSTRING, Interface};

pub const APP_NAME: &str = "MulchLauncher";
/// Add the Store app to the desktop once, using its stable package identity
/// rather than an executable path that changes whenever the Store updates it.
/// Remember success so a shortcut the user deletes stays deleted.
pub fn add_store_desktop_shortcut() -> io::Result<()> {
    use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentApplicationUserModelId;
    use windows::Win32::UI::Shell::SHParseDisplayName;
    use windows::core::PWSTR;

    if !mulch_core::paths::is_packaged() {
        return Ok(());
    }
    let data = mulch_core::paths::data_dir().ok_or_else(|| io::Error::other("No app data folder"))?;
    let marker = data.join("desktop-shortcut-created");
    if marker.exists() {
        return Ok(());
    }
    let shortcut = desktop_shortcut().ok_or_else(|| io::Error::other("No desktop folder"))?;
    if !shortcut.exists() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().map_err(io::Error::other)?;
            let result = (|| {
                let mut len = 0;
                let status = GetCurrentApplicationUserModelId(&mut len, None);
                if status != ERROR_INSUFFICIENT_BUFFER {
                    return Err(io::Error::other(format!("Cannot get app identity: {}", status.0)));
                }
                let mut id = vec![0u16; len as usize];
                GetCurrentApplicationUserModelId(&mut len, Some(PWSTR(id.as_mut_ptr())))
                    .ok()
                    .map_err(io::Error::other)?;
                let id = String::from_utf16_lossy(&id[..len.saturating_sub(1) as usize]);
                let mut pidl = std::ptr::null_mut();
                SHParseDisplayName(&HSTRING::from(format!("shell:AppsFolder\\{id}")), None, &mut pidl, 0, None)
                    .map_err(io::Error::other)?;
                let result = (|| {
                    let link: IShellLinkW =
                        CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(io::Error::other)?;
                    link.SetIDList(pidl).map_err(io::Error::other)?;
                    let file: IPersistFile = link.cast().map_err(io::Error::other)?;
                    file.Save(&HSTRING::from(shortcut.as_os_str()), true).map_err(io::Error::other)
                })();
                CoTaskMemFree(Some(pidl.cast()));
                result
            })();
            windows::Win32::System::Com::CoUninitialize();
            result?;
        }
    }
    fs::create_dir_all(&data)?;
    fs::write(marker, b"")
}

/// A user folder such as the desktop, wherever it's been moved (e.g. OneDrive).
fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as _));
        text.map(PathBuf::from)
    }
}

fn desktop_shortcut() -> Option<PathBuf> {
    known_folder(&FOLDERID_Desktop).map(|d| d.join(format!("{APP_NAME}.lnk")))
}

/// Folders older versions saved data in, outside the install folder.
fn legacy_data_dirs() -> Vec<PathBuf> {
    ["APPDATA", "LOCALAPPDATA"]
        .iter()
        .filter_map(|var| env::var_os(var))
        .map(|p| PathBuf::from(p).join(APP_NAME))
        .collect()
}

/// Development builds run from Cargo's `target` folder and never install.
pub fn is_dev_build() -> bool {
    env::current_exe()
        .map(|exe| exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target")))
        .unwrap_or(false)
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
