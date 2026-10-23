//! `web_app_request_phone`, as tdesktop answers it (`Panel::requestPhone` and
//! `WebViewInstance::botSharePhone`): a popup over the web view asks "Do you
//! want to share your phone number with this bot?"; "Share" unblocks the bot
//! first when it is blocked, then sends the person's own contact card into the
//! bot's chat, and the app hears `sent` once that went through and `cancelled`
//! otherwise.
//!
//! The popup is the web view helper's native one, so it shows over the app.

use super::*;
use quill::ids::ChatId;
use quill::telegram::requests::ContactShare;
use std::time::Instant;

/// How long unblocking the bot may take before the share fails.
const UNBLOCK_TIMEOUT: Duration = Duration::from_secs(15);
/// How long TDLib may take to take the contact card before the share fails.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// Where a phone request of the open mini app stands.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PhoneFlow {
    #[default]
    Idle,
    /// The confirmation popup is up.
    Asking,
    /// "Share" was pressed and the bot is being unblocked.
    Unblocking(Instant),
    /// The contact card is on its way.
    Sending(Instant),
}

/// What the flow does next, given what the session shows.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PhoneStep {
    Wait,
    Send,
    Finish(bool),
}

impl PhoneFlow {
    /// One poll step. `blocked` is whether the bot is still blocked and
    /// `sent` the card's outcome once TDLib answered.
    pub(super) fn step(self, now: Instant, blocked: bool, sent: Option<bool>) -> PhoneStep {
        match self {
            PhoneFlow::Idle | PhoneFlow::Asking => PhoneStep::Wait,
            PhoneFlow::Unblocking(since) => {
                if !blocked {
                    PhoneStep::Send
                } else if now.saturating_duration_since(since) >= UNBLOCK_TIMEOUT {
                    PhoneStep::Finish(false)
                } else {
                    PhoneStep::Wait
                }
            }
            PhoneFlow::Sending(since) => match sent {
                Some(done) => PhoneStep::Finish(done),
                None if now.saturating_duration_since(since) >= SEND_TIMEOUT => {
                    PhoneStep::Finish(false)
                }
                None => PhoneStep::Wait,
            },
        }
    }
}

fn phone_popup() -> Popup {
    popup(
        tr(keys::SETTINGS_PHONE_LABEL),
        tr(keys::BOT_SHARE_PHONE).into(),
        vec![
            ("cancel", tr(keys::CANCEL), "cancel"),
            ("share", tr(keys::BACKGROUND_SHARE), "default"),
        ],
    )
}

impl QuillApp {
    /// `web_app_request_phone`: ask first.
    pub(super) fn request_mini_app_phone(&mut self) {
        let idle = self
            .mini_apps
            .window
            .as_ref()
            .is_some_and(|window| window.phone == PhoneFlow::Idle);
        if !idle {
            self.with_window(|window| window.emit(BridgeReply::phone_requested(false)));
            return;
        }
        self.with_window(|window| {
            window.phone = PhoneFlow::Asking;
            window.show_popup(PopupPurpose::Phone, phone_popup());
        });
    }

