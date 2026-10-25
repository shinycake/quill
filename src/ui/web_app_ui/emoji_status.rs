//! A mini app and the person's emoji status, as tdesktop answers it
//! (`WebViewInstance::botRequestEmojiStatusAccess` and `botSetEmojiStatus`,
//! `ConfirmEmojiStatusAccessBox`, `ConfirmEmojiStatusBox`):
//!
//! - `web_app_request_emoji_status_access`: allowed at once when the bot
//!   already has the access; otherwise a prompt, and "Allow" (Premium only)
//!   calls `toggleBotCanManageEmojiStatus`.
//! - `web_app_set_emoji_status`: the suggested custom emoji is looked up, must
//!   be an emoji-set sticker, and "Confirm" (Premium only) sets the status,
//!   with an expiry when the app gave a duration.
//!
//! Without Premium, "Allow" and "Confirm" open the Premium page and the prompt
//! comes back, as tdesktop's box stays open behind its Premium preview.
//! The prompts are the web view helper's native popups, which carry text
//! only: the suggested emoji shows as its character beside the sentence, where
//! tdesktop paints the custom emoji.

use super::*;
use quill::lang::{keys, tr, tr_with};
use std::time::{Duration as StdDuration, Instant};

/// How long the suggested emoji may take to resolve before the request fails
/// with `SUGGESTED_EMOJI_INVALID`.
const RESOLVE_TIMEOUT: StdDuration = StdDuration::from_secs(15);

/// The emoji status prompts of the open mini app.
#[derive(Default)]
pub(super) struct EmojiStatusFlow {
    /// "Allow" was pressed and `toggleBotCanManageEmojiStatus` is in flight.
    granting: bool,
    /// The access prompt is up.
    asking: bool,
    set: Option<SetRequest>,
}

impl EmojiStatusFlow {
    #[cfg(test)]
    #[cfg(all(feature = "demo-capture", unix))]
    fn set_is_idle(&self) -> bool {
        self.set.is_none()
    }
}

struct SetRequest {
    custom_emoji_id: i64,
    duration_secs: i32,
    since: Instant,
    stage: SetStage,
}

enum SetStage {
    /// Looking the emoji up; `sent` once `getCustomEmojiStickers` is on its way.
    Resolving { sent: bool },
    /// The prompt is up, with the emoji's character.
    Confirming { alt: String },
}

fn access_popup(bot_name: &str) -> Popup {
    popup(
        tr(keys::PROFILE_BOT_EMOJI_STATUS_ACCESS),
        tr_with(
            keys::Q_MEDIA_REQUESTS_ACCESS_TO_SET_YOUR,
            &[("bot_name", bot_name), ("bot_name_2", bot_name)],
        ),
        vec![
            ("cancel", tr(keys::CANCEL), "cancel"),
            ("allow", tr(keys::BOT_EMOJI_STATUS_ACCESS_ALLOW), "default"),
        ],
    )
}

fn confirm_popup(bot_name: &str, alt: &str) -> Popup {
    let mut message = tr_with(keys::BOT_EMOJI_STATUS_TEXT, &[("bot", bot_name)]);
    if !alt.is_empty() {
        message.push_str("\n\n");
        message.push_str(alt);
    }
    popup(
        tr(keys::BOT_EMOJI_STATUS_TITLE),
        message,
        vec![
            ("cancel", tr(keys::CANCEL), "cancel"),
            ("confirm", tr(keys::BOT_EMOJI_STATUS_CONFIRM), "default"),
        ],
    )
}

impl QuillApp {
    fn bot_has_emoji_status_access(&self, bot_id: i64) -> bool {
        self.session()
            .and_then(|session| session.bots.bot_info.get(&bot_id))
            .and_then(|info| info.as_ref())
            .is_some_and(|info| info.can_manage_emoji_status)
    }

