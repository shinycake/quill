//! The rows of the Media, Photos, Videos and GIFs grid (tdesktop
//! `Info::Media` section layout): a month header, then rows of square tiles
//! whose count and size follow the panel width. The list is virtualized, so
//! only the rows on screen build tiles and ask for images.
#![deny(clippy::indexing_slicing)]

use super::MonthSection;
use crate::search_filters::YearMonth;

/// Space left and right of a row of tiles.
pub const GRID_PAD: f32 = 3.;
/// Space between tiles (tdesktop `st::infoMediaSkip`).
pub const GRID_GAP: f32 = 2.;
/// Narrowest tile (tdesktop `st::infoMediaMinGridSize`).
const MIN_TILE: f32 = 82.;
/// A month header row.
pub const HEADER_HEIGHT: f32 = 28.;
/// The Load more row.
pub const MORE_HEIGHT: f32 = 52.;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLayout {
    pub per_row: usize,
    /// Side of one tile, stretched so a row fills the width.
    pub tile: f32,
}

impl GridLayout {
    /// tdesktop `ListSection::resizeToWidth`: as many 82 px tiles as fit,
    /// stretched to fill the width.
    pub fn for_width(width: f32) -> Self {
        let inner = (width - 2. * GRID_PAD + GRID_GAP).max(MIN_TILE);
        let per_row = ((inner / (MIN_TILE + GRID_GAP)).floor() as usize).max(1);
        let tile = inner / per_row as f32 - GRID_GAP;
        Self { per_row, tile }
    }

    /// Height of a row of tiles.
    pub fn row_height(&self) -> f32 {
        self.tile + GRID_GAP
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridRow {
    Header(YearMonth),
    /// Items `start..end` of the tab.
    Tiles {
        start: usize,
        end: usize,
    },
    LoadMore,
}

impl GridRow {
    pub fn height(&self, layout: &GridLayout) -> f32 {
        match self {
            GridRow::Header(_) => HEADER_HEIGHT,
            GridRow::Tiles { .. } => layout.row_height(),
            GridRow::LoadMore => MORE_HEIGHT,
        }
    }
}

/// The rows of `sections`, with a Load more row when more can be fetched.
pub fn grid_rows(sections: &[MonthSection], per_row: usize, can_load_more: bool) -> Vec<GridRow> {
    let per_row = per_row.max(1);
    let mut rows = Vec::new();
    for section in sections {
        rows.push(GridRow::Header(section.month));
        let mut start = section.start;
        while start < section.end {
            let end = section.end.min(start.saturating_add(per_row));
            rows.push(GridRow::Tiles { start, end });
            start = end;
        }
    }
    if can_load_more {
        rows.push(GridRow::LoadMore);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(year: i64, month: u8, start: usize, end: usize) -> MonthSection {
        MonthSection {
            month: YearMonth { year, month },
            start,
            end,
        }
    }

    #[test]
    fn the_panel_width_decides_how_many_tiles_fit() {
        let panel = GridLayout::for_width(320.);
        assert_eq!(panel.per_row, 3);
        // Three tiles and their gaps fill the width between the paddings.
        let used = 3. * panel.tile + 2. * GRID_GAP;
        assert!((used - (320. - 2. * GRID_PAD)).abs() < 0.01, "{used}");
        assert_eq!(GridLayout::for_width(500.).per_row, 5);
        assert_eq!(GridLayout::for_width(40.).per_row, 1);
    }

    #[test]
    fn months_wrap_into_rows_and_end_with_load_more() {
        let sections = [section(2026, 10, 0, 5), section(2026, 8, 5, 6)];
        let rows = grid_rows(&sections, 3, true);
        let kinds: Vec<&str> = rows
            .iter()
            .map(|row| match row {
                GridRow::Header(_) => "header",
                GridRow::Tiles { .. } => "tiles",
                GridRow::LoadMore => "more",
            })
            .collect();
        assert_eq!(
            kinds,
            ["header", "tiles", "tiles", "header", "tiles", "more"]
        );
        assert_eq!(rows.get(1), Some(&GridRow::Tiles { start: 0, end: 3 }));
        assert_eq!(rows.get(2), Some(&GridRow::Tiles { start: 3, end: 5 }));
        assert_eq!(rows.get(4), Some(&GridRow::Tiles { start: 5, end: 6 }));
        assert!(!grid_rows(&sections, 3, false).contains(&GridRow::LoadMore));
    }
}
