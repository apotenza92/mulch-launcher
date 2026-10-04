//! When each game was last played, for sorting by recency, from every
//! source that records it reliably on this PC; the latest wins:
//! - The game's launcher, where it keeps that locally (Steam).
//! - Windows' Game Bar, which notes when each program it recognises as a
//!   game last ran, whichever launcher started it (see `game_bar`).
//! - MulchLauncher's own tracking, which covers everything else.
//!
//! A game counts as played when it's started from MulchLauncher, or whenever
//! a running process's executable lives in one of the game's folders, which
//! also catches games started from their own launcher while Mulch is open.
//! Stored in `data\history.json` next to MulchLauncher.exe.

use mulch_core::Game;
use serde::{Deserialize, Serialize};
mod game_bar;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct History {
    /// Game id -> last played, Unix seconds.
    last_played: HashMap<String, u64>,
    /// Game Bar's record: (lowercase exe path, last ran), re-read on demand.
    #[serde(skip)]
    game_bar: Vec<(String, u64)>,
}

fn path() -> Option<PathBuf> {
    mulch_core::paths::data_dir().map(|d| d.join("history.json"))
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl History {
    pub fn load() -> Self {
        let mut history: Self = path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        history.refresh_windows_record();
        history
    }

    /// Re-reads Windows' record of games run (cheap: one registry key).
    pub fn refresh_windows_record(&mut self) {
        self.game_bar = game_bar::last_run();
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }

    pub fn record(&mut self, game_id: &str, when: u64) {
        let entry = self.last_played.entry(game_id.to_string()).or_default();
        *entry = (*entry).max(when);
    }

    /// The latest of Mulch's record, the launcher's and Windows'.
    pub fn last_played(&self, game: &Game) -> Option<u64> {
        let dirs: Vec<String> = game
            .process_dirs
            .iter()
            .map(|d| format!("{}\\", d.to_string_lossy().trim_end_matches('\\').to_lowercase()))
            .collect();
        let windows = self
            .game_bar
            .iter()
            .filter(|(exe, _)| dirs.iter().any(|dir| exe.starts_with(dir.as_str())))
            .map(|(_, when)| *when)
            .max();
        self.last_played.get(&game.id).copied().max(game.last_played).max(windows)
    }

    /// Sorts most recently played first; never-played games follow A–Z.
    pub fn sort(&self, games: &mut [Game]) {
        games.sort_by(|a, b| {
            let (pa, pb) = (self.last_played(a), self.last_played(b));
            pb.cmp(&pa).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }
}

/// Ids of games with a process currently running from one of their folders.
pub fn running_games(games: &[Game]) -> Vec<String> {
    let running: Vec<String> = process_paths().into_iter().map(|p| p.to_lowercase()).collect();
    games
        .iter()
        .filter(|game| {
            game.process_dirs.iter().any(|dir| {
                let dir = format!("{}\\", dir.display().to_string().to_lowercase().trim_end_matches('\\'));
                running.iter().any(|exe| exe.starts_with(&dir))
            })
        })
        .map(|game| game.id.clone())
        .collect()
}

/// Full executable paths of running processes (those we're allowed to query).
fn process_paths() -> Vec<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::core::PWSTR;

    let mut paths = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return paths };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID) {
                    let mut buffer = [0u16; 1024];
                    let mut len = buffer.len() as u32;
                    if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len).is_ok() {
                        paths.push(String::from_utf16_lossy(&buffer[..len as usize]));
                    }
                    let _ = CloseHandle(process);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use mulch_core::{Action, Platform};

    fn game(id: &str, name: &str) -> Game {
        Game::new(id.into(), name.into(), Platform::Manual, None, Action::Uri(String::new()))
    }

    #[test]
    fn latest_source_wins() {
        let mut history = History::default();
        history.record("a", 100);
        history.game_bar = vec![(r"d:\games\alpha\bin\alpha.exe".into(), 300)];
        let mut g = game("a", "Alpha");
        g.process_dirs = vec![r"D:\Games\Alpha".into()];
        g.last_played = Some(200);
        assert_eq!(history.last_played(&g), Some(300));
        // A folder that merely starts the same isn't the game's.
        g.process_dirs = vec![r"D:\Games\Alp".into()];
        assert_eq!(history.last_played(&g), Some(200));
    }

    #[test]
    fn sorts_by_recency_then_name() {
        let mut history = History::default();
        history.record("b", 300);
        history.record("c", 200);
        let mut games = vec![game("a", "Alpha"), game("b", "Bravo"), game("c", "Charlie"), game("d", "delta")];
        history.sort(&mut games);
        let order: Vec<&str> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(order, ["Bravo", "Charlie", "Alpha", "delta"]);
    }

    #[test]
    fn finds_this_test_process() {
        // The test binary itself is running from target/; treat that as a game folder.
        let exe = std::env::current_exe().unwrap();
        let mut g = game("self", "Self");
        g.process_dirs = vec![exe.parent().unwrap().to_path_buf()];
        assert_eq!(running_games(&[g]), vec!["self".to_string()]);
    }
}
