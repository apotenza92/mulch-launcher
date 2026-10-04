//! Grid sizing: a pyramid of games, most recently played at the top.
//!
//! Built from the bottom up: the widest row at the bottom, each row above 2
//! games narrower (..., 9, 7, 5, 3), and the top row takes whatever is left
//! (split as 1 above the rest if that's even), so rows only ever grow
//! downwards and one game always sits in the middle. With more games than a pyramid holds, the widest row repeats at the
//! bottom. The widest row is whatever gives the biggest tiles that get every
//! game on screen. Every row is centred.
//!
//! Only when that would make tiles too small to read does the grid scroll
//! instead, with rows as wide as readable tiles allow.

/// Cover art is portrait 2:3.
pub const COVER_ASPECT: f32 = 1.5;
pub const GRID_GAP: f32 = 16.;
/// Name (text_sm) + platform (text_xs) + the padding above them.
pub const LABEL_HEIGHT: f32 = 50.;
/// Below this, names get too truncated to read, so the grid scrolls instead.
pub const MIN_TILE: f32 = 110.;

#[derive(Debug, Clone, PartialEq)]
pub struct GridLayout {
    pub tile_width: f32,
    /// How many games in each row, top to bottom.
    pub rows: Vec<usize>,
    /// True when the games don't all fit and the grid scrolls.
    pub scrolls: bool,
}

/// Widest tile that fits `columns` across `width`.
fn width_limit(width: f32, columns: usize) -> f32 {
    (width - (columns as f32 - 1.) * GRID_GAP) / columns as f32
}

/// Widest tile (labels included below it) that fits `rows` down `height`.
fn height_limit(height: f32, rows: usize) -> f32 {
    ((height - (rows as f32 - 1.) * GRID_GAP) / rows as f32 - LABEL_HEIGHT) / COVER_ASPECT
}

/// Games a pyramid with an odd `widest` row holds (1 + 3 + ... + widest).
fn capacity(widest: usize) -> usize {
    widest.div_ceil(2).pow(2)
}

/// Row sizes, top to bottom, for a pyramid whose bottom row has `widest` games.
fn pyramid(count: usize, widest: usize) -> Vec<usize> {
    let mut rows = Vec::new();
    let (mut left, mut size) = (count, widest);
    while left > 0 {
        let row = size.min(left);
        rows.push(row);
        left -= row;
        // Narrow by 2 once the rest fits in a pyramid of that width.
        if size > 2 && left <= capacity(size - 2) {
            size -= 2;
        }
    }
    rows.reverse();
    // An even top row has no middle game: lift one game above it.
    if rows.len() > 1 && rows[0] % 2 == 0 {
        rows[0] -= 1;
        rows.insert(0, 1);
    }
    rows
}

pub fn fit_tiles(count: usize, width: f32, height: f32) -> GridLayout {
    let count = count.max(1);
    let fits = |rows: &Vec<usize>| {
        let widest = rows.iter().copied().max().unwrap_or(1);
        width_limit(width, widest).min(height_limit(height, rows.len()))
    };
    // Every widest row (odd, so something is always in the middle); ties go
    // to the narrower, more pyramid-shaped one.
    let best = (1..=count + 1)
        .step_by(2)
        .map(|widest| pyramid(count, widest))
        .map(|rows| (fits(&rows), rows))
        .fold(None::<(f32, Vec<usize>)>, |best, option| match best {
            Some(best) if option.0 <= best.0 + 0.5 => Some(best),
            _ => Some(option),
        })
        .expect("at least one arrangement");

    if best.0 >= MIN_TILE {
        return GridLayout { tile_width: best.0, rows: best.1, scrolls: false };
    }
    // Too many games to show readably at once: rows as wide as readable
    // tiles allow, stretched to fill the width, and scroll.
    let fit = ((width + GRID_GAP) / (MIN_TILE + GRID_GAP)).floor() as usize;
    let widest = (fit - (fit + 1) % 2).max(1);
    let rows = pyramid(count, widest);
    let columns = rows.iter().copied().max().unwrap_or(1);
    GridLayout { tile_width: width_limit(width, columns).max(1.), rows, scrolls: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_size(layout: &GridLayout) -> (f32, f32) {
        let columns = layout.rows.iter().copied().max().unwrap() as f32;
        let rows = layout.rows.len() as f32;
        (
            columns * layout.tile_width + (columns - 1.) * GRID_GAP,
            rows * (layout.tile_width * COVER_ASPECT + LABEL_HEIGHT) + (rows - 1.) * GRID_GAP,
        )
    }

    #[test]
    fn pyramids_build_from_the_bottom() {
        assert_eq!(pyramid(17, 9), vec![1, 7, 9]);
        assert_eq!(pyramid(60, 9), vec![3, 5, 7, 9, 9, 9, 9, 9]);
        assert_eq!(pyramid(16, 7), vec![1, 3, 5, 7]);
        assert_eq!(pyramid(2, 99), vec![2]);
        assert_eq!(pyramid(17, 13), vec![1, 3, 13]);
    }

    #[test]
    fn every_game_is_placed() {
        for count in 1..60 {
            let layout = fit_tiles(count, 1200., 700.);
            assert_eq!(layout.rows.iter().sum::<usize>(), count);
        }
    }

    #[test]
    fn everything_fits_on_screen_when_it_can() {
        for (count, width, height) in [(17, 1200., 700.), (17, 900., 1000.), (5, 1600., 400.), (40, 2400., 1300.)] {
            let layout = fit_tiles(count, width, height);
            assert!(!layout.scrolls, "{count} games in {width}x{height}");
            let (w, h) = grid_size(&layout);
            assert!(w <= width + 0.01 && h <= height + 0.01, "{count} games in {width}x{height}: {w}x{h}");
        }
    }

    #[test]
    fn rows_grow_downwards_and_are_odd() {
        for count in 1..80 {
            let rows = fit_tiles(count, 1400., 800.).rows;
            assert!(rows.windows(2).all(|pair| pair[0] <= pair[1]), "{count} games: {rows:?}");
            assert!(rows.len() == 1 || rows.iter().all(|row| row % 2 == 1), "{count} games: {rows:?}");
        }
    }

    #[test]
    fn tiles_grow_with_the_window() {
        let mut last = 0.;
        for step in 0..200 {
            let size = 700. + step as f32 * 10.;
            let layout = fit_tiles(17, size * 1.5, size);
            if layout.scrolls {
                continue;
            }
            assert!(layout.tile_width >= last, "tiles shrank at {size}");
            last = layout.tile_width;
        }
    }

    #[test]
    fn too_many_games_scroll_with_readable_tiles() {
        let layout = fit_tiles(300, 1200., 700.);
        assert!(layout.scrolls);
        assert!(layout.tile_width >= MIN_TILE);
        assert!(grid_size(&layout).0 <= 1200.5);
    }
}
