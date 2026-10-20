//! A command picked from the `/` menu goes out as its own text message
//! (tdesktop `HistoryWidget::sendBotCommand`): the field, the tray, the
//! caption and the draft are not part of it, and what was typed around the
//! command stays where it was.

use super::*;
use quill::composer::SendOptions;

impl QuillApp {
    /// Send `command` (`/cmd` or `/cmd@bot`) to the open chat as a text-only
    /// message with the current reply target. It is not silent or scheduled
    /// unless the chat's own silent setting says so, and it respects slow
    /// mode and the viewer's rights. A chat that charges Stars holds the send
    /// for the usual confirmation (the paid guard), as tdesktop's
    /// `sendBotCommand` does. Returns false when the live path is not
    /// available (not signed in), so the caller falls back to the plain
    /// composer send. A demo session applies the message locally.
    pub(in crate::ui) fn send_menu_command(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.live.is_none() {
            if self.demo_session.is_none() {
                return false;
            }
            let chat_id = self.demo_session.as_ref().and_then(|s| s.open_chat);
            if chat_id.is_some_and(|chat| self.slow_mode_blocked(chat, cx)) {
                return true;
            }
            let reply = self.reply_for_send(cx);
            self.apply_demo_outgoing(command, None, reply.as_ref());
            if reply.is_some() {
                self.composer_ui.set_reply(None);
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_SEND_APPLIED_LOCALLY_NO,
            )));
            cx.notify();
            return true;
        }
        if !matches!(self.pane_mode(), PaneMode::Ready) {
            return false;
        }
        if self.session().is_some_and(|s| s.is_frozen()) {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_CHAT_YOUR_ACCOUNT_IS_FROZEN_AND,
            )));
            cx.notify();
            return true;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return true;
        };
        let supported = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.supported());
        if !supported {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_THIS_CHAT_TYPE_IS_NOT)));
            cx.notify();
            return true;
        }
        if self.deny_send(SendKind::Message, cx) || self.slow_mode_blocked(chat_id, cx) {
            return true;
        }
        let reply = self.reply_for_send(cx);
        let options = SendOptions {
            disable_notification: self.composer_effective_silent(),
            literal_text: true,
            ..SendOptions::default()
        };
        let Some(live) = self.live.as_mut() else {
            return true;
        };
        let snapshot =
            ComposerSnapshot::capture(chat_id, live.driver.session.view_generation, command)
                .with_reply(reply.clone())
                .with_send_options(options);
        match live.driver.send_snapshot(&snapshot) {
            Ok(_) => {
                if reply.is_some() {
                    self.composer_ui.set_reply(None);
                }
                let note = self.send_started_note(
                    quill::composer::ComposerScheduling::None,
                    tr(keys::Q_CHAT_SENDING),
                );
                self.connection.set_note(note);
            }
            Err(_) => self
                .connection
                .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SEND_MESSAGE))),
        }
        cx.notify();
        true
    }
}
