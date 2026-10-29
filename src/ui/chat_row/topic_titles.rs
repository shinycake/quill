//! A forum row's second line (Telegram Desktop's `dialogs_topics_view`): the
//! most recently active topics as one elided line, unread ones in bold, with
//! the unread-topic badge at the right. When the forum's last message sits in
//! an unread topic, that topic is a jump chip that spans the title and the
//! message line (arrow at the end of the message); a click on either part
//! opens that topic and nothing else on the lines is clickable.
#![deny(clippy::indexing_slicing)]

use super::topic_fit::{FittedTitles, Picture, Title};
use super::*;
use crate::ui::app::QuillApp;
use crate::ui::sticker_player::PreviewEmoji;
use quill::state::{RowForum, RowTopic};

/// A forum row's data: the state's view of it and the images of the topic
/// icons (custom emoji).
pub(in crate::ui) struct ForumLine {
    pub(super) forum: RowForum,
    pub(super) emoji: PreviewEmoji,
}

impl ForumLine {
    /// The topic the jump chip opens, unless the row is the open one (the
    /// chip is not drawn there).
    pub(super) fn jump(&self, selected: bool) -> Option<i32> {
        self.forum.jump.filter(|_| !selected)
    }
}

/// The titles of the line, each with its icon when the picture is ready.
fn titles(topics: &[&RowTopic], line: &ForumLine, unread_fg: Hsla) -> Vec<Title> {
    topics
        .iter()
        .map(|topic| Title {
            icon: picture(topic.icon, &line.emoji).map(|picture| (topic.icon, picture)),
            name: topic.name.clone(),
            unread: topic.unread.then_some(unread_fg),
        })
        .collect()
}

/// The picture of a topic icon (0 = none) once it has loaded.
fn picture(id: i64, emoji: &PreviewEmoji) -> Option<Picture> {
    if id <= 0 {
        return None;
    }
    match emoji.layered.get(&id) {
        Some(clip) => Some(Picture::Layered(clip.clone())),
        None => emoji.still.get(&id).map(|s| Picture::Still(s.clone())),
    }
}

/// The topics line: titles (or "Loading…" / "No chats") and the badges.
pub(super) fn forum_topics_line(
    chat_id: ChatId,
    line: &ForumLine,
    selected: bool,
    selecting: bool,
    muted_fg: Hsla,
    badges: Option<AnyElement>,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement + use<> {
    let forum = &line.forum;
    let unread_fg = cx.theme().foreground;
    let jump = line
        .jump(selected)
        .and_then(|id| forum.topics.iter().find(|t| t.id == id));
    let rest: Vec<&RowTopic> = forum
        .topics
        .iter()
        .filter(|t| jump.is_none_or(|j| j.id != t.id))
        .collect();
    let key = chat_id.0.to_string();
    let titles = div()
        .id(("forum-titles", chat_id.0 as u64))
        .test_support()
        .min_w_0()
        .flex_1()
        .overflow_hidden()
        .flex()
        .items_center()
        .child(FittedTitles {
            titles: titles(&rest, line, unread_fg),
            key: key.clone(),
        });
    let content = div()
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
            this.child(title_chip(
                chat_id, topic, line, selecting, unread_fg, muted_fg, cx,
            ))
        })
        .child(titles);
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(content)
        .children(badges)
}

/// The chip fill (`FillJumpToLastBg`).
fn chip_fill(muted_fg: Hsla) -> Hsla {
    muted_fg.opacity(0.16)
}

/// A click on a jump chip: the topic of the forum's last message opens (a new
/// window with Ctrl/Cmd). The press is not swallowed, so pinned drag and the
/// long-press peek work from the chip too; only the click stops here so the
/// row does not open the chat a second time.
fn on_jump(
    chat_id: ChatId,
    topic_id: i32,
    cx: &mut Context<QuillApp>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |this, event: &ClickEvent, window, cx| {
        cx.stop_propagation();
        this.chat_list.preview_press = None;
        if event.modifiers().secondary(cx) {
            this.open_chat_in_new_window(chat_id, window, cx);
        } else {
            this.select_listed_chat(chat_id, window, cx);
            this.select_topic_ui(topic_id, cx);
        }
    })
}

/// The first part of the chip: the unread topic's title.
fn title_chip(
    chat_id: ChatId,
    topic: &RowTopic,
    line: &ForumLine,
    selecting: bool,
    unread_fg: Hsla,
    muted_fg: Hsla,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement + use<> {
    let topic_id = topic.id;
    let icon = picture(topic.icon, &line.emoji)
        .map(|picture| picture.element(&chat_id.0.to_string(), topic.icon));
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
        .bg(chip_fill(muted_fg))
        .children(icon)
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(unread_fg)
                .child(topic.name.clone()),
        )
        // While selecting the chip is plain: the row toggles the check.
        .when(!selecting, |this| {
            this.cursor_pointer()
                .on_click(on_jump(chat_id, topic_id, cx))
        })
}

/// The second part of the chip: the message line on the same fill, the jump
/// arrow at its end.
pub(super) fn jump_message(
    chat_id: ChatId,
    topic_id: i32,
    selecting: bool,
    muted_fg: Hsla,
    message: AnyElement,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement + use<> {
    div()
        .id(("forum-jump-message", chat_id.0 as u64))
        .test_support()
        .flex()
        .flex_initial()
        .min_w_0()
        .items_center()
        .gap_1()
        .px_2()
        .rounded(px(11.))
        .bg(chip_fill(muted_fg))
        .child(message)
        .child(
            Icon::new(IconName::ChevronRight)
                .size(px(12.))
                .text_color(muted_fg),
        )
        .when(!selecting, |this| {
            this.cursor_pointer()
                .on_click(on_jump(chat_id, topic_id, cx))
        })
}
