//! The pointer on Picture-in-Picture (tdesktop `Pip::handleMouse*` and
//! `PipPanel::handleMouse*`, `processDrag`, `finishDrag`, `moveAnimated`,
//! `updateDecorations`): the controls' state, presses, moving and resizing
//! the window, the snap slide, and resizes the window system makes.

use super::{Press, Slide, VideoPip, open, place};
use gpui_kit::*;
use quill::media_viewer::pip::layout::{ControlsLayout, OverState, PressAction};
use quill::media_viewer::pip::panel::{
    DragStep, MOVE_ANIMATION_MS, PipPoint, PipPosition, PipRect, RectPart, START_DRAG_DISTANCE,
    count_position, drag_step, finish_drag, fit_system_resize, place_on_screen, resize_part,
    slide_point,
};
use quill::settings::SavedPipPosition;
use std::time::Duration;

impl VideoPip {
    pub(super) fn layout(&self, window: &Window) -> ControlsLayout {
        let viewport = window.viewport_size();
        let inner = place::to_rect(Bounds::new(Point::default(), viewport));
        ControlsLayout::new(PipRect::new(0, 0, inner.w, inner.h), self.controls_on_left)
    }

    pub(super) fn set_over(&mut self, over: OverState, cx: &mut Context<Self>) {
        if self.over != over {
            self.over = over;
            cx.notify();
        }
    }

