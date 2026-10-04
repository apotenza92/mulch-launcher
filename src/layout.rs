//! Grid layout: tiles are one fixed size (never resized as the window changes,
//! so nothing jumps around); the window only decides how many fit across. Games are grouped
//! by when they were last played (last week, last month, everything else),
//! each group starting a new row under its own label. Groups with no games
//! don't appear, and with only one group there are no labels, so a new user
//! sees a plain grid until they've played something.

/// Cover art is portrait 2:3.
pub const COVER_ASPECT: f32 = 1.5;
pub const GRID_GAP: f32 = 16.;
/// A game's name under its poster (text_sm) and the space above it.
pub const LABEL_HEIGHT: f32 = 30.;
/// A group's heading (text_xl) and the space under it, above the gap to its first row.
pub const HEADING_HEIGHT: f32 = 46.;

/// Every tile's width.
pub const TILE_WIDTH: f32 = 180.;

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
/// as the games themselves). Each group fills full rows, so widening the
/// window always reflows, with leftovers on a shorter last row; but never a
/// lone game there: the row above lends it one (5 at 4 across: 3 then 2).
pub fn layout(groups: &[usize], width: f32) -> GridLayout {
    let tile_width = TILE_WIDTH;
    let across = columns(width, tile_width);
    let sections: Vec<Section> = groups
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(group, &count)| {
            let mut rows = vec![across; count / across];
            if count % across > 0 {
                rows.push(count % across);
            }
            if let [.., above, last] = rows.as_mut_slice()
                && *last == 1
                && *above >= 3
            {
                *above -= 1;
                *last += 1;
            }
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
    fn as_many_fit_across_as_the_width_allows() {
        let width = grid_width(8, 180.);
        assert_eq!(columns(width, 180.), 8);
        assert_eq!(columns(width - 0.5, 180.), 8);
        assert_eq!(columns(width - 2., 180.), 7);
        assert_eq!(columns(50., 180.), 1);
    }

    #[test]
    fn each_group_fills_full_rows_without_a_lone_game() {
        let width = grid_width(7, TILE_WIDTH);
        assert_eq!(rows(&layout(&[3, 2, 12], width)), vec![vec![3], vec![2], vec![7, 5]]);
        assert_eq!(rows(&layout(&[0, 5, 15], width)), vec![vec![5], vec![7, 6, 2]]);
        assert_eq!(rows(&layout(&[5], grid_width(4, TILE_WIDTH))), vec![vec![3, 2]]);
        assert_eq!(rows(&layout(&[12], grid_width(10, TILE_WIDTH))), vec![vec![10, 2]]);
        // Too narrow to lend one: 2 across stays 2 then 1.
        assert_eq!(rows(&layout(&[3], grid_width(2, TILE_WIDTH))), vec![vec![2, 1]]);
        assert_eq!(layout(&[0, 5, 13], width).sections[0].group, 1);
    }

    #[test]
    fn only_several_groups_get_labels() {
        assert!(!layout(&[0, 0, 17], 1200.).labelled);
        assert!(layout(&[3, 0, 14], 1200.).labelled);
    }

    #[test]
    fn every_game_is_placed() {
        for count in 1..60 {
            let groups = [count / 5, count / 3, count - count / 5 - count / 3];
            let placed: usize = layout(&groups, 1200.).sections.iter().flat_map(|s| s.rows.iter()).sum();
            assert_eq!(placed, count);
        }
    }
}
