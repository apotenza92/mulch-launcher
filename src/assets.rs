//! The UI kit's built-in assets, plus a few icons of our own (from Lucide,
//! see `assets/icons/LICENSE-LUCIDE`) that the kit has but doesn't bundle,
//! and a filled play icon based on Lucide's.

use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

const OWN: &[(&str, &[u8])] = &[
    ("mulch/monitor.svg", include_bytes!("../assets/icons/monitor.svg")),
    ("mulch/play-filled.svg", include_bytes!("../assets/icons/play-filled.svg")),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        match OWN.iter().find(|(own, _)| *own == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}
