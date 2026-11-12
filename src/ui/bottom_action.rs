//! The action bar that replaces the composer: Unblock / Restart, Start,
//! Join group and Apply to join group (`quill::chat_bottom_bar`). Channels
//! keep their own footer (`groups::channel_footer`), which shares the
//! decision and gains the Discuss button.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use quill::connect::{ConnectDriver, JsonSender};

use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::chat_bottom_bar::{BarFacts, BottomBar, ChannelFacts, StartState, bottom_bar};
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr};
use quill::paid_messages::SendGate;
use quill::state::{ChatSummary, RequestPurpose, Session};
use quill::telegram::envelope::ChatKind;

/// A bot's Restart in this chat: the unblock is out, and the /start goes
/// when it is answered. The chat's value owns it, so another chat never
/// starts a bot on this one's answer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum BotRestart {
    #[default]
    Idle,
    Unblocking {
        request: RequestId,
        chat: ChatId,
    },
}

impl QuillApp {
    /// What the bar decision reads from the open chat.
    fn bottom_bar_facts(&self, session: &Session, chat: &ChatSummary) -> BarFacts {
        let peer = match chat.kind {
            ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. }
                if Some(user_id.0) != session.my_user_id =>
            {
                Some(user_id.0)
            }
            _ => None,
        };
        let bot = session.bot_user_id_for_chat(chat.id);
        let history = session.histories.get(&chat.id.0);
        let empty_history = history
            .is_some_and(|history| history.loaded_complete() && history.messages.is_empty())
            && chat.last_message().is_none();
        BarFacts {
            blocked: peer.is_some() && chat.blocked,
            is_bot: bot.is_some(),
            can_send: chat.can_post(),
            start: match bot {
                Some(_)
                    if session.bots.bot_start_params.contains_key(&chat.id.0) || empty_history =>
                {
                    session
                        .requests
                        .ids_for_chat(&[RequestPurpose::SendBotStartMessage], chat.id)
                        .first()
                        .map_or(StartState::Offered, |id| StartState::Starting(*id))
                }
                _ => StartState::NotStartable,
            },
            channel: matches!(chat.kind, ChatKind::Supergroup { .. }).then(|| ChannelFacts {
                broadcast: chat.is_channel(),
                membership: chat.member_status(),
                join_by_request: self.chat_join_by_request(chat.id),
                direct_messages: session.direct_messages_role(chat.id).is_some(),
            }),
            muted: chat.is_muted(),
            premium_required: session.send_gate(chat.id) == SendGate::PremiumRequired,
        }
    }

    /// The open chat's replacement bar, for the kinds this file draws.
    /// Channel Join and Mute stay with the channel footer.
    pub(super) fn bottom_action(&self) -> Option<BottomBar> {
        let session = self.session()?;
        if session.open_forum_topic().is_some() {
            return None;
        }
        let chat = session.chats.get(&session.open_chat_id()?.0)?;
        match bottom_bar(&self.bottom_bar_facts(session, chat))? {
            bar @ (BottomBar::Unblock { .. }
            | BottomBar::Start
            | BottomBar::Starting
            | BottomBar::PremiumRequired
            | BottomBar::JoinGroup
            | BottomBar::ApplyToJoin) => Some(bar),
            BottomBar::JoinChannel | BottomBar::MuteUnmute { .. } => None,
        }
    }

    pub(super) fn bottom_action_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let bar = self.bottom_action()?;
        let chat_id = self.session()?.open_chat_id()?;
        // `{name} only accepts messages from contacts and Premium users.`
        let sentence = (bar == BottomBar::PremiumRequired)
            .then(|| {
                SendGate::PremiumRequired
                    .sentence(&self.session()?.chats.get(&chat_id.0)?.title.clone())
            })
            .flatten();
        Some(
            div()
                .id("bottom-action-bar")
                .p_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .flex()
                .flex_col()
                .gap_2()
                .items_center()
                .justify_center()
                .when_some(sentence, |this, line| {
                    this.child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child(line),
                    )
                })
                .child(
                    Button::new("bottom-action")
                        .label(bar.label())
                        .primary()
                        .disabled(bar == BottomBar::Starting)
                        .w_full()
                        .max_w(px(360.))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.press_bottom_action(bar, chat_id, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    fn press_bottom_action(&mut self, bar: BottomBar, chat_id: ChatId, cx: &mut Context<Self>) {
        match bar {
            BottomBar::Unblock { restart } => self.unblock_peer(chat_id, restart, cx),
            BottomBar::Start => self.start_bot_from_bar(chat_id, cx),
            BottomBar::Starting => {}
            BottomBar::PremiumRequired => self.open_premium_from("require_premium", cx),
            BottomBar::JoinGroup | BottomBar::ApplyToJoin => {
                // Only a send that went out gets its own wording: a failed
                // one keeps `join_channel`'s error note.
                if self.join_channel(chat_id, cx) {
                    self.set_note(if bar == BottomBar::ApplyToJoin {
                        StatusNote::silent(tr(keys::Q_MEDIA_JOIN_REQUEST_SENT))
                    } else {
                        StatusNote::silent(tr(keys::Q_MEDIA_JOINING_GROUP))
                    });
                }
            }
            BottomBar::JoinChannel | BottomBar::MuteUnmute { .. } => {}
        }
        cx.notify();
    }

    /// Unblock without a confirmation, as Telegram Desktop does; a bot is
    /// restarted once the unblock is answered.
    pub(in crate::ui) fn unblock_peer(
        &mut self,
        chat_id: ChatId,
        restart: bool,
        cx: &mut Context<Self>,
    ) {
        let mut restart_now = false;
        if self.live.is_some() {
            // The wait lives in the value of the window that pressed.
            let mut waiting = std::mem::replace(&mut self.chat_ui.restart, BotRestart::Idle);
            let sent = self
                .live
                .as_mut()
                .map(|live| send_unblock(&mut live.driver, &mut waiting, chat_id, restart));
            self.chat_ui.restart = waiting;
            let Some(sent) = sent else {
                return;
            };
            let note = match sent {
                Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MEDIA_UNBLOCKING_USER)),
                Ok(None) => StatusNote::silent(tr(keys::Q_MEDIA_REQUEST_ALREADY_IN_FLIGHT)),
                Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
            };
            self.set_status_note(note, cx);
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.blocked = false;
            }
            self.set_note(StatusNote::silent(tr(keys::Q_MEDIA_UNBLOCKED_DEMO)));
            restart_now = restart;
        }
        // The /start of a live bot goes with the unblock's answer
        // (`pump_bot_restart`): behind its request it would reach a bot
        // still blocked on the server.
        if restart_now {
            self.start_bot_from_bar(chat_id, cx);
        }
    }

    /// Each window's frame: the answer to the unblock of a Restart sends
    /// the /start; a refusal sends nothing (its error note stands).
    pub(in crate::ui) fn pump_bot_restart(&mut self, cx: &mut Context<Self>) {
        let BotRestart::Unblocking { request, chat } = self.chat_ui.restart else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.chat_ui.restart = BotRestart::Idle;
            return;
        };
        let Some(answer) = live.driver.session.acks.take(request) else {
            return;
        };
        self.chat_ui.restart = BotRestart::Idle;
        if answer == quill::state::Answer::Done {
            self.start_bot_from_bar(chat, cx);
        }
    }

    fn start_bot_from_bar(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some((bot_id, parameter)) = self.session().and_then(|session| {
            Some((
                session.bot_user_id_for_chat(chat_id)?,
                session
                    .bots
                    .bot_start_params
                    .get(&chat_id.0)
                    .cloned()
                    .unwrap_or_default(),
            ))
        }) else {
            return;
        };
        if self.live.is_some() {
            self.press_bot_start(chat_id, bot_id, parameter, cx);
        } else {
            self.set_note(StatusNote::silent(tr(keys::Q_MEDIA_BOT_STARTED_DEMO)));
        }
    }
}

