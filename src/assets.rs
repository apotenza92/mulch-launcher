//! The UI kit's built-in assets, plus a filled play icon of our own, based on
//! Lucide's (see `assets/icons/LICENSE-LUCIDE`).

use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

const OWN: &[(&str, &[u8])] = &[("mulch/play-filled.svg", include_bytes!("../assets/icons/play-filled.svg"))];

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
