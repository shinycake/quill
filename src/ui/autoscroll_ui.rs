//! Middle-click autoscroll in the message history (`quill::autoscroll`).
//!
//! A middle press anchors a point and marks it; while the pointer stays
//! below or above it the history scrolls that way. A quick click toggles
//! the mode (the next click ends it), a press held past a fifth of a second
//! scrolls only while held. The loop feeds the list ordinary wheel events,
//! because the message scroller has no pixel offset of its own.

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::gpui::{
    Modifiers, MouseMoveEvent, PlatformInput, Point, ScrollDelta, ScrollWheelEvent, TouchPhase,
    point, px,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::autoscroll::{Pull, pull, release_stops, scroll_step};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(15);
/// `PaintCursorImage` is 27 px in tdesktop.
const MARK_SIZE: f32 = 27.;

#[derive(Default)]
pub(super) struct AutoscrollUi {
    active: Option<Active>,
}

struct Active {
    anchor: Point<Pixels>,
    chat: i64,
    /// Set while the middle button is down, for the hold-or-toggle rule.
    pressed_at: Option<Instant>,
    last_tick: Instant,
    pull: Pull,
}

impl QuillApp {
    /// Middle button pressed over the history.
    pub(super) fn autoscroll_press(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.autoscroll.active.is_some() {
            self.autoscroll_stop(cx);
            return;
        }
        let Some(chat) = self.session().and_then(|s| s.open_chat) else {
            return;
        };
        let now = Instant::now();
        self.autoscroll.active = Some(Active {
            anchor: position,
            chat: chat.0,
            pressed_at: Some(now),
            last_tick: now,
            pull: Pull::Neutral,
        });
        let handle = window.window_handle();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let Ok((pointer, window_active)) =
                    cx.update(|window, _| (window.mouse_position(), window.is_window_active()))
                else {
                    break;
                };
                let step = this
                    .update(cx, |this, cx| {
                        this.autoscroll_plan(pointer, window_active, cx)
                    })
                    .ok()
                    .flatten();
                let Some((anchor, dy)) = step else {
                    break;
                };
                if dy != 0. {
                    let _ = handle.update(cx, |_, window, cx| {
                        scroll_history_by(window, cx, anchor, pointer, dy);
                    });
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Middle button released: a long press was a hold and ends here.
    pub(super) fn autoscroll_release(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.autoscroll.active.as_mut() else {
            return;
        };
        let Some(pressed_at) = active.pressed_at.take() else {
            return;
        };
        if release_stops(pressed_at.elapsed()) {
            self.autoscroll_stop(cx);
        }
    }

    pub(super) fn autoscroll_stop(&mut self, cx: &mut Context<Self>) {
        if self.autoscroll.active.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn autoscroll_active(&self) -> bool {
        self.autoscroll.active.is_some()
    }

    /// One tick of the loop: how far to scroll, or `None` to end it (the
    /// window lost focus, another chat opened, or the mode was stopped).
    fn autoscroll_plan(
        &mut self,
        pointer: Point<Pixels>,
        window_active: bool,
        cx: &mut Context<Self>,
    ) -> Option<(Point<Pixels>, f32)> {
        let open = self.session().and_then(|s| s.open_chat).map(|id| id.0);
        let active = self.autoscroll.active.as_mut()?;
        if !window_active || open != Some(active.chat) {
            self.autoscroll.active = None;
            cx.notify();
            return None;
        }
        let now = Instant::now();
        let elapsed = now - active.last_tick;
        active.last_tick = now;
        let delta = f32::from(pointer.y - active.anchor.y);
        let next = pull(delta);
        if next != active.pull {
            active.pull = next;
            cx.notify();
        }
        Some((active.anchor, scroll_step(delta, elapsed)))
    }

    /// The anchor mark: a ring with an arrow for each direction, the one
    /// being pulled towards drawn strong.
    pub(super) fn autoscroll_mark(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let active = self.autoscroll.active.as_ref()?;
        let theme = cx.theme();
        let strong = theme.foreground;
        let faint = theme.muted_foreground;
        let arrow = |name, lit: bool| {
            Icon::new(name)
                .size(px(11.))
                .text_color(if lit { strong } else { faint })
        };
        Some(
            div()
                .id("autoscroll-mark")
                .absolute()
                .left(active.anchor.x - px(MARK_SIZE / 2.))
                .top(active.anchor.y - px(MARK_SIZE / 2.))
                .size(px(MARK_SIZE))
                .rounded_full()
                .border_1()
                .border_color(faint)
                .bg(theme.background.opacity(0.94))
                .flex()
                .flex_col()
                .items_center()
                .justify_between()
                .py(px(1.))
                .child(arrow(
                    gpui_kit::assets::IconName::ChevronUp,
                    active.pull != Pull::Down,
                ))
                .child(arrow(
                    gpui_kit::assets::IconName::ChevronDown,
                    active.pull != Pull::Up,
                ))
                .when(active.pull == Pull::Neutral, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(MARK_SIZE / 2. - 3.5))
                            .left(px(MARK_SIZE / 2. - 3.5))
                            .size(px(5.))
                            .rounded_full()
                            .bg(faint),
                    )
                })
                .into_any_element(),
        )
    }
}

/// Feeds one wheel event to the list under `anchor`. The list reacts only
/// to the hovered hitbox, so the logical pointer visits the anchor for the
/// event and goes back to where the real one is.
fn scroll_history_by(
    window: &mut Window,
    cx: &mut App,
    anchor: Point<Pixels>,
    pointer: Point<Pixels>,
    dy: f32,
) {
    let moved = |position| {
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        })
    };
    let at_anchor = pointer == anchor;
    if !at_anchor {
        window.dispatch_event(moved(anchor), cx);
    }
    // A wheel delta moves the content; content moving up shows newer rows.
    window.dispatch_event(
        PlatformInput::ScrollWheel(ScrollWheelEvent {
            position: anchor,
            delta: ScrollDelta::Pixels(point(px(0.), px(-dy))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        }),
        cx,
    );
    if !at_anchor {
        window.dispatch_event(moved(pointer), cx);
    }
}