/// Sends the unblock of `chat_id`. For a Restart, the unblock this call
/// sent is the one the bar waits on (its answer sends the /start, and an
/// older wait of the chat's value is forgotten); an unblock already out
/// stays with the surface that sent it.
fn send_unblock<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    waiting: &mut BotRestart,
    chat_id: ChatId,
    restart: bool,
) -> Result<Option<quill::ids::RequestId>, quill::connect::ConnectSendError> {
    let sent = driver.set_chat_user_blocked(chat_id, false);
    if let Some(request) = bar_restart_wait(&sent)
        && restart
    {
        driver.session.acks.expect(request);
        let old = std::mem::replace(
            waiting,
            BotRestart::Unblocking {
                request,
                chat: chat_id,
            },
        );
        if let BotRestart::Unblocking { request: old, .. } = old {
            driver.session.acks.forget(old);
        }
    }
    sent
}

/// The unblock the bar's /start waits for: only one this call sent. An
/// unblock already out belongs to the surface that sent it (the web app's
/// phone flow waits on its own), and an answer has one taker.
fn bar_restart_wait<E>(
    sent: &Result<Option<quill::ids::RequestId>, E>,
) -> Option<quill::ids::RequestId> {
    match sent {
        Ok(Some(request)) => Some(*request),
        Ok(None) | Err(_) => None,
    }
}