    /// The window's frame now: where a running slide has it (the slide
    /// stops, as `processDrag` stops `_positionAnimation`), else what the
    /// window system reports.
    fn current_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) -> PipRect {
        if let Some(slide) = self.slide.take() {
            let at = slide_point(slide.from, slide.to, slide_progress(slide, cx));
            self.set_frame(PipRect::from_parts(at, slide.size), window, cx);
            return self.frame;
        }
        self.frame = place::frame(window);
        self.frame
    }

    /// `handleMousePress` (both the window's and the panel's).
    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        window.focus(&self.focus, cx);
        let local = local_point(event.position);
        let over = self.over_at(window, local);
        self.set_over(over, cx);
        if event.click_count >= 2 {
            // `handleDoubleClick` takes the second press.
            if let Some(action) = self.presses.double_click(over) {
                self.run(action, window, cx);
            }
            return;
        }
        let frame = self.current_frame(window, cx);
        self.presses.press(over);
        self.dragged = false;
        self.press = Some(Press {
            part: resize_part(local, frame.size(), self.attached.with(self.snapped)),
            cursor: place::cursor(window),
            start: frame,
            dragging: false,
        });
    }

    /// `handleMouseMove`: the controls' state, the resize cursor, and the
    /// drag once the press moved far enough.
    pub(super) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let local = local_point(event.position);
        let over = self.over_at(window, local);
        self.set_over(over, cx);
        let Some(mut press) = self.press else {
            let edge = resize_part(local, self.frame.size(), self.attached.with(self.snapped));
            if self.edge != edge {
                self.edge = edge;
                cx.notify();
            }
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            // The release went elsewhere: end the press here.
            self.finish_press(local, window, cx);
            return;
        }
        let cursor = place::cursor(window);
        let delta = cursor.minus(press.cursor);
        if !press.dragging
            && delta.manhattan() > START_DRAG_DISTANCE
            && !self.presses.drag_disabled()
        {
            press.dragging = true;
            self.dragged = true;
            if self.wayland {
                // The compositor moves and resizes (`startSystemDrag`).
                self.press = None;
                self.presses.release(over, true);
                match resize_edge(press.part) {
                    Some(edge) => window.start_window_resize(edge),
                    None => window.start_window_move(),
                }
                return;
            }
            // `_dragStartGeometry` is the frame when the drag starts; a
            // move is attached and snapped to nothing until it settles
            // (`updateDecorations` with a center drag).
            press.start = self.current_frame(window, cx);
            if let Some(screen) = place::own_screen(window, cx) {
                let moving = press.part == RectPart::Center;
                (self.attached, self.snapped) =
                    count_position(press.start, screen, moving, self.wayland);
            }
        }
        self.press = Some(press);
        if !press.dragging {
            return;
        }
        let screen = if press.part == RectPart::Center {
            place::screen_at(window, cx, cursor)
        } else {
            place::own_screen(window, cx)
        };
        let Some(screen) = screen else {
            return;
        };
        match drag_step(
            press.start,
            delta,
            press.part,
            self.ratio,
            screen,
            self.attached,
        ) {
            Some(DragStep::Frame(frame)) => {
                self.slide = None;
                self.set_frame(frame, window, cx);
            }
            Some(DragStep::Slide(to)) => self.slide_to(to, cx),
            None => {}
        }
    }

    /// `handleMouseRelease`: a click on the picture pauses; a drag ends
    /// inside the screen.
    pub(super) fn on_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        self.finish_press(local_point(event.position), window, cx);
    }

    fn finish_press(&mut self, local: PipPoint, window: &mut Window, cx: &mut Context<Self>) {
        let over = self.over_at(window, local);
        let press = self.press.take();
        let action = self.presses.release(over, self.dragged);
        if press.is_some_and(|press| press.dragging) {
            self.finish_drag(window, cx);
        }
        // Buttons act through their own clicks (and keys); the picture
        // through this press.
        if action == Some(PressAction::TogglePause) {
            self.toggle_pause(window, cx);
        }
    }

    /// `finishDrag`: pull the window back inside the screen under the
    /// pointer, else settle where it is.
    fn finish_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = place::cursor(window);
        let target = place::screen_at(window, cx, cursor)
            .and_then(|screen| finish_drag(self.frame, screen, self.wayland));
        match target {
            Some(to) => self.slide_to(to, cx),
            None => {
                if self.slide.is_none() {
                    self.settle(window, cx);
                }
            }
        }
    }

    /// Start sliding the window's top-left to `to` (`moveAnimated`).
    fn slide_to(&mut self, to: PipPoint, cx: &mut Context<Self>) {
        if self.slide.is_some_and(|slide| slide.to == to) {
            return;
        }
        let from = self.slide.map_or(self.frame.origin(), |slide| {
            slide_point(slide.from, slide.to, slide_progress(slide, cx))
        });
        self.slide = Some(Slide {
            from,
            to,
            size: self.frame.size(),
            started: cx.background_executor().now(),
        });
        cx.notify();
    }

    /// One frame of the slide (`updatePositionAnimated`); `true` while it
    /// still runs.
    pub(super) fn step_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(slide) = self.slide else {
            return false;
        };
        let progress = slide_progress(slide, cx);
        let at = slide_point(slide.from, slide.to, progress);
        self.set_frame(PipRect::from_parts(at, slide.size), window, cx);
        if progress < 1.0 {
            return true;
        }
        self.slide = None;
        if !self.press.is_some_and(|press| press.dragging) {
            self.settle(window, cx);
        }
        false
    }

    fn set_frame(&mut self, frame: PipRect, window: &mut Window, cx: &mut Context<Self>) {
        if frame == self.frame {
            return;
        }
        self.frame = frame;
        place::set_frame(window, frame, cx);
    }

    /// `updateDecorations` after a move or resize: note the sides the
    /// window is attached or snapped to and save the position
    /// (`saveGeometry`).
    fn settle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(screen) = place::own_screen(window, cx) else {
            return;
        };
        let (attached, snapped) = count_position(self.frame, screen, false, self.wayland);
        self.attached = attached;
        self.snapped = snapped;
        let display = window.display(cx);
        let saved = SavedPipPosition {
            position: PipPosition {
                attached,
                snapped,
                geometry: self.frame,
                screen: display
                    .as_ref()
                    .map(|display| place::to_rect(display.bounds()))
                    .unwrap_or_default(),
            },
            screen_id: display
                .and_then(|display| display.uuid().ok())
                .map(|uuid| uuid.to_string()),
        };
        open::save_position(saved, cx);
    }

    /// The clip's aspect became known or changed (`setAspectRatio`):
    /// re-place the window keeping its sides.
    pub(super) fn sync_ratio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ratio) = self.picture_ratio() else {
            return;
        };
        if ratio == self.ratio || self.press.is_some_and(|press| press.dragging) {
            return;
        }
        self.ratio = ratio;
        let Some(screen) = place::own_screen(window, cx) else {
            return;
        };
        let position = PipPosition {
            attached: self.attached,
            snapped: self.snapped,
            geometry: self.frame,
            screen,
        };
        let placement = place_on_screen(&position, screen, ratio);
        self.slide = None;
        self.set_frame(placement.geometry, window, cx);
    }

    /// The window system resized the window (a Wayland compositor, an X11
    /// window manager): back to the aspect and its limits
    /// (`handleWaylandResize`; tdesktop's minimum and maximum size).
    /// Frames the window sets itself already fit; a pixel of rounding is
    /// left alone so the two never fight.
    pub(super) fn on_bounds_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.press.is_some() || self.slide.is_some() {
            return;
        }
        let Some(screen) = place::own_screen(window, cx) else {
            return;
        };
        let current = place::to_rect(window.bounds()).size();
        let fitted = fit_system_resize(current, self.ratio, screen.size());
        if (fitted.w - current.w).abs() > 1 || (fitted.h - current.h).abs() > 1 {
            window.resize(size(px(fitted.w as f32), px(fitted.h as f32)));
        }
    }
}

fn local_point(position: Point<Pixels>) -> PipPoint {
    place::to_rect(Bounds::new(position, Size::default())).origin()
}

fn slide_progress(slide: Slide, cx: &App) -> f32 {
    let elapsed = cx
        .background_executor()
        .now()
        .saturating_duration_since(slide.started);
    let total = Duration::from_millis(MOVE_ANIMATION_MS);
    (elapsed.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
}

/// The system resize edge for a press part (`RectPartToQtEdges`).
fn resize_edge(part: RectPart) -> Option<ResizeEdge> {
    Some(match part {
        RectPart::Center => return None,
        RectPart::TopLeft => ResizeEdge::TopLeft,
        RectPart::Top => ResizeEdge::Top,
        RectPart::TopRight => ResizeEdge::TopRight,
        RectPart::Right => ResizeEdge::Right,
        RectPart::BottomRight => ResizeEdge::BottomRight,
        RectPart::Bottom => ResizeEdge::Bottom,
        RectPart::BottomLeft => ResizeEdge::BottomLeft,
        RectPart::Left => ResizeEdge::Left,
    })
}
