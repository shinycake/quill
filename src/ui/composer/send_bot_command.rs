//! A command picked from the `/` menu goes out as its own text message
//! (tdesktop `HistoryWidget::sendBotCommand`): the field, the tray, the
//! caption and the draft are not part of it, and what was typed around the
//! command stays where it was. In a chat that charges Stars the send waits in
//! the paid guard for Pay, and [`MenuSendStage`] clears the reply bar and the
//! typed token when it goes out (tdesktop re-runs `sendBotCommand` from
//! `withPaymentApproved`: it sends, then cancels the reply if it is still the
//! one the send had). A Cancel keeps both.

use super::*;
use quill::composer::SendOptions;
use quill::ids::RequestId;
use quill::paid_messages_guard::PaidOutcome;
use std::ops::Range;

/// What came of sending a command picked from the menu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::ui) enum MenuSendOutcome {
    /// The message went out (or a demo session applied it).
    Sent,
    /// The chat charges Stars: the guard holds the send as `request` for the
    /// paid confirmation. Nothing went out yet, so the reply target and the
    /// typed token stay where they were (a declined Pay leaves them); the
    /// caller keeps `reply` in a [`MenuSendStage`], which
    /// clears them when the send goes out.
    Held {
        request: RequestId,
        reply: Option<ComposerReplyTo>,
    },
    /// A gate said no or the send failed; a note says why. Nothing went out.
    Refused,
    /// No live send here (connecting): the composer's own path decides.
    Unavailable,
}

impl QuillApp {
    /// Send `command` (`/cmd` or `/cmd@bot`) to the open chat as a text-only
    /// message with the current reply target. It is not silent or scheduled
    /// unless the chat's own silent setting says so, and it respects slow
    /// mode and the viewer's rights. A chat that charges Stars holds the send
    /// for the usual confirmation (the paid guard), as tdesktop's
    /// `sendBotCommand` does (`Held`: the reply and the typed token stay
    /// until it goes, then `MenuSendStage` clears them). `Unavailable` when the live path is not
    /// available (not signed in), so the caller falls back to the plain
    /// composer send. A demo session applies the message locally. The
    /// answer says whether anything went out: a gate that refuses (slow
    /// mode, rights, a frozen account, a failed send) leaves the composer's
    /// text to the caller, who drops the typed token only for `Sent`.
    pub(in crate::ui) fn send_menu_command(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) -> MenuSendOutcome {
        if self.live.is_none() {
            if self.demo_session.is_none() {
                return MenuSendOutcome::Unavailable;
            }
            let chat_id = self.demo_session.as_ref().and_then(|s| s.open_chat_id());
            if chat_id.is_some_and(|chat| self.slow_mode_blocked(chat, cx)) {
                return MenuSendOutcome::Refused;
            }
            let reply = self.reply_for_send(cx);
            self.apply_demo_outgoing(command, None, reply.as_ref());
            if reply.is_some() {
                self.chat_ui.set_reply(None);
            }
            self.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_SEND_APPLIED_LOCALLY_NO,
            )));
            cx.notify();
            return MenuSendOutcome::Sent;
        }
        if !matches!(self.pane_mode(), PaneMode::Ready) {
            return MenuSendOutcome::Unavailable;
        }
        if self.session().is_some_and(|s| s.is_frozen()) {
            self.set_note(StatusNote::error(tr(
                keys::Q_CHAT_YOUR_ACCOUNT_IS_FROZEN_AND,
            )));
            cx.notify();
            return MenuSendOutcome::Refused;
        }
        let Some(chat_id) = self.open_chat_id() else {
            return MenuSendOutcome::Refused;
        };
        let supported = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.supported());
        if !supported {
            self.set_note(StatusNote::silent(tr(keys::Q_CHAT_THIS_CHAT_TYPE_IS_NOT)));
            cx.notify();
            return MenuSendOutcome::Refused;
        }
        if self.deny_send(SendKind::Message, cx) || self.slow_mode_blocked(chat_id, cx) {
            return MenuSendOutcome::Refused;
        }
        let reply = self.reply_for_send(cx);
        let options = SendOptions {
            disable_notification: self.composer_effective_silent(),
            literal_text: true,
            ..SendOptions::default()
        };
        let Some(live) = self.live.as_mut() else {
            return MenuSendOutcome::Refused;
        };
        let snapshot =
            ComposerSnapshot::capture(chat_id, live.driver.session.view_generation, command)
                .with_reply(reply.clone())
                .with_send_options(options);
        let outcome = match live.driver.send_snapshot(&snapshot) {
            Ok(request) if live.driver.paid_send_is_held(request) => MenuSendOutcome::Held {
                request,
                reply: self.chat_ui.reply().cloned(),
            },
            Ok(_) => {
                if reply.is_some() {
                    self.chat_ui.set_reply(None);
                }
                let note = self.send_started_note(
                    quill::composer::ComposerScheduling::None,
                    tr(keys::Q_CHAT_SENDING),
                );
                self.set_note(note);
                MenuSendOutcome::Sent
            }
            Err(_) => {
                self.set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SEND_MESSAGE)));
                MenuSendOutcome::Refused
            }
        };
        cx.notify();
        outcome
    }
}


