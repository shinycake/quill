//! Bot and channel action rows of the info panel: Bot Help, Bot Settings,
//! Bot Privacy Policy, Report, Stop and block bot / Restart bot, and Join
//! Channel. Behaviour follows tdesktop's `ActionsFiller`
//! (`info/profile/info_profile_actions.cpp:2993-3209`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::pressable::action_row;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::{TestSupportExt, *};
use quill::bot_profile::{PrivacyTarget, find_command, privacy_target};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// The listed private chat with `user_id`, if there is one.
    pub(in crate::ui) fn listed_private_chat(&self, user_id: i64) -> Option<ChatId> {
        self.session()?
            .chats
            .values()
            .filter(|chat| chat.supported())
            .find(|chat| {
                matches!(&chat.kind, ChatKind::Private { user_id: peer } if peer.0 == user_id)
            })
            .map(|chat| chat.id)
    }

    /// Bot Help, Bot Settings and Bot Privacy Policy: the commands part of
    /// the block `if (CanReportBot(user))` of `fillUserActions`. `None`
    /// unless the user is a bot other than the viewer.
    pub(in crate::ui) fn bot_command_rows(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        session.user(user_id).filter(|user| user.is_bot)?;
        if session.my_user_id == Some(user_id) {
            return None;
        }
        let commands = session
            .bots
            .bot_info
            .get(&user_id)
            .and_then(|info| info.as_ref())
            .map(|info| info.commands.as_slice())
            .unwrap_or_default();
        let mut column = div().flex().flex_col().w_full().gap_0p5();
        for (id, icon, label, command) in [
            (
                "info-panel-bot-help",
                IconName::Info,
                tr(keys::PROFILE_BOT_HELP),
                "help",
            ),
            (
                "info-panel-bot-settings",
                IconName::Settings,
                tr(keys::PROFILE_BOT_SETTINGS),
                "settings",
            ),
        ] {
            if let Some(command) = find_command(commands, command) {
                let command = command.to_string();
                column = column.child(
                    action_row(id, Some(icon), label, false, cx)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.send_profile_bot_command(user_id, &command, window, cx);
                        }))
                        .test_support(),
                );
            }
        }
        column = column.child(
            action_row(
                "info-panel-bot-privacy",
                Some(IconName::Shield),
                tr(keys::PROFILE_BOT_PRIVACY),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_bot_privacy_policy(user_id, window, cx);
            }))
            .test_support(),
        );
        Some(column.into_any_element())
    }

    /// Report on a bot, drawn like Block and placed right before it. Shown
    /// once the bot's private chat is known and TDLib allows reporting it.
    pub(in crate::ui) fn bot_report_row(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        session.user(user_id).filter(|user| user.is_bot)?;
        if session.my_user_id == Some(user_id) {
            return None;
        }
        let chat_id = self.listed_private_chat(user_id)?;
        session.chats.get(&chat_id.0)?.can_be_reported.then(|| {
            action_row(
                "info-panel-bot-report",
                Some(IconName::Flag),
                tr(keys::PROFILE_REPORT),
                true,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_report(chat_id, Vec::new(), cx);
            }))
            .test_support()
            .into_any_element()
        })
    }

    /// Report on a broadcast channel the viewer does not own
    /// (`fillChannelActions`: `if (!channel->amCreator()) addReportAction`).
    pub(in crate::ui) fn channel_report_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let reportable = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_be_reported && !session.chat_is_owner(chat_id));
        reportable.then(|| {
            action_row(
                "info-panel-channel-report",
                Some(IconName::Flag),
                tr(keys::PROFILE_REPORT),
                true,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_report(chat_id, Vec::new(), cx);
            }))
            .test_support()
            .into_any_element()
        })
    }

    /// Join Channel, shown while the viewer is not in the channel
    /// (`addJoinChannelAction`).
    pub(in crate::ui) fn channel_join_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let not_in = self.session()?.chat_not_joined(chat_id);
        not_in.then(|| {
            action_row(
                "info-panel-channel-join",
                Some(IconName::UserPlus),
                tr(keys::PROFILE_JOIN_CHANNEL),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.join_channel(chat_id, cx);
            }))
            .test_support()
            .into_any_element()
        })
    }

    /// Send `/command` to the bot's private chat and show that chat
    /// (`BotCommandClickHandler` with the bot as context). The chat is
    /// created when the bot's panel opens (`ensure_private_chat`).
    pub(in crate::ui) fn send_profile_bot_command(
        &mut self,
        user_id: i64,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.listed_private_chat(user_id) else {
            self.open_user_chat(user_id, window, cx);
            return;
        };
        self.select_listed_chat(chat_id, window, cx);
        self.send_command_text(chat_id, &format!("/{command}"), cx);
    }

    /// Bot Privacy Policy: the published link, else `/privacy`, else the
    /// default page (`openPrivacyPolicy`).
    fn open_bot_privacy_policy(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .session()
            .and_then(|s| s.bots.bot_info.get(&user_id))
            .and_then(|info| info.as_ref())
            .map(|info| privacy_target(&info.privacy_policy_url, &info.commands))
            .unwrap_or(PrivacyTarget::Default);
        match target {
            PrivacyTarget::Url(url) => self.open_link(&url, cx),
            PrivacyTarget::Command(command) => {
                self.send_profile_bot_command(user_id, &command, window, cx)
            }
            PrivacyTarget::Default => self.open_link(tr(keys::PROFILE_BOT_PRIVACY_URL), cx),
        }
    }

    /// "Stop and block bot" blocks at once; "Restart bot" unblocks and
    /// starts the bot again, then shows its chat (`addBlockAction`).
    pub(in crate::ui) fn toggle_bot_block(
        &mut self,
        user_id: i64,
        blocked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !blocked {
            if let Some(live) = self.live.as_mut() {
                if live.driver.set_user_blocked(user_id, true).is_err() {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                    )));
                }
            } else if let Some(chat_id) = self.listed_private_chat(user_id)
                && let Some(chat) = self
                    .demo_session
                    .as_mut()
                    .and_then(|session| session.chats.get_mut(&chat_id.0))
            {
                chat.blocked = true;
            }
            cx.notify();
            return;
        }
        match self.listed_private_chat(user_id) {
            Some(chat_id) => {
                self.unblock_peer(chat_id, true, cx);
                self.select_listed_chat(chat_id, window, cx);
            }
            None => {
                if let Some(live) = self.live.as_mut()
                    && live.driver.set_user_blocked(user_id, false).is_err()
                {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_MEDIA_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                    )));
                }
                self.open_user_chat(user_id, window, cx);
            }
        }
    }
}
