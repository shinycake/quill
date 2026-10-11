//! Inline bot results panel: the justified media grid, list rows, the
//! results-button routing and the scroll-to-load rule.
//!
//! tdesktop (`inline_bots/inline_results_inner.cpp`, `layout/layout_mosaic.cpp`
//! and `inline_bot_layout_internal.cpp`) packs photos, GIFs and stickers
//! into justified rows and gives every other result its own full-width row.
//! This module ports that packing as pure integer math so the panel's
//! shape is unit-tested; `ui/inline_grid.rs` only draws it.

use crate::telegram::envelope::{InlineQueryResultSummary, InlineQueryResultsButton};

/// Width of the results panel in pixels (tdesktop: `emojiPanWidth` 345).
pub const PANEL_WIDTH: i32 = 380;
/// Padding left and right of the grid (tdesktop `emojiPanMargins`).
pub const PANEL_PADDING: i32 = 10;
/// Height of a media cell (tdesktop `inlineMediaHeight`).
pub const MEDIA_HEIGHT: i32 = 96;
/// Gap between cells and below a media row (tdesktop `inlineResultsSkip`).
pub const SKIP: i32 = 3;
/// Narrowest a media cell gets (tdesktop `inlineResultsMinWidth`).
pub const MIN_CELL_WIDTH: i32 = 48;
/// Side of a sticker cell (tdesktop `stickerPanWidthMin`).
pub const STICKER_SIZE: i32 = 64;
/// Most cells in one justified row (tdesktop `kInlineItemsMaxPerRow`).
pub const MAX_PER_ROW: usize = 5;
/// Leading square of a list row with a picture (tdesktop `inlineThumbSize`).
pub const LIST_THUMB: i32 = 64;
/// Leading square of a file-like list row (tdesktop `inlineFileSize`).
pub const FILE_THUMB: i32 = 44;
/// Vertical margin inside a list row plus its border line
/// (tdesktop `inlineRowMargin` * 2 + `inlineRowBorder`).
pub const LIST_ROW_EXTRA: i32 = 13;

/// How a result is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    /// Photo or GIF: packed into justified rows by aspect ratio.
    Media,
    /// Sticker: a fixed square, packed the same way.
    Sticker,
    /// Article, contact, location, venue, game, video: a full-width row
    /// with a 64px picture.
    Rich,
    /// Audio, voice note, document: a full-width row with a 44px square.
    File,
}

impl CellKind {
    /// Cells packed side by side in the grid; the rest take a full row.
    pub fn is_grid(self) -> bool {
        matches!(self, Self::Media | Self::Sticker)
    }

    /// Side of the leading square of a list row.
    pub fn list_thumb(self) -> i32 {
        match self {
            Self::File => FILE_THUMB,
            _ => LIST_THUMB,
        }
    }
}

/// One result as the packer sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MosaicItem {
    /// Index into the results the panel was built from.
    pub result: usize,
    pub kind: CellKind,
    /// Width the item wants (tdesktop `maxWidth()`).
    pub max_width: i32,
    /// Height of the item's row (media rows include the bottom skip).
    pub height: i32,
}

/// Classifies a result. `None` skips it, like tdesktop does for a layout
/// that reports zero width (a GIF or photo without a size).
pub fn mosaic_item(result: usize, summary: &InlineQueryResultSummary) -> Option<MosaicItem> {
    let (kind, max_width, height) = match summary.kind.as_str() {
        "photo" | "animation" => {
            let media = summary.media?;
            let animation = summary.kind == "animation";
            if animation && (media.width <= 0 || media.height <= 0) {
                return None;
            }
            // tdesktop photos with an unknown size count as 1x1.
            let (w, h) = if media.width <= 0 || media.height <= 0 {
                (1, 1)
            } else {
                (media.width, media.height)
            };
            let width = (i64::from(w) * i64::from(MEDIA_HEIGHT) / i64::from(h))
                .clamp(0, i64::from(i32::MAX / 4)) as i32;
            (
                CellKind::Media,
                width.max(MIN_CELL_WIDTH),
                MEDIA_HEIGHT + SKIP,
            )
        }
        "sticker" => (CellKind::Sticker, STICKER_SIZE, STICKER_SIZE),
        "audio" | "voice_note" | "document" => {
            (CellKind::File, list_width(), FILE_THUMB + LIST_ROW_EXTRA)
        }
        _ => (CellKind::Rich, list_width(), LIST_THUMB + LIST_ROW_EXTRA),
    };
    Some(MosaicItem {
        result,
        kind,
        max_width,
        height,
    })
}

