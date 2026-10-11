//! Bot verification marks: the verifying bot's custom emoji before a name
//! (Telegram Desktop `PeerBadge::drawVerified`, scaled to 0.88 of the emoji
//! size) and the description line on a verified profile
//! (`DetailsFiller::addBotVerify`). Facts and rules: `quill::bot_verification`.

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::ChatSummary;
use std::path::PathBuf;

/// `emojiSize` (18) times `kBotVerifiedScale` (0.88), rounded.
const ICON_PX: f32 = 16.;

/// The verifier's emoji as a fixed-size image.
pub(super) fn bot_verify_icon(path: PathBuf) -> AnyElement {
    img(path)
        .id("bot-verify-icon")
        .size(px(ICON_PX))
        .aspect_square()
        .flex_none()
        .object_fit(ObjectFit::Contain)
        .into_any_element()
}

impl QuillApp {
    /// The icon file for a chat's verifying bot, once the emoji has
    /// downloaded; nothing before that (the title just starts earlier).
    pub(super) fn bot_verify_icon_path(&self, chat: &ChatSummary) -> Option<PathBuf> {
        let id = self.session()?.chat_bot_verification_icon(chat)?;
        self.custom_emoji_still(id)
    }

    /// The same for a user shown in a profile.
    pub(super) fn user_bot_verify_icon_path(&self, user_id: i64) -> Option<PathBuf> {
        let id = self.session()?.user_bot_verification_icon(user_id)?;
        self.custom_emoji_still(id)
    }

    /// The verification description under a profile's details, as muted
    /// centered text with the bot's icon in front.
    pub(super) fn bot_verification_row(
        &self,
        text: &str,
        icon_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon = (icon_id != 0)
            .then(|| self.custom_emoji_still(icon_id))
            .flatten();
        div()
            .id("profile-bot-verification")
            .flex()
            .items_start()
            .gap_2()
            .px_2()
            .py_1p5()
            .w_full()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .when_some(icon, |this, path| this.child(bot_verify_icon(path)))
            .child(div().min_w_0().child(text.to_string()))
            .into_any_element()
    }
}
