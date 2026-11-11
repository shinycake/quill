//! Dragging the video controller around the viewer
//! (`quill::media_viewer::ControllerPlace`): a press on the panel's
//! background picks it up, it follows the pointer inside the safe area,
//! snaps back onto its home spot close to it, and a double-click puts it
//! home from anywhere. While it is held, an outline marks home.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::video_player::controls::PanelPress;
use quill::media_viewer::ViewRect;
use std::rc::Rc;

/// Where the controller is drawn and the area it may go: both from the
/// frame being drawn, so the drag works on what the user sees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ControllerFrame {
    pub(super) home: ViewRect,
    pub(super) area: ViewRect,
}

fn point_of(at: Point<Pixels>) -> (f32, f32) {
    (at.x.as_f32(), at.y.as_f32())
}

impl QuillApp {
    /// The panel's handler for a press on its background.
    pub(super) fn viewer_controller_press(
        &self,
        frame: ControllerFrame,
        cx: &mut Context<Self>,
    ) -> PanelPress {
        let view = cx.entity().downgrade();
        Rc::new(move |at, clicks, _, cx| {
            let _ = view.update(cx, |this, cx| {
                if clicks >= 2 {
                    this.viewer.controller.reset();
                } else {
                    this.viewer
                        .controller
                        .press(point_of(at), frame.home, frame.area);
                }
                this.notify_viewer_panel(cx);
            });
        })
    }

    /// How far the controller is drawn from its home spot.
    pub(super) fn viewer_controller_offset(&self, frame: ControllerFrame) -> (f32, f32) {
        self.viewer.controller.offset(frame.home, frame.area)
    }

    /// While the controller is held: the outline of its home spot and the
    /// window-wide listeners that move and drop it. The pointer leaves the
    /// panel during a fast drag and the release can land anywhere, so the
    /// listeners are the window's, not the panel's.
    pub(super) fn viewer_controller_drag_layer(
        &self,
        frame: ControllerFrame,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.viewer.controller.is_held() {
            return None;
        }
        let moved = self.viewer.controller.spot() != ControllerSpot::Home;
        let view = cx.entity().downgrade();
        let listeners = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let move_view = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = move_view.update(cx, |this, cx| {
                        let place = &mut this.viewer.controller;
                        let changed = if event.pressed_button == Some(MouseButton::Left) {
                            place.drag_to(point_of(event.position), frame.home, frame.area)
                        } else {
                            // The release went to another window or app.
                            place.release()
                        };
                        if changed {
                            this.notify_viewer_panel(cx);
                        }
                    });
                });
                let up_view = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = up_view.update(cx, |this, cx| {
                        if this.viewer.controller.release() {
                            this.notify_viewer_panel(cx);
                        }
                    });
                });
            },
        )
        .absolute()
        .size_0();
        let home = frame.home;
        let outline = moved.then(|| {
            div()
                .absolute()
                .left(px(home.x))
                .top(px(home.y))
                .w(px(home.w))
                .h(px(home.h))
                .rounded(cx.theme().radius_lg)
                .border_1()
                .border_dashed()
                .border_color(gpui_kit::white().opacity(0.45))
                .bg(gpui_kit::white().opacity(0.06))
        });
        Some(
            div()
                .absolute()
                .inset_0()
                .child(listeners)
                .children(outline)
                .into_any_element(),
        )
    }
}
