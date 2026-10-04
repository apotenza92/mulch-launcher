//! The UI kit's built-in assets, plus a few icons of our own (from Lucide,
//! see `assets/icons/LICENSE-LUCIDE`) that the kit has but doesn't bundle.

use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

const OWN: &[(&str, &[u8])] = &[
    ("mulch/zoom-in.svg", include_bytes!("../assets/icons/zoom-in.svg")),
    ("mulch/zoom-out.svg", include_bytes!("../assets/icons/zoom-out.svg")),
    ("mulch/search.svg", include_bytes!("../assets/icons/search.svg")),
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
