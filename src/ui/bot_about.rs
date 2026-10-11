//! "What can this bot do?": the bot's description shown in an empty bot
//! chat, as tdesktop's `AboutView::makeAboutBot` does.

use super::app::QuillApp;
use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::*;
use quill::telegram::envelope::BotInfo;

/// The text the intro shows, or `None` when the bot has no description.
pub(super) fn bot_about_text(info: &BotInfo) -> Option<&str> {
    let text = info.description.trim();
    (!text.is_empty()).then_some(text)
}

impl QuillApp {
    /// The intro card for an empty bot chat, when the bot has a description.
    pub(super) fn bot_about_intro(
        &self,
        chat: Option<&quill::state::ChatSummary>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let chat = chat?;
        let session = self.session()?;
        let text = bot_about_text(session.bot_info_for_chat(chat.id)?)?.to_string();
        Some(
            div()
                .id("bot-about")
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .max_w(px(430.))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .bg(cx.theme().muted)
                        .child(div().font_semibold().child("What can this bot do?"))
                        .child(div().text_sm().child(text)),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::bot_about_text;
    use quill::telegram::envelope::BotInfo;

    #[test]
    fn shows_only_a_non_empty_description() {
        let mut info = BotInfo::default();
        assert_eq!(bot_about_text(&info), None);
        info.description = "  \n ".into();
        assert_eq!(bot_about_text(&info), None);
        info.description = " I fetch the weather. ".into();
        assert_eq!(bot_about_text(&info), Some("I fetch the weather."));
    }
}
