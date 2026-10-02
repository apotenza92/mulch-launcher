//! Battle.net: each installed game has a Windows uninstall entry whose
//! uninstall command contains `--uid=<internal id>`. The internal id maps to a
//! product code (used to launch it) through the list below, taken from
//! Playnite's Battle.net library.

use super::registry::{self, UninstallEntry};
use super::{Action, Game, Platform};
use std::path::PathBuf;

/// (product code, internal id prefix, name). Matched by prefix because ids
/// carry region/branch suffixes, e.g. `wow_enus`.
const PRODUCTS: &[(&str, &str, &str)] = &[
    ("WoW", "wow", "World of Warcraft"),
    ("D3", "diablo3", "Diablo III"),
    ("S2", "s2", "StarCraft II"),
    ("S1", "s1", "StarCraft"),
    ("WTCG", "hs_beta", "Hearthstone"),
    ("Hero", "heroes", "Heroes of the Storm"),
    ("Pro", "prometheus", "Overwatch 2"),
    ("VIPR", "viper", "Call of Duty: Black Ops 4"),
    ("ODIN", "odin", "Call of Duty: Modern Warfare"),
    ("W3", "w3", "Warcraft III: Reforged"),
    ("LAZR", "lazarus", "Call of Duty: Modern Warfare 2 Campaign Remastered"),
    ("ZEUS", "zeus", "Call of Duty: Black Ops Cold War"),
    ("WLBY", "wlby", "Crash Bandicoot 4: It's About Time"),
    ("OSI", "osi", "Diablo II: Resurrected"),
    ("RTRO", "rtro", "Blizzard Arcade Collection"),
    ("FORE", "fore", "Call of Duty: Vanguard"),
    ("ANBS", "anbs", "Diablo Immortal"),
    ("AUKS", "auks", "Call of Duty"),
    ("Fen", "fen", "Diablo IV"),
    ("D1", "d1", "Diablo"),
    ("W1R", "w1r", "Warcraft: Remastered"),
    ("W2R", "w2r", "Warcraft II: Remastered"),
    ("W1", "w1", "Warcraft: Orcs & Humans"),
    ("W2", "w2", "Warcraft II: Battle.net Edition"),
    ("GRY", "gryphon", "Warcraft Rumble"),
    ("ARIS", "aris", "DOOM: The Dark Ages"),
    ("SCOR", "scorpio", "Sea of Thieves"),
    ("ARK", "arkansas", "The Outer Worlds 2"),
    ("LBRA", "libra", "Tony Hawk's Pro Skater 3 + 4"),
    ("PNTA", "pinta", "Call of Duty: Modern Warfare III"),
    ("AQUA", "aqua", "Avowed"),
];

pub fn scan(entries: &[UninstallEntry]) -> Vec<Game> {
    let Some(client) = client_exe(entries) else { return Vec::new() };
    let mut games = Vec::new();

    for entry in entries {
        let Some(uid) = uid(&entry.uninstall_string) else { continue };
        if uid.eq_ignore_ascii_case("battle.net") || entry.display_name.ends_with("Test") || entry.display_name.ends_with("Beta") {
            continue;
        }
        // Longest matching prefix wins, so `w1r` isn't mistaken for `w1`.
        let Some(&(code, _, name)) = PRODUCTS
            .iter()
            .filter(|(_, internal, _)| uid.to_lowercase().starts_with(internal))
            .max_by_key(|(_, internal, _)| internal.len())
        else {
            continue;
        };
        let install_dir = registry::clean_path(&entry.install_location);
        if !install_dir.is_dir() || games.iter().any(|g: &Game| g.id == format!("battlenet:{code}")) {
            continue;
        }

        games.push(Game {
            id: format!("battlenet:{code}"),
            name: name.to_string(),
            platform: Platform::BattleNet,
            install_dir: Some(install_dir),
            launch: Action::Exe { path: client.clone(), args: vec![format!("--exec=launch {code}")], working_dir: None },
            uninstall: registry::exe_from_command(&entry.uninstall_string).map(|exe| Action::Exe {
                path: exe,
                args: command_args(&entry.uninstall_string),
                working_dir: None,
            }),
            art: None,
        });
    }
    games
}

/// `--uid=<id>` from a Battle.net uninstall command.
fn uid(uninstall_string: &str) -> Option<&str> {
    let lower = uninstall_string.to_ascii_lowercase();
    if !lower.contains("battle.net") {
        return None;
    }
    let start = lower.find("--uid=")? + "--uid=".len();
    let rest = &uninstall_string[start..];
    Some(rest.split_whitespace().next()?.trim_matches('"'))
}

/// The arguments after the executable in a command line.
fn command_args(command: &str) -> Vec<String> {
    let command = command.trim();
    let rest = if let Some(stripped) = command.strip_prefix('"') {
        stripped.split_once('"').map(|(_, rest)| rest).unwrap_or("")
    } else {
        let lower = command.to_ascii_lowercase();
        lower.find(".exe").map(|i| &command[i + 4..]).unwrap_or("")
    };
    rest.split_whitespace().map(|a| a.trim_matches('"').to_string()).collect()
}

fn client_exe(entries: &[UninstallEntry]) -> Option<PathBuf> {
    registry::protocol_handler_exe("battlenet").or_else(|| {
        entries
            .iter()
            .find(|e| e.uninstall_string.to_ascii_lowercase().contains("--uid=battle.net"))
            .map(|e| registry::clean_path(&e.install_location).join("Battle.net.exe"))
            .filter(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_uid_from_uninstall_command() {
        let cmd = r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=enUS --uid=prometheus --displayname="Overwatch""#;
        assert_eq!(uid(cmd), Some("prometheus"));
        assert_eq!(command_args(cmd)[0], "--lang=enUS");
    }
}
