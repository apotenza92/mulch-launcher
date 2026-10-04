//! Portrait posters for games whose launcher keeps no cover art on disk,
//! fetched once from public sources (no logins, no API keys) and cached.
//!
//! Sources, tried in order until one has a poster:
//! - Steam games: Steam's poster for the game's own app id.
//! - Xbox / Microsoft Store: Microsoft's store catalogue, by package family name.
//! - Any game: Steam's poster for a game with exactly the same name.
//! - Any game: Wikidata's record of the game's Steam app id. This also covers
//!   games no longer sold on Steam (e.g. Rocket League), whose posters Steam
//!   still hosts.
//! - Ubisoft: Ubisoft's own thumbnail, from its public CDN (not 2:3, so last).
//! - Any game: the portrait cover image on its English Wikipedia article,
//!   for games that were never on Steam.
//!
//! Name lookups try the game's name, then (for games added by hand) the name
//! its program gives itself, e.g. "World of Warcraft" for `Wow.exe`.
//!
//! Games with no poster anywhere keep their icon; the miss is remembered for
//! a week so startup doesn't keep asking.

use mulch_core::{Action, Art, Game, Platform};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const RETRY_MISSES_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const PARALLEL_FETCHES: usize = 6;

fn cache_dir() -> Option<PathBuf> {
    mulch_core::paths::cache_dir("posters")
}

fn cache_key(game: &Game) -> String {
    let mut hasher = DefaultHasher::new();
    game.id.hash(&mut hasher);
    game.name.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Gives games without cover art a poster: from the cache instantly, or by
/// fetching it (slow, network), several at a time.
pub fn fill_missing(games: &mut [Game]) {
    let Some(dir) = cache_dir() else { return };
    let _ = fs::create_dir_all(&dir);
    let ubisoft = ubisoft_thumbnails();

    let wanted: Vec<usize> = games
        .iter()
        .enumerate()
        .filter(|(_, g)| !matches!(g.art, Some(Art::Cover(_))))
        .map(|(ix, _)| ix)
        .collect();

    for batch in wanted.chunks(PARALLEL_FETCHES) {
        let found: Vec<(usize, Option<PathBuf>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|&ix| {
                    let game = &games[ix];
                    let dir = &dir;
                    let ubisoft = &ubisoft;
                    scope.spawn(move || (ix, poster_for(game, dir, ubisoft)))
                })
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        for (ix, poster) in found {
            if let Some(path) = poster {
                games[ix].art = Some(Art::Cover(path));
            }
        }
    }
}

fn poster_for(game: &Game, dir: &Path, ubisoft: &[(u64, String)]) -> Option<PathBuf> {
    let key = cache_key(game);
    if let Some(cached) = ["jpg", "png", "webp"].iter().map(|ext| dir.join(format!("{key}.{ext}"))).find(|p| p.is_file()) {
        return Some(cached);
    }
    let miss = dir.join(format!("{key}.none"));
    if recently_missed(&miss) {
        return None;
    }

    let own_source = match game.platform {
        Platform::Steam => game.id.strip_prefix("steam:").map(steam_cdn_poster),
        Platform::Xbox => xbox_poster_url(game),
        _ => None,
    };
    let names = search_names(game);
    let url = own_source
        .into_iter()
        .chain(names.iter().filter_map(|name| steam_poster_url(name)))
        .chain(names.iter().filter_map(|name| wikidata_steam_id(name).map(|id| steam_cdn_poster(&id))))
        .chain(
            std::iter::once_with(|| (game.platform == Platform::Ubisoft).then(|| ubisoft_thumbnail_url(game, ubisoft)).flatten())
                .flatten(),
        )
        .chain(names.iter().filter_map(|name| wikipedia_cover_url(name)));
    // Try each candidate in turn until one actually downloads as an image.
    let downloaded = url.into_iter().find_map(|url| download(&url, dir, &key));
    match downloaded {
        Some(path) => Some(path),
        None => {
            let _ = fs::write(&miss, b"");
            None
        }
    }
}

/// Names to search for: the game's, and for games added by hand, the
/// product name in its program's version information.
fn search_names(game: &Game) -> Vec<String> {
    let mut names = vec![game.name.clone()];
    if let (Platform::Manual, Action::Exe { path, .. }) = (game.platform, &game.launch) {
        if let Some(product) = mulch_core::exe_info::product_name(path) {
            if normalise(&product) != normalise(&game.name) {
                names.push(product);
            }
        }
    }
    names
}

/// The portrait image at the top of the game's English Wikipedia article
/// (usually its box art), found by searching for the name as a video game
/// and taking an article titled exactly that.
fn wikipedia_cover_url(name: &str) -> Option<String> {
    let wanted = normalise(name);
    if wanted.is_empty() {
        return None;
    }
    let result = get_json(&format!(
        "https://en.wikipedia.org/w/api.php?action=query&generator=search&gsrsearch={}&gsrlimit=5\
         &prop=pageimages&piprop=original&pilicense=any&format=json",
        url_encode(&format!("{} video game", search_term(name)))
    ))?;
    result["query"]["pages"].as_object()?.values().find_map(|page| {
        // "Doom (2016 video game)" counts as "Doom".
        let title = page["title"].as_str()?;
        let title = title.split(" (").next().unwrap_or(title);
        if normalise(title) != wanted {
            return None;
        }
        let image = &page["original"];
        let (width, height) = (image["width"].as_u64()?, image["height"].as_u64()?);
        (height > width).then(|| image["source"].as_str().map(str::to_string)).flatten()
    })
}

fn recently_missed(marker: &Path) -> bool {
    fs::metadata(marker)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < RETRY_MISSES_AFTER)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(TIMEOUT).user_agent("MulchLauncher/0.1 (https://github.com/apotenza92/mulch-launcher)").build()
}

