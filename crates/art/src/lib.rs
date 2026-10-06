//! Artwork for games whose launcher keeps no cover art on disk: the game's own
//! icon, extracted by Windows' shell at the largest size available and cached
//! as a PNG so it only costs time once per game.

mod glyph;
pub use glyph::glyph;
use mulch_core::{Art, Game, Launcher};
use std::collections::hash_map::DefaultHasher;
use std::ffi::c_void;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP,
    HGDIOBJ, ReleaseDC,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::core::HSTRING;

const ICON_SIZE: i32 = 256;

/// Gives every game without art its icon, where one can be found. Slow-ish on
/// first run (shell calls), instant afterwards (cached PNGs).
pub fn fill_missing(games: &mut [Game]) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    for game in games.iter_mut() {
        match &game.art {
            None => {
                if let Some(icon) = game.icon_source.as_deref().and_then(cached_icon) {
                    game.art = Some(Art::Icon(icon));
                }
            }
            // Package logos often sit in a sea of transparent padding; trim it
            // so every icon fills its tile the same way.
            Some(Art::Icon(path)) => {
                if let Some(trimmed) = trimmed_png(path) {
                    game.art = Some(Art::Icon(trimmed));
                }
            }
            Some(Art::Cover(_)) => {}
        }
    }
}

/// Same for launcher buttons.
pub fn fill_launchers(launchers: &mut [Launcher]) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    for launcher in launchers.iter_mut() {
        launcher.icon = match &launcher.icon {
            None => launcher.icon_source.as_deref().and_then(cached_icon),
            Some(path) => trimmed_png(path).or_else(|| Some(path.clone())),
        };
    }
}

/// A copy of an icon at exactly `size` × `size` pixels (fitted and centred,
/// never stretched), cached. Shown at that size it's drawn pixel for pixel,
/// instead of being scaled on screen, which looks blurry and jagged.
pub fn sized(icon: &Path, size: u32) -> Option<PathBuf> {
    let out = cache_dir()?.join(format!("{}-{size}px.png", cache_key(icon)?));
    if out.is_file() {
        return Some(out);
    }
    let image = image::open(icon).ok()?.to_rgba8();
    let (w, h) = image.dimensions();
    let scale = size as f32 / w.max(h) as f32;
    let (fw, fh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    // Scaled with its colours premultiplied by their opacity, so edges don't
    // pick up dark fringes from the transparent pixels around them.
    let mut premultiplied = image;
    for p in premultiplied.pixels_mut() {
        let a = p[3] as f32 / 255.;
        for c in 0..3 {
            p[c] = (p[c] as f32 * a).round() as u8;
        }
    }
    let mut small = image::imageops::resize(&premultiplied, fw, fh, image::imageops::FilterType::Lanczos3);
    for p in small.pixels_mut() {
        let a = p[3] as f32 / 255.;
        if a > 0. {
            for c in 0..3 {
                p[c] = (p[c] as f32 / a).round().min(255.) as u8;
            }
        }
    }
    let mut square = image::RgbaImage::new(size, size);
    image::imageops::overlay(&mut square, &small, ((size - fw) / 2) as i64, ((size - fh) / 2) as i64);
    square.save(&out).ok()?;
    Some(out)
}

/// Cache key for a source file: its path and modification time, so an
/// updated game or launcher gets fresh art.
fn cache_key(source: &Path) -> Option<String> {
    let modified = fs::metadata(source).ok()?.modified().ok()?;
    let mut hasher = DefaultHasher::new();
    source.to_string_lossy().to_lowercase().hash(&mut hasher);
    modified.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// A copy of a PNG with its transparent border trimmed, cached.
fn trimmed_png(source: &Path) -> Option<PathBuf> {
    let dir = cache_dir()?;
    if source.starts_with(&dir) {
        return None; // already one of ours
    }
    let cached = dir.join(format!("{}-trim.png", cache_key(source)?));
    if cached.is_file() {
        return Some(cached);
    }
    let (width, height, rgba) = read_rgba_png(source)?;
    let (width, height, rgba) = trim_transparent(width, height, &rgba);
    fs::create_dir_all(&dir).ok()?;
    write_png(&cached, width, height, &rgba)?;
    Some(cached)
}

fn read_rgba_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).ok()?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::GrayscaleAlpha => pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        // No alpha channel means nothing to trim.
        _ => return None,
    };
    Some((info.width, info.height, rgba))
}

/// Crops away fully (or almost fully) transparent rows and columns.
fn trim_transparent(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let alpha = |x: u32, y: u32| rgba[((y * width + x) * 4 + 3) as usize];
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if alpha(x, y) > 8 {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    if left > right || top > bottom {
        return (width, height, rgba.to_vec());
    }
    let (new_width, new_height) = (right - left + 1, bottom - top + 1);
    let mut out = Vec::with_capacity((new_width * new_height * 4) as usize);
    for y in top..=bottom {
        let start = ((y * width + left) * 4) as usize;
        out.extend_from_slice(&rgba[start..start + (new_width * 4) as usize]);
    }
    (new_width, new_height, out)
}

fn cache_dir() -> Option<PathBuf> {
    mulch_core::paths::cache_dir("icons")
}

/// The cached PNG for an executable's (or .ico's) icon, extracting it on first use.
/// Keyed by path and modification time, so an updated game gets a fresh icon.
fn cached_icon(source: &Path) -> Option<PathBuf> {
    // Launchers often record paths with forward slashes, which the shell's
    // parsing-name lookup rejects.
    let source = PathBuf::from(source.to_string_lossy().replace('/', "\\"));
    let source = source.as_path();
    let cached = cache_dir()?.join(format!("{}-icon.png", cache_key(source)?));
    if cached.is_file() {
        return Some(cached);
    }

    let (width, height, rgba) = extract_icon(source)?;
    let (width, height, rgba) = trim_transparent(width, height, &rgba);
    fs::create_dir_all(cached.parent()?).ok()?;
    write_png(&cached, width, height, &rgba)?;
    Some(cached)
}

fn extract_icon(source: &Path) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(source.as_os_str()), None).ok()?;
        let bitmap =
            factory.GetImage(SIZE { cx: ICON_SIZE, cy: ICON_SIZE }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let pixels = bitmap_pixels(bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        pixels
    }
}

/// Reads a 32-bit bitmap's pixels as straight (non-premultiplied) RGBA.
unsafe fn bitmap_pixels(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let mut info = BITMAP::default();
        let read =
            GetObjectW(HGDIOBJ(bitmap.0), size_of::<BITMAP>() as i32, Some(&mut info as *mut BITMAP as *mut c_void));
        if read == 0 || info.bmWidth <= 0 || info.bmHeight <= 0 {
            return None;
        }
        let (width, height) = (info.bmWidth, info.bmHeight);

        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        if lines == 0 {
            return None;
        }

        // BGRA, premultiplied alpha -> RGBA, straight alpha.
        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
            let alpha = px[3] as u32;
            if alpha > 0 && alpha < 255 {
                for channel in &mut px[..3] {
                    *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
        }
        Some((width as u32, height as u32, pixels))
    }
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Option<()> {
    let file = fs::File::create(path).ok()?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(rgba).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_transparent_border() {
        // 4x4 image with one opaque pixel at (1, 2).
        let mut rgba = vec![0u8; 4 * 4 * 4];
        rgba[((2 * 4 + 1) * 4 + 3) as usize] = 255;
        let (w, h, out) = trim_transparent(4, 4, &rgba);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out[3], 255);
    }
}
