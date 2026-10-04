//! Grid sizing: games in groups by when they were last played (last week,
//! last month, everything else), each group under its own label. Groups with
//! no games don't appear, and with only one group there are no labels, so a
//! new user sees a plain grid until they've played something.
//!
//! Every tile is the same size: the biggest that gets every game on screen.
//! Each group wraps onto as many full rows as it needs, left to right, with
//! any leftover games on a shorter last row.
//!
//! Only when that would make tiles too small to read does the grid scroll
//! instead, with rows as wide as readable tiles allow.

/// Cover art is portrait 2:3.
pub const COVER_ASPECT: f32 = 1.5;
pub const GRID_GAP: f32 = 16.;
/// Name (text_sm) + platform (text_xs) + the padding above them.
pub const LABEL_HEIGHT: f32 = 50.;
/// A group's label (text_sm) and the space under it, above the gap to its first row.
pub const HEADING_HEIGHT: f32 = 32.;
/// Below this, names get too truncated to read, so the grid scrolls instead.
pub const MIN_TILE: f32 = 110.;

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
    /// The groups that have games, top to bottom.
    pub sections: Vec<Section>,
    /// Whether each section has a label (only when there's more than one).
    pub labelled: bool,
    /// True when the games don't all fit and the grid scrolls.
    pub scrolls: bool,
}

/// Height of one row of tiles, labels included.
#[cfg(test)]
fn row_height(tile: f32) -> f32 {
    tile * COVER_ASPECT + LABEL_HEIGHT
}

/// Widest tile that fits `columns` across `width`.
fn width_limit(width: f32, columns: usize) -> f32 {
    (width - (columns as f32 - 1.) * GRID_GAP) / columns as f32
}

/// Widest tile that fits `rows` rows in `sections` labelled sections down `height`.
fn height_limit(height: f32, rows: usize, sections: usize, labelled: bool) -> f32 {
    let labels = if labelled { sections as f32 * HEADING_HEIGHT } else { 0. };
    let row = (height - (rows as f32 - 1.) * GRID_GAP - labels) / rows as f32;
    (row - LABEL_HEIGHT) / COVER_ASPECT
}

/// Sections for groups of games, with at most `widest` games in a row.
fn sections_for(groups: &[usize], widest: usize) -> Vec<Section> {
    groups
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(group, &count)| {
            let mut rows = vec![widest; count / widest];
            if count % widest > 0 {
                rows.push(count % widest);
            }
            Section { group, rows }
        })
        .collect()
}

fn layout(sections: Vec<Section>, tile_width: f32, scrolls: bool) -> GridLayout {
    GridLayout { labelled: sections.len() > 1, tile_width, sections, scrolls }
}

/// `groups`: how many games in each group, top to bottom (in the same order
/// as the games themselves).
pub fn fit_tiles(groups: &[usize], width: f32, height: f32) -> GridLayout {
    let largest = groups.iter().copied().max().unwrap_or(0).max(1);
    let best = (1..=largest)
        .map(|widest| {
            let sections = sections_for(groups, widest);
            let rows: usize = sections.iter().map(|s| s.rows.len()).sum();
            let columns = sections.iter().flat_map(|s| s.rows.iter().copied()).max().unwrap_or(1);
            let tile = width_limit(width, columns)
                .min(height_limit(height, rows.max(1), sections.len().max(1), sections.len() > 1));
            (tile, sections)
        })
        // Ties go to the narrower layout (fuller rows).
        .fold(None::<(f32, Vec<Section>)>, |best, option| match best {
            Some(best) if option.0 <= best.0 + 0.5 => Some(best),
            _ => Some(option),
        })
        .expect("at least one layout");

    if best.0 >= MIN_TILE {
        return layout(best.1, best.0, false);
    }
    // Too many games to show readably at once: rows as wide as readable
    // tiles allow, stretched to fill the width, and scroll.
    let widest = (((width + GRID_GAP) / (MIN_TILE + GRID_GAP)).floor() as usize).max(1);
    let sections = sections_for(groups, widest);
    let columns = sections.iter().flat_map(|s| s.rows.iter().copied()).max().unwrap_or(1);
    layout(sections, width_limit(width, columns).max(1.), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(layout: &GridLayout) -> Vec<usize> {
        layout.sections.iter().flat_map(|s| s.rows.iter().copied()).collect()
    }

    fn grid_size(layout: &GridLayout) -> (f32, f32) {
        let all = rows(layout);
        let columns = all.iter().copied().max().unwrap() as f32;
        let sections = layout.sections.len() as f32;
        let row_count = all.len() as f32;
        let labels = if layout.labelled { sections * HEADING_HEIGHT } else { 0. };
        (
            columns * layout.tile_width + (columns - 1.) * GRID_GAP,
            row_count * row_height(layout.tile_width) + (row_count - 1.) * GRID_GAP + labels,
        )
    }

    #[test]
    fn each_group_starts_a_row_and_wraps() {
        let rows_of = |groups: &[usize], widest| -> Vec<Vec<usize>> {
            sections_for(groups, widest).into_iter().map(|s| s.rows).collect()
        };
        assert_eq!(rows_of(&[3, 2, 12], 99), vec![vec![3], vec![2], vec![12]]);
        assert_eq!(rows_of(&[3, 2, 12], 7), vec![vec![3], vec![2], vec![7, 5]]);
        assert_eq!(sections_for(&[0, 5, 13], 7)[0].group, 1);
    }

    #[test]
    fn only_several_groups_get_labels() {
        assert!(!fit_tiles(&[0, 0, 17], 1200., 700.).labelled);
        assert!(fit_tiles(&[3, 0, 14], 1200., 700.).labelled);
    }

    #[test]
    fn every_game_is_placed() {
        for count in 1..60 {
            let groups = [count / 5, count / 3, count - count / 5 - count / 3];
            assert_eq!(rows(&fit_tiles(&groups, 1200., 700.)).iter().sum::<usize>(), count);
        }
    }

    #[test]
    fn everything_fits_on_screen_when_it_can() {
        for (groups, width, height) in
            [(vec![3, 2, 12], 1800., 900.), (vec![0, 0, 17], 800., 900.), (vec![5], 1600., 400.), (vec![4, 6, 30], 2400., 1400.)]
        {
            let layout = fit_tiles(&groups, width, height);
            assert!(!layout.scrolls, "{groups:?} in {width}x{height}");
            let (w, h) = grid_size(&layout);
            assert!(w <= width + 0.01 && h <= height + 0.01, "{groups:?} in {width}x{height}: {w}x{h}");
        }
    }

    #[test]
    fn a_short_wide_window_fills_one_row() {
        let layout = fit_tiles(&[0, 0, 17], 2600., 300.);
        assert_eq!(rows(&layout), vec![17]);
        assert!((grid_size(&layout).0 - 2600.).abs() < 0.5);
    }

    #[test]
    fn tiles_grow_with_the_window() {
        let mut last = 0.;
        for step in 0..200 {
            let size = 600. + step as f32 * 10.;
            let layout = fit_tiles(&[3, 2, 12], size * 1.5, size);
            if layout.scrolls {
                continue;
            }
            assert!(layout.tile_width >= last, "tiles shrank at {size}");
            last = layout.tile_width;
        }
    }

    #[test]
    fn too_many_games_scroll_with_readable_tiles() {
        let layout = fit_tiles(&[5, 10, 300], 1200., 700.);
        assert!(layout.scrolls);
        assert!(layout.tile_width >= MIN_TILE);
        assert!(grid_size(&layout).0 <= 1200.5);
    }
}
