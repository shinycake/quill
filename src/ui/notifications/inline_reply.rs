//! Notification activations that need no window: an inline reply typed into
//! the OS notification and "Mark as read" run from the platform's response
//! callback (macOS, Windows) or the poll loop (Linux), so a hidden or
//! minimized window never has to come forward for them. Opening the chat
//! still goes through `flush_notifications` on the next render.

use super::*;
use quill::composer::{ComposerReplyTo, ComposerSnapshot};
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// Register the GPUI callback for OS notification activations: calls
    /// and chats are told apart by their tags; a response for another
    /// account is ignored.
    pub(in crate::ui) fn register_notification_responses(cx: &mut Context<Self>) {
        let notification_app = cx.weak_entity();
        cx.on_system_notification_response(move |response, cx| {
            if let Some((account, call_id)) =
                quill::notify_call::parse_call_notification_tag(&response.tag)
            {
                let _ = notification_app.update(cx, |this, cx| {
                    if this
                        .session()
                        .is_none_or(|s| account != format!("account:{}", s.account.0))
                    {
                        return;
                    }
                    let action = quill::notify_call::CallNotificationAction::from_id(
                        response.action_id.as_ref().map(|id| id.as_ref()),
                    );
                    if let Ok(mut clicks) = this.calls.notify_clicks.lock() {
                        clicks.push((call_id, action));
                    }
                    cx.notify();
                });
                // Same as a chat click: a hidden window may never render on
                // its own, and the pick runs from `flush_notifications`.
                activate_everything(cx);
                return;
            }
            let Some((account, chat_id)) = quill::notify::parse_notification_tag(&response.tag)
            else {
                return;
            };
            let _ = notification_app.update(cx, |this, cx| {
                let action =
                    NotificationAction::from_id(response.action_id.as_ref().map(|id| id.as_ref()));
                // Another account's notification (`parity:notify-all-accounts`):
                // read it there, or switch to it and open the chat.
                if this
                    .session()
                    .is_none_or(|s| account != format!("account:{}", s.account.0))
                {
                    this.queue_account_click(account, chat_id, action, cx);
                    return;
                }
                if let Ok(mut clicks) = this.notify.notify_clicks.lock() {
                    clicks.push((chat_id, action));
                }
                this.drain_background_clicks(cx);
                cx.notify();
            });
        });
    }

    /// Run the notification clicks that need no window (inline replies,
    /// "Mark as read") and bring the app forward for the ones that open a
    /// chat, which `flush_notifications` runs on the render that follows.
    /// Called from the response callback and from every poll (Linux clicks
    /// arrive on the listener thread).
    pub(in crate::ui) fn drain_background_clicks(&mut self, cx: &mut Context<Self>) {
        let clicks: Vec<(ChatId, NotificationAction)> = self
            .notify
            .notify_clicks
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default();
        if clicks.is_empty() {
            return;
        }
        let mut open = Vec::new();
        for (chat_id, action) in clicks {
            match action {
                NotificationAction::MarkRead => self.mark_read_from_notification(chat_id, cx),
                NotificationAction::Reply => {
                    let tag = self.notification_tag(chat_id);
                    let typed = super::native_extras::take_reply_text(&tag, chat_id);
                    if let Some(text) = typed.as_deref() {
                        // Length only: message text never goes to the log.
                        eprintln!(
                            "inline reply typed into a notification: {} chars",
                            text.chars().count()
                        );
                    }
                    let sent = typed.is_some_and(|text| self.send_inline_reply(chat_id, &text, cx));
                    if !sent {
                        open.push((chat_id, action));
                    }
                }
                NotificationAction::Open => open.push((chat_id, action)),
            }
        }
        if open.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.notify.notify_clicks.lock() {
            guard.extend(open);
        }
        // A hidden (close-to-tray) or minimized window may never render
        // again on its own: bring the app forward so `flush_notifications`
        // runs and opens the chat.
        activate_everything(cx);
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
    /// whether a message went out.
    pub(in crate::ui) fn send_inline_reply(
        &mut self,
        chat_id: ChatId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let locked = self.account.passcode.locked;
        let Some(session) = self.session() else {
            return false;
        };
        let private_chat = session.chats.get(&chat_id.0).is_none_or(|chat| {
            matches!(
                chat.kind,
                ChatKind::Private { .. } | ChatKind::Secret { .. }
            )
        });
        let message_id = self.notify.reply_targets.get(&chat_id).copied();
        let Some(plan) = quill::notify::plan_inline_reply(text, locked, private_chat, message_id)
        else {
            return false;
        };
        let snapshot = ComposerSnapshot::capture(chat_id, session.view_generation, plan.text)
            .with_reply(
                plan.reply_to
                    .map(|message_id| ComposerReplyTo::new(chat_id, message_id, "")),
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
                self.connection.status_note = "Reply from the notification was not sent".into();
                false
            }
        }
    }

    /// The GPUI tag of the chat's notification for the current account.
    pub(in crate::ui) fn notification_tag(&self, chat_id: ChatId) -> String {
        let account = self
            .session()
            .map(|s| s.account.0.as_str())
            .unwrap_or("primary");
        quill::notify::notification_tag(account, chat_id)
    }
}

/// Bring the app and every window forward.
fn activate_everything(cx: &mut App) {
    cx.activate(true);
    for window in cx.windows() {
        let _ = window.update(cx, |_, window, _| window.activate_window());
    }
}
