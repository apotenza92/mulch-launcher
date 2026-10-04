//! Battle.net: each installed game has a Windows uninstall entry whose
//! uninstall command contains `--uid=<internal id>`. The internal id maps to a
//! product code (used to launch it) through the list below, taken from
//! Playnite's Battle.net library.
//!
//! When each game was last played comes from Battle.net's own config
//! (`%APPDATA%\Battle.net\Battle.net.config`, `Games > <key> > LastPlayed`,
//! keyed by the same internal id as `ServerUid`).

use mulch_core::registry::{self, UninstallEntry};
use mulch_core::{Action, Game, Launcher, Library, Platform, ScanContext};
use std::collections::HashMap;
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

pub struct BattleNet;

impl Library for BattleNet {
    fn id(&self) -> &'static str {
        "battlenet"
    }

    fn launcher(&self, cx: &ScanContext) -> Option<Launcher> {
        Some(Launcher::from_exe(Platform::BattleNet, "Battle.net", client_exe(cx.uninstall_entries())?))
    }

    fn games(&self, cx: &ScanContext) -> Vec<Game> {
        scan(cx.uninstall_entries())
    }
}

fn scan(entries: &[UninstallEntry]) -> Vec<Game> {
    let Some(client) = client_exe(entries) else { return Vec::new() };
    let last_played = config_last_played();
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

        let mut game = Game::new(
            format!("battlenet:{code}"),
            name.to_string(),
            Platform::BattleNet,
            Some(install_dir),
            // Same form Playnite uses: `Battle.net.exe --exec="launch <code>"`.
            Action::CommandLine(format!("\"{}\" --exec=\"launch {code}\"", client.display())),
        );
        // `battlenet://<code>` opens the game's tab in Battle.net.
        game.show_in_launcher = Some(Action::Uri(format!("battlenet://{code}")));
        game.icon_source = registry::icon_path(&entry.display_icon);
        game.last_played = last_played.get(&uid.to_lowercase()).copied();
        games.push(game);
    }
    games
}

/// Internal id (lowercase) -> last played (Unix seconds), from Battle.net's config.
fn config_last_played() -> HashMap<String, u64> {
    let Some(appdata) = std::env::var_os("APPDATA") else { return HashMap::new() };
    let path = PathBuf::from(appdata).join(r"Battle.net\Battle.net.config");
    let Some(config) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()) else {
        return HashMap::new();
    };
    parse_last_played(&config)
}

fn parse_last_played(config: &serde_json::Value) -> HashMap<String, u64> {
    let Some(games) = config["Games"].as_object() else { return HashMap::new() };
    games
        .iter()
        .filter_map(|(key, game)| {
            let when = &game["LastPlayed"];
            let when = when.as_u64().or_else(|| when.as_str()?.trim().parse().ok())?;
            // Seconds, though accept milliseconds too.
            let when = if when > 100_000_000_000 { when / 1000 } else { when };
            let uid = game["ServerUid"].as_str().unwrap_or(key);
            (when > 0).then(|| (uid.to_lowercase(), when))
        })
        .collect()
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
    fn reads_last_played_from_config() {
        let config = serde_json::json!({ "Games": {
            "battle_net": { "ServerUid": "battle.net", "Resumable": "false" },
            "wow": { "ServerUid": "wow_enus", "LastPlayed": "1759500000" },
            "prometheus": { "LastPlayed": 1759400000000u64 }
        }});
        let played = parse_last_played(&config);
        assert_eq!(played.get("wow_enus"), Some(&1759500000));
        assert_eq!(played.get("prometheus"), Some(&1759400000));
        assert_eq!(played.len(), 2);
    }

    #[test]
    fn reads_uid_from_uninstall_command() {
        let cmd = r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=enUS --uid=prometheus --displayname="Overwatch""#;
        assert_eq!(uid(cmd), Some("prometheus"));
    }
}