fn get_json(url: &str) -> Option<Value> {
    agent().get(url).call().ok()?.into_json().ok()
}

/// Downloads an image into the cache as `<key>.<ext>`, with the extension
/// taken from the file's contents so the image loader never has to guess.
fn download(url: &str, dir: &Path, key: &str) -> Option<PathBuf> {
    let response = agent().get(url).call().ok()?;
    let mut bytes = Vec::new();
    response.into_reader().take(MAX_IMAGE_BYTES).read_to_end(&mut bytes).ok()?;
    let ext = image_extension(&bytes)?; // anything else is an error page, not an image
    let path = dir.join(format!("{key}.{ext}"));
    let partial = path.with_extension("part");
    fs::write(&partial, &bytes).ok()?;
    fs::rename(&partial, &path).ok()?;
    Some(path)
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 1024 {
        None
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG") {
        Some("png")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else {
        None
    }
}

/// The "Poster" image (2:3) from Microsoft's public store catalogue.
fn xbox_poster_url(game: &Game) -> Option<String> {
    let family_name = game.id.strip_prefix("xbox:")?;
    let catalog = get_json(&format!(
        "https://displaycatalog.mp.microsoft.com/v7.0/products/lookup?alternateId=PackageFamilyName\
         &value={family_name}&market=US&languages=en-us&fieldsTemplate=Details"
    ))?;
    let images = catalog["Products"][0]["LocalizedProperties"][0]["Images"].as_array()?;
    let poster = images.iter().find(|i| i["ImagePurpose"] == "Poster")?;
    let uri = poster["Uri"].as_str()?;
    Some(format!("https:{uri}?w=600"))
}

/// Steam's poster for a Steam app with exactly the same (normalised) name.
fn steam_poster_url(name: &str) -> Option<String> {
    let wanted = normalise(name);
    if wanted.is_empty() {
        return None;
    }
    let results = get_json(&format!("https://steamcommunity.com/actions/SearchApps/{}", url_encode(&search_term(name))))?;
    let app_id = results.as_array()?.iter().find_map(|app| {
        if normalise(app["name"].as_str()?) != wanted {
            return None;
        }
        // Steam returns the id as text, but accept a number too.
        app["appid"].as_str().map(str::to_string).or_else(|| app["appid"].as_u64().map(|id| id.to_string()))
    })?;
    Some(steam_cdn_poster(&app_id))
}

fn steam_cdn_poster(app_id: &str) -> String {
    format!("https://steamcdn-a.akamaihd.net/steam/apps/{app_id}/library_600x900_2x.jpg")
}

/// The Steam app id Wikidata records (property P1733) for a video game with
/// exactly this name.
fn wikidata_steam_id(name: &str) -> Option<String> {
    let wanted = normalise(name);
    let search = get_json(&format!(
        "https://www.wikidata.org/w/api.php?action=wbsearchentities&search={}&language=en&type=item&limit=7&format=json",
        url_encode(&search_term(name))
    ))?;
    let ids: Vec<&str> = search["search"]
        .as_array()?
        .iter()
        .filter(|hit| hit["label"].as_str().is_some_and(|label| normalise(label) == wanted))
        .filter(|hit| hit["description"].as_str().is_some_and(|d| d.to_lowercase().contains("video game")))
        .filter_map(|hit| hit["id"].as_str())
        .collect();
    if ids.is_empty() {
        return None;
    }
    let entities = get_json(&format!(
        "https://www.wikidata.org/w/api.php?action=wbgetentities&ids={}&props=claims&format=json",
        ids.join("|")
    ))?;
    ids.iter().find_map(|id| {
        entities["entities"][*id]["claims"]["P1733"]
            .as_array()?
            .iter()
            .find_map(|claim| claim["mainsnak"]["datavalue"]["value"].as_str().map(str::to_string))
    })
}

/// The name as typed into a search box: without trademark symbols, which
/// launchers add ("Rocket League®") but search engines don't index.
fn search_term(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '®' | '™' | '©')).collect::<String>().trim().to_string()
}

