//! message menu overlay, copy/open URLs, pin banner, delete confirm.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::{ChatPreviewState, chat_preview_line};
use super::context_menu_keys::{MenuKeyTarget, row_key, submenu_key};
use super::message_menu_ui::{
    MenuRow, MessageMenuPage, info_row, menu_row, menu_separator, stack_rows,
};
use super::message_text::message_rich_block;
use super::search_ui::chat_search_jump_note;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::DeleteConfirm;
use quill::connect::PREVIEW_HISTORY_LIMIT;
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::message_menu::order;
use quill::poll::can_stop_poll;
use quill::state::{RequestPurpose, effective_preview};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChatKind, MessageContent, PollType, effective_content};
use std::sync::Arc;
use std::time::Instant;
impl QuillApp {
    /// "Stop sharing" on your own live location. The card flips to
    /// "Live location ended" when Telegram confirms with the new content.
    pub(super) fn stop_live_location(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_CHAT_STOPPING_A_LIVE_LOCATION_NEEDS,
            )));
            cx.notify();
            return;
        };
        self.connection
            .set_note(match live.driver.stop_live_location(chat_id, message_id) {
                Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_STOPPED_SHARING_YOUR_LIVE_LOCATION)),
                Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULDNT_STOP_SHARING_TRY_AGAIN)),
            });
        cx.notify();
    }

    /// Telegram Desktop's refusal for copying out of a protected chat;
    /// true when the chat is protected (and the note was shown).
    pub(super) fn refuse_protected_copy(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        if !session.chat_has_protected_content(chat_id) {
            return false;
        }
        let kind = session.chats.get(&chat_id.0).map(|chat| chat.kind.clone());
        self.connection.set_note(match kind {
            Some(ChatKind::Supergroup {
                is_channel: true, ..
            }) => StatusNote::info(tr(keys::ERROR_NOCOPY_CHANNEL)),
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. }) | None => {
                StatusNote::info(tr(keys::ERROR_NOCOPY_USER))
            }
            _ => StatusNote::info(tr(keys::ERROR_NOCOPY_GROUP)),
        });
        cx.notify();
        true
    }
}

/// A confirmation in Telegram Desktop's style: the question, Cancel and
/// a primary action that runs `on_confirm`.
fn confirm(
    window: &mut Window,
    cx: &mut App,
    question: &'static str,
    action: &'static str,
    on_confirm: impl Fn(&mut App) + 'static,
) {
    let on_confirm = std::rc::Rc::new(on_confirm);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_confirm = on_confirm.clone();
        alert
            .description(question)
            .ok_text(action)
            .cancel_text(tr(keys::CANCEL))
            .show_cancel(true)
            .on_ok(move |_, _, cx| {
                on_confirm(cx);
                true
            })
    });
}

/// Hide the pinned bar until a newer message is pinned (Telegram
/// Desktop's "Don't show pinned messages" for readers).
fn confirm_hide_pinned(
    window: &mut Window,
    cx: &mut App,
    app: WeakEntity<QuillApp>,
    chat_id: ChatId,
    newest: MessageId,
) {
    confirm(
        window,
        cx,
        quill::selection_pin::HIDE_PINNED_QUESTION,
        tr(keys::Q_CHAT_HIDE),
        move |cx| {
            let _ = app.update(cx, |this, cx| {
                this.history.hidden_pinned.insert(chat_id.0, newest);
                this.history.pinned_list_open = false;
                cx.notify();
            });
        },
    );
}

mod chat_menu_submenus;
mod clear_history_box;
mod close_chat_preview;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod go_to_tests;
mod message_menu_overlay;
mod top_person_menu;
mod unpin_from_banner;