/// The `/` token a picked command replaces: where it stood and what it was,
/// so a field that changed meanwhile is not cut at a stale place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct MenuToken {
    pub(in crate::ui) range: Range<usize>,
    pub(in crate::ui) text: String,
}

/// A menu command's send, by stage. The chat's value: a chat change drops
/// it, and an account switch drops it with the screen. It leaves when the
/// guard says how the hold ended ([`Self::settle`]).
#[derive(Debug, Default)]
pub(in crate::ui) enum MenuSendStage {
    #[default]
    Idle,
    /// The guard holds the send as `request`. What the pick left in place
    /// until Pay: the reply it was sent with and the token it replaces.
    Held {
        request: RequestId,
        reply: Option<ComposerReplyTo>,
        token: Option<MenuToken>,
    },
}

/// What a released hold clears.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::ui) struct MenuRelease {
    /// The reply bar goes only if it still shows this reply.
    pub(in crate::ui) reply: Option<ComposerReplyTo>,
    pub(in crate::ui) token: Option<MenuToken>,
}

impl MenuSendStage {
    /// The request the guard holds, if a command waits.
    pub(in crate::ui) fn request(&self) -> Option<RequestId> {
        match self {
            Self::Idle => None,
            Self::Held { request, .. } => Some(*request),
        }
    }

    /// The stage after the guard's word on the request: still held, it
    /// stays; released, it ends with what to clear; discarded (or lost),
    /// it ends and everything stays where it was.
    pub(in crate::ui) fn settle(&mut self, outcome: PaidOutcome) -> Option<MenuRelease> {
        if self.request().is_none() || outcome == PaidOutcome::Held {
            return None;
        }
        let Self::Held { reply, token, .. } = std::mem::take(self) else {
            return None;
        };
        (outcome == PaidOutcome::Released).then_some(MenuRelease { reply, token })
    }
}

impl QuillApp {
    /// Each window's frame: a held menu command whose hold ended. Released,
    /// the send is done as a free chat's is; discarded, nothing changes.
    pub(in crate::ui) fn settle_menu_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.chat_ui.menu_send.request() else {
            return;
        };
        let outcome = self.live.as_ref().map_or(PaidOutcome::Unknown, |live| {
            live.driver.paid_send_outcome(request)
        });
        if let Some(release) = self.chat_ui.menu_send.settle(outcome) {
            self.apply_menu_release(release, window, cx);
        }
    }

    /// The command went out after Pay: the reply bar (if it still shows the
    /// reply the send had), the token and the open menu go, and the note
    /// says it is sending.
    pub(in crate::ui) fn apply_menu_release(
        &mut self,
        release: MenuRelease,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sent_with) = release.reply
            && self.chat_ui.reply() == Some(&sent_with)
        {
            self.chat_ui.set_reply(None);
        }
        let token = release.token.and_then(|token| {
            let value = self.composer.read(cx).value();
            (value.get(token.range.clone()) == Some(token.text.as_str())).then_some(token.range)
        });
        self.close_command_menu(cx);
        self.finish_menu_pick(token, window, cx);
        self.keep_text_after_menu_send(cx);
        let note = self.send_started_note(
            quill::composer::ComposerScheduling::None,
            tr(keys::Q_CHAT_SENDING),
        );
        self.set_note(note);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuSendStage, MenuToken, PaidOutcome, RequestId};

    fn held() -> MenuSendStage {
        MenuSendStage::Held {
            request: RequestId(7),
            reply: None,
            token: Some(MenuToken {
                range: 0..3,
                text: "/st".into(),
            }),
        }
    }

    /// A hold that is still out keeps the stage and everything with it.
    #[test]
    fn a_held_command_waits_while_the_guard_holds_it() {
        let mut stage = held();
        assert_eq!(stage.settle(PaidOutcome::Held), None);
        assert_eq!(stage.request(), Some(RequestId(7)));
    }

    /// Pay: the stage ends and hands over the reply and the token to clear.
    #[test]
    fn a_released_command_ends_the_stage_with_what_to_clear() {
        let mut stage = held();
        let release = stage.settle(PaidOutcome::Released).expect("released");
        assert_eq!(release.token.map(|t| t.range), Some(0..3));
        assert_eq!(stage.request(), None);
    }

    /// Cancel (or a hold that is gone): the stage ends and clears nothing.
    #[test]
    fn a_discarded_or_lost_command_clears_nothing() {
        for outcome in [PaidOutcome::Discarded, PaidOutcome::Unknown] {
            let mut stage = held();
            assert_eq!(stage.settle(outcome), None);
            assert_eq!(stage.request(), None, "the stage ends");
        }
    }
}