/// Lowercase letters and digits only, so "Call of Duty®" matches "Call of Duty".
fn normalise(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Ubisoft's thumbnail image for a game, by its Ubisoft id.
fn ubisoft_thumbnail_url(game: &Game, thumbnails: &[(u64, String)]) -> Option<String> {
    let id: u64 = game.id.strip_prefix("ubisoft:")?.parse().ok()?;
    let (_, file) = thumbnails.iter().find(|(uplay_id, _)| *uplay_id == id)?;
    Some(format!("https://ubistatic3-a.akamaihd.net/orbit/uplay_launcher_3_0/assets/{file}"))
}

/// (Ubisoft id, thumbnail file name) for every game in Ubisoft Connect's local
/// catalogue cache. The cache is protobuf: repeated field 1 of
/// `{ 1: uplay id, 2: install id, 3: YAML config }`; the YAML has `thumb_image:`.
fn ubisoft_thumbnails() -> Vec<(u64, String)> {
    let Some(path) = std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join(r"Ubisoft Game Launcher\cache\configuration\configurations"))
    else {
        return Vec::new();
    };
    let Ok(data) = fs::read(path) else { return Vec::new() };
    let mut found = Vec::new();
    for (field, value) in proto_fields(&data) {
        let ProtoValue::Bytes(game) = value else { continue };
        if field != 1 {
            continue;
        }
        let (mut id, mut yaml) = (None, None);
        for (field, value) in proto_fields(game) {
            match (field, value) {
                (1, ProtoValue::Varint(v)) => id = Some(v),
                (3, ProtoValue::Bytes(b)) => yaml = Some(String::from_utf8_lossy(b)),
                _ => {}
            }
        }
        let thumb = yaml.as_deref().and_then(|y| {
            y.lines().find_map(|line| {
                let value = line.trim().strip_prefix("thumb_image:")?.trim().trim_matches(['"', '\'']);
                (value.ends_with(".jpg") || value.ends_with(".png")).then(|| value.to_string())
            })
        });
        if let (Some(id), Some(thumb)) = (id, thumb) {
            found.push((id, thumb));
        }
    }
    found
}

enum ProtoValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

/// A minimal protobuf reader: varint and length-delimited fields only, which
/// is all Ubisoft's cache uses. Stops quietly at anything unexpected.
fn proto_fields(data: &[u8]) -> Vec<(u32, ProtoValue<'_>)> {
    fn varint(data: &[u8], pos: &mut usize) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *data.get(*pos)?;
            *pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                return Some(value);
            }
        }
        None
    }
    let mut fields = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let Some(key) = varint(data, &mut pos) else { break };
        let field = (key >> 3) as u32;
        match key & 7 {
            0 => match varint(data, &mut pos) {
                Some(v) => fields.push((field, ProtoValue::Varint(v))),
                None => break,
            },
            2 => {
                let Some(len) = varint(data, &mut pos) else { break };
                let end = pos.saturating_add(len as usize);
                let Some(bytes) = data.get(pos..end) else { break };
                fields.push((field, ProtoValue::Bytes(bytes)));
                pos = end;
            }
            _ => break,
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_ignoring_symbols_and_case() {
        assert_eq!(normalise("Call of Duty®"), normalise("call of duty"));
        assert_eq!(normalise("Battlefield™ 6"), "battlefield6");
        assert_ne!(normalise("Trackmania"), normalise("TrackMania Nations Forever"));
        assert_eq!(search_term("Rocket League®"), "Rocket League");
    }

    #[test]
    fn reads_nested_protobuf() {
        // field 1 { field 1: 5595, field 3: "thumb_image: a.jpg" }
        let yaml = b"thumb_image: a.jpg";
        let mut inner = vec![0x08, 0xDB, 0x2B, 0x1A, yaml.len() as u8];
        inner.extend_from_slice(yaml);
        let mut outer = vec![0x0A, inner.len() as u8];
        outer.extend_from_slice(&inner);
        let fields = proto_fields(&outer);
        let ProtoValue::Bytes(game) = fields[0].1 else { panic!() };
        let inner_fields = proto_fields(game);
        assert!(matches!(inner_fields[0], (1, ProtoValue::Varint(5595))));
    }
}
