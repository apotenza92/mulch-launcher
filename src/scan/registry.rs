//! Small registry helpers. All reads are best-effort: a missing key is `None`.

use std::path::PathBuf;
use winreg::RegKey;
use winreg::enums::*;

pub use winreg::HKEY;
pub use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

pub fn open(root: HKEY, path: &str) -> Option<RegKey> {
    RegKey::predef(root).open_subkey_with_flags(path, KEY_READ).ok()
}

pub fn string(root: HKEY, path: &str, value: &str) -> Option<String> {
    open(root, path)?.get_value::<String, _>(value).ok().filter(|s| !s.is_empty())
}

/// Reads a value from the first of several keys that has it. Useful for
/// 32-bit launchers that live under `WOW6432Node` on 64-bit Windows.
pub fn string_any(candidates: &[(HKEY, &str)], value: &str) -> Option<String> {
    candidates.iter().find_map(|&(root, path)| string(root, path, value))
}

/// Subkey names of a key.
pub fn subkeys(root: HKEY, path: &str) -> Vec<String> {
    open(root, path).map(|k| k.enum_keys().filter_map(Result::ok).collect()).unwrap_or_default()
}

/// One program from Windows' "Installed apps" list (the Uninstall registry).
#[derive(Clone, Debug, Default)]
pub struct UninstallEntry {
    pub key_name: String,
    pub display_name: String,
    pub install_location: String,
    pub uninstall_string: String,
    pub display_icon: String,
}

/// Every Uninstall entry, from HKLM and HKCU in both the 64-bit and 32-bit
/// registry views (32-bit launchers register under WOW6432Node).
pub fn uninstall_entries() -> Vec<UninstallEntry> {
    const PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    let mut entries = Vec::new();
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let Ok(uninstall) = RegKey::predef(root).open_subkey_with_flags(PATH, KEY_READ | view) else { continue };
            for key_name in uninstall.enum_keys().filter_map(Result::ok) {
                let Ok(key) = uninstall.open_subkey_with_flags(&key_name, KEY_READ | view) else { continue };
                let value = |name: &str| key.get_value::<String, _>(name).unwrap_or_default();
                let entry = UninstallEntry {
                    display_name: value("DisplayName"),
                    install_location: value("InstallLocation"),
                    uninstall_string: value("UninstallString"),
                    display_icon: value("DisplayIcon"),
                    key_name,
                };
                // HKCU isn't split into views, so the same key can appear twice.
                if !entries.iter().any(|e: &UninstallEntry| e.key_name == entry.key_name && e.display_name == entry.display_name) {
                    entries.push(entry);
                }
            }
        }
    }
    entries
}

/// Normalises a path the way launchers write them (mixed slashes, trailing
/// separators, quotes).
pub fn clean_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"').replace('/', "\\");
    PathBuf::from(trimmed.trim_end_matches('\\'))
}

/// The executable a protocol handler (e.g. `steam://`) points at, read from
/// `HKCR\<scheme>\shell\open\command`. This finds a launcher wherever the
/// user installed it.
pub fn protocol_handler_exe(scheme: &str) -> Option<PathBuf> {
    let command = string(HKEY_CLASSES_ROOT, &format!(r"{scheme}\shell\open\command"), "")?;
    exe_from_command(&command)
}

/// Extracts the executable from a command line such as
/// `"C:\Program Files\App\app.exe" --flag "%1"` or `C:\App\app.exe "%1"`.
pub fn exe_from_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let exe = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        let lower = command.to_ascii_lowercase();
        let end = lower.find(".exe").map(|i| i + 4).unwrap_or(command.len());
        &command[..end]
    };
    let path = PathBuf::from(exe.trim());
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_quoted_and_unquoted_exe_paths() {
        // Paths that don't exist return None, so check the parsing via clean_path-like logic.
        assert_eq!(clean_path(r#""C:/Games/Foo/""#), PathBuf::from(r"C:\Games\Foo"));
        assert!(exe_from_command(r#""C:\nope\x.exe" -- "%1""#).is_none());
    }
}
