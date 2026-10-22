//! Emoji, sticker and GIF panel and their search fields.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
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
    /// The GIFs tab's search: typing searches once it pauses
    /// (`gif_search_typed`); `gif_keystroke` counts the edits so only the
    /// last one searches.
    pub(super) gif_search_input: Entity<InputState>,
    pub(super) gif_keystroke: u64,
    pub(super) gif_scroll: ScrollHandle,
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
        let gif_search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::GIFS_SEARCH)));
        cx.subscribe_in(
            &gif_search_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    // Each chat window has its own field: type in its scope.
                    this.in_window_of(window, cx, |this, _, cx| this.gif_search_typed(cx));
                }
            },
        )
        .detach();
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
            gif_keystroke: 0,
            gif_scroll: ScrollHandle::new(),
            sticker_search_input,
        }
    }
}
