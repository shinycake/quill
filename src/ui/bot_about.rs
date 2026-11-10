//! "What can this bot do?": the bot's description shown in an empty bot
//! chat, as tdesktop's `AboutView::makeAboutBot` does.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::StyledExt;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use quill::lang::{keys, tr};
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
        let info = session.bot_info_for_chat(chat.id)?;
        let text = bot_about_text(info)?;
        let body = crate::ui::message_text::rich_text_line(
            text,
            &info.description_entities,
            (chat.id.0, 0),
            false,
            &self.message_ui.spoiler_revealed,
            self.msg_font(),
            &std::collections::HashMap::new(),
            cx,
        );
        let bubble = super::chat_palette::with_palette(self.history.palette.get(), false, || {
            super::chat_theme::bg_bubble_incoming()
        });
        // tdesktop draws it as an incoming message bubble in the middle of
        // the empty history (`AboutView`).
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
                        .max_w(px(ABOUT_MAX_WIDTH))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .px_3()
                        .py_2()
                        .rounded(cx.theme().radius_2xl())
                        .bg(bubble)
                        .text_color(cx.theme().foreground)
                        .child(div().font_semibold().child(tr(keys::BOT_DESCRIPTION)))
                        .child(body),
                )
                .into_any_element(),
        )
    }
}

/// tdesktop `msgMaxWidth`.
const ABOUT_MAX_WIDTH: f32 = 430.;

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
