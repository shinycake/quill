//! Bot and channel action rows of the info panel: Bot Help, Bot Settings,
//! Bot Privacy Policy, Report, Stop and block bot / Restart bot, and Join
//! Channel. Behaviour follows tdesktop's `ActionsFiller`
//! (`info/profile/info_profile_actions.cpp:2993-3209`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::pressable::action_row;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::*;
use quill::bot_profile::{PrivacyTarget, find_command, privacy_target};
use quill::composer::ComposerSnapshot;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind};

impl QuillApp {
    /// The listed private chat with `user_id`, if there is one.
    fn listed_private_chat(&self, user_id: i64) -> Option<ChatId> {
        self.session()?
            .chats
            .values()
            .filter(|chat| chat.supported())
            .find(|chat| {
                matches!(&chat.kind, ChatKind::Private { user_id: peer } if peer.0 == user_id)
            })
            .map(|chat| chat.id)
    }

    /// Bot Help, Bot Settings and Bot Privacy Policy, then Report: the
    /// block `if (CanReportBot(user))` of `fillUserActions`. `None` unless
    /// the user is a bot other than the viewer.
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
                column = column.child(action_row(id, Some(icon), label, false, cx).on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.send_profile_bot_command(user_id, &command, window, cx);
                    }),
                ));
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
            })),
        );
        column = column.child(
            action_row(
                "info-panel-bot-report",
                Some(IconName::Flag),
                tr(keys::PROFILE_REPORT),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                match this.listed_private_chat(user_id) {
                    Some(chat_id) => this.open_message_report(chat_id, Vec::new(), cx),
                    None => this.open_user_chat(user_id, window, cx),
                }
            })),
        );
        Some(column.into_any_element())
    }

    /// Report on a broadcast channel the viewer does not own
    /// (`fillChannelActions`: `if (!channel->amCreator()) addReportAction`).
    pub(in crate::ui) fn channel_report_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        if session.chat_is_owner(chat_id) {
            return None;
        }
        Some(
            action_row(
                "info-panel-channel-report",
                Some(IconName::Flag),
                tr(keys::PROFILE_REPORT),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_report(chat_id, Vec::new(), cx);
            }))
            .into_any_element(),
        )
    }

    /// Join Channel, shown while the viewer is not in the channel
    /// (`addJoinChannelAction`).
    pub(in crate::ui) fn channel_join_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let left = self
            .session()?
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.my_member_status == Some(ChannelMemberStatus::Left));
        left.then(|| {
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
            .into_any_element()
        })
    }

    /// Send `/command` to the bot's private chat and show that chat
    /// (`BotCommandClickHandler` with the bot as context). A bot without a
    /// listed chat opens one first; the command is then tapped again.
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
        let Some(view_generation) = self.session().map(|s| s.view_generation) else {
            return;
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let text = format!("/{command}");
        if let Some(live) = self.live.as_mut() {
            let snapshot = ComposerSnapshot::capture(chat_id, view_generation, &text);
            let note = match live.driver.send_snapshot(&snapshot) {
                Ok(_) => StatusNote::silent(tr(keys::Q_OPS_SENDING)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_SEND_THE_COMMAND)),
            };
            self.connection.set_note(note);
        } else {
            self.apply_demo_outgoing(&text, None, None);
        }
        cx.notify();
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
            PrivacyTarget::Url(url) => self.open_message_url(&url, cx),
            PrivacyTarget::Command(command) => {
                self.send_profile_bot_command(user_id, &command, window, cx)
            }
            PrivacyTarget::Default => self.open_message_url(&tr(keys::PROFILE_BOT_PRIVACY_URL), cx),
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
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.set_user_blocked(user_id, false);
                }
                self.open_user_chat(user_id, window, cx);
            }
        }
    }
}
