//! Chat-row swipe actions, the UI half (tdesktop
//! `dialogs/dialogs_inner_widget.cpp` `setSwipeContextData` and
//! `dialogs/ui/dialogs_layout.cpp` row painting). The gesture state machine
//! and the translation curve are in `quill::chat_swipe`.
//!
//! A precise (trackpad) horizontal scroll over a row slides its content
//! left, revealing a coloured panel with the action's icon and label; past
//! the threshold the action runs once the finger lifts, otherwise the row
//! springs back. Frames come from the frame clock, attributed to the chat
//! list slice; the action itself fires from a short-lived timer that runs
//! only while a swipe is active.

use super::app::QuillApp;
use super::chat_theme::accent_strong;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chat_swipe::{
    ACTION_WIDTH, ChatSwipeFacts, ICON_SIZE, Phase, RowSwipe, SwipeAction, SwipeLabel,
    SwipeMachine, resolve_label,
};
use quill::ids::ChatId;
use quill::state::ChatSummary;
use quill::telegram::envelope::MUTE_FOREVER;
use std::time::{Duration, Instant};

/// Swipe state owned by the app.
pub(super) struct ChatSwipeState {
    machine: SwipeMachine<i64>,
    epoch: Instant,
    /// Runs while a gesture or a spring-back is active.
    poll: Option<Task<()>>,
}

impl Default for ChatSwipeState {
    fn default() -> Self {
        Self {
            machine: SwipeMachine::new(),
            epoch: Instant::now(),
            poll: None,
        }
    }
}

/// How often the active-swipe timer checks for a due action (the spring
/// back lasts 150 ms; a coarser step would delay the action visibly).
const POLL_MS: u64 = 20;

fn icon_for(label: SwipeLabel) -> IconName {
    match label {
        SwipeLabel::Mute => IconName::BellOff,
        SwipeLabel::Unmute => IconName::Bell,
        SwipeLabel::Pin => IconName::Pin,
        SwipeLabel::Unpin => IconName::PinOff,
        SwipeLabel::Read => IconName::CircleCheck,
        SwipeLabel::Unread => IconName::MessageSquareDot,
        SwipeLabel::Archive => IconName::Archive,
        SwipeLabel::Unarchive => IconName::ArchiveRestore,
        SwipeLabel::Delete => IconName::Trash,
        SwipeLabel::Disabled => IconName::Menu,
    }
}

impl QuillApp {
    fn swipe_now_ms(&self) -> u64 {
        self.chat_list.swipe.epoch.elapsed().as_millis() as u64
    }

    /// The facts the action label depends on for one chat.
    pub(super) fn chat_swipe_facts(&self, chat: &ChatSummary) -> ChatSwipeFacts {
        ChatSwipeFacts {
            muted: chat.is_muted(),
            pinned: if chat.in_archive {
                chat.archive_is_pinned
            } else {
                chat.is_pinned
            },
            unread: chat.is_unread(),
            archived: chat.in_archive,
            is_saved: self.session().is_some_and(|s| s.is_saved_messages(chat.id)),
            can_archive: true,
            // The same gate as the context menu's "Delete chat".
            can_delete: chat.can_be_deleted_only_for_self || chat.can_be_deleted_for_all_users,
        }
    }

    /// What swiping this row would do (`Disabled` when nothing: the
    /// setting is off, or the chat cannot take the action).
    pub(super) fn chat_swipe_label(&self, chat: &ChatSummary) -> SwipeLabel {
        match self.appearance.swipe_action {
            SwipeAction::Disabled => SwipeLabel::Disabled,
            action => resolve_label(action, self.chat_swipe_facts(chat)),
        }
    }

