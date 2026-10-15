//! Emoji, sticker and GIF panel and their search fields.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};

pub(crate) struct PickerUi {
    /// The composer's emoji / sticker / GIF popover.
    pub(super) media_panel: super::media_panel::MediaPanel,
    pub(super) emoji_status_hours_input: Entity<TextareaState>,
    pub(super) emoji_set_search_input: Entity<TextareaState>,
    pub(super) emoji_search_input: Entity<TextareaState>,
    /// The reaction selector's own search (cleared on each open).
    pub(super) reaction_search_input: Entity<TextareaState>,
    pub(super) gif_search_input: Entity<TextareaState>,
    pub(super) sticker_search_input: Entity<TextareaState>,
}

impl PickerUi {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        emoji_search_input: Entity<TextareaState>,
        reaction_search_input: Entity<TextareaState>,
    ) -> Self {
        let emoji_status_hours_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_CUSTOM_DURATION_IN_HOURS))
                .auto_grow(1, 1)
        });
        let emoji_set_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_EMOJI_PACKS))
                .auto_grow(1, 1)
        });
        let gif_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::GIFS_SEARCH))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let sticker_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_STICKERS_AND_SETS))
                .auto_grow(1, 1)
        });
        Self {
            media_panel: super::media_panel::MediaPanel::default(),
            emoji_status_hours_input,
            emoji_set_search_input,
            emoji_search_input,
            reaction_search_input,
            gif_search_input,
            sticker_search_input,
        }
    }
}
