//! The history's corner "@" and heart buttons (tdesktop
//! `HistoryView::CornerButtons`, `history_view_corner_buttons.cpp`): round
//! buttons stacked above the scroll-to-bottom arrow, each with an unread
//! count badge. A click jumps to the oldest unread mention / reaction;
//! right-click offers "Mark all as read". The buttons show whenever the
//! chat has unread markers (they do not wait for a scroll), without a
//! fade so nothing keeps the window redrawing.

use super::app::QuillApp;
use super::chat_row::compact_count;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::UnreadJumpKind;

/// tdesktop `historyToDownBadgeSize`.
const BADGE_SIZE: f32 = 22.;

impl QuillApp {
    /// The "@" / heart stack for the open chat, bottom-right of the
    /// history; `None` when nothing is unread.
    pub(super) fn jump_corner_buttons(
        &self,
        mentions: i32,
        reactions: i32,
        poll_votes: i32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if mentions <= 0 && reactions <= 0 && poll_votes <= 0 {
            return None;
        }
        let weak = cx.weak_entity();
        Some(
            div()
                .absolute()
                .right_4()
                .bottom_4()
                .flex()
                .flex_col()
                .items_end()
                .gap_3()
                .when(mentions > 0, |this| {
                    this.child(corner_button(
                        UnreadJumpKind::Mention,
                        mentions,
                        weak.clone(),
                    ))
                })
                .when(reactions > 0, |this| {
                    this.child(corner_button(
                        UnreadJumpKind::Reaction,
                        reactions,
                        weak.clone(),
                    ))
                })
                .when(poll_votes > 0, |this| {
                    this.child(corner_button(UnreadJumpKind::PollVote, poll_votes, weak))
                })
                .into_any_element(),
        )
    }

    pub(super) fn jump_to_unread_marker(&mut self, kind: UnreadJumpKind, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.jump_to_unread_marker(kind) {
                Ok(()) => String::new(),
                Err(_) => "could not find the message".into(),
            };
            cx.notify();
        }
    }

    /// Chat row menu: mark one chat's mentions, reactions or poll votes read.
    pub(super) fn read_chat_unread_markers(
        &mut self,
        chat_id: quill::ids::ChatId,
        kind: UnreadJumpKind,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live.driver.read_all_chat_unread_markers(chat_id, kind) {
                    Ok(()) => String::new(),
                    Err(_) => "could not mark as read".into(),
                };
        }
        cx.notify();
    }

    pub(super) fn read_all_unread_markers(&mut self, kind: UnreadJumpKind, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.read_all_unread_markers(kind) {
                Ok(()) => String::new(),
                Err(_) => "could not mark as read".into(),
            };
            cx.notify();
        }
    }
}

fn corner_button(kind: UnreadJumpKind, count: i32, owner: WeakEntity<QuillApp>) -> AnyElement {
    let (id, icon, label, tooltip) = match kind {
        UnreadJumpKind::Mention => (
            "jump-mention",
            IconName::AtSign,
            "Next unread mention",
            "Next unread mention · right-click for more",
        ),
        UnreadJumpKind::Reaction => (
            "jump-reaction",
            IconName::Heart,
            "Next unread reaction",
            "Next unread reaction · right-click for more",
        ),
        // tdesktop `lng_jump_to_poll_votes`.
        UnreadJumpKind::PollVote => (
            "jump-poll-vote",
            IconName::ChartBar,
            "Jump to poll votes",
            "Jump to poll votes · right-click for more",
        ),
    };
    // tdesktop `lng_context_mark_read_poll_votes_all` vs "Mark all as read".
    let read_all_label = match kind {
        UnreadJumpKind::PollVote => "Read all poll votes",
        _ => "Mark all as read",
    };
    let jump_owner = owner.clone();
    div()
        .id(format!("{id}-wrap"))
        .relative()
        .pt(px(BADGE_SIZE / 2.))
        .context_menu(move |menu, _, _| {
            let owner = owner.clone();
            menu.item(
                PopupMenuItem::new(read_all_label).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.read_all_unread_markers(kind, cx));
                }),
            )
        })
        .child(
            Button::new(id)
                .secondary()
                .icon(icon)
                .rounded_full()
                .border_1()
                .tooltip(tooltip)
                .accessibility_label(format!("{label}, {count}"))
                .on_click(move |_, _, cx| {
                    let _ = jump_owner.update(cx, |this, cx| this.jump_to_unread_marker(kind, cx));
                }),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .h(px(BADGE_SIZE))
                        .min_w(px(BADGE_SIZE))
                        .px_1p5()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(super::accent_strong())
                        .text_color(super::text_on_fill())
                        .text_xs()
                        .font_semibold()
                        .child(compact_count(count)),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use quill::state::{UnreadJumpKind, oldest_message_id, unread_bar_text};

    #[test]
    fn unread_bar_text_pluralises_like_tdesktop() {
        assert_eq!(unread_bar_text(1), "1 Unread Message");
        assert_eq!(unread_bar_text(2), "2 Unread Messages");
        assert_eq!(unread_bar_text(120), "120 Unread Messages");
        assert_eq!(unread_bar_text(0), "Unread Messages");
    }

    #[test]
    fn oldest_unread_marker_is_the_smallest_id() {
        let ids = [90, 40, 70].map(quill::ids::MessageId);
        assert_eq!(oldest_message_id(ids), Some(quill::ids::MessageId(40)));
        assert_eq!(oldest_message_id([]), None);
    }

    #[test]
    fn marker_kinds_map_to_tdlib_filters() {
        assert_eq!(
            UnreadJumpKind::Mention.filter_constructor(),
            "searchMessagesFilterUnreadMention"
        );
        assert_eq!(
            UnreadJumpKind::Reaction.filter_constructor(),
            "searchMessagesFilterUnreadReaction"
        );
        assert_eq!(
            UnreadJumpKind::PollVote.filter_constructor(),
            "searchMessagesFilterUnreadPollVote"
        );
    }
}
