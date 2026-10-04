//! Grid layout: tiles come in a few fixed sizes the user steps through with
//! zoom buttons (never resized as the window changes, so nothing jumps
//! around); the window only decides how many fit across. Games are grouped
//! by when they were last played (last week, last month, everything else),
//! each group starting a new row under its own label. Groups with no games
//! don't appear, and with only one group there are no labels, so a new user
//! sees a plain grid until they've played something.

/// Cover art is portrait 2:3.
pub const COVER_ASPECT: f32 = 1.5;
pub const GRID_GAP: f32 = 16.;
/// Name (text_sm) + platform (text_xs) + the padding above them.
pub const LABEL_HEIGHT: f32 = 50.;
/// A group's label (text_sm) and the space under it, above the gap to its first row.
pub const HEADING_HEIGHT: f32 = 32.;

/// Tile widths for each zoom step, smallest first.
pub const TILE_SIZES: [f32; 5] = [120., 150., 180., 220., 270.];
/// The middle size.
pub const DEFAULT_SIZE: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Which of the groups passed in.
    pub group: usize,
    /// How many games in each row, top to bottom.
    pub rows: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GridLayout {
    pub tile_width: f32,
    /// Tiles across a full row.
    pub columns: usize,
    /// The groups that have games, top to bottom.
    pub sections: Vec<Section>,
    /// Whether each section has a label (only when there's more than one).
    pub labelled: bool,
}

/// How many tiles of `tile` width fit across `width` (at least one). Allows a
/// pixel of rounding, so a window sized to fit exactly N columns gets N.
pub fn columns(width: f32, tile: f32) -> usize {
    (((width + GRID_GAP + 1.) / (tile + GRID_GAP)).floor() as usize).max(1)
}

/// Width needed for `columns` tiles of `tile` width.
pub fn grid_width(columns: usize, tile: f32) -> f32 {
    columns as f32 * (tile + GRID_GAP) - GRID_GAP
}

/// `groups`: how many games in each group, top to bottom (in the same order
/// as the games themselves). Each group takes as few rows as fit, with its
/// games spread as evenly as possible and any extra on the top rows (5 games
/// at 4 across: 3 then 2, never 4 then 1).
pub fn layout(groups: &[usize], width: f32, size: usize) -> GridLayout {
    let tile_width = TILE_SIZES[size.min(TILE_SIZES.len() - 1)];
    let across = columns(width, tile_width);
    let sections: Vec<Section> = groups
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(group, &count)| {
            let row_count = count.div_ceil(across);
            let rows = (0..row_count).map(|ix| count / row_count + usize::from(ix < count % row_count)).collect();
            Section { group, rows }
        })
        .collect();
    GridLayout { tile_width, columns: across, labelled: sections.len() > 1, sections }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(layout: &GridLayout) -> Vec<Vec<usize>> {
        layout.sections.iter().map(|s| s.rows.clone()).collect()
    }

    #[test]
    fn tiles_stay_the_same_size_whatever_the_window() {
        for width in [500., 900., 1400., 2500.] {
            assert_eq!(layout(&[0, 0, 17], width, DEFAULT_SIZE).tile_width, TILE_SIZES[DEFAULT_SIZE]);
        }
    }

    #[test]
    fn as_many_fit_across_as_the_width_allows() {
        let width = grid_width(8, 180.);
        assert_eq!(columns(width, 180.), 8);
        assert_eq!(columns(width - 0.5, 180.), 8);
        assert_eq!(columns(width - 2., 180.), 7);
        assert_eq!(columns(50., 180.), 1);
    }

    #[test]
    fn each_group_starts_a_row_and_rows_are_balanced() {
        let width = grid_width(7, 180.);
        assert_eq!(rows(&layout(&[3, 2, 12], width, DEFAULT_SIZE)), vec![vec![3], vec![2], vec![6, 6]]);
        assert_eq!(rows(&layout(&[0, 5, 13], width, DEFAULT_SIZE)), vec![vec![5], vec![7, 6]]);
        assert_eq!(rows(&layout(&[5], grid_width(4, 180.), DEFAULT_SIZE)), vec![vec![3, 2]]);
        assert_eq!(rows(&layout(&[17], width, DEFAULT_SIZE)), vec![vec![6, 6, 5]]);
        assert_eq!(layout(&[0, 5, 13], width, DEFAULT_SIZE).sections[0].group, 1);
    }

    #[test]
    fn only_several_groups_get_labels() {
        assert!(!layout(&[0, 0, 17], 1200., DEFAULT_SIZE).labelled);
        assert!(layout(&[3, 0, 14], 1200., DEFAULT_SIZE).labelled);
    }

    #[test]
    fn every_game_is_placed() {
        for count in 1..60 {
            let groups = [count / 5, count / 3, count - count / 5 - count / 3];
            let placed: usize = layout(&groups, 1200., 0).sections.iter().flat_map(|s| s.rows.iter()).sum();
            assert_eq!(placed, count);
        }
    }

    #[test]
    fn sizes_step_up() {
        assert!(TILE_SIZES.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(layout(&[1], 1000., 99).tile_width, TILE_SIZES[4]);
    }
}
