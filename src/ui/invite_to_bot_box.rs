//! The "Invite to Bot" box (TDLib 1.8.71 `inviteUsersToBot`, tdesktop
//! `boxes/peers/invite_to_bot_box.cpp`): the people the user has chats
//! with, up to ten ticked, then Invite. Only for a bot the user owns.
//! The model and its stages are `quill::invite_to_bot`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::invite_to_bot::{INVITE_LIMIT, InviteRefusal, InviteStage, InviteToBot, Toggled};
use quill::lang::{keys, tr, tr_with, trn};
use quill::state::Answer;
use quill::telegram::envelope::ChatKind;

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;
/// People listed at most; the chat list's order puts the recent ones first.
const LISTED: usize = 200;

impl QuillApp {
    /// Opens the box for `bot_id` when the user owns the bot; `false` (and
    /// nothing opened) otherwise.
    pub(super) fn open_invite_to_bot(&mut self, bot_id: i64, cx: &mut Context<Self>) -> bool {
        let owned = self
            .session()
            .and_then(|session| session.user(bot_id))
            .is_some_and(|user| user.is_bot && user.can_be_edited);
        if !owned {
            return false;
        }
        self.drop_invite_to_bot();
        self.dialogs.invite_to_bot = Some(InviteToBot::new(bot_id));
        self.open_box(DialogKind::InviteToBot);
        cx.notify();
        true
    }

    /// The one closer: the box's Cancel, its close, Esc and a finished
    /// invite end here. An invite still out is forgotten with it.
    pub(super) fn close_invite_to_bot(&mut self, cx: &mut Context<Self>) {
        self.drop_invite_to_bot();
        cx.notify();
    }

    fn drop_invite_to_bot(&mut self) {
        let request = self
            .dialogs
            .invite_to_bot
            .take()
            .and_then(|invite| invite.waits_for());
        if let (Some(request), Some(live)) = (request, self.live.as_mut()) {
            live.driver.forget_bot_invite(request);
        }
    }

