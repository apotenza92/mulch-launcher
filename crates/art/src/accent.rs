//! A poster's accent colour: its most colourful prominent hue, for tinting
//! things around it (like its hover glow).

use std::path::Path;

/// The accent colour of an image, as RGB, brightened enough to glow.
/// None if it can't be read or has no colour to speak of.
pub fn accent(path: &Path) -> Option<[u8; 3]> {
    let image = image::open(path).ok()?.thumbnail(48, 72).to_rgba8();
    // Average the pixels, weighting colourful, mid-bright ones most, so a
    // grey or black background doesn't wash the result out.
    let (mut r, mut g, mut b, mut total) = (0f32, 0f32, 0f32, 0f32);
    for p in image.pixels() {
        if p[3] < 128 {
            continue;
        }
        let [pr, pg, pb] = [p[0] as f32 / 255., p[1] as f32 / 255., p[2] as f32 / 255.];
        let (max, min) = (pr.max(pg).max(pb), pr.min(pg).min(pb));
        let saturation = if max == 0. { 0. } else { (max - min) / max };
        let brightness = 1. - (max - 0.6).abs();
        let weight = saturation * saturation * brightness + 0.002;
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        total += weight;
    }
    if total <= 0. {
        return None;
    }
    let (r, g, b) = (r / total, g / total, b / total);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max - min < 0.06 {
        return None; // essentially grey
    }
    // Stretch to full saturation-ish and a bright-but-not-white level.
    let boost = |c: f32| (((c - min) / (max - min)) * 0.75 + 0.25) * 0.95;
    Some([boost(r), boost(g), boost(b)].map(|c| (c.clamp(0., 1.) * 255.) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_colour_of_a_mostly_red_image() {
        let path = std::env::temp_dir().join("mulch-accent-test.png");
        let mut img = image::RgbaImage::from_pixel(40, 60, image::Rgba([20, 20, 20, 255]));
        for x in 0..40 {
            for y in 0..30 {
                img.put_pixel(x, y, image::Rgba([220, 30, 30, 255]));
            }
        }
        img.save(&path).unwrap();
        let [r, g, b] = accent(&path).unwrap();
        assert!(r > 200 && g < 100 && b < 100, "{r} {g} {b}");
        let _ = std::fs::remove_file(path);
    }
}
