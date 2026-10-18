//! A forum row's topic titles (Telegram Desktop's `dialogs_topics_view`):
//! the most recently active topics in a line, unread ones in bold, and a
//! click opens that topic.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::app::QuillApp;
use crate::ui::nested_click::SwallowPress;
use quill::ids::ChatId;
use quill::state::RowTopic;

/// The titles as one text line (the sender line of a 3-line row).
pub(super) fn topics_text(topics: &[RowTopic]) -> Option<String> {
    (!topics.is_empty()).then(|| {
        topics
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// The clickable title line of a 2-line row. A title opens its chat on
/// that topic; the press does not reach the row.
pub(super) fn topic_titles_line(
    chat_id: ChatId,
    topics: &[RowTopic],
    muted_fg: Hsla,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement + use<> {
    let unread_fg = cx.theme().foreground;
    let last = topics.len().saturating_sub(1);
    div()
        .min_w_0()
        .flex_1()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_xs()
        .text_color(muted_fg)
        .children(topics.iter().enumerate().map(|(i, topic)| {
            let topic_id = topic.id;
            div()
                .flex()
                .flex_none()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "row-topic-{}-{}",
                            chat_id.0, topic.id
                        )))
                        .test_support()
                        .cursor_pointer()
                        .swallow_press()
                        .hover(|s| s.underline())
                        .when(topic.unread, |this| {
                            this.font_semibold().text_color(unread_fg)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_listed_chat(chat_id, window, cx);
                            this.select_topic_ui(topic_id, cx);
                        }))
                        .child(topic.name.clone()),
                )
                .when(i != last, |this| this.child(div().pr_1().child(",")))
        }))
}
