//! Where MulchLauncher keeps everything it saves: a `data` folder next to its
//! own executable (inside the install folder), so uninstalling removes all of
//! it with the program and nothing is left elsewhere on the PC.

use std::path::PathBuf;

/// `<folder of MulchLauncher.exe>\data`: settings, play history, games added
/// by hand.
pub fn data_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("data"))
}

/// `data\cache\<name>`: downloaded posters, extracted icons. Safe to delete.
pub fn cache_dir(name: &str) -> Option<PathBuf> {
    Some(data_dir()?.join("cache").join(name))
}
