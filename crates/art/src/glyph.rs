//! One-colour glyphs of launcher icons, for showing inside glass buttons the
//! way the other button icons look.
//!
//! Most launcher icons are a logo on a coloured tile or disc (Xbox's X on
//! green, EA on blue): the glyph is the logo, the parts that stand out from
//! that background colour. Icons with no single background (a logo on
//! transparency) become their whole shape.

use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};

/// The glyph of `icon` in `ink` (RGB), cached next to it. None if unreadable.
pub fn glyph(icon: &Path, ink: [u8; 3]) -> Option<PathBuf> {
    let name = format!("{}-glyph3-{:02x}{:02x}{:02x}.png", icon.file_stem()?.to_string_lossy(), ink[0], ink[1], ink[2]);
    let out = icon.with_file_name(name);
    if out.is_file() {
        return Some(out);
    }
    let image = image::open(icon).ok()?.to_rgba8();
    smooth(&trim_square(&make_glyph(&image, ink))).save(&out).ok()?;
    Some(out)
}

fn make_glyph(image: &RgbaImage, ink: [u8; 3]) -> RgbaImage {
    let background = dominant_colour(image);
    let typical = typical_luminance(image);
    // A thin band just inside the icon's outline: the tile's own edge, not logo.
    let band = (image.width().min(image.height()) / 22).max(2) as i32;
    let near_edge = |x: u32, y: u32| {
        let (x, y) = (x as i32, y as i32);
        [(band, 0), (-band, 0), (0, band), (0, -band), (band, band), (-band, -band), (band, -band), (-band, band)]
            .iter()
            .any(|(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                nx < 0
                    || ny < 0
                    || nx >= image.width() as i32
                    || ny >= image.height() as i32
                    || image.get_pixel(nx as u32, ny as u32)[3] < 128
            })
    };
    let mut out = RgbaImage::new(image.width(), image.height());
    for (x, y, p) in image.enumerate_pixels() {
        let coverage = p[3] as f32 / 255.;
        let strength = match (background, typical) {
            // Away from the background colour = ink, ramped for smooth edges.
            (Some(bg), _) if !near_edge(x, y) => ((distance(p, bg) - 0.12) / 0.25).clamp(0., 1.),
            (Some(_), _) => 0.,
            // No single background (a gradient): keep what's clearly lighter or
            // darker than the icon's usual brightness.
            (None, Some(typical)) if !near_edge(x, y) => ((luminance(p) - typical).abs() - 0.2).clamp(0., 0.2) * 5.,
            (None, Some(_)) => 0.,
            (None, None) => 1.,
        };
        let alpha = (coverage * strength * 255.) as u8;
        out.put_pixel(x, y, Rgba([ink[0], ink[1], ink[2], alpha]));
    }
    // Icons that are just a shape on transparency (nothing stood out inside):
    // use the whole shape.
    if out.pixels().filter(|p| p[3] > 128).count() < 20 {
        for (x, y, p) in image.enumerate_pixels() {
            out.put_pixel(x, y, Rgba([ink[0], ink[1], ink[2], p[3]]));
        }
    }
    out
}

/// Glyphs are shown small (about 21 px), and shrinking a big image while
/// drawing it leaves jagged edges: so soften its edges a touch, then shrink
/// it ahead of time with a high-quality filter to a size that's still crisp
/// on high-DPI screens.
fn smooth(glyph: &RgbaImage) -> RgbaImage {
    let soft = image::imageops::blur(glyph, glyph.width() as f32 / 256.);
    let mut out = image::imageops::resize(&soft, GLYPH_SIZE, GLYPH_SIZE, image::imageops::FilterType::Lanczos3);
    // Keep the ink colour exact; only the alpha carries the shape.
    let ink = glyph.pixels().find(|p| p[3] > 0).map_or([0, 0, 0], |p| [p[0], p[1], p[2]]);
    for p in out.pixels_mut() {
        *p = Rgba([ink[0], ink[1], ink[2], p[3]]);
    }
    out
}

/// The size glyphs are stored at.
const GLYPH_SIZE: u32 = 64;

/// Crops away empty space around the glyph (where the icon's tile was),
/// keeping it square and centred so every glyph fills its button alike.
fn trim_square(glyph: &RgbaImage) -> RgbaImage {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (x, y, p) in glyph.enumerate_pixels() {
        if p[3] > 24 {
            let b = bounds.get_or_insert((x, y, x, y));
            *b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
    }
    let Some((left, top, right, bottom)) = bounds else { return glyph.clone() };
    let (w, h) = (right - left + 1, bottom - top + 1);
    let side = w.max(h);
    let mut out = RgbaImage::new(side, side);
    let (ox, oy) = ((side - w) / 2, (side - h) / 2);
    for y in 0..h {
        for x in 0..w {
            out.put_pixel(ox + x, oy + y, *glyph.get_pixel(left + x, top + y));
        }
    }
    out
}

fn luminance(p: &Rgba<u8>) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.
}

/// The median brightness of the icon's opaque pixels, if it has a solid
/// area (a tile or disc) for a logo to sit on.
fn typical_luminance(image: &RgbaImage) -> Option<f32> {
    let mut values: Vec<f32> = image.pixels().filter(|p| p[3] > 200).map(luminance).collect();
    let solid = values.len() as f32 / (image.width() * image.height()) as f32;
    if solid < 0.4 {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values[values.len() / 2])
}
/// The colour most of the icon's opaque pixels share, if one clearly
/// dominates (the tile or disc behind the logo).
fn dominant_colour(image: &RgbaImage) -> Option<[f32; 3]> {
    let mut counts = std::collections::HashMap::<(u8, u8, u8), (u32, [f32; 3])>::new();
    let mut opaque = 0u32;
    for p in image.pixels().filter(|p| p[3] > 200) {
        opaque += 1;
        let key = (p[0] >> 4, p[1] >> 4, p[2] >> 4);
        let entry = counts.entry(key).or_insert((0, [0.; 3]));
        entry.0 += 1;
        for k in 0..3 {
            entry.1[k] += p[k] as f32 / 255.;
        }
    }
    let (count, sum) = counts.into_values().max_by_key(|(count, _)| *count)?;
    (opaque > 0 && count as f32 / opaque as f32 > 0.3).then(|| sum.map(|c| c / count as f32))
}

fn distance(p: &Rgba<u8>, bg: [f32; 3]) -> f32 {
    let d: f32 = (0..3).map(|k| (p[k] as f32 / 255. - bg[k]).powi(2)).sum();
    d.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_logo_and_drops_its_tile() {
        // A green tile with a white bar across the middle.
        let mut tile = RgbaImage::from_pixel(32, 32, Rgba([16, 124, 16, 255]));
        for x in 4..28 {
            for y in 14..18 {
                tile.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let glyph = make_glyph(&tile, [0, 0, 0]);
        assert_eq!(glyph.get_pixel(16, 16)[3], 255);
        assert_eq!(glyph.get_pixel(2, 2)[3], 0);
    }
}