#[cfg(test)]
mod restart_wait_tests {
    use super::{BotRestart, bar_restart_wait, send_unblock};
    use crate::ui::driver_fixture::driver_with;
    use quill::ids::{ChatId, RequestId};
    use quill::state::{Answer, RequestAcks};

    /// The real path: the phone flow's unblock is out and waited on in the
    /// session's acks; the bar's Restart sends nothing more, waits on
    /// nothing, and the phone flow still gets its answer.
    #[test]
    fn a_restart_over_an_unblock_the_phone_flow_awaits_leaves_its_ack() {
        let (mut driver, _recorder) = driver_with(&[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#.to_string(),
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#.to_string(),
        ]);
        let chat = ChatId(11);
        // The phone flow's own unblock.
        let phone = driver
            .set_chat_user_blocked(chat, false)
            .unwrap()
            .expect("sent");
        driver.session.acks.expect(phone);
        // The bar's Restart press while it is out.
        let mut waiting = BotRestart::Idle;
        let sent = send_unblock(&mut driver, &mut waiting, chat, true);
        assert_eq!(sent, Ok(None), "already in flight: nothing sent");
        assert!(
            matches!(waiting, BotRestart::Idle),
            "the bar waits on nothing"
        );
        driver.session.acks.settle(phone, Answer::Done);
        assert_eq!(driver.session.acks.take(phone), Some(Answer::Done));
    }

    /// The unblock the bar sent is the one it waits on, and a second press
    /// after its answer forgets nothing of another surface's.
    #[test]
    fn a_restart_waits_on_the_unblock_it_sent() {
        let (mut driver, _recorder) = driver_with(&[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#.to_string(),
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#.to_string(),
        ]);
        let mut waiting = BotRestart::Idle;
        let sent = send_unblock(&mut driver, &mut waiting, ChatId(11), true);
        let Ok(Some(request)) = sent else {
            panic!("sent: {sent:?}");
        };
        assert!(matches!(
            waiting,
            BotRestart::Unblocking { request: r, .. } if r == request
        ));
    }

    /// The phone flow's unblock is out and waited on: the bar's click
    /// (`Ok(None)`) takes nothing, so the phone flow still gets its answer.
    #[test]
    fn the_bar_leaves_an_unblock_another_surface_awaits() {
        let mut acks = RequestAcks::default();
        let phone = RequestId(5);
        acks.expect(phone);
        assert_eq!(bar_restart_wait::<()>(&Ok(None)), None);
        acks.settle(phone, Answer::Done);
        assert_eq!(acks.take(phone), Some(Answer::Done));
    }

    #[test]
    fn the_bar_waits_on_the_unblock_it_sent() {
        assert_eq!(
            bar_restart_wait::<()>(&Ok(Some(RequestId(9)))),
            Some(RequestId(9))
        );
        assert_eq!(bar_restart_wait(&Err(())), None);
    }
}
