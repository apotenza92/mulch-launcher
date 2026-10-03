//! Grid sizing, like a responsive web grid: tiles always stretch to fill the
//! window's width exactly, and the column count is the fewest columns that
//! fit every game in the window's height. Wider means bigger tiles until a row
//! no longer fits, then exactly one more column. When even minimum-size tiles
//! can't fit everything, the grid fills the width at minimum size and scrolls.
//!
//! The result depends only on the window size and game count, so the same size
//! always gives the same layout, and tiles grow and shrink smoothly between
//! column changes.

/// Cover art is portrait 2:3.
pub const COVER_ASPECT: f32 = 1.5;
pub const GRID_GAP: f32 = 16.;
/// Name (text_sm) + platform (text_xs) + the padding above them.
pub const LABEL_HEIGHT: f32 = 50.;
/// Tiles never grow past this; with very few games they stop filling the
/// width and sit centred instead.
pub const MAX_TILE: f32 = 260.;
/// Smallest tile width before the grid scrolls rather than shrinking further.
pub const MIN_TILE: f32 = 100.;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLayout {
    pub tile_width: f32,
    pub columns: usize,
    /// True when the games don't all fit and the grid scrolls.
    pub scrolls: bool,
}

/// Width of each tile when `columns` tiles fill `width` exactly.
fn filling_width(width: f32, columns: usize) -> f32 {
    (width - (columns as f32 - 1.) * GRID_GAP) / columns as f32
}

fn grid_height(count: usize, columns: usize, tile_width: f32) -> f32 {
    let rows = count.div_ceil(columns).max(1) as f32;
    rows * (tile_width * COVER_ASPECT + LABEL_HEIGHT) + (rows - 1.) * GRID_GAP
}

pub fn fit_tiles(count: usize, width: f32, height: f32) -> GridLayout {
    let count = count.max(1);
    for columns in 1..=count {
        let tile_width = filling_width(width, columns);
        if tile_width > MAX_TILE {
            continue; // tiles would be huge; more columns
        }
        if tile_width < MIN_TILE {
            break; // too small to be readable; scroll instead
        }
        if grid_height(count, columns, tile_width) <= height {
            return GridLayout { tile_width, columns, scrolls: false };
        }
    }

    // Few games in a wide window: every game in one row at the largest size.
    if filling_width(width, count) > MAX_TILE && grid_height(count, count, MAX_TILE) <= height {
        return GridLayout { tile_width: MAX_TILE, columns: count, scrolls: false };
    }

    // Doesn't fit: fill the width with as many minimum-size columns as fit.
    let columns = (((width + GRID_GAP) / (MIN_TILE + GRID_GAP)).floor() as usize).clamp(1, count);
    let tile_width = filling_width(width, columns).clamp(MIN_TILE.min(width), MAX_TILE);
    let scrolls = grid_height(count, columns, tile_width) > height;
    GridLayout { tile_width, columns, scrolls }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fills_width(layout: GridLayout, width: f32) -> bool {
        (layout.columns as f32 * (layout.tile_width + GRID_GAP) - GRID_GAP - width).abs() < 0.5
    }

    #[test]
    fn fits_and_fills_the_window() {
        let layout = fit_tiles(17, 1200., 700.);
        assert!(!layout.scrolls);
        assert!(fills_width(layout, 1200.));
        assert!(grid_height(17, layout.columns, layout.tile_width) <= 700.);
    }

    #[test]
    fn same_size_same_layout_and_wider_never_means_fewer_columns() {
        assert_eq!(fit_tiles(17, 1000., 700.), fit_tiles(17, 1000., 700.));
        let mut last_columns = 0;
        for width in (600..2400).step_by(10) {
            let layout = fit_tiles(17, width as f32, 700.);
            assert!(layout.columns >= last_columns, "columns dropped at width {width}");
            last_columns = layout.columns;
        }
    }

    #[test]
    fn few_games_are_capped_and_centred() {
        let layout = fit_tiles(2, 1600., 1000.);
        assert_eq!(layout.tile_width, MAX_TILE);
        assert_eq!(layout.columns, 2);
    }

    #[test]
    fn small_windows_fill_width_and_scroll() {
        let layout = fit_tiles(60, 700., 400.);
        assert!(layout.scrolls);
        assert!(layout.tile_width >= MIN_TILE);
        assert!(fills_width(layout, 700.));
    }
}
