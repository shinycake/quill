//! Emoji, sticker and GIF panel and their search fields.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use quill::lang::{keys, tr};

/// A search that waits for typing to pause. The timer is held here, so a new
/// keystroke, a close or the screen going away drops it and it never fires.
#[derive(Default)]
pub(crate) enum Debounce {
    #[default]
    Idle,
    /// `query` goes out when the timer ends.
    Waiting { query: String, _timer: Task<()> },
}

impl Debounce {
    /// Wait for `timer` (replacing, and so cancelling, an earlier one).
    pub(super) fn arm(&mut self, query: String, timer: Task<()>) {
        *self = Self::Waiting {
            query,
            _timer: timer,
        };
    }

    /// Typing that was waiting no longer searches.
    pub(super) fn cancel(&mut self) {
        *self = Self::Idle;
    }

    /// The timer ended: the query to search, once.
    pub(super) fn fire(&mut self) -> Option<String> {
        match std::mem::take(self) {
            Self::Waiting { query, .. } => Some(query),
            Self::Idle => None,
        }
    }
}

pub(crate) struct PickerUi {
    /// The composer's emoji / sticker / GIF popover.
    pub(super) media_panel: super::media_panel::MediaPanel,
    /// The emoji status selector's search (cleared on each open).
    pub(super) status_search_input: Entity<TextareaState>,
    /// The emoji packs box's search: typing searches once it pauses
    /// (`emoji_sets_search_typed`); `emoji_set_debounce` holds the timer so
    /// only the last edit searches, and `emoji_set_return_tab` is the tab an
    /// emptied field goes back to.
    pub(super) emoji_set_search_input: Entity<InputState>,
    pub(super) emoji_set_debounce: Debounce,
    pub(super) emoji_set_return_tab: quill::emoji::EmojiSetTab,
    pub(super) emoji_set_scroll: ScrollHandle,
    pub(super) emoji_search_input: Entity<TextareaState>,
    /// The reaction selector's own search (cleared on each open).
    pub(super) reaction_search_input: Entity<TextareaState>,
    /// The GIFs tab's search: typing searches once it pauses
    /// (`gif_search_typed`); `gif_debounce` holds the timer so only the last
    /// edit searches.
    pub(super) gif_search_input: Entity<InputState>,
    pub(super) gif_debounce: Debounce,
    /// What this window's GIF tab shows (`gifs_ui`).
    pub(super) gif_wanted: super::gifs_ui::GifWanted,
    /// What this window last asked the session for (`gif_tab_upkeep`).
    pub(super) gif_asked: super::gifs_ui::GifAsked,
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
            emoji_set_debounce: Debounce::Idle,
            emoji_set_return_tab: quill::emoji::EmojiSetTab::default(),
            emoji_set_scroll: ScrollHandle::new(),
            emoji_search_input,
            reaction_search_input,
            gif_search_input,
            gif_debounce: Debounce::Idle,
            gif_wanted: Default::default(),
            gif_asked: Default::default(),
            gif_scroll: ScrollHandle::new(),
            sticker_search_input,
        }
    }
}

/// The account's session keeps the GIF gate and the status-choices
/// resolver open while a panel shows them: both close on the client being
/// left, so neither keeps downloading and resolving behind the next
/// account (the next screen's first sync opens what it shows).
impl super::on_screen::Farewell for PickerUi {
    fn farewell(app: &mut QuillApp, _window: &mut Window, _cx: &mut Context<QuillApp>) {
        if let Some(live) = app.live.as_mut() {
            live.driver.close_gif_panel();
            live.driver.close_emoji_status_choices();
        } else if let Some(session) = app.demo_session.as_mut() {
            session.stickers.gifs.close();
        }
    }
}