    /// The confirmation was answered.
    pub(super) fn answer_phone_prompt(&mut self, button: &str, cx: &mut Context<Self>) {
        if button != "share" {
            self.finish_phone(false, cx);
            return;
        }
        let Some(bot_id) = self
            .mini_apps
            .window
            .as_ref()
            .map(|window| window.launch.bot_user_id)
        else {
            return;
        };
        if !self.bot_chat_blocked(bot_id) {
            self.send_phone_contact(bot_id, cx);
            return;
        }
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.set_chat_user_blocked(ChatId(bot_id), false));
        if matches!(sent, Some(Ok(Some(_)))) {
            self.with_window(|window| window.phone = PhoneFlow::Unblocking(Instant::now()));
        } else {
            self.finish_phone(false, cx);
        }
    }

    fn bot_chat_blocked(&self, bot_id: i64) -> bool {
        self.session()
            .and_then(|session| session.chats.get(&bot_id))
            .is_some_and(|chat| chat.blocked)
    }

    fn send_phone_contact(&mut self, bot_id: i64, cx: &mut Context<Self>) {
        let contact = self.session().and_then(|session| {
            let me = session.my_user_id?;
            let user = session.user(me)?;
            Some(ContactShare {
                phone_number: user.phone_number.clone(),
                first_name: user.first_name.clone(),
                last_name: user.last_name.clone(),
                user_id: me,
            })
        });
        let sent = contact.and_then(|contact| {
            self.live
                .as_mut()
                .map(|live| live.driver.share_phone_with_bot(bot_id, &contact))
        });
        if matches!(sent, Some(Ok(_))) {
            self.with_window(|window| window.phone = PhoneFlow::Sending(Instant::now()));
        } else {
            self.finish_phone(false, cx);
        }
    }

    fn finish_phone(&mut self, shared: bool, cx: &mut Context<Self>) {
        self.with_window(|window| {
            window.phone = PhoneFlow::Idle;
            window.emit(BridgeReply::phone_requested(shared));
        });
        cx.notify();
    }

    /// Moves a running phone request along; called by the mini app poll.
    pub(super) fn poll_phone(&mut self, cx: &mut Context<Self>) {
        let Some((flow, bot_id)) = self
            .mini_apps
            .window
            .as_ref()
            .map(|window| (window.phone, window.launch.bot_user_id))
        else {
            return;
        };
        if matches!(flow, PhoneFlow::Idle | PhoneFlow::Asking) {
            return;
        }
        let blocked = self.bot_chat_blocked(bot_id);
        let sent = self.live.as_mut().and_then(|live| {
            let share = &mut live.driver.session.bots.web_apps.phone_share;
            match *share {
                Some((id, done)) if id == bot_id => {
                    *share = None;
                    Some(done)
                }
                _ => None,
            }
        });
        match flow.step(Instant::now(), blocked, sent) {
            PhoneStep::Wait => {}
            PhoneStep::Send => self.send_phone_contact(bot_id, cx),
            PhoneStep::Finish(done) => self.finish_phone(done, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    // No `super::*`: the kit's glob brings its own `test` attribute.
    use super::{PhoneFlow, PhoneStep, SEND_TIMEOUT, UNBLOCK_TIMEOUT};
    use std::time::Instant;

    #[test]
    fn unblocking_sends_once_the_bot_is_unblocked() {
        let start = Instant::now();
        let flow = PhoneFlow::Unblocking(start);
        assert_eq!(flow.step(start, true, None), PhoneStep::Wait);
        assert_eq!(flow.step(start, false, None), PhoneStep::Send);
    }

    #[test]
    fn unblocking_gives_up_after_the_timeout() {
        let start = Instant::now();
        let flow = PhoneFlow::Unblocking(start);
        assert_eq!(
            flow.step(start + UNBLOCK_TIMEOUT, true, None),
            PhoneStep::Finish(false)
        );
    }

    #[test]
    fn sending_reports_the_outcome_or_times_out() {
        let start = Instant::now();
        let flow = PhoneFlow::Sending(start);
        assert_eq!(flow.step(start, false, None), PhoneStep::Wait);
        assert_eq!(flow.step(start, false, Some(true)), PhoneStep::Finish(true));
        assert_eq!(
            flow.step(start, false, Some(false)),
            PhoneStep::Finish(false)
        );
        assert_eq!(
            flow.step(start + SEND_TIMEOUT, false, None),
            PhoneStep::Finish(false)
        );
    }

    #[test]
    fn an_unanswered_prompt_waits() {
        let start = Instant::now();
        assert_eq!(
            PhoneFlow::Asking.step(start, false, Some(true)),
            PhoneStep::Wait
        );
        assert_eq!(PhoneFlow::Idle.step(start, true, None), PhoneStep::Wait);
    }
}