/// Width a list row asks for: the whole grid.
pub fn list_width() -> i32 {
    PANEL_WIDTH - 2 * PANEL_PADDING
}

/// Tallest the panel grows before it scrolls.
pub const MAX_PANEL_HEIGHT: i32 = 360;
/// Height of the results button (tdesktop `switchPmButton`).
pub const BUTTON_HEIGHT: i32 = 36;

/// A cell after layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub item: MosaicItem,
    /// Left edge from the panel's left edge.
    pub x: i32,
    pub width: i32,
}

/// One packed row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MosaicRow {
    /// Top from the top of the grid.
    pub y: i32,
    pub height: i32,
    pub cells: Vec<Placed>,
}

/// The packed grid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mosaic {
    pub rows: Vec<MosaicRow>,
    pub height: i32,
}

impl Mosaic {
    /// Result indices in display order.
    pub fn order(&self) -> Vec<usize> {
        self.rows
            .iter()
            .flat_map(|row| row.cells.iter().map(|cell| cell.item.result))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

struct RawRow {
    items: Vec<MosaicItem>,
    /// Sum of the items' wanted widths when the row is justified, 0 for
    /// the last partial row (it keeps natural widths).
    max_width: i32,
}

/// Packs items the way tdesktop's `AbstractMosaicLayout` does: a row closes
/// once it holds five items or wants at least `big_width`; the final row
/// keeps natural widths; a closed row is scaled so its items fill
/// `full_width` minus the padding, narrowest first.
pub fn layout_mosaic(
    items: &[MosaicItem],
    big_width: i32,
    full_width: i32,
    padding: i32,
    right_skip: i32,
) -> Mosaic {
    let mut raw: Vec<RawRow> = Vec::new();
    let mut current: Vec<MosaicItem> = Vec::new();
    let mut sum = 0;
    let finalize = |current: &mut Vec<MosaicItem>,
                    sum: &mut i32,
                    force: bool,
                    raw: &mut Vec<RawRow>|
     -> bool {
        if current.is_empty() {
            return false;
        }
        let full = current.len() >= MAX_PER_ROW;
        let big = *sum >= big_width;
        if full || big || force {
            raw.push(RawRow {
                items: std::mem::take(current),
                max_width: if full || big { *sum } else { 0 },
            });
            *sum = 0;
            return true;
        }
        false
    };
    for item in items {
        finalize(&mut current, &mut sum, false, &mut raw);
        sum += item.max_width;
        if !current.is_empty() && right_skip > 0 {
            sum += right_skip;
        }
        current.push(*item);
    }
    finalize(&mut current, &mut sum, true, &mut raw);

    let mut mosaic = Mosaic::default();
    let mut y = 0;
    for row in raw {
        let (cells, height) = layout_row(&row, full_width, padding, right_skip);
        mosaic.rows.push(MosaicRow { y, height, cells });
        y += height;
    }
    mosaic.height = y;
    mosaic
}

fn layout_row(row: &RawRow, full_width: i32, padding: i32, right_skip: i32) -> (Vec<Placed>, i32) {
    let count = row.items.len();
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by_key(|&i| row.items[i].max_width);
    let mut widths = vec![0; count];
    let mut desired = row.max_width;
    let mut available = full_width - 2 * padding;
    let mut height = 0;
    for &index in &order {
        let item = &row.items[index];
        let w = if desired > 0 {
            (i64::from(item.max_width) * i64::from(available) / i64::from(desired)) as i32
        } else {
            item.max_width
        };
        let actual = w.max(MIN_CELL_WIDTH);
        height = height.max(item.height);
        // A list row never grows past what it asked for.
        widths[index] = if item.kind.is_grid() {
            actual
        } else {
            actual.min(item.max_width)
        };
        if desired > 0 {
            available -= actual;
            desired -= item.max_width;
            if index > 0 && right_skip > 0 {
                available -= right_skip;
                desired -= right_skip;
            }
        }
    }
    let mut x = padding;
    let cells = row
        .items
        .iter()
        .zip(widths)
        .map(|(item, width)| {
            let placed = Placed {
                item: *item,
                x,
                width,
            };
            x += width + right_skip;
            placed
        })
        .collect();
    (cells, height)
}

/// The panel's grid for a page of results: skipped results drop out.
pub fn layout_results(results: &[InlineQueryResultSummary]) -> Mosaic {
    let items: Vec<MosaicItem> = results
        .iter()
        .enumerate()
        .filter_map(|(index, summary)| mosaic_item(index, summary))
        .collect();
    layout_mosaic(&items, list_width(), PANEL_WIDTH, PANEL_PADDING, SKIP)
}

/// Distance from the panel's top to the grid: the padding, plus the
/// button and the skip below it.
pub fn grid_top(has_button: bool) -> i32 {
    PANEL_PADDING + if has_button { BUTTON_HEIGHT + SKIP } else { 0 }
}

/// Height of everything the panel scrolls.
pub fn content_height(mosaic: &Mosaic, has_button: bool) -> i32 {
    grid_top(has_button) + mosaic.height + PANEL_PADDING
}

/// New scroll position that brings `top..top + height` into a viewport
/// starting at `scroll_top`, moving as little as possible.
pub fn scroll_to_reveal(scroll_top: i32, viewport: i32, top: i32, height: i32) -> i32 {
    if top < scroll_top || height > viewport {
        top
    } else if top + height > scroll_top + viewport {
        (top + height - viewport).max(0)
    } else {
        scroll_top
    }
}

/// What the results button does when pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ButtonAction {
    /// Open the bot's chat with a start parameter waiting in the composer
    /// (tdesktop sets the bot's start token and shows its history).
    StartBot { parameter: String },
    /// Open the bot's mini app at the url (tdesktop `attachWebView().open`
    /// with the switch source).
    WebApp { url: String },
}

/// The action behind a button, or `None` when the button has no text or
/// its type is unknown or incomplete (tdesktop shows a button only for a
/// non-empty text).
pub fn button_action(button: &InlineQueryResultsButton) -> Option<ButtonAction> {
    if button.text.trim().is_empty() {
        return None;
    }
    match button.kind.as_str() {
        "start_bot" => Some(ButtonAction::StartBot {
            parameter: button.parameter.clone(),
        }),
        "web_app" if !button.url.is_empty() => Some(ButtonAction::WebApp {
            url: button.url.clone(),
        }),
        _ => None,
    }
}

/// Whether to ask for the next page: tdesktop does so once the scroll
/// position gets within one viewport of the end (`top + height >
/// scrollTopMax`), which also fires when the content is shorter than the
/// panel.
pub fn should_load_more(
    scroll_top: f32,
    viewport: f32,
    content: f32,
    has_next: bool,
    loading: bool,
) -> bool {
    if !has_next || loading {
        return false;
    }
    let scroll_max = (content - viewport).max(0.0);
    scroll_top + viewport > scroll_max
}

/// Moves a highlight over `count` cells, wrapping at both ends.
pub fn step_selection(selected: usize, delta: i32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    (selected.min(count - 1) as i32 + delta).rem_euclid(count as i32) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FileId;
    use crate::telegram::envelope::InlineMedia;

    fn media(kind: &str, w: i32, h: i32) -> InlineQueryResultSummary {
        InlineQueryResultSummary {
            id: format!("{kind}-{w}x{h}"),
            kind: kind.into(),
            media: Some(InlineMedia {
                file_id: FileId(1),
                width: w,
                height: h,
            }),
            ..Default::default()
        }
    }

    fn plain(kind: &str) -> InlineQueryResultSummary {
        InlineQueryResultSummary {
            id: kind.into(),
            kind: kind.into(),
            ..Default::default()
        }
    }

    #[test]
    fn media_width_follows_aspect_ratio() {
        let item = mosaic_item(0, &media("photo", 200, 100)).unwrap();
        assert_eq!(item.max_width, 192);
        assert_eq!(item.height, MEDIA_HEIGHT + SKIP);
        // Very narrow images still get the minimum width.
        let thin = mosaic_item(0, &media("photo", 10, 100)).unwrap();
        assert_eq!(thin.max_width, MIN_CELL_WIDTH);
    }

    #[test]
    fn unknown_photo_size_counts_as_square_but_gif_is_skipped() {
        let photo = mosaic_item(0, &media("photo", 0, 0)).unwrap();
        assert_eq!(photo.max_width, MEDIA_HEIGHT);
        assert!(mosaic_item(0, &media("animation", 0, 0)).is_none());
        assert!(mosaic_item(0, &plain("photo")).is_none());
    }

    #[test]
    fn kinds_split_into_grid_and_list() {
        assert_eq!(
            mosaic_item(0, &media("sticker", 512, 512)).unwrap().kind,
            CellKind::Sticker
        );
        for kind in ["article", "contact", "location", "venue", "game", "video"] {
            let item = mosaic_item(0, &plain(kind)).unwrap();
            assert_eq!(item.kind, CellKind::Rich, "{kind}");
            assert_eq!(item.max_width, list_width());
        }
        for kind in ["audio", "voice_note", "document"] {
            assert_eq!(mosaic_item(0, &plain(kind)).unwrap().kind, CellKind::File);
        }
    }

    fn packed(results: &[InlineQueryResultSummary]) -> Mosaic {
        layout_results(results)
    }

    #[test]
    fn a_full_row_is_justified_to_the_panel_width() {
        // Three 4:3 photos want 128 each (+ skips 6) = 390 >= 360: one
        // closed row scaled to the available width.
        let results = vec![media("photo", 4, 3); 4];
        let mosaic = packed(&results);
        assert_eq!(mosaic.rows.len(), 2);
        let first = &mosaic.rows[0];
        assert_eq!(first.cells.len(), 3);
        let spent: i32 = first.cells.iter().map(|c| c.width).sum::<i32>() + 2 * SKIP;
        // Integer division can lose a pixel or two, never gain.
        assert!(
            (list_width() - 3..=list_width()).contains(&spent),
            "{spent}"
        );
        assert_eq!(first.cells[0].x, PANEL_PADDING);
        assert_eq!(
            first.cells[1].x,
            PANEL_PADDING + first.cells[0].width + SKIP
        );
        assert_eq!(first.height, MEDIA_HEIGHT + SKIP);
    }

    #[test]
    fn the_last_row_keeps_natural_widths() {
        let results = vec![media("photo", 4, 3); 4];
        let mosaic = packed(&results);
        let last = mosaic.rows.last().unwrap();
        assert_eq!(last.cells.len(), 1);
        assert_eq!(last.cells[0].width, 128);
    }

    #[test]
    fn rows_close_at_five_cells() {
        // Tiny stickers never reach the big width, so the cap closes rows.
        let results = vec![media("sticker", 512, 512); 7];
        let mosaic = packed(&results);
        assert_eq!(mosaic.rows.len(), 2);
        assert_eq!(mosaic.rows[0].cells.len(), MAX_PER_ROW);
        assert_eq!(mosaic.rows[1].cells.len(), 2);
        // Stickers fill their closed row.
        let spent: i32 = mosaic.rows[0].cells.iter().map(|c| c.width).sum::<i32>() + 4 * SKIP;
        assert!(spent <= list_width());
    }

    #[test]
    fn list_rows_take_their_own_row_after_a_full_media_row() {
        let results = vec![
            media("photo", 4, 3),
            media("photo", 4, 3),
            media("photo", 4, 3),
            plain("article"),
            plain("audio"),
        ];
        let mosaic = packed(&results);
        assert_eq!(mosaic.rows.len(), 3);
        assert_eq!(mosaic.rows[0].cells.len(), 3);
        assert_eq!(mosaic.rows[1].cells[0].width, list_width());
        assert_eq!(mosaic.rows[1].height, LIST_THUMB + LIST_ROW_EXTRA);
        assert_eq!(mosaic.rows[2].height, FILE_THUMB + LIST_ROW_EXTRA);
        assert_eq!(mosaic.order(), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_pending_media_cell_shares_its_row_with_a_list_cell_like_tdesktop() {
        // tdesktop only closes a row when it is full or wide enough, so a
        // lone photo followed by an article ends up in one scaled row.
        let mosaic = packed(&[media("photo", 4, 3), plain("article")]);
        assert_eq!(mosaic.rows.len(), 1);
        assert_eq!(mosaic.rows[0].cells.len(), 2);
        assert_eq!(mosaic.rows[0].height, MEDIA_HEIGHT + SKIP);
    }

    #[test]
    fn rows_stack_and_report_total_height() {
        let results = vec![plain("article"), plain("article"), plain("audio")];
        let mosaic = packed(&results);
        assert_eq!(mosaic.rows[1].y, LIST_THUMB + LIST_ROW_EXTRA);
        assert_eq!(
            mosaic.height,
            2 * (LIST_THUMB + LIST_ROW_EXTRA) + FILE_THUMB + LIST_ROW_EXTRA
        );
    }

    #[test]
    fn skipped_results_leave_the_display_order() {
        let results = vec![media("animation", 0, 0), plain("article"), plain("contact")];
        assert_eq!(packed(&results).order(), vec![1, 2]);
        assert!(packed(&[]).is_empty());
    }

    #[test]
    fn button_routing() {
        let button =
            |text: &str, kind: &str, parameter: &str, url: &str| InlineQueryResultsButton {
                text: text.into(),
                kind: kind.into(),
                parameter: parameter.into(),
                url: url.into(),
            };
        assert_eq!(
            button_action(&button("Connect", "start_bot", "auth", "")),
            Some(ButtonAction::StartBot {
                parameter: "auth".into()
            })
        );
        assert_eq!(
            button_action(&button("Open", "web_app", "", "https://example.org/app")),
            Some(ButtonAction::WebApp {
                url: "https://example.org/app".into()
            })
        );
        assert_eq!(button_action(&button("", "start_bot", "x", "")), None);
        assert_eq!(button_action(&button("Open", "web_app", "", "")), None);
        assert_eq!(button_action(&button("Open", "unknown", "", "")), None);
        // An empty start parameter still starts the bot.
        assert_eq!(
            button_action(&button("Start", "start_bot", "", "")),
            Some(ButtonAction::StartBot {
                parameter: String::new()
            })
        );
    }

    #[test]
    fn pagination_fires_near_the_end_only_with_a_next_page() {
        // Content shorter than the panel loads right away.
        assert!(should_load_more(0.0, 300.0, 120.0, true, false));
        // Far from the end: wait.
        assert!(!should_load_more(0.0, 300.0, 1200.0, true, false));
        // Within one viewport of the end: load.
        assert!(should_load_more(650.0, 300.0, 1200.0, true, false));
        // No next page, or a request running: never.
        assert!(!should_load_more(900.0, 300.0, 1200.0, false, false));
        assert!(!should_load_more(900.0, 300.0, 1200.0, true, true));
    }

    #[test]
    fn content_height_counts_padding_and_button() {
        let mosaic = packed(&[plain("article")]);
        let row = LIST_THUMB + LIST_ROW_EXTRA;
        assert_eq!(content_height(&mosaic, false), 2 * PANEL_PADDING + row);
        assert_eq!(
            content_height(&mosaic, true),
            2 * PANEL_PADDING + BUTTON_HEIGHT + SKIP + row
        );
        assert_eq!(grid_top(true), PANEL_PADDING + BUTTON_HEIGHT + SKIP);
    }

    #[test]
    fn reveal_moves_the_least() {
        // Already visible: stay.
        assert_eq!(scroll_to_reveal(100, 300, 150, 50), 100);
        // Above the viewport: align the top edge.
        assert_eq!(scroll_to_reveal(100, 300, 40, 50), 40);
        // Below: align the bottom edge.
        assert_eq!(scroll_to_reveal(100, 300, 380, 50), 130);
        // Never negative.
        assert_eq!(scroll_to_reveal(0, 300, 0, 400), 0);
    }

    #[test]
    fn selection_wraps() {
        assert_eq!(step_selection(0, -1, 3), 2);
        assert_eq!(step_selection(2, 1, 3), 0);
        assert_eq!(step_selection(9, 1, 3), 0);
        assert_eq!(step_selection(0, 1, 0), 0);
    }
}
