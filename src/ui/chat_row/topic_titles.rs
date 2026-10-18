//! A forum row's second line (Telegram Desktop's `dialogs_topics_view`): the
//! most recently active topics as one elided line, unread ones in bold, with
//! the unread-topic badge at the right. When the forum's last message sits in
//! an unread topic, that topic is a jump chip that spans the title and the
//! message line (arrow at the end of the message); a click on either part
//! opens that topic and nothing else on the lines is clickable.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::app::QuillApp;
use crate::ui::sticker_playback::PreviewEmoji;
use quill::state::{RowForum, RowTopic};

/// Gap between two titles (`topicsSkip`, 8px at the 12px row font): an en
/// space and a thin space.
const SKIP: &str = "\u{2002}\u{2009}";

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

fn unread_style(fg: Hsla) -> HighlightStyle {
    HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        color: Some(fg),
        ..Default::default()
    }
}

/// One piece of the line: text that ends in an ellipsis when it is the last
/// one, or a topic icon.
enum Piece {
    Text(String, Vec<(std::ops::Range<usize>, HighlightStyle)>),
    Icon(i64),
}

/// The titles as text pieces (unread ones bold) with the icons between.
fn pieces(topics: &[&RowTopic], line: &ForumLine, unread_fg: Hsla) -> Vec<Piece> {
    let has_picture =
        |id: i64| line.emoji.layered.contains_key(&id) || line.emoji.still.contains_key(&id);
    let mut out = Vec::new();
    let mut text = String::new();
    let mut runs = Vec::new();
    for (i, topic) in topics.iter().enumerate() {
        if i > 0 {
            text.push_str(SKIP);
        }
        if topic.icon > 0 && has_picture(topic.icon) {
            if !text.is_empty() {
                out.push(Piece::Text(
                    std::mem::take(&mut text),
                    std::mem::take(&mut runs),
                ));
            }
            out.push(Piece::Icon(topic.icon));
            text.push(' ');
        }
        let start = text.len();
        text.push_str(&topic.name);
        if topic.unread {
            runs.push((start..text.len(), unread_style(unread_fg)));
        }
    }
    if !text.is_empty() {
        out.push(Piece::Text(text, runs));
    }
    out
}

fn icon_element(id: i64, key: &str, emoji: &PreviewEmoji) -> Option<AnyElement> {
    if let Some(clip) = emoji.layered.get(&id) {
        let frames = crate::ui::anim_layer::frames(clip.clone(), 30);
        return Some(
            div()
                .flex_none()
                .child(frames.size(px(14.)).flex_none())
                .into_any_element(),
        );
    }
    let source = emoji.still.get(&id)?;
    Some(
        div()
            .flex_none()
            .child(
                img(source.clone())
                    .id(SharedString::from(format!("topic-icon-{key}-{id}")))
                    .size(px(14.))
                    .aspect_square()
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
    )
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
    let parts = pieces(&rest, line, unread_fg);
    let last_text = parts.iter().rposition(|p| matches!(p, Piece::Text(..)));
    let titles = div()
        .id(("forum-titles", chat_id.0 as u64))
        .test_support()
        .min_w_0()
        .flex_1()
        .overflow_hidden()
        .flex()
        .items_center()
        .children(
            parts
                .into_iter()
                .enumerate()
                .filter_map(|(i, part)| match part {
                    Piece::Text(text, runs) => {
                        let el = div()
                            .whitespace_nowrap()
                            .child(StyledText::new(text).with_highlights(runs));
                        Some(if Some(i) == last_text {
                            el.min_w_0().flex_shrink(1.).truncate().into_any_element()
                        } else {
                            el.flex_none().into_any_element()
                        })
                    }
                    Piece::Icon(id) => icon_element(id, &key, &line.emoji),
                }),
        );
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
        if event.modifiers().secondary() {
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
    let icon = (topic.icon > 0)
        .then(|| icon_element(topic.icon, &chat_id.0.to_string(), &line.emoji))
        .flatten();
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

#[cfg(test)]
mod tests {
    use super::{ForumLine, Piece, pieces};
    use crate::ui::sticker_playback::PreviewEmoji;
    use gpui_kit::{ImageSource, rgb};
    use quill::state::{RowForum, RowTopic};

    fn topic(id: i32, name: &str, unread: bool, icon: i64) -> RowTopic {
        RowTopic {
            id,
            name: name.into(),
            unread,
            icon,
        }
    }

    #[test]
    fn a_title_with_a_loaded_icon_gets_the_picture_first_and_the_others_stay_one_text() {
        let mut emoji = PreviewEmoji::default();
        emoji
            .still
            .insert(7, ImageSource::from(std::path::PathBuf::from("icon.png")));
        let line = ForumLine {
            forum: RowForum {
                topics: Vec::new(),
                loaded: true,
                unread_topics: 0,
                dot: false,
                badge_muted: false,
                jump: None,
            },
            emoji,
        };
        let (a, b, c, d) = (
            topic(1, "Design", false, 7),
            topic(2, "Backend", true, 0),
            topic(3, "Frontend", false, 0),
            topic(4, "Lost", false, 99),
        );
        let got = pieces(&[&a, &b, &c, &d], &line, rgb(0).into());
        // Icon, then one text for the rest; an icon whose picture is not
        // loaded is left out.
        assert_eq!(got.len(), 2);
        assert!(matches!(got.first(), Some(Piece::Icon(7))));
        let Some(Piece::Text(text, runs)) = got.get(1) else {
            panic!("a text piece");
        };
        assert!(text.starts_with(" Design"));
        assert!(text.contains("Backend") && text.ends_with("Lost"));
        assert_eq!(runs.len(), 1);
        let range = runs.first().map(|run| run.0.clone()).unwrap_or_default();
        assert_eq!(text.get(range), Some("Backend"));
    }
}