    /// Wrap a built chat row so it takes part in swipes: a scroll-wheel
    /// listener for the gesture, and the sliding row over the action panel
    /// while it is swiped. Rows with no action returned unchanged.
    pub(super) fn chat_swipe_wrap(
        &self,
        row: AnyElement,
        id: ChatId,
        label: SwipeLabel,
        height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if label == SwipeLabel::Disabled {
            return row;
        }
        let now = self.swipe_now_ms();
        let swiped = self.chat_list.swipe.machine.row(id.0, now);
        if self.chat_list.swipe.machine.animating(now) {
            self.request_animation_tick(60, cx);
        }
        let inner = match swiped {
            Some(swipe) => self.swiped_row(row, label, swipe, height, cx),
            None => row,
        };
        div()
            .w_full()
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                this.chat_swipe_wheel(id, event, cx);
            }))
            .child(inner)
            .into_any_element()
    }

    /// The row slid left by `swipe.shift`, over the revealed action panel
    /// (`dialogs_layout.cpp:1015-1062`): a filled panel as wide as the
    /// shift, the icon and label pinned to its right edge, and once the
    /// threshold is reached a grey circle growing from the icon.
    fn swiped_row(
        &self,
        row: AnyElement,
        label: SwipeLabel,
        swipe: RowSwipe,
        height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let h = f32::from(height);
        let shift = swipe.shift;
        let fill = if label == SwipeLabel::Delete {
            cx.theme().danger
        } else {
            accent_strong().into()
        };
        let radius = swipe.reach_radius();
        // tdesktop centres the flood on the icon's box: 1.5 icons from the
        // right edge and from the top (`offset`, dialogs_layout.cpp:1037).
        let centre = ICON_SIZE * 1.5;
        let block_top = ((h - ICON_SIZE * 2.) / 2.).max(0.);
        let panel = div()
            .absolute()
            .top_0()
            .right_0()
            .h(px(h))
            .w(px(shift))
            .overflow_hidden()
            .bg(fill)
            .when(radius > 0.5, |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(shift - centre - radius))
                        .top(px(centre - radius))
                        .size(px(radius * 2.))
                        .rounded_full()
                        .bg(cx.theme().muted_foreground),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(block_top))
                    .right_0()
                    .w(px(ACTION_WIDTH))
                    .h(px(ICON_SIZE * 2.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_between()
                    .text_color(gpui_kit::white())
                    .child(
                        Icon::new(icon_for(label))
                            .size(px(ICON_SIZE))
                            .text_color(gpui_kit::white()),
                    )
                    .child(
                        div()
                            .text_size(px(label.font_px()))
                            .font_semibold()
                            .whitespace_nowrap()
                            .child(label.text()),
                    ),
            );
        div()
            .relative()
            .w_full()
            .h(px(h))
            .overflow_hidden()
            .child(panel)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(-shift))
                    .w_full()
                    .h(px(h))
                    .child(row),
            )
            .into_any_element()
    }

    /// One wheel event over a swipeable row. Trackpad scrolls only:
    /// without precise deltas there is no gesture to follow, and a
    /// platform that sends no phases never starts one.
    fn chat_swipe_wheel(&mut self, id: ChatId, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let ScrollDelta::Pixels(delta) = event.delta else {
            return;
        };
        let phase = match event.touch_phase {
            TouchPhase::Started => Phase::Started,
            TouchPhase::Moved => Phase::Moved,
            TouchPhase::Ended => Phase::Ended,
            TouchPhase::Cancelled => Phase::Cancelled,
        };
        let now = self.swipe_now_ms();
        let feed = self.chat_list.swipe.machine.feed(
            phase,
            f32::from(delta.x),
            f32::from(delta.y),
            Some(id.0),
            now,
        );
        if feed.consumed {
            // A horizontal swipe on a row: the list must not scroll.
            cx.stop_propagation();
        }
        if feed.redraw {
            self.notify_sidebar(cx);
        }
        if self.chat_list.swipe.machine.swiping() || self.chat_list.swipe.machine.animating(now) {
            self.schedule_swipe_poll(cx);
        }
    }

    /// Start the active-swipe timer unless it runs.
    fn schedule_swipe_poll(&mut self, cx: &mut Context<Self>) {
        if self.chat_list.swipe.poll.is_some() {
            return;
        }
        self.chat_list.swipe.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(POLL_MS))
                    .await;
                let Ok(more) = this.update(cx, |this, cx| this.poll_swipe(cx)) else {
                    return;
                };
                if !more {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| this.chat_list.swipe.poll = None);
        }));
    }

    /// Screenshot fixture: leave `chat` held mid-swipe at `ratio`, as a
    /// trackpad gesture that has not ended.
    pub(super) fn demo_hold_swipe(&mut self, chat: i64, ratio: f32) {
        let now = self.swipe_now_ms();
        let m = &mut self.chat_list.swipe.machine;
        m.feed(Phase::Started, 0.0, 0.0, Some(chat), now);
        m.feed(Phase::Moved, -1.0, 0.0, Some(chat), now);
        m.feed(Phase::Moved, -20.0, 0.0, Some(chat), now);
        let raw = ratio * quill::chat_swipe::THRESHOLD / quill::chat_swipe::WHEEL_SLOW - 20.0;
        m.feed(Phase::Moved, -raw, 0.0, Some(chat), now);
    }

    /// Run a due action and abandon a gesture that lost its end event.
    /// Returns whether the timer must keep running.
    fn poll_swipe(&mut self, cx: &mut Context<Self>) -> bool {
        let now = self.swipe_now_ms();
        if let Some(chat) = self.chat_list.swipe.machine.poll(now) {
            self.perform_swipe_action(ChatId(chat), cx);
        }
        // The last spring-back frame must paint the settled row.
        self.notify_sidebar(cx);
        self.chat_list.swipe.machine.swiping() || self.chat_list.swipe.machine.animating(now)
    }

    /// The configured action on a chat (`PerformQuickDialogAction`). All of
    /// them are the chat menu's own handlers, so gating, live requests and
    /// the Delete confirmation are unchanged.
    pub(super) fn perform_swipe_action(&mut self, id: ChatId, cx: &mut Context<Self>) {
        let Some(label) = self
            .session()
            .and_then(|s| s.chats.get(&id.0))
            .map(|chat| self.chat_swipe_label(chat))
        else {
            return;
        };
        match label {
            SwipeLabel::Disabled => return,
            SwipeLabel::Mute => self.apply_chat_mute(id, MUTE_FOREVER, cx),
            SwipeLabel::Unmute => self.apply_chat_mute(id, 0, cx),
            SwipeLabel::Pin | SwipeLabel::Unpin => self.toggle_chat_pin(id, cx),
            SwipeLabel::Read | SwipeLabel::Unread => self.toggle_chat_marked_as_unread(id, cx),
            SwipeLabel::Archive | SwipeLabel::Unarchive => self.toggle_archive(id, cx),
            SwipeLabel::Delete => {
                // Never deletes by itself: the usual confirm dialog.
                self.open_group_confirm(id, GroupConfirmAction::RemoveFromList, cx);
            }
        }
        // tdesktop's toast; Pin keeps its own note, which may be the
        // pin-limit message.
        if !matches!(
            label,
            SwipeLabel::Pin | SwipeLabel::Unpin | SwipeLabel::Delete
        ) && !self.connection.status_note.starts_with("could not")
        {
            self.connection.status_note = label.toast().into();
        }
        cx.notify();
    }
}