    /// tdesktop `InviteToBotController::createRow`: people only, never the
    /// user, a bot or a deleted account; in chat-list order.
    fn invite_to_bot_people(&self) -> Vec<(i64, String)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        session
            .forward_destinations("")
            .into_iter()
            .filter_map(|chat| match chat.kind {
                ChatKind::Private { user_id } => Some((user_id.0, chat.title.clone())),
                _ => None,
            })
            .filter(|(user_id, _)| {
                session.my_user_id != Some(*user_id)
                    && session
                        .user(*user_id)
                        .is_some_and(|user| !user.is_bot && !user.is_deleted)
            })
            .take(LISTED)
            .collect()
    }

    /// `shown` is the tick the row was drawn with. A click on the tick also
    /// reaches the row; the second handler finds the pick already changed
    /// and leaves it.
    fn toggle_invite_pick(&mut self, user_id: i64, shown: bool, cx: &mut Context<Self>) {
        let Some(invite) = self.dialogs.invite_to_bot.as_mut() else {
            return;
        };
        if invite.is_picked(user_id) != shown {
            return;
        }
        if invite.toggle(user_id) == Toggled::AtLimit {
            self.set_status_note(
                StatusNote::info(trn(keys::BOT_INVITE_LIMIT, INVITE_LIMIT as i64)),
                cx,
            );
        }
        cx.notify();
    }

    fn send_bot_invite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(invite) = self.dialogs.invite_to_bot.as_ref().filter(|i| i.can_send()) else {
            return;
        };
        let (bot_id, picked) = (invite.bot_id, invite.picked().to_vec());
        let Some(live) = self.live.as_mut() else {
            // The offline demo has no TDLib: it reads as sent.
            self.finish_bot_invite(picked.len(), window, cx);
            return;
        };
        match live.driver.invite_users_to_bot(bot_id, &picked) {
            Ok(request) => {
                if let Some(invite) = self.dialogs.invite_to_bot.as_mut() {
                    invite.stage = InviteStage::Sending { request };
                }
            }
            Err(_) => self.set_status_note(StatusNote::error(tr(keys::BOT_INVITE_FAILED)), cx),
        }
        cx.notify();
    }

    fn finish_bot_invite(&mut self, count: usize, window: &mut Window, cx: &mut Context<Self>) {
        let sent = if count == 1 {
            keys::BOT_INVITE_SENT_ONE
        } else {
            keys::BOT_INVITE_SENT_MANY
        };
        self.dialogs.invite_to_bot = None;
        self.set_status_note(StatusNote::info(tr(sent)), cx);
        self.close_kit_dialog_if_done(DialogKind::InviteToBot, window, cx);
    }

    /// Each frame the box is drawn: the answer to the invite that is out.
    /// Done closes the box; a refusal reads as tdesktop's toast and the
    /// ticks stay for another try.
    fn settle_bot_invite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(invite) = self.dialogs.invite_to_bot.as_ref() else {
            return;
        };
        let Some(request) = invite.waits_for() else {
            return;
        };
        let picked = invite.picked().to_vec();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(answer) = live.driver.session.acks.take(request) else {
            return;
        };
        let refusal = live.driver.session.bots.take_invite_refusal(request);
        match answer {
            Answer::Done => self.finish_bot_invite(picked.len(), window, cx),
            Answer::Failed(reason) => {
                if let Some(invite) = self.dialogs.invite_to_bot.as_mut() {
                    invite.stage = InviteStage::Picking;
                }
                let text = match refusal {
                    Some(InviteRefusal::AlreadyStarted) => match picked.as_slice() {
                        [one] => tr_with(
                            keys::BOT_INVITE_ALREADY_ONE,
                            &[("user", &self.contact_display_name(*one).to_string())],
                        ),
                        _ => tr(keys::BOT_INVITE_ALREADY_MANY).to_string(),
                    },
                    Some(InviteRefusal::Failed) => tr(keys::BOT_INVITE_FAILED).to_string(),
                    Some(InviteRefusal::Other(reason)) => reason,
                    None => reason,
                };
                self.set_status_note(StatusNote::error(text), cx);
            }
        }
    }

    fn invite_to_bot_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.settle_bot_invite(window, cx);
        let Some(invite) = self.dialogs.invite_to_bot.as_ref() else {
            return div().into_any_element();
        };
        let busy = invite.waits_for().is_some();
        let rows = self
            .invite_to_bot_people()
            .into_iter()
            .map(|(user_id, name)| {
                let photo = self
                    .session()
                    .and_then(|session| session.user_photo_path(user_id))
                    .map(std::path::PathBuf::from);
                let checked = invite.is_picked(user_id);
                div()
                    .id(SharedString::from(format!("invite-to-bot-row-{user_id}")))
                    .debug_selector(move || format!("invite-to-bot-row-{user_id}"))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|row| row.bg(cx.theme().secondary))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_invite_pick(user_id, checked, cx);
                    }))
                    .child(chat_avatar(&name, photo.as_deref(), 32.))
                    .child(div().flex_1().min_w_0().truncate().child(name))
                    .child(
                        Checkbox::new(SharedString::from(format!("invite-to-bot-tick-{user_id}")))
                            .checked(checked)
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_invite_pick(user_id, checked, cx);
                            })),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .id("invite-to-bot-list")
            .flex()
            .flex_col()
            .max_h(px(420.))
            .overflow_y_scroll()
            .children(rows)
            .into_any_element()
    }

    fn build_invite_to_bot_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::InviteToBot, |this, _, cx| {
                this.close_invite_to_bot(cx);
            });
        let (count, can_send, busy) =
            app.read(cx)
                .dialogs
                .invite_to_bot
                .as_ref()
                .map_or((0, false, false), |invite| {
                    (
                        invite.picked().len(),
                        invite.can_send(),
                        invite.waits_for().is_some(),
                    )
                });
        let title = format!("{}  {count} / {INVITE_LIMIT}", tr(keys::BOT_INVITE_TITLE));
        let body_app = app.clone();
        let footer = app.update(cx, |_, cx| {
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("invite-to-bot-send")
                        .label(tr(keys::BOT_INVITE_BUTTON))
                        .primary()
                        .disabled(!can_send)
                        .loading(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.send_bot_invite(window, cx);
                        })),
                )
                .child(
                    Button::new("invite-to-bot-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_invite_to_bot(cx);
                            this.close_kit_dialog_if_done(DialogKind::InviteToBot, window, cx);
                        })),
                )
        });
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(title))
            .content(move |content, window, cx| {
                content.child(body_app.update(cx, |this, cx| this.invite_to_bot_body(window, cx)))
            })
            .footer(footer)
            .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    /// "Invite to Bot" for a bot the user owns.
    InviteToBot => DialogSpec::new(
        6305,
        |app| app.dialogs.invite_to_bot.is_some(),
        QuillApp::build_invite_to_bot_dialog,
    )
    .close(|app, _, cx| app.close_invite_to_bot(cx)),
}
