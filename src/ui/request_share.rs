//! Bot reply keyboard "request" buttons: share your phone number, users or
//! a chat with the bot. tdesktop shows a confirmation for the phone number
//! (`lng_bot_share_phone`) and `ChoosePeerBox` with a send confirmation
//! (`lng_request_peer_confirm`) for users and chats.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::request_share::{PeerFacts, chat_candidate, confirm_text, empty_text, user_candidate};
use quill::telegram::envelope::{ChatKind, RequestChatSpec, RequestUsersSpec};
use std::cell::RefCell;
use std::rc::Rc;

/// The most chats the picker lists.
const MAX_ROWS: usize = 60;

#[derive(Clone)]
pub(super) enum RequestShareKind {
    Phone { bot_user_id: i64 },
    Users(RequestUsersSpec),
    Chat(RequestChatSpec),
}

#[derive(Clone)]
pub(super) struct RequestShare {
    chat_id: ChatId,
    message_id: MessageId,
    bot_name: String,
    kind: RequestShareKind,
    selected: Vec<ChatId>,
    /// The confirmation page (after picking).
    confirm: bool,
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
        });
        cx.notify();
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
                    RequestShareKind::Phone { .. } => false,
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
            RequestShareKind::Phone { .. } => {}
        }
        cx.notify();
    }

    fn request_send(&mut self, cx: &mut Context<Self>) {
        let Some(share) = self.message_ui.request_share.take() else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note =
                "sharing with a bot needs a live connection (demo)".into();
            cx.notify();
            return;
        };
        let result = match &share.kind {
            RequestShareKind::Phone { bot_user_id } => live
                .driver
                .share_phone_number(share.chat_id, *bot_user_id)
                .map(|_| ()),
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
        self.connection.status_note = match result {
            Ok(()) => format!("shared with {}", share.bot_name),
            Err(_) => "could not share with the bot".into(),
        };
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
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.message_ui.request_share = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::RequestShare, window, cx);
                    })),
            );
            let (title, confirm_label) = match (&share.kind, share.confirm) {
                (RequestShareKind::Phone { .. }, _) => {
                    body = body.child(
                        div()
                            .text_sm()
                            .child("Do you want to share your phone number with this bot?"),
                    );
                    ("Share phone number", Some("Share"))
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
                    ("Send to bot", Some("Send"))
                }
                (kind, false) => {
                    let users = matches!(kind, RequestShareKind::Users(_));
                    let rows = this.request_candidates(kind);
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(text_muted())
                            .child("Select a chat"),
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
                                .label("Next")
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
                    ("Select a chat", None)
                }
            };
            if let Some(label) = confirm_label {
                footer = footer.child(
                    Button::new("request-share-confirm")
                        .label(label)
                        .primary()
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
