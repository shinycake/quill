//! The chat window's body, built by `QuillApp` with the window's view state
//! swapped in (`view::WindowScope`): the conversation header, the history
//! and a compose field, no chat list (Telegram Desktop's single-chat
//! window: `SeparateId::hasChatsList()` is false for `SeparateType::Chat`).

use super::super::app::{QuillApp, pane_placeholder};
use super::super::history::history_skeleton;
use super::ChatWindowView;
use gpui_kit::TestSupportExt;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme, Icon, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;

impl QuillApp {
    /// Header, history and compose field for `chat_id`. Runs with the
    /// window's `HistoryUi` in `self.history` and `windows.scope` set, so
    /// the shared conversation code reads the window's chat.
    pub(super) fn chat_window_body(
        &mut self,
        chat_id: ChatId,
        composer: &Entity<TextareaState>,
        view: WeakEntity<ChatWindowView>,
        note: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locked = self.account.passcode.locked;
        if locked {
            return self.chat_window_lock_cover(cx);
        }
        // Rows this window painted last frame are what the user saw.
        self.report_visible_history(window.is_window_active(), cx);
        let session = self.session();
        let Some(chat) = session.and_then(|s| s.chats.get(&chat_id.0).cloned()) else {
            return div().size_full().into_any_element();
        };
        let title = chat.title.clone();
        let actions = Some((
            chat.id,
            chat.is_muted(),
            chat.notification_settings.is_muted_forever(),
            chat.in_archive,
        ));
        let header = self
            .conversation_header(&title, actions, chat.is_peer_typing(), cx)
            .into_any_element();
        let history = self.chat_window_history(chat_id, &chat, cx);
        let can_write = chat.supported() && chat.can_post();
        div()
            .id("chat-window-body")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(header)
            .child(
                div()
                    .id("chat-window-history")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(history),
            )
            .when(can_write, |this| {
                this.child(self.chat_window_composer(composer, view, note, cx))
            })
            .into_any_element()
    }

    /// The history list for the window's chat: loading, failed, empty, or
    /// the rows (the same virtualized list as the main window's).
    fn chat_window_history(
        &mut self,
        chat_id: ChatId,
        chat: &quill::state::ChatSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let history = session.and_then(|s| s.histories.get(&chat_id.0));
        let empty = history.is_none_or(|h| h.messages.is_empty());
        let loading = session.is_some_and(|s| s.history_loading(chat_id));
        let failed = session.is_some_and(|s| s.history_load_failed(chat_id));
        if empty {
            return if failed {
                div()
                    .id("chat-window-load-failed")
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .p_6()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Couldn’t load messages ·"),
                    )
                    .child(
                        Button::new("chat-window-retry")
                            .label("Retry")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.retry_history_for(chat_id);
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            } else if loading {
                history_skeleton().into_any_element()
            } else {
                pane_placeholder(
                    "No messages yet",
                    "Say hello to start the conversation.",
                    cx,
                )
                .into_any_element()
            };
        }
        let media_roots = self.media_display_roots();
        let (key, messages) =
            self.history_rows_input("window-history", Some(chat_id), None, history, None);
        let list = self.history_message_list(
            "window-history",
            messages,
            Some(chat),
            &chat.title,
            None,
            media_roots,
            cx,
        );
        self.history.rows_key = key;
        list
    }

    /// The compose field and the Send button, with the last send error
    /// under them. Text only for now: attachments, replies and edits stay
    /// with the main window's composer.
    fn chat_window_composer(
        &self,
        composer: &Entity<TextareaState>,
        view: WeakEntity<ChatWindowView>,
        note: Option<String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("chat-window-compose")
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .id("chat-window-composer")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .key_context(super::super::keybindings::COMPOSER_CONTEXT)
                            .child(
                                Textarea::new(composer)
                                    .appearance(false)
                                    .bordered(false)
                                    .aria_label("Message")
                                    .token(self.field_token_renderer(composer, cx)),
                            ),
                    )
                    .child(
                        Button::new("chat-window-send")
                            .icon(IconName::Send)
                            .primary()
                            .tooltip("Send")
                            .accessibility_label("Send message")
                            .on_click(move |_, window, cx| {
                                let _ = view.update(cx, |this, cx| this.send(window, cx));
                            }),
                    ),
            )
            .when_some(note, |this, note| {
                this.child(
                    div()
                        .id("chat-window-note")
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(note),
                )
            })
    }

    /// While the local passcode is on, the window shows nothing of the
    /// chat; unlocking happens in the main window.
    fn chat_window_lock_cover(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("chat-window-locked")
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                Icon::new(IconName::Lock)
                    .size(px(28.))
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().font_semibold().child("Quill is locked"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Enter your passcode in the main window to continue."),
            )
            .into_any_element()
    }
}
