//! Grid sizing: tiles shrink to fit every game in the window, down to a
//! minimum readable size, after which the grid scrolls instead.

/// Cover art is portrait 2:3.
pub const COVER_ASPECT: f32 = 1.5;
pub const GRID_GAP: f32 = 16.;
/// Name (text_sm) + platform (text_xs) + the padding above them.
pub const LABEL_HEIGHT: f32 = 50.;
pub const MAX_TILE: f32 = 220.;
/// Smallest tile width before the grid scrolls rather than shrinking further.
pub const MIN_TILE: f32 = 100.;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLayout {
    pub tile_width: f32,
    pub columns: usize,
    /// True when tiles hit the minimum size and the grid needs scrolling.
    pub scrolls: bool,
}

/// Keep the current column count unless another gives tiles at least this
/// much bigger, so the grid doesn't flip between layouts at borderline sizes.
const SWITCH_THRESHOLD: f32 = 1.10;

/// The largest tile width at which all `count` tiles (with their labels) fit
/// in `width` x `height`.
pub fn fit_tiles(count: usize, width: f32, height: f32) -> GridLayout {
    fit_tiles_stable(None, count, width, height)
}

/// Like [`fit_tiles`], but sticks with `previous`'s column count when it's
/// close to the best.
pub fn fit_tiles_stable(previous: Option<&GridLayout>, count: usize, width: f32, height: f32) -> GridLayout {
    let size_for = |columns: usize| {
        let rows = count.div_ceil(columns).max(1) as f32;
        let by_width = (width - (columns as f32 - 1.) * GRID_GAP) / columns as f32;
        let row_height = (height - (rows - 1.) * GRID_GAP) / rows;
        let by_height = (row_height - LABEL_HEIGHT) / COVER_ASPECT;
        by_width.min(by_height).min(MAX_TILE)
    };
    let (mut tile_width, mut columns) = (1..=count.max(1))
        .map(|columns| (size_for(columns), columns))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((MAX_TILE, 1));

    if let Some(previous) = previous.filter(|p| !p.scrolls && p.columns >= 1 && p.columns <= count.max(1)) {
        let kept = size_for(previous.columns);
        if kept >= MIN_TILE && kept * SWITCH_THRESHOLD >= tile_width {
            (tile_width, columns) = (kept, previous.columns);
        }
    }

    if tile_width >= MIN_TILE {
        return GridLayout { tile_width, columns, scrolls: false };
    }
    let columns = ((width + GRID_GAP) / (MIN_TILE + GRID_GAP)).floor().max(1.) as usize;
    GridLayout { tile_width: MIN_TILE, columns, scrolls: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fits(layout: GridLayout, count: usize, width: f32, height: f32) -> bool {
        let rows = count.div_ceil(layout.columns) as f32;
        let row_height = layout.tile_width * COVER_ASPECT + LABEL_HEIGHT;
        let grid_width = layout.columns as f32 * (layout.tile_width + GRID_GAP) - GRID_GAP;
        rows * row_height + (rows - 1.) * GRID_GAP <= height + 0.5 && grid_width <= width + 0.5
    }

    #[test]
    fn all_tiles_fit_when_there_is_room() {
        let layout = fit_tiles(17, 1200., 700.);
        assert!(!layout.scrolls);
        assert!(fits(layout, 17, 1200., 700.));
    }

    #[test]
    fn keeps_column_count_through_small_resizes() {
        let first = fit_tiles(17, 1200., 700.);
        // A slightly different window where another column count is only marginally better.
        let next = fit_tiles_stable(Some(&first), 17, 1190., 712.);
        assert_eq!(next.columns, first.columns);
        assert!(fits(next, 17, 1190., 712.));
    }

    #[test]
    fn few_games_are_capped_at_max_size() {
        assert_eq!(fit_tiles(2, 1600., 1000.).tile_width, MAX_TILE);
    }

    #[test]
    fn small_windows_scroll_at_minimum_size() {
        let layout = fit_tiles(60, 700., 400.);
        assert!(layout.scrolls);
        assert_eq!(layout.tile_width, MIN_TILE);
        assert!(layout.columns as f32 * (MIN_TILE + GRID_GAP) - GRID_GAP <= 700.);
    }
}
