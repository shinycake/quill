//! Middle-click autoscroll in the message history (`quill::autoscroll`).
//!
//! A middle press anchors a point and marks it; while the pointer stays
//! below or above it the history scrolls that way. A quick click toggles
//! the mode (the next click ends it), a press held past a fifth of a second
//! scrolls only while held. The loop feeds the list ordinary wheel events,
//! because the message scroller has no pixel offset of its own.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::TestSupportExt;
use gpui_kit::component::*;
use gpui_kit::gpui::{
    Modifiers, MouseMoveEvent, PlatformInput, Point, ScrollDelta, ScrollEvent, TouchPhase, point,
    px,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::autoscroll::{Pull, pull, release_stops, scroll_step};
use quill::ids::AccountChat;
use std::rc::{Rc, Weak};
use std::time::Instant;

/// `PaintCursorImage` is 27 px in tdesktop.
const MARK_SIZE: f32 = 27.;

#[derive(Default)]
pub(super) struct AutoscrollUi {
    active: Option<Active>,
}

/// What a window's root draws for the mode of the history it shows: the
/// anchor mark, and a left or right press anywhere ending the mode
/// (`ListWidget::mousePressEvent`). Built with the window's state in
/// place; the main window and each chat window put it on their root,
/// whose origin is the window's, as the anchor's is.
pub(super) struct AutoscrollLayer {
    mark: Option<AnyElement>,
    active: bool,
}

impl AutoscrollLayer {
    /// Add the layer to `root`. A press is handled with the pressed
    /// window's state in place (its `scope_probe` runs first).
    pub(super) fn on<E>(self, root: E, app: WeakEntity<QuillApp>) -> E
    where
        E: ParentElement + InteractiveElement + FluentBuilder,
    {
        let stop_app = app.clone();
        let stop = move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
            let _ = stop_app.update(cx, |app, cx| app.autoscroll_stop(cx));
        };
        // A held press scrolls the pointer off the history, so its release
        // lands elsewhere in the window: the root hears it too.
        let release = move |_: &MouseUpEvent, _: &mut Window, cx: &mut App| {
            let _ = app.update(cx, |app, cx| app.autoscroll_release(cx));
        };
        root.children(self.mark).when(self.active, |this| {
            this.on_mouse_down(MouseButton::Left, stop.clone())
                .on_mouse_down(MouseButton::Right, stop)
                .on_mouse_up(MouseButton::Middle, release)
        })
    }
}

struct Active {
    anchor: Point<Pixels>,
    /// The chat of the account whose history the mode scrolls.
    chat: AccountChat,
    button: Button,
    last_tick: Instant,
    pull: Pull,
    /// The life of the frame loop that scrolls: each frame's step holds a
    /// `Weak` of it and ends once the mode does, so a quick new press never
    /// runs beside the old loop.
    _frames: Rc<()>,
}

/// The middle button of the press that anchored the mode, for the
/// hold-or-toggle rule: down since `since`, or released as a click.
#[derive(Clone, Copy)]
enum Button {
    Held { since: Instant },
    Toggled,
}

impl QuillApp {
    /// Middle button pressed over the history.
    pub(super) fn autoscroll_press(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history.autoscroll.active.is_some() {
            self.autoscroll_stop(cx);
            return;
        }
        let Some(chat) = self.open_account_chat() else {
            return;
        };
        let now = Instant::now();
        let frames = Rc::new(());
        autoscroll_frame(self.scoped_app(cx), Rc::downgrade(&frames), window);
        self.history.autoscroll.active = Some(Active {
            anchor: position,
            chat,
            button: Button::Held { since: now },
            last_tick: now,
            pull: Pull::Neutral,
            _frames: frames,
        });
        cx.notify();
    }

    /// The open chat of the account on screen.
    fn open_account_chat(&self) -> Option<AccountChat> {
        let session = self.session()?;
        Some(AccountChat::new(
            session.account.clone(),
            session.open_chat_id()?,
        ))
    }

