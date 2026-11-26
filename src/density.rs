//! Display density (Settings → Appearance → Compact mode): one typed value
//! and one set of sizes that every density-aware surface reads, so the chat
//! list's virtual list, its row renderer and the sidebar header fit cannot
//! drift apart. Pure data: the UI turns these into pixels and kit sizes
//! (`ui/appearance/density.rs`).
//!
//! Sizes are pixels at the kit's fixed 16px rem (the interface scale zooms
//! whole windows instead). `None` keeps the value the element inherits.
#![deny(clippy::indexing_slicing)]

use crate::layout_style::RowScale;
use serde::{Deserialize, Serialize};

/// How tightly the chat list and the conversation are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Standard,
    Compact,
}

impl Density {
    /// The sizes this density draws with in Quill's own row scale.
    pub fn metrics(self) -> &'static DensityMetrics {
        self.metrics_in(RowScale::Quill)
    }

    /// The sizes this density draws with in a layout's row scale: Web K's
    /// taller rows and larger photos are Standard's alone; Compact keeps
    /// its own (docs/decisions/floating-layout.md).
    pub fn metrics_in(self, rows: RowScale) -> &'static DensityMetrics {
        match (self, rows) {
            (Density::Standard, RowScale::Quill) => &STANDARD,
            (Density::Standard, RowScale::WebK) => &STANDARD_WEBK,
            (Density::Compact, _) => &COMPACT,
        }
    }
}

/// The kit component size (`gpui_kit::Size`) the density's controls take.
/// Core has no kit types; the UI maps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlSize {
    Medium,
    Small,
}

/// Every size that changes with the density.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensityMetrics {
    /// The sidebar header's and the title bar's buttons, the sidebar and
    /// folder tabs, and the message field.
    pub control: ControlSize,
    /// Edge of a kit icon-only button at `control` (`size_8` at Medium,
    /// `size_6` at Small).
    pub icon_button: f32,

    /// Every window's title bar. Dialogs' dismiss band, sheets and
    /// notifications keep clear of it.
    pub title_bar: f32,

    /// The sidebar's leading and trailing inset.
    pub sidebar_inset_x: f32,
    /// The sidebar's top and bottom inset.
    pub sidebar_inset_y: f32,
    /// Between the sidebar's header, folder tabs and list.
    pub sidebar_gap: f32,
    /// A labelled sidebar tab's horizontal padding plus its share of the
    /// segmented bar (`TabVariant::Segmented` at `control`).
    pub tab_padding: f32,

    /// A two-line chat-list row: see [`crate::chatlist_style::chat_row_height_px`].
    pub row_base: f32,
    /// Each preview line past the second.
    pub row_line: f32,
    /// The folder-tag strip under a row.
    pub row_tags: f32,
    /// The chat photo in a list row.
    pub avatar: f32,
    /// A row's leading and trailing padding.
    pub row_inset: f32,
    /// Between a row's photo and its text.
    pub row_gap: f32,
    /// Between a row's title and its preview.
    pub row_lines_gap: f32,
    /// Line height of a row's text, and of the conversation header's title
    /// and status. The row heights above fit `row_base / line_height` lines.
    pub line_height: Option<f32>,

    /// The conversation header's leading and trailing padding.
    pub header_inset_x: f32,
    /// The conversation header's top and bottom padding.
    pub header_inset_y: f32,

    /// The pinned bar's top and bottom padding.
    pub pinned_inset_y: f32,
    /// The pinned bar's title and preview block.
    pub pinned_lines: f32,
    /// Line height in that block.
    pub pinned_line_height: Option<f32>,
    /// The pinned bar's height until it was laid out once: the lines, the
    /// padding and the bottom border.
    pub pinned_bar: f32,

    /// The composer's (and the bar in its place) top and bottom padding.
    pub composer_inset_y: f32,
    /// Between the composer's buttons and field.
    pub composer_gap: f32,
    /// The message field's least height: a `control` field shorter than the
    /// Medium icon buttons beside it is centred on them.
    pub composer_field_min_h: Option<f32>,
    /// A channel's Mute/Unmute bar in place of the composer: one composer
    /// row.
    pub channel_bar: f32,

    /// Under each message.
    pub message_gap: f32,
    /// Above the first message of a sender's run.
    pub sender_gap: f32,

    /// The chat search field.
    pub search_field: f32,
}

/// The default density.
pub const STANDARD: DensityMetrics = DensityMetrics {
    control: ControlSize::Medium,
    icon_button: 32.,
    title_bar: 34.,
    sidebar_inset_x: 12.,
    sidebar_inset_y: 12.,
    sidebar_gap: 8.,
    tab_padding: 34.,
    row_base: 64.,
    row_line: 16.,
    row_tags: 24.,
    avatar: 46.,
    row_inset: 8.,
    row_gap: 12.,
    row_lines_gap: 2.,
    line_height: None,
    header_inset_x: 16.,
    header_inset_y: 8.,
    pinned_inset_y: 6.,
    pinned_lines: 34.,
    pinned_line_height: None,
    pinned_bar: 48.,
    composer_inset_y: 12.,
    composer_gap: 8.,
    composer_field_min_h: None,
    channel_bar: 52.,
    message_gap: 4.,
    sender_gap: 8.,
    search_field: 40.,
};

/// Standard in the Floating layout: Web K's 72 px rows, 54 px photos and
/// 44 px search field; everything else is Standard's.
pub const STANDARD_WEBK: DensityMetrics = DensityMetrics {
    row_base: 72.,
    avatar: 54.,
    search_field: 44.,
    ..STANDARD
};

/// Compact mode: the kit's Small controls, tighter rows and bars. The title
/// bar is the kit's XSmall row step (`Size::table_row_height`), which holds a
/// Small (24px) button.
pub const COMPACT: DensityMetrics = DensityMetrics {
    control: ControlSize::Small,
    icon_button: 24.,
    title_bar: 26.,
    sidebar_inset_x: 8.,
    sidebar_inset_y: 8.,
    sidebar_gap: 4.,
    tab_padding: 30.,
    // Two 18px lines and a little air; the third line fits topic chips and
    // the unread badge.
    row_base: 40.,
    row_line: 20.,
    row_tags: 24.,
    avatar: 32.,
    row_inset: 4.,
    row_gap: 8.,
    row_lines_gap: 0.,
    line_height: Some(18.),
    header_inset_x: 12.,
    header_inset_y: 4.,
    pinned_inset_y: 2.,
    pinned_lines: 32.,
    pinned_line_height: Some(16.),
    pinned_bar: 37.,
    composer_inset_y: 4.,
    composer_gap: 4.,
    composer_field_min_h: Some(32.),
    channel_bar: 36.,
    message_gap: 0.,
    sender_gap: 4.,
    search_field: 32.,
};

// The compact title bar still holds a compact button.
const _: () = assert!(COMPACT.title_bar >= COMPACT.icon_button);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fit_their_photo_and_text_lines() {
        for m in [&STANDARD, &STANDARD_WEBK, &COMPACT] {
            assert!(m.avatar <= m.row_base);
            if let Some(line) = m.line_height {
                assert!(2. * line + m.row_lines_gap <= m.row_base);
                assert!(line <= m.row_line);
            }
        }
    }

    #[test]
    fn the_pinned_bar_guess_covers_its_lines_padding_and_border() {
        for m in [&STANDARD, &STANDARD_WEBK, &COMPACT] {
            assert!(m.pinned_bar >= m.pinned_lines + 2. * m.pinned_inset_y + 1.);
        }
    }
}
