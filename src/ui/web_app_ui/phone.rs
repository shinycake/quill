//! `web_app_request_phone`, as tdesktop answers it (`Panel::requestPhone` and
//! `WebViewInstance::botSharePhone`): a popup over the web view asks "Do you
//! want to share your phone number with this bot?"; "Share" unblocks the bot
//! first when it is blocked, then sends the person's own contact card into the
//! bot's chat, and the app hears `sent` once that went through and `cancelled`
//! otherwise.
//!
//! The popup is the web view helper's native one, so it shows over the app.

use super::*;
use quill::ids::{ChatId, RequestId};
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
    /// "Share" was pressed and the bot is being unblocked by `request`; its
    /// answer moves the flow on, its error ends it.
    Unblocking { since: Instant, request: RequestId },
    /// The contact card is on its way; only the answer to `request`
    /// finishes it.
    Sending { since: Instant, request: RequestId },
}

/// What the flow does next, given what the session shows.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PhoneStep {
    Wait,
    Send,
    Finish(bool),
}

impl PhoneFlow {
    /// The request the flow waits on, if any.
    pub(super) fn waiting_on(self) -> Option<RequestId> {
        match self {
            PhoneFlow::Unblocking { request, .. } | PhoneFlow::Sending { request, .. } => {
                Some(request)
            }
            PhoneFlow::Idle | PhoneFlow::Asking => None,
        }
    }

    /// One poll step. `unblocked` is how the unblock ended once TDLib
    /// answered it (`true`: ok, `false`: refused) and `sent` the card's
    /// outcome once TDLib answered.
    pub(super) fn step(
        self,
        now: Instant,
        unblocked: Option<bool>,
        sent: Option<bool>,
    ) -> PhoneStep {
        match self {
            PhoneFlow::Idle | PhoneFlow::Asking => PhoneStep::Wait,
            PhoneFlow::Unblocking { since, .. } => match unblocked {
                Some(true) => PhoneStep::Send,
                Some(false) => PhoneStep::Finish(false),
                None if now.saturating_duration_since(since) >= UNBLOCK_TIMEOUT => {
                    PhoneStep::Finish(false)
                }
                None => PhoneStep::Wait,
            },
            PhoneFlow::Sending { since, .. } => match sent {
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
        if let Some(Ok(Some(request))) = sent {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.acks.expect(request);
            }
            self.with_window(|window| {
                window.phone = PhoneFlow::Unblocking {
                    since: Instant::now(),
                    request,
                }
            });
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
        if let Some(Ok(request)) = sent {
            self.with_window(|window| {
                window.phone = PhoneFlow::Sending {
                    since: Instant::now(),
                    request,
                }
            });
        } else {
            self.finish_phone(false, cx);
        }
    }

    fn finish_phone(&mut self, shared: bool, cx: &mut Context<Self>) {
        let waiting = self
            .mini_apps
            .window
            .as_ref()
            .and_then(|window| window.phone.waiting_on());
        // An answer that comes after this (a timed-out send) is nobody's.
        if let Some(live) = self.live.as_mut() {
            if let Some(request) = waiting {
                live.driver
                    .session
                    .bots
                    .web_apps
                    .phone_share
                    .retain(|answer| answer.request() != request);
                live.driver.session.acks.forget(request);
            }
        }
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
        let unblocked = match (flow, self.live.as_mut()) {
            (PhoneFlow::Unblocking { request, .. }, Some(live)) => live
                .driver
                .session
                .acks
                .take(request)
                .map(|answer| matches!(answer, quill::state::Answer::Done)),
            _ => None,
        };
        let sent = self.live.as_mut().and_then(|live| {
            let PhoneFlow::Sending { request, .. } = flow else {
                return None;
            };
            let shares = &mut live.driver.session.bots.web_apps.phone_share;
            let at = shares.iter().position(|answer| answer.request() == request)?;
            Some(shares.remove(at).is_sent())
        });
        match flow.step(Instant::now(), unblocked, sent) {
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
    use quill::ids::RequestId;
    use std::time::Instant;

    #[test]
    fn unblocking_sends_once_the_unblock_is_answered() {
        let start = Instant::now();
        let flow = PhoneFlow::Unblocking {
            since: start,
            request: RequestId(4),
        };
        assert_eq!(flow.step(start, None, None), PhoneStep::Wait);
        assert_eq!(flow.step(start, Some(true), None), PhoneStep::Send);
    }

    #[test]
    fn a_refused_unblock_ends_the_flow_at_once() {
        let start = Instant::now();
        let flow = PhoneFlow::Unblocking {
            since: start,
            request: RequestId(4),
        };
        assert_eq!(
            flow.step(start, Some(false), None),
            PhoneStep::Finish(false)
        );
    }

    #[test]
    fn unblocking_gives_up_after_the_timeout() {
        let start = Instant::now();
        let flow = PhoneFlow::Unblocking {
            since: start,
            request: RequestId(4),
        };
        assert_eq!(
            flow.step(start + UNBLOCK_TIMEOUT, None, None),
            PhoneStep::Finish(false)
        );
    }

    #[test]
    fn sending_reports_the_outcome_or_times_out() {
        let start = Instant::now();
        let flow = PhoneFlow::Sending {
            since: start,
            request: RequestId(5),
        };
        assert_eq!(flow.step(start, None, None), PhoneStep::Wait);
        assert_eq!(flow.step(start, None, Some(true)), PhoneStep::Finish(true));
        assert_eq!(
            flow.step(start, None, Some(false)),
            PhoneStep::Finish(false)
        );
        assert_eq!(
            flow.step(start + SEND_TIMEOUT, None, None),
            PhoneStep::Finish(false)
        );
    }

    #[test]
    fn an_unanswered_prompt_waits() {
        let start = Instant::now();
        assert_eq!(
            PhoneFlow::Asking.step(start, None, Some(true)),
            PhoneStep::Wait
        );
        assert_eq!(
            PhoneFlow::Idle.step(start, Some(true), None),
            PhoneStep::Wait
        );
    }
}
