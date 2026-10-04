//! Self-updating from GitHub releases.
//!
//! A release is tagged `vX.Y.Z` and has two assets: `MulchLauncher.exe` and
//! `MulchLauncher.exe.sha256` (its SHA-256, hex). Updating downloads the new
//! exe next to the running one, checks it against the checksum, then swaps
//! it in: Windows lets a running program be renamed, so the old one moves
//! aside to `MulchLauncher.old.exe` and the new one takes its name. The next
//! launch is the new version; the old file is deleted then.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where releases are published.
const LATEST_RELEASE: &str = "https://api.github.com/repos/apotenza92/mulch-launcher/releases/latest";
const EXE: &str = "MulchLauncher.exe";
const OLD_EXE: &str = "MulchLauncher.old.exe";
const NEW_EXE: &str = "MulchLauncher.new.exe";
/// A release exe is a few megabytes; anything far bigger is not one.
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    exe_url: String,
    checksum_url: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("MulchLauncher/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// The latest release, if it's newer than `current` (e.g. "0.1.0").
pub fn newer_release(current: &str) -> Option<Release> {
    let json: serde_json::Value = agent().get(LATEST_RELEASE).call().ok()?.into_json().ok()?;
    let release = parse_release(&json)?;
    is_newer(&release.version, current).then_some(release)
}

fn parse_release(json: &serde_json::Value) -> Option<Release> {
    if json["draft"].as_bool() == Some(true) || json["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let version = json["tag_name"].as_str()?.trim_start_matches('v').to_string();
    let asset = |name: &str| {
        json["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name))?["browser_download_url"]
            .as_str()
            .map(str::to_string)
    };
    Some(Release { version, exe_url: asset(EXE)?, checksum_url: asset(&format!("{EXE}.sha256"))? })
}

/// Whether version `a` is later than `b` (dotted numbers, e.g. "0.10.2").
pub fn is_newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parts(a), parts(b));
    (0..a.len().max(b.len()))
        .map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
        .find(|(x, y)| x != y)
        .is_some_and(|(x, y)| x > y)
}

/// Downloads `release`, checks it, and swaps it in for the exe in `install_dir`.
pub fn install(release: &Release, install_dir: &Path) -> io::Result<()> {
    let bytes = download(&release.exe_url)?;
    let expected = String::from_utf8_lossy(&download(&release.checksum_url)?)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    swap_in(&bytes, &expected, install_dir)
}

/// Checks ytes against expected (SHA-256, hex) and swaps them in as
/// the exe in install_dir.
fn swap_in(bytes: &[u8], expected: &str, install_dir: &Path) -> io::Result<()> {
    let actual: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    if expected.len() != 64 || actual != expected {
        return Err(io::Error::other("the download didn't match its checksum"));
    }
    let new = install_dir.join(NEW_EXE);
    fs::write(&new, bytes)?;
    let (current, old) = (install_dir.join(EXE), install_dir.join(OLD_EXE));
    let _ = fs::remove_file(&old);
    fs::rename(&current, &old)?;
    if let Err(err) = fs::rename(&new, &current) {
        // Put the old one back, so there's always a working program.
        let _ = fs::rename(&old, &current);
        return Err(err);
    }
    Ok(())
}

fn download(url: &str) -> io::Result<Vec<u8>> {
    let response = agent().get(url).call().map_err(io::Error::other)?;
    let mut bytes = Vec::new();
    response.into_reader().take(MAX_DOWNLOAD).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Removes what a previous update left behind (the old exe, a half download).
pub fn clean_up(install_dir: &Path) {
    for leftover in [OLD_EXE, NEW_EXE] {
        let _ = fs::remove_file(install_dir.join(leftover));
    }
}

/// The installed exe, to restart into after an update.
pub fn exe_in(install_dir: &Path) -> PathBuf {
    install_dir.join(EXE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn reads_a_release() {
        let json = serde_json::json!({
            "tag_name": "v0.3.1", "draft": false, "prerelease": false,
            "assets": [
                { "name": "MulchLauncher.exe", "browser_download_url": "https://x/MulchLauncher.exe" },
                { "name": "MulchLauncher.exe.sha256", "browser_download_url": "https://x/MulchLauncher.exe.sha256" }
            ]
        });
        let release = parse_release(&json).unwrap();
        assert_eq!(release.version, "0.3.1");
        assert!(parse_release(&serde_json::json!({ "tag_name": "v1", "assets": [] })).is_none());
    }

    #[test]
    fn swaps_in_a_checked_download_and_refuses_a_bad_one() {
        let dir = std::env::temp_dir().join("mulch-update-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(EXE), b"old").unwrap();
        let hash = |b: &[u8]| -> String { Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect() };

        // A download that doesn't match its checksum changes nothing.
        assert!(swap_in(b"tampered", &hash(b"new"), &dir).is_err());
        assert_eq!(fs::read(dir.join(EXE)).unwrap(), b"old");

        swap_in(b"new", &hash(b"new"), &dir).unwrap();
        assert_eq!(fs::read(dir.join(EXE)).unwrap(), b"new");
        assert_eq!(fs::read(dir.join(OLD_EXE)).unwrap(), b"old");
        clean_up(&dir);
        assert!(!dir.join(OLD_EXE).exists());
        let _ = fs::remove_dir_all(dir);
    }
}
