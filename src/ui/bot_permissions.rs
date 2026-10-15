//! The "Permissions" section of a bot's profile (tdesktop
//! `DetailsFiller::makeBotPermissions`): the "Emoji Status" switch, shown for
//! a bot the person let set their emoji status, to take that access back (or
//! give it again). A mini app asks for it with
//! `web_app_request_emoji_status_access` (`web_app_ui/emoji_status.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The section, or `None` when the bot has no access to show. Like
    /// tdesktop, the section stays once shown (the profile is not rebuilt
    /// when the switch goes off), so the access can be given again.
    pub(super) fn bot_permissions_section(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let allowed = self
            .session()?
            .bots
            .bot_info
            .get(&user_id)
            .and_then(|info| info.as_ref())
            .map(|info| info.can_manage_emoji_status)?;
        if !allowed && !self.mini_apps.emoji_access_shown.contains(&user_id) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        Some(
            div()
                .id("info-panel-bot-permissions")
                .flex()
                .flex_col()
                .w_full()
                .gap_0p5()
                .child(
                    div()
                        .px_2()
                        .pt_1()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::MANAGE_PEER_PERMISSIONS)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .px_2()
                        .py_1p5()
                        .child(
                            div()
                                .text_sm()
                                .child(tr(keys::PROFILE_BOT_EMOJI_STATUS_ACCESS)),
                        )
                        .child(
                            Switch::new("info-panel-bot-emoji-status")
                                .checked(allowed)
                                .accessibility_label(tr(keys::Q_MEDIA_LET_THIS_BOT_SET_YOUR))
                                .on_click(cx.listener(move |this, &next: &bool, _, cx| {
                                    this.set_bot_emoji_status_access(user_id, next, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The switch: remember the section is on screen, then ask TDLib.
    pub(super) fn set_bot_emoji_status_access(
        &mut self,
        user_id: i64,
        allowed: bool,
        cx: &mut Context<Self>,
    ) {
        self.mini_apps.emoji_access_shown.insert(user_id);
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .set_bot_emoji_status_access(user_id, allowed)
                .is_err()
            {
                self.set_status_note(tr(keys::Q_MEDIA_COULDNT_CHANGE_THE_EMOJI_STATUS), cx);
            }
        } else if let Some(Some(info)) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.bots.bot_info.get_mut(&user_id))
        {
            info.can_manage_emoji_status = allowed;
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "bot_permissions_tests.rs"]
mod tests;
