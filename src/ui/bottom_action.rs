//! The action bar that replaces the composer: Unblock / Restart, Start,
//! Join group and Apply to join group (`quill::chat_bottom_bar`). Channels
//! keep their own footer (`groups::channel_footer`), which shares the
//! decision and gains the Discuss button.

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;

use gpui_kit::*;
use quill::chat_bottom_bar::{BarFacts, BottomBar, ChannelFacts, bottom_bar};
use quill::ids::ChatId;
use quill::state::{ChatSummary, Session};
use quill::telegram::envelope::ChatKind;

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
            .is_some_and(|history| history.loaded_complete && history.messages.is_empty())
            && chat.last_message.is_none();
        BarFacts {
            blocked: peer.is_some() && chat.blocked,
            is_bot: bot.is_some(),
            can_send: chat.can_post(),
            bot_start_pending: bot.is_some()
                && (session.bot_start_params.contains_key(&chat.id.0) || empty_history),
            channel: matches!(chat.kind, ChatKind::Supergroup { .. }).then(|| ChannelFacts {
                broadcast: chat.is_channel(),
                membership: chat.my_member_status,
                join_by_request: self.chat_join_by_request(chat.id),
            }),
            muted: chat.is_muted(),
        }
    }

    /// The open chat's replacement bar, for the kinds this file draws.
    /// Channel Join and Mute stay with the channel footer.
    pub(super) fn bottom_action(&self) -> Option<BottomBar> {
        let session = self.session()?;
        if session.open_topic.is_some() {
            return None;
        }
        let chat = session.chats.get(&session.open_chat?.0)?;
        match bottom_bar(&self.bottom_bar_facts(session, chat))? {
            bar @ (BottomBar::Unblock { .. }
            | BottomBar::Start
            | BottomBar::JoinGroup
            | BottomBar::ApplyToJoin) => Some(bar),
            BottomBar::JoinChannel | BottomBar::MuteUnmute { .. } => None,
        }
    }

    pub(super) fn bottom_action_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let bar = self.bottom_action()?;
        let chat_id = self.session()?.open_chat?;
        Some(
            div()
                .id("bottom-action-bar")
                .p_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Button::new("bottom-action")
                        .label(bar.label())
                        .primary()
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
            BottomBar::JoinGroup | BottomBar::ApplyToJoin => {
                self.join_channel(chat_id, cx);
                self.connection.status_note = if bar == BottomBar::ApplyToJoin {
                    "join request sent".into()
                } else {
                    "joining group…".into()
                };
            }
            BottomBar::JoinChannel | BottomBar::MuteUnmute { .. } => {}
        }
        cx.notify();
    }

    /// Unblock without a confirmation, as Telegram Desktop does; a bot is
    /// restarted right after.
    fn unblock_peer(&mut self, chat_id: ChatId, restart: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let note = match live.driver.set_chat_user_blocked(chat_id, false) {
                Ok(Some(_)) => "unblocking user…",
                Ok(None) => "request already in flight",
                Err(_) => "Couldn't reach Telegram; try again.",
            };
            self.set_status_note(note, cx);
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.blocked = false;
            }
            self.connection.status_note = "unblocked (demo)".into();
        }
        if restart {
            self.start_bot_from_bar(chat_id, cx);
        }
    }

    fn start_bot_from_bar(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some((bot_id, parameter)) = self.session().and_then(|session| {
            Some((
                session.bot_user_id_for_chat(chat_id)?,
                session
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
            self.connection.status_note = "bot started (demo)".into();
        }
    }
}
