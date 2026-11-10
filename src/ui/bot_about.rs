//! "What can this bot do?": the bot's description shown in an empty bot
//! chat, as tdesktop's `AboutView::makeAboutBot` does.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::StyledExt;
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
        &mut self,
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
        let picture = info
            .description_photo
            .iter()
            .rev()
            .find(|size| size.width <= PHOTO_MAX_SIZE_WIDTH)
            .or(info.description_photo.last())
            .map(|size| (size.file_id, (size.width, size.height)));
        let picture_path = picture.and_then(|(file_id, _)| {
            let roots = self.media_display_roots();
            let file = session.media.files.get(&file_id.0)?.usable_path()?;
            quill::local_path::sandboxed_display_path(file, &roots).map(|path| path.to_path_buf())
        });
        let bubble = super::chat_palette::with_palette(self.history.palette.get(), false, || {
            super::chat_theme::bg_bubble_incoming()
        });
        if let Some((file_id, _)) = picture.filter(|_| picture_path.is_none())
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.ensure_media_files(&[file_id]);
        }
        let picture = picture.map(|(_, dims)| about_picture(picture_path.as_deref(), dims, cx));
        // tdesktop draws it as an incoming message bubble in the middle of
        // the empty history (`AboutView`), the description photo on top.
        Some(
            div()
                .id("bot-about")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .items_center()
                // Centered while it fits; a card taller than the room
                // scrolls from its top instead of being cut at both ends.
                .overflow_y_scroll()
                .p_6()
                .child(
                    div()
                        .my_auto()
                        .flex_none()
                        .max_w(px(ABOUT_MAX_WIDTH))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .rounded(cx.theme().radius_2xl())
                        .bg(bubble)
                        .text_color(cx.theme().foreground)
                        .children(picture)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .px_3()
                                .py_2()
                                .child(div().font_semibold().child(tr(keys::BOT_DESCRIPTION)))
                                .child(body),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// tdesktop `msgMaxWidth`.
const ABOUT_MAX_WIDTH: f32 = 430.;
/// The widest photo size worth loading for a card this wide on a 2x
/// screen.
const PHOTO_MAX_SIZE_WIDTH: i32 = 1000;

/// The description photo across the card's width, at its own aspect
/// (clamped so a tall picture leaves room for the text); a muted
/// placeholder of the same size while it downloads.
fn about_picture(path: Option<&std::path::Path>, (w, h): (i32, i32), cx: &App) -> AnyElement {
    let width = ABOUT_MAX_WIDTH;
    let ratio = if w > 0 && h > 0 {
        (h as f32 / w as f32).clamp(0.3, 1.)
    } else {
        0.5
    };
    let frame = (px(width), px(width * ratio));
    let slot = div()
        .w(frame.0)
        .h(frame.1)
        .rounded_t(cx.theme().radius_2xl())
        .bg(cx.theme().muted);
    match path {
        Some(path) => slot
            .child(
                img(super::image_budget::sized_media(
                    path,
                    frame,
                    Some((w, h)),
                    super::image_budget::Fit::Cover,
                ))
                .size_full()
                .rounded_t(cx.theme().radius_2xl())
                .object_fit(ObjectFit::Cover),
            )
            .into_any_element(),
        None => slot.into_any_element(),
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
