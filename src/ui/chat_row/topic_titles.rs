//! A forum row's second line (Telegram Desktop's `dialogs_topics_view`): the
//! most recently active topics as one elided line, unread ones in bold, with
//! the unread-topic badge at the right. When the forum's last message sits in
//! an unread topic, that topic is a jump chip that opens it; nothing else on
//! the line is clickable.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::app::QuillApp;
use crate::ui::nested_click::SwallowPress;
use quill::state::{RowForum, RowTopic};

/// Gap between two titles (`topicsSkip`, 8px at the 12px row font): an en
/// space and a three-per-em space.
const SKIP: &str = "\u{2002}\u{2004}";

/// The titles as one text and the runs that style the unread ones.
fn titles_text(topics: &[RowTopic], unread_fg: Hsla) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut runs = Vec::new();
    for (i, topic) in topics.iter().enumerate() {
        if i > 0 {
            text.push_str(SKIP);
        }
        let start = text.len();
        text.push_str(&topic.name);
        if topic.unread {
            runs.push((start..text.len(), unread_style(unread_fg)));
        }
    }
    (text, runs)
}

fn unread_style(fg: Hsla) -> HighlightStyle {
    HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        color: Some(fg),
        ..Default::default()
    }
}

/// The topics line: titles (or "Loading…" / "No chats") and the badges.
pub(super) fn forum_topics_line(
    chat_id: ChatId,
    forum: &RowForum,
    selected: bool,
    selecting: bool,
    muted_fg: Hsla,
    badges: Option<AnyElement>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement + use<> {
    let unread_fg = cx.theme().foreground;
    // The chip is not drawn on the open row (`!context.active`).
    let jump = forum
        .jump
        .filter(|_| !selected)
        .and_then(|id| forum.topics.iter().find(|t| t.id == id));
    let rest: Vec<RowTopic> = forum
        .topics
        .iter()
        .filter(|t| jump.is_none_or(|j| j.id != t.id))
        .cloned()
        .collect();
    let line = div()
        .min_w_0()
        .flex_1()
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(muted_fg)
        .when(forum.topics.is_empty(), |this| {
            let key = if forum.loaded {
                keys::FILTERS_NO_CHATS
            } else {
                keys::CONTACTS_LOADING
            };
            this.child(div().truncate().child(tr(key)))
        })
        .when_some(jump, |this, topic| {
            this.child(jump_chip(chat_id, topic, selecting, unread_fg, muted_fg, cx))
        })
        .when(!rest.is_empty(), |this| {
            let (text, runs) = titles_text(&rest, unread_fg);
            this.child(
                div()
                    .id(("forum-titles", chat_id.0 as u64))
                    .test_support()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(StyledText::new(text).with_highlights(runs)),
            )
        });
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(line)
        .children(badges)
}

/// `FillJumpToLastBg`: the unread topic of the forum's last message on a
/// rounded fill, with the jump arrow; a click opens that topic.
fn jump_chip(
    chat_id: ChatId,
    topic: &RowTopic,
    selecting: bool,
    unread_fg: Hsla,
    muted_fg: Hsla,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement + use<> {
    let topic_id = topic.id;
    div()
        .id(("forum-jump", chat_id.0 as u64))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .max_w(px(160.))
        .px_2()
        .rounded(px(11.))
        .bg(muted_fg.opacity(0.16))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(unread_fg)
                .child(topic.name.clone()),
        )
        .child(Icon::new(IconName::ChevronRight).size(px(12.)).text_color(muted_fg))
        // While selecting the chip is plain: the row toggles the check.
        .when(!selecting, |this| {
            this.cursor_pointer()
                .swallow_press()
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    if event.modifiers().secondary() {
                        this.open_chat_in_new_window(chat_id, window, cx);
                    } else {
                        this.select_listed_chat(chat_id, window, cx);
                        this.select_topic_ui(topic_id, cx);
                    }
                }))
        })
}
