//! "Reply in Another Chat": a modal chat chooser for the composer's reply
//! (tdesktop `ShowReplyToChatBox`, window/window_peer_menu.cpp). The
//! message's author comes first when the message is from a group, then the
//! chat list; one click takes the reply to that chat. Rows and search are the
//! forward box's (`forward_box`). See `docs/decisions/codex-reply-elsewhere.md`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::forward_box::{RowPick, chooser_row, list_height};
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::agent::ui_driver::redact;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::telegram::envelope::{ChatKind, MessageSender};

/// tdesktop `boxWideWidth`, as the forward box.
const BOX_WIDTH: f32 = 420.;

fn section_label(text: &'static str, cx: &App) -> Div {
    div()
        .px_2()
        .pt_1()
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

impl QuillApp {
    /// The author of the replied-to message, when they have a chat of their
    /// own to answer in that the chooser can list.
    fn reply_author_chat(&self) -> Option<ChatId> {
        let reply = self.chat_ui.reply()?;
        let session = self.session()?;
        let message = session
            .histories
            .get(&reply.chat_id.0)?
            .messages
            .get(&reply.message_id.0)?;
        let MessageSender::User { user_id } = message.sender? else {
            return None;
        };
        let chat = session.chats.get(&user_id)?;
        (chat.id != reply.chat_id && !matches!(chat.kind, ChatKind::Secret { .. }))
            .then_some(chat.id)
    }

    fn reply_elsewhere_body(&self, viewport: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let query = self.share.search_input.read(cx).value().to_string();
        let searching = self
            .session()
            .is_some_and(|s| s.messages.share_search.is_searching());
        let secret = |id: ChatId| {
            self.session().is_some_and(|s| {
                s.chats
                    .get(&id.0)
                    .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
            })
        };
        let author = query
            .trim()
            .is_empty()
            .then(|| self.reply_author_chat())
            .flatten()
            .and_then(|id| self.chooser_rows("").into_iter().find(|row| row.id == id));
        let rows: Vec<_> = self
            .chooser_rows(&query)
            .into_iter()
            .filter(|row| !secret(row.id))
            .collect();
        let muted = cx.theme().muted_foreground;
        let mut list = div()
            .id("reply-dest-list")
            .flex()
            .flex_col()
            .h(list_height(viewport))
            .overflow_y_scroll();
        if let Some(author) = author {
            list = list
                .child(section_label(tr(keys::REPLY_IN_AUTHOR), cx))
                .child(chooser_row(author, RowPick::Reply, cx))
                .child(section_label(tr(keys::REPLY_IN_CHATS_LIST), cx));
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(muted)
                    .child(if searching {
                        tr(keys::Q_OPS_SEARCHING)
                    } else {
                        tr(keys::Q_OPS_NO_MATCHING_CHATS)
                    }),
            );
        } else {
            for row in rows {
                list = list.child(chooser_row(row, RowPick::Reply, cx));
            }
        }
        // The chooser lists chats, so it is a list to the UI driver: only
        // the tagged rows of granted chats and the search survive the
        // granted scope (`redact::LIST`, `redact::CONTROLS`).
        div()
            .id("reply-elsewhere")
            .role(gpui_kit::Role::Group)
            .a11y_tag(redact::LIST)
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id("reply-elsewhere-controls")
                    .role(gpui_kit::Role::Group)
                    .a11y_tag(redact::CONTROLS)
                    .child(self.chooser_search(tr(keys::Q_OPS_SEARCH_CHATS), cx)),
            )
            .child(list)
            .into_any_element()
    }

    pub(super) fn build_reply_elsewhere_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        _cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::ReplyElsewhere,
            |this, window, cx| {
                this.close_reply_panels(window, cx);
            },
        );
        let body_app = app.clone();
        let cancel_app = app.downgrade();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(tr(
                keys::REPLY_IN_ANOTHER_TITLE,
            )))
            .content(move |content, window, cx| {
                let viewport = window.viewport_size().height;
                let body = body_app.update(cx, |this, cx| {
                    if std::mem::take(&mut this.share.focus_search) {
                        let search = this.share.search_input.clone();
                        window.defer(cx, move |window, cx| {
                            search.update(cx, |input, cx| input.focus(window, cx));
                        });
                    }
                    this.reply_elsewhere_body(viewport, cx)
                });
                content.child(body)
            })
            .footer(
                div().flex().justify_end().w_full().child(
                    Button::new("reply-elsewhere-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(move |_, window, cx| {
                            let _ = cancel_app.update(cx, |this, cx| {
                                this.close_reply_panels(window, cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::ReplyElsewhere,
                                    window,
                                    cx,
                                );
                            });
                        }),
                ),
            )
            .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    /// "Reply in Another Chat" over the window (tdesktop `ShowReplyToChatBox`).
    ReplyElsewhere => DialogSpec::new(
        2931,
        |app| app.share.forward.reply_chooser_open() && app.chat_ui.has_reply(),
        QuillApp::build_reply_elsewhere_dialog,
    )
    .per_window()
    .close(|app, window, cx| app.close_reply_panels(window, cx)),
}
