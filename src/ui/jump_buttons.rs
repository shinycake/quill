//! The history's corner "@" and heart buttons (tdesktop
//! `HistoryView::CornerButtons`, `history_view_corner_buttons.cpp`): round
//! buttons stacked above the scroll-to-bottom arrow, each with an unread
//! count badge. A click jumps to the oldest unread mention / reaction;
//! right-click offers "Mark all as read". The buttons show whenever the
//! chat has unread markers (they do not wait for a scroll). Each fades
//! and rises in over `historyToDownDuration` (150 ms) and fades out in
//! 120 ms (`surface_reveal`); the stack sits above the jump-to-bottom
//! button while that shows, moving with it (`CornerButtons::updatePositions`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::compact_count;
use super::surface_reveal;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::UnreadJumpKind;

/// tdesktop `historyToDownBadgeSize`.
const BADGE_SIZE: f32 = 22.;
/// How far a corner button starts below its place as it fades in.
const RISE: f32 = 8.;
/// The jump-to-bottom button's height and the gap the stack keeps above
/// it (`historyToDownPosition.y`): what the stack rises by while that
/// button shows.
const JUMP_BUTTON_ROOM: f32 = 32. + 12.;
/// The stack's inset from the history's bottom-right corner.
const CORNER_INSET: f32 = 16.;

impl QuillApp {
    /// Whether the kit's jump-to-bottom button shows this frame, by its
    /// own rule (`MessageScroller::render`): scrolled up past
    /// `historyToDownShownAfter` (480 px) from the bottom, any distance
    /// while unread messages are below, or always while the window
    /// stops short of the latest message.
    fn jump_button_shown(&self, unread_count: i32, has_newer: bool, cx: &App) -> bool {
        if has_newer {
            return true;
        }
        let state = self.history.scroller.read(cx);
        let shown_after = px(if unread_count > 0 { 0. } else { 480. });
        let far_enough = state
            .distance_to_edges()
            .is_none_or(|(_, below, _)| below > shown_after);
        state.is_scrolled_up() && far_enough
    }

    /// The "@" / heart stack for the open chat, bottom-right of the
    /// history; `None` when nothing is unread and nothing is fading out.
    pub(super) fn jump_corner_buttons(
        &self,
        mentions: i32,
        reactions: i32,
        poll_votes: i32,
        unread_count: i32,
        has_newer: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let now = surface_reveal::now();
        let animate = surface_reveal::animate(cx);
        let scope = self.open_chat_id().map_or(0, |chat| chat.0 as u64);
        let lift_to = if self.jump_button_shown(unread_count, has_newer, cx) {
            1.
        } else {
            0.
        };
        let (drawn, lift, moving) = {
            let mut corners = self.frame.surfaces.corners.borrow_mut();
            let mut lift = self.frame.surfaces.corner_lift.borrow_mut();
            lift.go(lift_to, surface_reveal::CORNER, animate, now);
            let mut moving = lift.animating(now);
            let mut drawn = Vec::with_capacity(3);
            for (presence, (kind, count)) in corners.iter_mut().zip([
                (UnreadJumpKind::Mention, mentions),
                (UnreadJumpKind::Reaction, reactions),
                (UnreadJumpKind::PollVote, poll_votes),
            ]) {
                let state = (count > 0).then_some(count);
                if let Some(button) = presence.follow(state, scope, animate, now) {
                    drawn.push((kind, button));
                }
                moving |= presence.moving(now);
            }
            (drawn, lift.value(now).clamp(0., 1.), moving)
        };
        if moving {
            self.request_animation_tick(60, cx);
        }
        if drawn.is_empty() {
            return None;
        }
        let weak = cx.weak_entity();
        Some(
            div()
                .absolute()
                .right(px(CORNER_INSET))
                .bottom(px(CORNER_INSET + JUMP_BUTTON_ROOM * lift))
                .flex()
                .flex_col()
                .items_end()
                .gap_3()
                .children(drawn.into_iter().map(|(kind, button)| {
                    let rise = RISE * (1. - button.shown);
                    div()
                        .opacity(button.shown)
                        // Rises into place; the margins cancel, so the
                        // stack keeps its height while a button moves.
                        .mt(px(rise))
                        .mb(px(-rise))
                        .child(corner_button(kind, button.value, weak.clone()))
                }))
                .into_any_element(),
        )
    }

    pub(super) fn jump_to_unread_marker(&mut self, kind: UnreadJumpKind, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.screen
                .connection
                .set_note(match live.driver.jump_to_unread_marker(kind) {
                    Ok(()) => StatusNote::default(),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_FIND_THE_MESSAGE)),
                });
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
            self.screen.connection.set_note(
                match live.driver.read_all_chat_unread_markers(chat_id, kind) {
                    Ok(()) => StatusNote::default(),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_MARK_AS_READ)),
                },
            );
        }
        cx.notify();
    }

    pub(super) fn read_all_unread_markers(&mut self, kind: UnreadJumpKind, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.screen
                .connection
                .set_note(match live.driver.read_all_unread_markers(kind) {
                    Ok(()) => StatusNote::default(),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_MARK_AS_READ)),
                });
            cx.notify();
        }
    }
}

fn corner_button(kind: UnreadJumpKind, count: i32, owner: WeakEntity<QuillApp>) -> AnyElement {
    let (id, icon, label, tooltip) = match kind {
        UnreadJumpKind::Mention => (
            "jump-mention",
            IconName::AtSign,
            tr(keys::Q_MISC_NEXT_UNREAD_MENTION),
            tr(keys::Q_MISC_NEXT_UNREAD_MENTION_RIGHT_CLICK),
        ),
        UnreadJumpKind::Reaction => (
            "jump-reaction",
            IconName::Heart,
            tr(keys::Q_MISC_NEXT_UNREAD_REACTION),
            tr(keys::Q_MISC_NEXT_UNREAD_REACTION_RIGHT_CLICK),
        ),
        // tdesktop `lng_jump_to_poll_votes`.
        UnreadJumpKind::PollVote => (
            "jump-poll-vote",
            IconName::ChartBar,
            tr(keys::JUMP_TO_POLL_VOTES),
            tr(keys::Q_MISC_JUMP_TO_POLL_VOTES_RIGHT),
        ),
    };
    // tdesktop `lng_context_mark_read_poll_votes_all` vs "Mark all as read".
    let read_all_label = match kind {
        UnreadJumpKind::PollVote => tr(keys::CONTEXT_MARK_READ_POLL_VOTES_ALL),
        _ => tr(keys::Q_MISC_MARK_ALL_AS_READ),
    };
    let jump_owner = owner.clone();
    div()
        .id(format!("{id}-wrap"))
        // Floats over the history: a click on the button or its badge
        // stays here instead of also reaching the media under it.
        .block_mouse_except_scroll()
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
    use quill::state::{UnreadJumpKind, oldest_message_id};

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
