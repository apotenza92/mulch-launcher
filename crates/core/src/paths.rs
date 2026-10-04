//! Where MulchLauncher keeps everything it saves.
//!
//! - Installed from GitHub: a `data` folder next to its own executable
//!   (inside the install folder), so uninstalling removes all of it with the
//!   program and nothing is left elsewhere on the PC.
//! - From the Microsoft Store: the program's folder is read-only, so data
//!   goes in `%LOCALAPPDATA%\MulchLauncher`, which Windows keeps in the
//!   package's own storage and removes when it's uninstalled.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Whether this copy runs as a Microsoft Store (MSIX) package. The Store
/// then installs, updates and uninstalls it.
pub fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
        let mut len = 0u32;
        // Fails with "no package" for ordinary programs; otherwise asks for room for the name.
        let result = unsafe { GetCurrentPackageFullName(&mut len, None) };
        result.0 != 15700 // APPMODEL_ERROR_NO_PACKAGE
    })
}

/// Settings, play history and games added by hand.
pub fn data_dir() -> Option<PathBuf> {
    if is_packaged() {
        return Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("MulchLauncher"));
    }
    Some(std::env::current_exe().ok()?.parent()?.join("data"))
}

/// `data\cache\<name>`: downloaded posters, extracted icons. Safe to delete.
pub fn cache_dir(name: &str) -> Option<PathBuf> {
    Some(data_dir()?.join("cache").join(name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn tests_are_not_packaged() {
        assert!(!super::is_packaged());
    }
}
