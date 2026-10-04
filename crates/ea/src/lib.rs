//! EA app. Only the launcher is detected: every known way of listing EA's
//! installed games (including all of Playnite's) needs an EA account login,
//! so EA games appear only when they're also in another library (e.g. Steam).

use mulch_core::registry;
use mulch_core::{Game, Launcher, Library, Platform, ScanContext};

pub struct Ea;

impl Library for Ea {
    fn id(&self) -> &'static str {
        "ea"
    }

    fn launcher(&self, cx: &ScanContext) -> Option<Launcher> {
        let exe = registry::launcher_exe(cx.uninstall_entries(), &["origin2", "link2ea"], Some("EA app"))?;
        Some(Launcher::from_exe(Platform::Ea, "EA app", exe))
    }

    fn games(&self, _: &ScanContext) -> Vec<Game> {
        Vec::new()
    }
}
