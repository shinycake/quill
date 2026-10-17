//! Search inside the gallery's Files, Music and Links tabs (tdesktop
//! `info/media`: the Info search field, `EmptyWidget::setSearchQuery`).
//! The field sits under the tab strip; the tab's `query` and its results
//! decide everything the panel shows.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::connect::SEARCH_DEBOUNCE;
use quill::lang::{Key, keys, tr};
use quill::state::SharedMediaTab;

use super::shared_media_search_state::{SearchKey, SharedMediaSearchUi};

/// tdesktop `lng_media_*_empty_search` for the searchable tabs.
fn search_empty_key(tab: SharedMediaTab) -> Option<Key> {
    match tab {
        SharedMediaTab::Files => Some(keys::MEDIA_FILE_EMPTY_SEARCH),
        SharedMediaTab::Music => Some(keys::MEDIA_SONG_EMPTY_SEARCH),
        SharedMediaTab::Links => Some(keys::MEDIA_LINK_EMPTY_SEARCH),
        _ => None,
    }
}

impl QuillApp {
    /// The search field under the tab strip, for a tab that searches.
    pub(super) fn shared_media_search_row(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.session()?.shared_media_search_shown(tab) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        Some(
            div()
                .id("shared-media-search")
                .debug_selector(|| "shared-media-search".into())
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    Input::new(&self.media_search.input)
                        .small()
                        .cleanable(true)
                        .prefix(
                            Icon::new(gpui_kit::assets::IconName::Search)
                                .size(px(14.))
                                .text_color(muted),
                        )
                        .aria_label(tr(keys::DLG_FILTER)),
                )
                .into_any_element(),
        )
    }

    /// The empty state of a search that found nothing (tdesktop shows the
    /// text alone, without the tab's invitation to share something).
    pub(super) fn shared_media_search_empty_state(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(("shared-media-search-empty", tab.index() as u64))
            .debug_selector(|| "shared-media-search-empty".into())
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_6()
            .py_10()
            .child(div().text_3xl().child(tab.glyph()))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .children(search_empty_key(tab).map(tr)),
            )
            .into_any_element()
    }

    /// The field text changed: an empty text goes back to the plain list
    /// at once, typing waits for a quiet moment.
    pub(super) fn shared_media_search_typed(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(key) = self.shared_media_search_key() else {
            return;
        };
        if self.media_search.key != Some(key) {
            return;
        }
        self.media_search.keystroke = self.media_search.keystroke.wrapping_add(1);
        let keystroke = self.media_search.keystroke;
        let text = text.trim().to_string();
        if text.is_empty() || self.live.is_none() {
            self.search_shared_media_now(key, &text, cx);
            return;
        }
        self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if this.media_search.keystroke == keystroke && this.media_search.key == Some(key) {
                    this.search_shared_media_now(key, &text, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn search_shared_media_now(&mut self, key: SearchKey, text: &str, cx: &mut Context<Self>) {
        let (_, tab) = key;
        if let Some(live) = self.live.as_mut() {
            if live.driver.search_shared_media(tab, text).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_OPS_COULD_NOT_LOAD_THAT_TAB)));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.media.shared_media.begin_search(tab, text);
        }
        cx.notify();
    }

    /// The `(chat, tab)` the field belongs to: the open gallery's tab,
    /// when it searches.
    fn shared_media_search_key(&self) -> Option<SearchKey> {
        let gallery = &self.session()?.media.shared_media;
        let chat_id = gallery.chat_id?;
        (gallery.open && gallery.active_tab.searchable()).then_some((chat_id, gallery.active_tab))
    }

    /// Called as the shell lays out. A field made for another chat or
    /// tab is replaced by a fresh empty one, so every way of changing the
    /// tab or closing the gallery leaves no text behind and no pending
    /// search (its timer checks the key).
    pub(super) fn keep_shared_media_search_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = self.shared_media_search_key();
        if self.media_search.key == key {
            return;
        }
        self.media_search.keystroke = self.media_search.keystroke.wrapping_add(1);
        // The user was typing in the old field: carry the cursor to the new
        // one, so a tab change from elsewhere does not strand the keyboard
        // on an input that is no longer drawn.
        let was_typing = self
            .media_search
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        self.media_search.input = SharedMediaSearchUi::fresh_input(window, cx);
        self.media_search.key = key;
        if was_typing && key.is_some() {
            self.media_search
                .input
                .update(cx, |input, cx| input.focus(window, cx));
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "shared_media_search_tests.rs"]
mod tests;
