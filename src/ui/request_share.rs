//! Bot reply keyboard "request" buttons: share your phone number, your
//! location, users or a chat with the bot. tdesktop shows a confirmation for the phone number
//! (`lng_bot_share_phone`) and `ChoosePeerBox` with a send confirmation
//! (`lng_request_peer_confirm`) for users and chats.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{ReplyItem, SendOptions};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::location_picker::GeoOutcome;
use quill::request_share::{PeerFacts, chat_candidate, confirm_text, empty_text, user_candidate};
use quill::telegram::envelope::{ChatKind, RequestChatSpec, RequestUsersSpec};
use quill::telegram::requests::SendReply;
use std::cell::RefCell;
use std::rc::Rc;

/// The most chats the picker lists.
const MAX_ROWS: usize = 60;

#[derive(Clone)]
pub(super) enum RequestShareKind {
    Phone {
        bot_user_id: i64,
    },
    /// The device's current location (Telegram X `shareCurrentLocation`;
    /// tdesktop only says it is unavailable). `one_time` hides the keyboard
    /// once the location is sent.
    Location {
        one_time: bool,
    },
    Users(RequestUsersSpec),
    Chat(RequestChatSpec),
}

#[derive(Clone)]
pub(super) struct RequestShare {
    chat_id: ChatId,
    message_id: MessageId,
    bot_name: String,
    pub(super) kind: RequestShareKind,
    selected: Vec<ChatId>,
    /// The confirmation page (after picking).
    confirm: bool,
    /// Location request: what the platform said; `None` while it looks.
    pub(super) locate: Option<GeoOutcome>,
}

impl QuillApp {
    /// A request button on the bot's keyboard was pressed.
    pub(super) fn open_request_share(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        kind: RequestShareKind,
        cx: &mut Context<Self>,
    ) {
        let is_location = matches!(kind, RequestShareKind::Location { .. });
        let bot_name = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        self.message_ui.request_share = Some(RequestShare {
            chat_id,
            message_id,
            bot_name,
            kind,
            selected: Vec::new(),
            confirm: false,
            locate: None,
        });
        if is_location {
            self.locate_for_request(cx);
        }
        cx.notify();
    }

    /// A location button was pressed: ask for the device location and show
    /// the confirmation while it looks.
    pub(super) fn press_request_location(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        one_time: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .session()
            .and_then(|s| s.bot_user_id_for_chat(chat_id))
            .is_none()
        {
            self.set_status_note(StatusNote::info(tr(keys::Q_LOCATION_REQUEST_BOT_ONLY)), cx);
            return;
        }
        self.open_request_share(
            chat_id,
            message_id,
            RequestShareKind::Location { one_time },
            cx,
        );
    }

    /// (Re)start the location lookup of the open request.
    fn locate_for_request(&mut self, cx: &mut Context<Self>) {
        // The device location is the user's to share, never an agent's.
        if quill::agent::ui_driver::guard::refuse_if_injecting("look up the device location") {
            return;
        }
        if let Some(share) = self.message_ui.request_share.as_mut() {
            share.locate = None;
        }
        self.message_ui.request_locate = Some(self.locate_then(cx, |this, outcome, cx| {
            if let Some(share) = this.message_ui.request_share.as_mut() {
                share.locate = Some(outcome);
            }
            cx.notify();
        }));
    }

    /// Facts about a chat for the request filters.
    fn request_peer_facts(&self, chat_id: ChatId) -> Option<PeerFacts> {
        let session = self.session()?;
        let chat = session.chats.get(&chat_id.0)?;
        let mut facts = PeerFacts {
            is_user: false,
            is_bot: false,
            is_channel: false,
            is_forum: chat.is_forum_chat(),
            has_username: false,
            is_creator: session.chat_is_owner(chat_id),
            is_secret: false,
        };
        match chat.kind {
            ChatKind::Private { user_id } => {
                facts.is_user = true;
                facts.is_bot = session.is_bot_user(user_id.0);
            }
            ChatKind::Secret { .. } => facts.is_secret = true,
            ChatKind::BasicGroup { .. } => {}
            ChatKind::Supergroup {
                supergroup_id,
                is_channel,
            } => {
                facts.is_channel = is_channel;
                facts.has_username = session
                    .groups
                    .supergroup_usernames
                    .get(&supergroup_id)
                    .is_some_and(|name| !name.is_empty());
            }
            ChatKind::Unknown => return None,
        }
        Some(facts)
    }

