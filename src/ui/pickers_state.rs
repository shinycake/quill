//! Emoji, sticker and GIF panel and their search fields.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use quill::lang::{keys, tr};

pub(crate) struct PickerUi {
    /// The composer's emoji / sticker / GIF popover.
    pub(super) media_panel: super::media_panel::MediaPanel,
    /// The emoji status selector's search (cleared on each open).
    pub(super) status_search_input: Entity<TextareaState>,
    /// The emoji packs box's search: typing searches once it pauses
    /// (`emoji_sets_search_typed`); `emoji_set_keystroke` counts the edits so
    /// only the last one searches, and `emoji_set_return_tab` is the tab an
    /// emptied field goes back to.
    pub(super) emoji_set_search_input: Entity<InputState>,
    pub(super) emoji_set_keystroke: u64,
    pub(super) emoji_set_return_tab: quill::emoji::EmojiSetTab,
    pub(super) emoji_set_scroll: ScrollHandle,
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
    /// The stickers manager box: removals, dragged order and scroll.
    pub(super) sticker_manager: super::stickers_manager::StickersManagerUi,
}

impl PickerUi {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        emoji_search_input: Entity<TextareaState>,
        reaction_search_input: Entity<TextareaState>,
    ) -> Self {
        let status_search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_EMOJI_STATUS_SEARCH))
                .auto_grow(1, 1)
        });
        cx.subscribe(&status_search_input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let emoji_set_search_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MISC_SEARCH_EMOJI_PACKS)));
        // window-scope: the packs box lives in the main window only, and
        // `emoji_sets_search_typed` runs its search through `spawn_scoped`.
        cx.subscribe(
            &emoji_set_search_input,
            |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.emoji_sets_search_typed(cx);
                }
            },
        )
        .detach();
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
            status_search_input,
            emoji_set_search_input,
            emoji_set_keystroke: 0,
            emoji_set_return_tab: quill::emoji::EmojiSetTab::default(),
            emoji_set_scroll: ScrollHandle::new(),
            emoji_search_input,
            reaction_search_input,
            gif_search_input,
            gif_keystroke: 0,
            gif_scroll: ScrollHandle::new(),
            sticker_search_input,
            sticker_manager: Default::default(),
        }
    }
}