    fn premium_for_emoji_status(&self) -> bool {
        self.session()
            .is_some_and(|session| session.my_is_premium())
    }

    /// `web_app_request_emoji_status_access`.
    pub(super) fn request_emoji_status_access(&mut self, cx: &mut Context<Self>) {
        let Some((bot_id, bot_name, busy)) = self.mini_apps.window.as_ref().map(|window| {
            (
                window.launch.bot_user_id,
                window.launch.bot_name.clone(),
                window.emoji.asking || window.emoji.granting,
            )
        }) else {
            return;
        };
        if busy {
            self.with_window(|window| {
                window.emit(BridgeReply::emoji_status_access_requested(false));
            });
            return;
        }
        if self.bot_has_emoji_status_access(bot_id) {
            self.with_window(|window| {
                window.emit(BridgeReply::emoji_status_access_requested(true));
            });
            return;
        }
        self.with_window(|window| {
            window.emoji.asking = true;
            window.show_popup(PopupPurpose::EmojiStatusAccess, access_popup(&bot_name));
        });
        cx.notify();
    }

    /// The access prompt was answered.
    pub(super) fn answer_emoji_status_access(&mut self, button: &str, cx: &mut Context<Self>) {
        let Some((bot_id, bot_name)) = self
            .mini_apps
            .window
            .as_ref()
            .map(|window| (window.launch.bot_user_id, window.launch.bot_name.clone()))
        else {
            return;
        };
        if button != "allow" {
            self.with_window(|window| {
                window.emoji.asking = false;
                window.emit(BridgeReply::emoji_status_access_requested(false));
            });
            return;
        }
        if !self.premium_for_emoji_status() {
            // tdesktop shows the Premium preview and leaves the box open.
            self.open_premium_at(quill::premium_hub::PremiumFeature::EmojiStatus, cx);
            self.with_window(|window| {
                window.show_popup(PopupPurpose::EmojiStatusAccess, access_popup(&bot_name));
            });
            return;
        }
        self.with_window(|window| {
            window.emoji.asking = false;
            window.emoji.granting = true;
        });
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.set_bot_emoji_status_access(bot_id, true));
        if !matches!(sent, Some(Ok(_))) {
            self.with_window(|window| {
                window.emoji.granting = false;
                window.emit(BridgeReply::emoji_status_access_requested(false));
            });
        }
    }

    /// `web_app_set_emoji_status` with an id and duration the bridge checked.
    pub(super) fn request_set_emoji_status(
        &mut self,
        custom_emoji_id: i64,
        duration_secs: i32,
        cx: &mut Context<Self>,
    ) {
        let busy = self
            .mini_apps
            .window
            .as_ref()
            .is_none_or(|window| window.emoji.set.is_some());
        if busy {
            self.with_window(|window| {
                window.emit(BridgeReply::emoji_status_failed("UNKNOWN_ERROR"));
            });
            return;
        }
        self.with_window(|window| {
            window.emoji.set = Some(SetRequest {
                custom_emoji_id,
                duration_secs,
                since: Instant::now(),
                stage: SetStage::Resolving { sent: false },
            });
        });
        self.poll_emoji_status(cx);
    }

    /// The confirmation prompt was answered.
    pub(super) fn answer_set_emoji_status(&mut self, button: &str, cx: &mut Context<Self>) {
        let Some(bot_name) = self
            .mini_apps
            .window
            .as_ref()
            .map(|window| window.launch.bot_name.clone())
        else {
            return;
        };
        let Some(request) = self
            .mini_apps
            .window
            .as_mut()
            .and_then(|window| window.emoji.set.take())
        else {
            return;
        };
        let SetStage::Confirming { alt } = &request.stage else {
            return;
        };
        if button != "confirm" {
            self.with_window(|window| {
                window.emit(BridgeReply::emoji_status_failed("USER_DECLINED"));
            });
            return;
        }
        if !self.premium_for_emoji_status() {
            // The prompt stays, behind the Premium preview.
            self.open_premium_at(quill::premium_hub::PremiumFeature::EmojiStatus, cx);
            let popup = confirm_popup(&bot_name, alt);
            self.with_window(|window| {
                window.show_popup(PopupPurpose::EmojiStatus, popup);
                window.emoji.set = Some(request);
            });
            return;
        }
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .change_emoji_status(Some(request.custom_emoji_id), request.duration_secs)
        });
        // tdesktop answers as soon as it has asked; the status itself arrives
        // with the next user update.
        self.with_window(|window| {
            window.emit(match sent {
                Some(Ok(_)) => BridgeReply::emoji_status_set(),
                _ => BridgeReply::emoji_status_failed("UNKNOWN_ERROR"),
            });
        });
    }

    /// One tick of the emoji status flows: the grant's answer, and the
    /// suggested emoji while it resolves.
    pub(super) fn poll_emoji_status(&mut self, cx: &mut Context<Self>) {
        let access = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.bots.web_apps.emoji_access.take());
        if let Some((bot_id, done)) = access {
            let waiting =
                self.mini_apps.window.as_ref().is_some_and(|window| {
                    window.launch.bot_user_id == bot_id && window.emoji.granting
                });
            if waiting {
                self.with_window(|window| {
                    window.emoji.granting = false;
                    window.emit(BridgeReply::emoji_status_access_requested(done));
                });
            }
        }
        self.resolve_suggested_emoji(cx);
    }

    fn resolve_suggested_emoji(&mut self, cx: &mut Context<Self>) {
        let Some((id, sent, since)) = self.mini_apps.window.as_ref().and_then(|window| {
            let request = window.emoji.set.as_ref()?;
            match request.stage {
                SetStage::Resolving { sent } => {
                    Some((request.custom_emoji_id, sent, request.since))
                }
                SetStage::Confirming { .. } => None,
            }
        }) else {
            return;
        };
        let found = self.session().and_then(|session| {
            session
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(id))
                .map(|item| item.emoji.clone())
        });
        if let Some(alt) = found {
            let bot_name = self
                .mini_apps
                .window
                .as_ref()
                .map(|window| window.launch.bot_name.clone())
                .unwrap_or_default();
            self.with_window(|window| {
                if let Some(request) = window.emoji.set.as_mut() {
                    let prompt = confirm_popup(&bot_name, &alt);
                    request.stage = SetStage::Confirming { alt };
                    window.show_popup(PopupPurpose::EmojiStatus, prompt);
                }
            });
            cx.notify();
            return;
        }
        let in_flight = self.session().is_some_and(|session| {
            session
                .requests
                .has_purpose(quill::state::RequestPurpose::GetCustomEmojiStickers)
        });
        let mut failed = since.elapsed() > RESOLVE_TIMEOUT;
        if !failed && !sent {
            // The shared lookup may be busy with another batch: try again
            // on the next tick.
            match self
                .live
                .as_mut()
                .map(|live| live.driver.resolve_custom_emoji(id))
            {
                Some(Ok(Some(_))) => self.with_window(|window| {
                    if let Some(request) = window.emoji.set.as_mut() {
                        request.stage = SetStage::Resolving { sent: true };
                    }
                }),
                Some(Ok(None)) => {}
                _ => failed = true,
            }
        } else if !failed && sent && !in_flight {
            // Answered, and the emoji is not an emoji-set sticker (or does
            // not exist): tdesktop's `SUGGESTED_EMOJI_INVALID`.
            failed = true;
        }
        if failed {
            self.with_window(|window| {
                window.emoji.set = None;
                window.emit(BridgeReply::emoji_status_failed("SUGGESTED_EMOJI_INVALID"));
            });
        }
    }
}

#[cfg(test)]
#[cfg(all(feature = "demo-capture", unix))]
#[path = "emoji_status_tests.rs"]
mod tests;