    /// The chats that qualify for the open request, best first.
    fn request_candidates(&self, kind: &RequestShareKind) -> Vec<(ChatId, String)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        session
            .forward_destinations("")
            .into_iter()
            .filter_map(|chat| {
                let facts = self.request_peer_facts(chat.id)?;
                let ok = match kind {
                    RequestShareKind::Users(spec) => user_candidate(spec, facts),
                    RequestShareKind::Chat(spec) => chat_candidate(spec, facts),
                    RequestShareKind::Phone { .. } | RequestShareKind::Location { .. } => false,
                };
                ok.then(|| (chat.id, chat.title.clone()))
            })
            .take(MAX_ROWS)
            .collect()
    }

    fn request_toggle(&mut self, id: ChatId, cx: &mut Context<Self>) {
        let Some(share) = self.message_ui.request_share.as_mut() else {
            return;
        };
        match &share.kind {
            RequestShareKind::Users(spec) => {
                if let Some(index) = share.selected.iter().position(|c| *c == id) {
                    share.selected.remove(index);
                } else if share.selected.len() < spec.max_quantity.max(1) as usize {
                    share.selected.push(id);
                }
                // One user: pick and go straight to the confirmation.
                if spec.max_quantity <= 1 && !share.selected.is_empty() {
                    share.confirm = true;
                }
            }
            RequestShareKind::Chat(_) => {
                share.selected = vec![id];
                share.confirm = true;
            }
            RequestShareKind::Phone { .. } | RequestShareKind::Location { .. } => {}
        }
        cx.notify();
    }

    fn request_send(&mut self, cx: &mut Context<Self>) {
        let Some(share) = self.message_ui.request_share.take() else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_OPS_SHARING_WITH_A_BOT_NEEDS)));
            cx.notify();
            return;
        };
        let mut hide_keyboard = false;
        let result = match &share.kind {
            RequestShareKind::Phone { bot_user_id } => live
                .driver
                .share_phone_number(share.chat_id, *bot_user_id)
                .map(|_| ()),
            RequestShareKind::Location { one_time } => match share.locate {
                Some(GeoOutcome::Found(point)) => {
                    let reply_to = Some(SendReply {
                        message_id: share.message_id,
                        quote: None,
                        source_chat: None,
                        item: ReplyItem::Message,
                    });
                    let sent = live
                        .driver
                        .share_location_to_chat(
                            share.chat_id,
                            point.latitude(),
                            point.longitude(),
                            reply_to,
                            &SendOptions::default(),
                        )
                        .map(|_| ());
                    hide_keyboard = sent.is_ok() && *one_time;
                    sent
                }
                _ => return,
            },
            RequestShareKind::Users(spec) => {
                let ids: Vec<i64> = share
                    .selected
                    .iter()
                    .filter_map(|chat| match live.driver.session.chats.get(&chat.0)?.kind {
                        ChatKind::Private { user_id } => Some(user_id.0),
                        _ => None,
                    })
                    .collect();
                live.driver
                    .share_users_with_bot(share.chat_id, share.message_id, spec.id, &ids)
                    .map(|_| ())
            }
            RequestShareKind::Chat(spec) => match share.selected.first() {
                Some(picked) => live
                    .driver
                    .share_chat_with_bot(share.chat_id, share.message_id, spec.id, *picked)
                    .map(|_| ()),
                None => Ok(()),
            },
        };
        if hide_keyboard {
            self.dismiss_custom_keyboard(share.chat_id, share.message_id, cx);
        }
        self.connection.set_note(match result {
            Ok(()) => StatusNote::silent(tr_with(
                keys::Q_OPS_SHARED_WITH,
                &[("bot_name", &share.bot_name.to_string())],
            )),
            Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_SHARE_WITH_THE)),
        });
        cx.notify();
    }

    pub(super) fn build_request_share_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::RequestShare, |this, _, cx| {
                this.message_ui.request_share = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let share = this.message_ui.request_share.clone();
            let Some(share) = share else {
                return dialog.on_close(on_close);
            };
            let mut body = div().flex().flex_col().gap_2();
            let mut footer = div().flex().justify_end().gap_2().child(
                Button::new("request-share-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.message_ui.request_share = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::RequestShare, window, cx);
                    })),
            );
            let mut ready = true;
            let (title, confirm_label) = match (&share.kind, share.confirm) {
                (RequestShareKind::Phone { .. }, _) => {
                    body = body.child(div().text_sm().child(tr(keys::BOT_SHARE_PHONE)));
                    (
                        tr(keys::Q_OPS_SHARE_PHONE_NUMBER),
                        Some(tr(keys::BACKGROUND_SHARE)),
                    )
                }
                (RequestShareKind::Location { .. }, _) => {
                    let (text, retry) = match share.locate {
                        None => (tr(keys::Q_LOCATION_REQUEST_LOCATING).to_string(), false),
                        Some(GeoOutcome::Found(_)) => (
                            tr_with(
                                keys::Q_LOCATION_REQUEST_TEXT,
                                &[("bot_name", &share.bot_name)],
                            ),
                            false,
                        ),
                        Some(GeoOutcome::Denied) => {
                            (tr(keys::Q_LOCATION_REQUEST_DENIED).to_string(), true)
                        }
                        Some(GeoOutcome::Unavailable) => {
                            (tr(keys::Q_LOCATION_REQUEST_UNAVAILABLE).to_string(), true)
                        }
                    };
                    let failed = retry;
                    body = body.child(div().text_sm().child(text));
                    if failed {
                        body = body.child(
                            div().flex().child(
                                Button::new("request-location-retry")
                                    .label(tr(keys::Q_LOCATION_REQUEST_RETRY))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.locate_for_request(cx);
                                        cx.notify();
                                    })),
                            ),
                        );
                    }
                    ready = matches!(share.locate, Some(GeoOutcome::Found(_)));
                    (
                        tr(keys::Q_LOCATION_REQUEST_TITLE),
                        Some(tr(keys::BACKGROUND_SHARE)),
                    )
                }
                (_, true) => {
                    let names: Vec<String> = share
                        .selected
                        .iter()
                        .filter_map(|id| {
                            this.session()
                                .and_then(|s| s.chats.get(&id.0))
                                .map(|chat| chat.title.clone())
                        })
                        .collect();
                    body = body.child(div().text_sm().child(confirm_text(&names, &share.bot_name)));
                    (tr(keys::Q_OPS_SEND_TO_BOT), Some(tr(keys::SHARE_CONFIRM)))
                }
                (kind, false) => {
                    let users = matches!(kind, RequestShareKind::Users(_));
                    let rows = this.request_candidates(kind);
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(text_muted())
                            .child(tr(keys::Q_OPS_SELECT_A_CHAT)),
                    );
                    if rows.is_empty() {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(text_muted())
                                .child(empty_text(users)),
                        );
                    }
                    for (id, title) in rows {
                        let picked = share.selected.contains(&id);
                        let mut row =
                            Button::new(SharedString::from(format!("request-peer-{}", id.0)))
                                .label(title)
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.request_toggle(id, cx);
                                }));
                        if picked {
                            row = row.icon(gpui_kit::assets::IconName::Check);
                        }
                        body = body.child(row);
                    }
                    if let RequestShareKind::Users(spec) = kind
                        && spec.max_quantity > 1
                    {
                        footer = footer.child(
                            Button::new("request-share-next")
                                .label(tr(keys::CREATE_GROUP_NEXT))
                                .primary()
                                .disabled(share.selected.is_empty())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(share) = this.message_ui.request_share.as_mut() {
                                        share.confirm = !share.selected.is_empty();
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                    (tr(keys::Q_OPS_SELECT_A_CHAT), None)
                }
            };
            if let Some(label) = confirm_label {
                footer = footer.child(
                    Button::new("request-share-confirm")
                        .label(label)
                        .primary()
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.request_send(cx);
                            this.close_kit_dialog_if_done(DialogKind::RequestShare, window, cx);
                        })),
                );
            }
            let body = body.into_any_element();
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// A bot keyboard's share-phone / share-users / share-chat request.
    RequestShare => DialogSpec::new(
        2600,
        |app| app.message_ui.request_share.is_some(),
        QuillApp::build_request_share_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "request_share_tests.rs"]
mod tests;