    /// Middle button released: a long press was a hold and ends here.
    pub(super) fn autoscroll_release(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.history.autoscroll.active.as_mut() else {
            return;
        };
        let Button::Held { since } = active.button else {
            return;
        };
        active.button = Button::Toggled;
        if release_stops(since.elapsed()) {
            self.autoscroll_stop(cx);
        }
    }

    pub(super) fn autoscroll_stop(&mut self, cx: &mut Context<Self>) {
        if self.history.autoscroll.active.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn autoscroll_active(&self) -> bool {
        self.history.autoscroll.active.is_some()
    }

    /// One tick of the loop: how far to scroll, or `None` to end it (the
    /// window lost focus, another chat opened, or the mode was stopped).
    fn autoscroll_plan(
        &mut self,
        pointer: Point<Pixels>,
        window_active: bool,
        cx: &mut Context<Self>,
    ) -> Option<(Point<Pixels>, f32)> {
        let open = self.open_account_chat();
        let active = self.history.autoscroll.active.as_mut()?;
        if !window_active || open.as_ref() != Some(&active.chat) {
            self.history.autoscroll.active = None;
            cx.notify();
            return None;
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(active.last_tick);
        active.last_tick = now;
        let delta = f32::from(pointer.y - active.anchor.y);
        let next = pull(delta);
        if next != active.pull {
            active.pull = next;
            cx.notify();
        }
        Some((active.anchor, scroll_step(delta, elapsed)))
    }

    /// The mark and the press that ends the mode, for the window whose
    /// state is in place.
    pub(super) fn autoscroll_layer(&self, cx: &mut Context<Self>) -> AutoscrollLayer {
        AutoscrollLayer {
            mark: self.autoscroll_mark(cx),
            active: self.autoscroll_active(),
        }
    }

    /// The anchor mark: a ring with an arrow for each direction, the one
    /// being pulled towards drawn strong.
    fn autoscroll_mark(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let active = self.history.autoscroll.active.as_ref()?;
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
                .test_support()
                .into_any_element(),
        )
    }
}

/// One step of the mode at the next frame of `window`, then the one after:
/// the pointer is read and the history scrolled by the time the frame took,
/// as often as the window draws and not at all while it does not (hidden,
/// minimized). Ends when `alive` is gone or the plan says so.
fn autoscroll_frame(app: super::chat_window::ScopedApp, alive: Weak<()>, window: &Window) {
    // screen-scope: `ScopedApp::update` puts the starting screen in place.
    // window-scope: `ScopedApp::update` puts the starting window's state in place.
    window.on_next_frame(move |window, cx| {
        // The step runs after this frame's draw, outside it.
        // screen-scope: `ScopedApp::update` puts the starting screen in place.
        // window-scope: `ScopedApp::update` puts the starting window's state in place.
        window.defer(cx, move |window, cx| {
            if alive.strong_count() == 0 {
                return;
            }
            let pointer = window.mouse_position();
            let window_active = window.is_window_active();
            let step = app
                .update(cx, |this, cx| {
                    this.autoscroll_plan(pointer, window_active, cx)
                })
                .ok()
                .flatten();
            let Some((anchor, dy)) = step else {
                return;
            };
            if dy != 0. {
                scroll_history_by(window, cx, anchor, pointer, dy);
            }
            autoscroll_frame(app, alive, window);
        });
    });
}

/// Feeds one wheel event to the list under `anchor`. The list reacts only
/// to the hovered hitbox, so the logical pointer visits the anchor for the
/// event and goes back to where the real one is. Drag selection's edge
/// autoscroll (`selection_drag`) scrolls the same way.
pub(super) fn scroll_history_by(
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
        PlatformInput::Scroll(ScrollEvent {
            position: anchor,
            delta: ScrollDelta::Pixels(point(px(0.), px(-dy))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: gpui_kit::ScrollSource::Pointer,
        }),
        cx,
    );
    if !at_anchor {
        window.dispatch_event(moved(pointer), cx);
    }
}
