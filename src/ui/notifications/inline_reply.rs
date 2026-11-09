//! Notification activations that need no window: an inline reply typed into
//! the OS notification and "Mark as read" run from the platform's response
//! callback (macOS, Windows) or the poll loop (Linux), so a hidden or
//! minimized window never has to come forward for them. Opening the chat
//! still goes through `flush_notifications` on the next render.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::composer::{ComposerReplyTo, ComposerSnapshot};
use quill::ids::AccountKey;
use quill::lang::{keys, tr};
use quill::locks::LockExt;
use quill::notify_accounts::ChatClick;
use quill::notify_call::CallPick;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// Register the GPUI callback for OS notification activations: calls
    /// and chats are told apart by their tags, and every pick keeps the
    /// account its tag names.
    pub(in crate::ui) fn register_notification_responses(cx: &mut Context<Self>) {
        let notification_app = cx.weak_entity();
        cx.on_system_notification_response(move |response, cx| {
            let action_id = response.action_id.as_ref().map(|id| id.as_ref());
            if let Some((tag_account, call_id)) =
                quill::notify_call::parse_call_notification_tag(&response.tag)
            {
                let Some(account) = quill::notify_accounts::account_from_tag(tag_account) else {
                    return;
                };
                let action = quill::notify_call::CallNotificationAction::from_id(action_id);
                let _ = notification_app.update(cx, |this, cx| {
                    // The account on screen, or a background account whose
                    // call rings (`background_accounts::calls`); the pick
                    // only answers that account's call.
                    let ringing_here = this.background_ring_account() == Some(&account);
                    let on_screen = this.session().is_some_and(|s| s.account == account);
                    if !ringing_here && !on_screen {
                        return;
                    }
                    if let Ok(mut clicks) = this.calls.notify_clicks.lock() {
                        clicks.push(CallPick {
                            account,
                            call_id,
                            action,
                        });
                    }
                    cx.notify();
                });
                // Same as a chat click: a hidden window may never render on
                // its own, and the pick runs from `flush_notifications`.
                let skip = notification_app
                    .read_with(cx, |this, _| this.delivery.windows.window_ids())
                    .unwrap_or_default();
                activate_everything(&skip, cx);
                return;
            }
            let Some((tag_account, chat_id)) = quill::notify::parse_notification_tag(&response.tag)
            else {
                return;
            };
            let Some(account) = quill::notify_accounts::account_from_tag(tag_account) else {
                return;
            };
            let action = NotificationAction::from_id(action_id);
            let _ = notification_app.update(cx, |this, cx| {
                // Run on the account on screen; another account's
                // notification (`parity:notify-all-accounts`) reads there,
                // or switches to it and opens the chat.
                this.queue_account_click(
                    ChatClick {
                        account,
                        chat_id,
                        action,
                    },
                    cx,
                );
            });
        });
    }

    /// Take the queued clicks of the account on screen. One shown for
    /// another account (switched away since) goes back to
    /// `background.clicks`, which runs it on its own account.
    pub(in crate::ui) fn take_on_screen_clicks(&mut self) -> Vec<ChatClick> {
        let queued: Vec<ChatClick> = self
            .delivery
            .notify_clicks
            .lock_or_recover()
            .drain(..)
            .collect();
        if queued.is_empty() {
            return queued;
        }
        let active = self.current_account();
        let (here, elsewhere) = quill::notify_accounts::split_by_account(queued, active.as_ref());
        if !elsewhere.is_empty() {
            self.background.clicks.lock_or_recover().extend(elsewhere);
            // The poll routes them (`drain_account_clicks`).
            quill::wake::UPDATES.poke();
        }
        here
    }

    /// Run the notification clicks that need no window (inline replies,
    /// "Mark as read") and bring the app forward for the ones that open a
    /// chat, which `flush_notifications` runs on the render that follows.
    /// Called from the response callback and from every poll (Linux clicks
    /// arrive on the listener thread).
    pub(in crate::ui) fn drain_background_clicks(&mut self, cx: &mut Context<Self>) {
        let clicks = self.take_on_screen_clicks();
        if clicks.is_empty() {
            return;
        }
        let mut open = Vec::new();
        for click in clicks {
            let chat_id = click.chat_id;
            match click.action {
                NotificationAction::MarkRead => self.mark_read_from_notification(chat_id, cx),
                NotificationAction::Reply => {
                    if !self.send_typed_notification_reply(&click.account, chat_id, cx) {
                        open.push(click);
                    }
                }
                NotificationAction::Open => open.push(click),
            }
        }
        if open.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.delivery.notify_clicks.lock() {
            guard.extend(open);
        }
        // A hidden (close-to-tray) or minimized window may never render
        // again on its own: bring the app forward so `flush_notifications`
        // runs and opens the chat.
        activate_everything(&self.delivery.windows.window_ids(), cx);
    }

    /// Send what was typed into `account`'s notification for the chat, if
    /// anything was: the one place the typed text is taken, so it is sent
    /// by whichever path runs the Reply click (on screen, or after the
    /// account switch a click on another account's toast asks for) and
    /// never waits to be sent by a later click. Returns whether a message
    /// went out; the caller opens the chat otherwise.
    pub(in crate::ui) fn send_typed_notification_reply(
        &mut self,
        account: &AccountKey,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> bool {
        let tag = quill::notify::notification_tag(&account.0, chat_id);
        let Some(text) = super::native_extras::take_reply_text(&tag, account, chat_id) else {
            return false;
        };
        // Length only: message text never goes to the log.
        eprintln!(
            "inline reply typed into a notification: {} chars",
            text.chars().count()
        );
        self.send_inline_reply(account, chat_id, &text, cx)
    }

    /// "Mark as read" from the notification: `viewMessages` through the
    /// chat's last message, as Telegram Desktop does, without moving the
    /// window; the toast goes away.
    pub(in crate::ui) fn mark_read_from_notification(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.mark_chat_as_read(chat_id);
        }
        self.dismiss_os_notification(chat_id, cx);
    }

    /// Send the text typed into the chat's notification
    /// (`quill::notify::plan_inline_reply`): a reply to the notified message
    /// in groups, a plain message in private chats, nothing while the app is
    /// locked or the text is blank. Sending reads the chat, as Telegram
    /// Desktop's `ApiWrap::sendAction` does, and withdraws the toast. Returns
    /// whether a message went out. `account` is the one the notification
    /// was shown for: nothing is sent while another account is on screen.
    pub(in crate::ui) fn send_inline_reply(
        &mut self,
        account: &AccountKey,
        chat_id: ChatId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let locked = self.account.passcode.is_locked();
        let Some(session) = self.session().filter(|s| &s.account == account) else {
            return false;
        };
        let private_chat = session.chats.get(&chat_id.0).is_none_or(|chat| {
            matches!(
                chat.kind,
                ChatKind::Private { .. } | ChatKind::Secret { .. }
            )
        });
        let message_id = self
            .delivery
            .reply_targets
            .get(&(account.clone(), chat_id))
            .copied();
        let Some(plan) = quill::notify::plan_inline_reply(text, locked, private_chat, message_id)
        else {
            return false;
        };
        let snapshot = ComposerSnapshot::capture(chat_id, session.view_generation, plan.text)
            .with_reply(
                plan.reply_to
                    .map(|message_id| ComposerReplyTo::new(chat_id, message_id)),
            );
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        match live.driver.send_text_snapshot(&snapshot) {
            Ok(_) => {
                let _ = live.driver.mark_chat_as_read(chat_id);
                self.dismiss_os_notification(chat_id, cx);
                true
            }
            Err(_) => {
                // Not sendable here (no rights, a closed topic, a channel):
                // open the chat instead so the user sees why.
                self.set_note(StatusNote::silent(tr(
                    keys::Q_PREFS_REPLY_FROM_THE_NOTIFICATION_WAS,
                )));
                false
            }
        }
    }
}

/// Bring the app and every window forward except Quill's notification
/// windows (`skip`): activating one would take the focus off the main
/// window, and on Windows GPUI's activation sends a synthetic Alt press
/// into it.
pub(in crate::ui) fn activate_everything(skip: &[WindowId], cx: &mut App) {
    cx.activate(true);
    for window in cx.windows() {
        if skip.contains(&window.window_id()) {
            continue;
        }
        let _ = window.update(cx, |_, window, _| window.activate_window());
    }
}
