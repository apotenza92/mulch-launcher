//! A poster's glow: a soft halo in its own colours, edge by edge, for
//! showing behind it on hover (like a TV's ambient backlight).

use image::{GrayImage, Luma, Rgba, RgbaImage};
use std::path::{Path, PathBuf};

/// The poster is shrunk to this size (2:3, as tiles are) to make its glow.
pub const GLOW_WIDTH: u32 = 90;
const GLOW_HEIGHT: u32 = 135;
/// How far the halo reaches past each edge, in the shrunk poster's pixels.
pub const GLOW_PAD: u32 = 8;
/// How soft it is.
const GLOW_BLUR: f32 = 4.;

/// The glow image for `cover`, cached in MulchLauncher's own cache (never
/// beside the cover, which may be another launcher's file): `GLOW_PAD`
/// bigger on each side than the poster (at `GLOW_WIDTH` wide), each edge's
/// colours spilling outward and fading. None if the cover can't be read.
pub fn glow(cover: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let dir = mulch_core::paths::cache_dir("glows")?;
    std::fs::create_dir_all(&dir).ok()?;
    // Named by the cover's path and when it last changed, so a new poster gets a new glow.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cover.hash(&mut hasher);
    std::fs::metadata(cover).ok()?.modified().ok()?.hash(&mut hasher);
    let out = dir.join(format!("{:016x}-glow2.png", hasher.finish()));
    if out.is_file() {
        return Some(out);
    }
    let poster = image::open(cover).ok()?.resize_exact(GLOW_WIDTH, GLOW_HEIGHT, image::imageops::FilterType::Triangle);
    make_glow(&poster.to_rgba8()).save(&out).ok()?;
    Some(out)
}

fn make_glow(poster: &RgbaImage) -> RgbaImage {
    let (w, h) = (poster.width() + 2 * GLOW_PAD, poster.height() + 2 * GLOW_PAD);
    // The poster with its edge pixels stretched out into the padding, then
    // blurred: each edge's own colours, carried past it.
    let pad = GLOW_PAD as i64;
    let extended = RgbaImage::from_fn(w, h, |x, y| {
        let px = (x as i64 - pad).clamp(0, poster.width() as i64 - 1) as u32;
        let py = (y as i64 - pad).clamp(0, poster.height() as i64 - 1) as u32;
        let p = poster.get_pixel(px, py);
        Rgba([p[0], p[1], p[2], 255])
    });
    let colours = image::imageops::blur(&extended, GLOW_BLUR);
    // Fading out past the poster's edge: a blurred copy of its outline.
    let outline = GrayImage::from_fn(w, h, |x, y| {
        let inside = x >= GLOW_PAD && x < w - GLOW_PAD && y >= GLOW_PAD && y < h - GLOW_PAD;
        Luma([if inside { 255 } else { 0 }])
    });
    let fade = image::imageops::blur(&outline, GLOW_BLUR);
    // Fully clear at the image's own edge: stretched over the screen, any
    // colour left in its outermost pixels would show as a hard line.
    let clear = |x: u32, y: u32| x < GLOW_CLEAR || y < GLOW_CLEAR || x >= w - GLOW_CLEAR || y >= h - GLOW_CLEAR;
    RgbaImage::from_fn(w, h, |x, y| {
        if clear(x, y) {
            return Rgba([0, 0, 0, 0]);
        }
        let c = colours.get_pixel(x, y);
        Rgba([c[0], c[1], c[2], fade.get_pixel(x, y)[0]])
    })
}
/// How many pixels at the glow image's edge are left fully clear.
const GLOW_CLEAR: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_each_edge_colour_outward_and_fades() {
        // Red on the left half, blue on the right.
        let poster = RgbaImage::from_fn(GLOW_WIDTH, GLOW_HEIGHT, |x, _| {
            if x < GLOW_WIDTH / 2 { Rgba([220, 20, 20, 255]) } else { Rgba([20, 20, 220, 255]) }
        });
        let glow = make_glow(&poster);
        let mid = glow.height() / 2;
        let left = glow.get_pixel(GLOW_PAD / 2, mid);
        let right = glow.get_pixel(glow.width() - GLOW_PAD / 2, mid);
        assert!(left[0] > 150 && left[2] < 80, "left {left:?}");
        assert!(right[2] > 150 && right[0] < 80, "right {right:?}");
        // Clear at the very edge, solid well inside.
        assert_eq!(glow.get_pixel(0, mid)[3], 0);
        assert_eq!(glow.get_pixel(glow.width() - 1, mid)[3], 0);
        assert_eq!(glow.get_pixel(glow.width() / 2, mid)[3], 255);
    }
}
