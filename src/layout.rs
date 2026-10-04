//! Grid sizing, like a responsive web grid: tiles always stretch to fill the
//! window's width exactly, in an even number of columns (2, 4, 6, 8, ...).
//! 8 columns by default; more when that gets every game on screen without
//! scrolling (down to a minimum tile size), and more when tiles would
//! otherwise get too big. Narrow windows drop columns when tiles would get
//! too small. The grid scrolls only when games can't all fit.
//!
//! The same window size always gives the same layout, and tiles grow and
//! shrink smoothly between column changes.

/// Cover art is portrait 2:3.
pub const COVER_ASPECT: f32 = 1.5;
pub const GRID_GAP: f32 = 16.;
/// Name (text_sm) + platform (text_xs) + the padding above them.
pub const LABEL_HEIGHT: f32 = 50.;
pub const DEFAULT_COLUMNS: usize = 8;
/// Smallest tile width; narrower windows use fewer columns.
pub const MIN_TILE: f32 = 80.;
/// Largest tile width; wider windows use more columns.
pub const MAX_TILE: f32 = 260.;

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

/// Most even columns of at least `MIN_TILE` that fit in `width` (at least 2,
/// even if that makes tiles a little smaller than the minimum).
fn columns_at_min(width: f32) -> usize {
    let fit = ((width + GRID_GAP) / (MIN_TILE + GRID_GAP)).floor() as usize;
    (fit - fit % 2).max(2)
}

/// Fewest even columns that keep tiles at or under `MAX_TILE`.
fn columns_at_max(width: f32) -> usize {
    let needed = (((width + GRID_GAP) / (MAX_TILE + GRID_GAP)).ceil() as usize).max(2);
    needed + needed % 2
}

pub fn fit_tiles(count: usize, width: f32, height: f32) -> GridLayout {
    let most = columns_at_min(width);
    let mut columns = DEFAULT_COLUMNS.min(most).max(columns_at_max(width));
    // Prefer more, smaller tiles over scrolling: add columns two at a time
    // until every game fits on screen or tiles reach the minimum size.
    while columns + 2 <= most && grid_height(count, columns, filling_width(width, columns)) > height {
        columns += 2;
    }
    let tile_width = filling_width(width, columns).max(1.);
    GridLayout { tile_width, columns, scrolls: grid_height(count, columns, tile_width) > height }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fills_width(layout: GridLayout, width: f32) -> bool {
        (layout.columns as f32 * (layout.tile_width + GRID_GAP) - GRID_GAP - width).abs() < 0.5
    }

    #[test]
    fn eight_columns_at_normal_sizes() {
        for width in [1000., 1200., 1600., 2000.] {
            let layout = fit_tiles(8, width, 1200.);
            assert_eq!(layout.columns, DEFAULT_COLUMNS, "width {width}");
            assert!(fills_width(layout, width));
        }
    }

    #[test]
    fn narrow_windows_use_fewer_columns() {
        let layout = fit_tiles(17, 600., 700.);
        assert!(layout.columns < DEFAULT_COLUMNS);
        assert!(layout.tile_width >= MIN_TILE);
        assert!(fills_width(layout, 600.));
    }

    #[test]
    fn very_wide_windows_use_more_columns() {
        let layout = fit_tiles(17, 3000., 1200.);
        assert!(layout.columns > DEFAULT_COLUMNS);
        assert!(layout.tile_width <= MAX_TILE);
        assert!(fills_width(layout, 3000.));
    }

    #[test]
    fn columns_never_drop_as_the_window_widens() {
        let mut last = 0;
        for width in (300..4000).step_by(7) {
            let columns = fit_tiles(17, width as f32, 700.).columns;
            assert!(columns >= last, "columns dropped at width {width}");
            last = columns;
        }
    }

    #[test]
    fn adds_columns_to_avoid_scrolling() {
        // 17 games at 8 columns need 3 rows, which don't fit 700px; 10 columns fit in 2.
        let layout = fit_tiles(17, 1200., 700.);
        assert_eq!(layout.columns, 10);
        assert!(!layout.scrolls);
        assert!(fills_width(layout, 1200.));
    }

    #[test]
    fn columns_are_always_even() {
        for width in (200..5000).step_by(13) {
            let columns = fit_tiles(17, width as f32, 700.).columns;
            assert_eq!(columns % 2, 0, "{columns} columns at width {width}");
        }
    }

    #[test]
    fn scrolls_only_when_games_do_not_fit() {
        assert!(!fit_tiles(8, 1200., 700.).scrolls);
        assert!(fit_tiles(60, 1200., 700.).scrolls);
    }
}
