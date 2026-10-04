//! Windows' Game Bar record: for each program Windows recognises as a game,
//! the full path of its executable and when it last ran, whichever launcher
//! (or none) started it. Kept per user in
//! `HKCU\System\GameConfigStore\Children\<id>` as `MatchedExeFullPath` and
//! `LastAccessed` (a FILETIME).

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

/// Seconds between 1601-01-01 (FILETIME) and 1970-01-01 (Unix).
const FILETIME_TO_UNIX: u64 = 11_644_473_600;

/// (lowercase exe path, last ran in Unix seconds) for every game Windows
/// has seen run.
pub fn last_run() -> Vec<(String, u64)> {
    let Ok(children) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"System\GameConfigStore\Children") else {
        return Vec::new();
    };
    children
        .enum_keys()
        .filter_map(Result::ok)
        .filter_map(|name| {
            let key = children.open_subkey(name).ok()?;
            let exe: String = key.get_value("MatchedExeFullPath").ok()?;
            let filetime: u64 = key.get_value("LastAccessed").ok()?;
            let unix = (filetime / 10_000_000).checked_sub(FILETIME_TO_UNIX)?;
            (!exe.is_empty() && unix > 0).then(|| (exe.to_lowercase(), unix))
        })
        .collect()
}
