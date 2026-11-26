//! One row of a forum's topic list, drawn from the chat-list row's pieces
//! (Telegram Desktop draws topics with the dialogs row painter): the topic
//! icon at the chat photo's size, the name and the time, then the sender
//! and the last message's preview with the row's counters at the trailing
//! edge. The density's metrics size it, so Compact shrinks it as it does
//! chat rows.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::chat_row::row_badges::row_badges;
use crate::ui::chat_row::{
    chat_row_height, draft_prefix, row_glyph, row_text_lines, selected_secondary_fg,
    tinted_open_row_hover_fill,
};
use crate::ui::chatlist_style::chat_list_preview_line_layered;
use crate::ui::pressable::PressableDiv;
use quill::lang::{keys, tr_with};
use quill::state::TopicBadge;

impl QuillApp {
    /// The row of `topic` in the topic list of the forum `chat_id`.
    pub(in crate::ui) fn forum_topic_row(
        &self,
        chat_id: ChatId,
        topic: &ForumTopic,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let m = self.density();
        let topic_id = topic.forum_topic_id;
        let muted = session.is_some_and(|s| s.topic_is_muted(chat_id, topic));
        let badge = session.map_or(TopicBadge::None, |s| s.topic_badge(chat_id, topic));
        let has_mentions = topic.unread_mention_count > 0;
        let has_reactions = topic.unread_reaction_count > 0;
        let muted_fg = if selected {
            selected_secondary_fg(cx.theme().foreground)
        } else {
            cx.theme().muted_foreground
        };
        let name = if topic.name.is_empty() {
            tr_with(
                keys::Q_PEOPLE_TOPIC,
                &[("forum_topic_id", &topic_id.to_string())],
            )
        } else {
            topic.name.clone()
        };
        // A draft replaces the last message, with a red "Draft:" lead, as
        // on a chat row.
        let draft = topic.draft_preview();
        let last = topic.last_message.as_ref().filter(|_| draft.is_none());
        let (preview, draft_reply) = match draft {
            Some(draft) => (draft.text, Some(draft.reply)),
            None => (topic.last_message_preview.clone(), None),
        };
        let sender = last.and_then(|last| session?.topic_preview_sender(last));
        let style = last.map(|last| &last.style);
        let (emoji, icon) = self.topic_row_emoji(topic, m.avatar, cx);
        let stamp = topic
            .last_message
            .as_ref()
            .and_then(|last| quill::local_time::list_stamp(last.date));
        let badges = row_badges(
            ("forum-topic-unread", topic_id as u64).into(),
            topic.is_pinned,
            crate::ui::chat_row::BadgeInk::Own,
            muted_fg,
            muted,
            unread_indicator(badge, has_mentions),
            has_reactions,
            has_mentions,
            1.,
            cx,
        );
        let label = if draft_reply.is_some() {
            tr_with(
                keys::Q_CHATLIST_DRAFT,
                &[("title", &name), ("preview", &preview)],
            )
        } else {
            format!("{name} — {preview}")
        };
        // A sender before a right-to-left preview is one line with it, as
        // on a chat row (`chat_list_preview_line_layered`).
        let combine_prefix = sender.is_some() && quill::text::has_rtl_text(&preview);
        let menu = self.forum_topic_menu(chat_id, topic, cx);
        div()
            .id(("forum-topic-row", topic_id as u64))
            .w_full()
            .h(chat_row_height(false, 2, m))
            .px(px(m.row_inset))
            .flex()
            .flex_col()
            .justify_center()
            .rounded_md()
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .aria_selected(selected)
            .tab_index(0)
            .when(!selected, |this| this.pressable(cx.theme()))
            .test_support()
            .when(selected, |this| {
                this.hover(|s| s.bg(tinted_open_row_hover_fill(cx.theme())))
                    .active(|s| s.bg(cx.theme().primary.opacity(0.4)))
            })
            .bg(if selected {
                cx.theme().primary.opacity(0.25)
            } else {
                cx.theme().sidebar
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_topic_ui(topic_id, cx);
            }))
            .context_menu(menu)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(m.row_gap))
                    .child(super::super::subsection_tabs::topic_icon_or_emoji(
                        topic, m.avatar, icon, cx,
                    ))
                    .child(
                        row_text_lines(m)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .min_w_0()
                                            .flex_1()
                                            .when(topic.is_closed, |this| {
                                                this.child(row_glyph(IconName::Lock, muted_fg))
                                            })
                                            .child(
                                                div()
                                                    .id(("forum-topic-name", topic_id as u64))
                                                    .test_support()
                                                    .font_semibold()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(
                                                        crate::ui::emoji_sprites::one_line(
                                                            &name,
                                                            px(14.),
                                                        )
                                                        .unwrap_or_else(|| {
                                                            crate::ui::bidi_line::one_line_plain(
                                                                name,
                                                            )
                                                            .into_any_element()
                                                        }),
                                                    ),
                                            )
                                            .when(muted, |this| {
                                                this.child(row_glyph(IconName::BellOff, muted_fg))
                                            }),
                                    )
                                    .when_some(stamp, |this, stamp| {
                                        this.child(
                                            div()
                                                .id(("forum-topic-stamp", topic_id as u64))
                                                .test_support()
                                                .flex_none()
                                                .text_xs()
                                                .text_color(muted_fg)
                                                .child(stamp),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(("forum-topic-preview", topic_id as u64))
                                            .test_support()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .when_some(draft_reply, |this, reply| {
                                                this.child(draft_prefix(reply, preview.is_empty()))
                                            })
                                            .when_some(
                                                sender.clone().filter(|_| !combine_prefix),
                                                |this, sender| {
                                                    this.child(
                                                        div()
                                                            .flex_none()
                                                            .text_xs()
                                                            .text_color(if selected {
                                                                muted_fg
                                                            } else {
                                                                accent().into()
                                                            })
                                                            .child(format!("{sender}: ")),
                                                    )
                                                },
                                            )
                                            .child(
                                                div().min_w_0().flex_1().child(
                                                    chat_list_preview_line_layered(
                                                        style.and_then(|s| s.icon),
                                                        &preview,
                                                        style.map_or(&[], |s| &s.entities),
                                                        &emoji.still,
                                                        &emoji.layered,
                                                        sender
                                                            .filter(|_| combine_prefix)
                                                            .map(|sender| format!("{sender}: ")),
                                                        muted_fg,
                                                    ),
                                                ),
                                            ),
                                    )
                                    .child(badges),
                            ),
                    ),
            )
            .into_any_element()
    }
}

/// `(count, is_dot)` for the topic's unread pill, by the chat rows' rule:
/// with mentions and a single unread message the @ mark carries it.
fn unread_indicator(badge: TopicBadge, has_mentions: bool) -> Option<(i32, bool)> {
    match badge {
        TopicBadge::Count(1) if has_mentions => None,
        TopicBadge::Count(count) => Some((count, false)),
        TopicBadge::Dot => Some((0, true)),
        TopicBadge::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::unread_indicator;
    use quill::state::TopicBadge;

    #[test]
    fn a_single_mention_is_carried_by_the_mention_mark() {
        assert_eq!(unread_indicator(TopicBadge::Count(1), true), None);
        assert_eq!(
            unread_indicator(TopicBadge::Count(2), true),
            Some((2, false))
        );
        assert_eq!(
            unread_indicator(TopicBadge::Count(1), false),
            Some((1, false))
        );
        assert_eq!(unread_indicator(TopicBadge::Dot, false), Some((0, true)));
        assert_eq!(unread_indicator(TopicBadge::None, true), None);
    }
}
