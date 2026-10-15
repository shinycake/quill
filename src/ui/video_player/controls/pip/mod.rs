//! Picture-in-Picture: the viewer's own [`VideoPlayer`] in a borderless
//! window that floats above the others, with tdesktop's PiP chrome
//! (`Telegram/SourceFiles/media/view/media_view_pip.cpp`):
//!
//! - the picture fills the window, which keeps the clip's aspect; it opens
//!   320 px on its longer side, snapped to the top-left of the screen the
//!   main window is on, or where the last one was left;
//! - dragging moves it and snaps it 20 px inside a screen edge it comes
//!   within 16 px of; the 10 px band along each free edge resizes it, the
//!   aspect kept, between 120 px and the screen less 60 px;
//! - the controls fade in over 200 ms while the pointer is over the
//!   window: close and back-to-viewer in the title-controls corner, the
//!   volume toggle and line in the other, play / pause in the middle and a
//!   2 px progress line at the bottom that grows to 4 px under the
//!   pointer, with the elapsed and remaining times above it;
//! - a click on the picture pauses or resumes, a double click goes back to
//!   the viewer, Escape closes and Space pauses or resumes.
//!
//! The pure geometry and press rules live in `quill::media_viewer::pip`;
//! [`place`] moves the native window; [`chrome`] draws. The window owns
//! the player: closing it drops the player (the sound stops), "back to the
//! viewer" hands the same player to the owner's [`PipHooks::enlarge`].
#![deny(clippy::indexing_slicing)]

mod chrome;
mod place;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) use place::tests::take_frames;

use super::super::{PlayState, VideoPlayer};
use super::WaitingGate;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::media_viewer::pip::layout::{
    ControlsLayout, OverState, PipKey, PressAction, Presses, pip_key,
};
use quill::media_viewer::pip::panel::{
    DragStep, MOVE_ANIMATION_MS, PipPoint, PipPosition, PipRect, PipSize, Placement, RectPart,
    START_DRAG_DISTANCE, Sides, aspect, count_position, drag_step, finish_drag, place_on_screen,
    resize_part, slide_point, wayland_normalized,
};
use quill::settings::SavedPipPosition;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The message the clip belongs to (to go back to the viewer on it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PipSource {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    /// The video's size from the message, for the aspect until the first
    /// picture is known.
    pub(crate) dimensions: Option<(u32, u32)>,
}

/// What the window tells its owner. Each runs after the window's own
/// event returned (deferred), so the owner may touch the window again.
#[derive(Clone)]
pub(crate) struct PipHooks {
    /// The window closed and its player was dropped.
    pub(crate) closed: Rc<dyn Fn(&mut App)>,
    /// "Back to the viewer": the window closed and hands its player back.
    pub(crate) enlarge: Rc<dyn Fn(PipSource, VideoPlayer, &mut App)>,
    /// The volume changed (0–1).
    pub(crate) volume: Rc<dyn Fn(f32, &mut App)>,
}

/// What [`open_pip_window`] needs.
pub(crate) struct PipOpen {
    pub(crate) player: VideoPlayer,
    pub(crate) source: PipSource,
    pub(crate) hooks: PipHooks,
    /// The volume the toggle brings back after a mute.
    pub(crate) last_volume: f32,
    /// The screen to open on (the main window's), else the primary one.
    pub(crate) display: Option<DisplayId>,
}

/// An open Picture-in-Picture window.
#[derive(Clone)]
pub(crate) struct PipWindow {
    pub(crate) window: WindowHandle<Root>,
    pub(crate) view: WeakEntity<VideoPip>,
}

/// Open the window on `open.player`. `None` when no window could be made
/// (the player is dropped then).
pub(crate) fn open_pip_window(open: PipOpen, cx: &mut App) -> Option<PipWindow> {
    let PipOpen {
        player,
        source,
        hooks,
        last_volume,
        display,
    } = open;
    let screen = display
        .and_then(|id| cx.displays().into_iter().find(|d| d.id() == id))
        .or_else(|| cx.primary_display())?;
    let full = place::to_rect(screen.bounds());
    let available = place::to_rect(screen.visible_bounds());
    let ratio = picture_ratio(&player, &source);
    let screen_id = screen.uuid().ok().map(|uuid| uuid.to_string());
    let position = load_position()
        .filter(|saved| match (&saved.screen_id, &screen_id) {
            (Some(saved), Some(current)) => saved == current,
            _ => saved.position.screen == full,
        })
        .map(|saved| saved.position)
        .unwrap_or_else(|| PipPosition::default_on(full));
    let placement = place_on_screen(&position, available, ratio);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(place::to_bounds(placement.geometry))),
                // Borderless (`Qt::FramelessWindowHint`).
                titlebar: None,
                // tdesktop's panel never takes focus from the window the
                // user works in (`Qt::WindowDoesNotAcceptFocus`).
                focus: false,
                show: true,
                // An `NSPanel` at the floating level on macOS. Elsewhere a
                // plain window: GPUI makes a floating one transient for
                // the focused window, which tdesktop undoes
                // (`ClearTransientParent`); it is kept on top below.
                kind: if cfg!(target_os = "macos") {
                    WindowKind::Floating
                } else {
                    WindowKind::Normal
                },
                // Moves and resizes are the window's own (`PipPanel`).
                is_movable: false,
                // Wayland resizes go to the compositor, which needs it.
                is_resizable: cfg!(target_os = "linux"),
                is_minimizable: false,
                display_id: Some(screen.id()),
                window_min_size: Some(size(
                    px(placement.minimal.w as f32),
                    px(placement.minimal.h as f32),
                )),
                window_decorations: Some(WindowDecorations::Client),
                // GPUI caps an unfocused window at 30 frames a second;
                // this one is unfocused by design and plays at the
                // clip's own rate.
                inactive_frame_interval: None,
                ..Default::default()
            },
            move |window, cx| {
                place::configure(window);
                let view = cx.new(|cx| {
                    VideoPip::new(
                        player,
                        source,
                        hooks,
                        last_volume,
                        ratio,
                        placement,
                        window,
                        cx,
                    )
                });
                *slot_in.borrow_mut() = Some(view.downgrade());
                let close_view = view.downgrade();
                // The system's close (Alt+F4, the window menu) closes it
                // like the button.
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = close_view.update(cx, |pip, cx| pip.release_player(cx));
                    true
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok()?;
    let view = slot.borrow_mut().take()?;
    Some(PipWindow { window, view })
}

/// The aspect: the picture's when there is one, else the message's.
fn picture_ratio(player: &VideoPlayer, source: &PipSource) -> PipSize {
    let (width, height) = player
        .picture_size()
        .filter(|(w, h)| *w > 0 && *h > 0)
        .or(source.dimensions)
        .unwrap_or((1, 1));
    aspect(width, height)
}

#[cfg(not(test))]
fn load_position() -> Option<SavedPipPosition> {
    quill::settings::load_video_pip_position()
}

/// Tests never read the user's saved position.
#[cfg(test)]
fn load_position() -> Option<SavedPipPosition> {
    None
}

/// A press in progress: where it went down and, once it moved far enough,
/// the drag (tdesktop `_pressState`, `_pressPoint`, `_dragState`).
#[derive(Clone, Copy, Debug)]
struct Press {
    part: RectPart,
    cursor: PipPoint,
    start: PipRect,
    dragging: bool,
}

/// The window sliding to a snap position (`moveAnimated`).
#[derive(Clone, Copy, Debug)]
struct Slide {
    from: PipPoint,
    to: PipPoint,
    size: PipSize,
    started: Instant,
}

/// The Picture-in-Picture window's view (see the module docs).
pub(crate) struct VideoPip {
    player: Option<VideoPlayer>,
    source: PipSource,
    hooks: PipHooks,
    focus: FocusHandle,
    ratio: PipSize,
    /// The frame last set, in screen coordinates.
    frame: PipRect,
    attached: Sides,
    snapped: Sides,
    controls_on_left: bool,
    wayland: bool,
    over: OverState,
    /// The resize part under the pointer (its cursor).
    edge: RectPart,
    presses: Presses,
    press: Option<Press>,
    /// The last press moved the window: a button under the release does
    /// nothing.
    dragged: bool,
    slide: Option<Slide>,
    /// The progress line, 0–1 of the clip.
    seek: Entity<SliderState>,
    seek_preview: Option<f64>,
    paused_by_seek: bool,
    /// The volume line, 0–1.
    volume: Entity<SliderState>,
    volume_dragging: bool,
    last_volume: f32,
    waiting: WaitingGate,
    _subscriptions: Vec<Subscription>,
}

impl VideoPip {
    #[allow(clippy::too_many_arguments)]
    fn new(
        player: VideoPlayer,
        source: PipSource,
        hooks: PipHooks,
        last_volume: f32,
        ratio: PipSize,
        placement: Placement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let transport = player.transport();
        let seek = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.001)
                .default_value(position_fraction(transport.position, transport.total()))
                .label(tr(keys::Q_MEDIA_PIP_SEEK))
        });
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(transport.volume.clamp(0.0, 1.0))
                .label(tr(keys::Q_MEDIA_PIP_VOLUME))
        });
        let subscriptions = vec![
            cx.subscribe(&seek, |this, _, event: &SliderEvent, cx| {
                this.on_seek_event(event, cx)
            }),
            cx.subscribe(&volume, |this, _, event: &SliderEvent, cx| {
                this.on_volume_event(event, cx)
            }),
            cx.observe_window_bounds(window, |this, window, cx| {
                this.on_bounds_changed(window, cx)
            }),
        ];
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // The player's transport changes redraw this window only.
        player.clear_watchers();
        player.watch(cx.entity_id());
        let wayland = place::is_wayland(window);
        let (attached, snapped) = match place::own_screen(window, cx) {
            Some(screen) => count_position(placement.geometry, screen, false, wayland),
            None => (Sides::NONE, Sides::NONE),
        };
        Self {
            player: Some(player),
            source,
            hooks,
            focus,
            ratio,
            frame: placement.geometry,
            attached,
            snapped,
            controls_on_left: controls_on_left(cx),
            wayland,
            over: OverState::None,
            edge: RectPart::Center,
            presses: Presses::default(),
            press: None,
            dragged: false,
            slide: None,
            seek,
            seek_preview: None,
            paused_by_seek: false,
            volume,
            volume_dragging: false,
            last_volume: if last_volume > 0.0 { last_volume } else { 1.0 },
            waiting: WaitingGate::default(),
            _subscriptions: subscriptions,
        }
    }

    /// The player this window plays, until it closes or hands it back.
    pub(crate) fn player(&self) -> Option<&VideoPlayer> {
        self.player.as_ref()
    }

    /// The window is closing: drop the player now (the sound stops at
    /// once, not when GPUI releases the view) and tell the owner.
    fn release_player(&mut self, cx: &mut Context<Self>) {
        if self.player.take().is_some() {
            let closed = self.hooks.closed.clone();
            cx.defer(move |cx| closed(cx));
        }
    }

    /// Close the window, dropping the player (tdesktop `handleClose`).
    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release_player(cx);
        window.remove_window();
    }

    /// Back to the viewer with the same player (`closeAndContinue`).
    pub(crate) fn enlarge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(player) = self.player.take() else {
            return;
        };
        player.clear_watchers();
        let enlarge = self.hooks.enlarge.clone();
        let source = self.source;
        cx.defer(move |cx| enlarge(source, player, cx));
        window.remove_window();
    }

    /// `playbackPauseResume`: a failed clip closes, an ended one starts
    /// over, otherwise pause or resume.
    pub(crate) fn toggle_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(player) = self.player.clone() else {
            return;
        };
        match player.state() {
            PlayState::Failed => self.close(window, cx),
            _ if player.wants_play() => player.pause(),
            _ => player.play(),
        }
        cx.notify();
    }

    /// `volumeToggled`: mute, or back to the last volume above zero.
    fn toggle_volume(&mut self, cx: &mut Context<Self>) {
        let current = self
            .player
            .as_ref()
            .map_or(0.0, |player| player.transport().volume);
        let next = if current > 0.0 { 0.0 } else { self.last_volume };
        self.set_volume(next, cx);
    }

    /// `volumeChanged`: the player's volume, and the app's video volume.
    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if volume > 0.0 {
            self.last_volume = volume;
        }
        if let Some(player) = &self.player {
            player.set_volume(volume);
        }
        let hook = self.hooks.volume.clone();
        cx.defer(move |cx| hook(volume, cx));
        cx.notify();
    }

    /// A button's press went down and up on it; a press that moved the
    /// window does nothing (`handleMouseRelease` with `dragging()`).
    fn button(&mut self, action: PressAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.dragged {
            return;
        }
        self.run(action, window, cx);
    }

    fn run(&mut self, action: PressAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            PressAction::Close => self.close(window, cx),
            PressAction::Enlarge => self.enlarge(window, cx),
            PressAction::ToggleVolume => self.toggle_volume(cx),
            PressAction::TogglePause => self.toggle_pause(window, cx),
            PressAction::UndoToggleAndEnlarge => {
                // Un-click the first click, then go back.
                self.toggle_pause(window, cx);
                self.enlarge(window, cx);
            }
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = pip_key(&event.keystroke.key) else {
            return;
        };
        cx.stop_propagation();
        match key {
            PipKey::Close => self.close(window, cx),
            PipKey::TogglePause if !event.is_held => self.toggle_pause(window, cx),
            PipKey::TogglePause => {}
        }
    }

    /// `seekProgress` / `seekFinish`: dragging the line pauses a playing
    /// clip and previews the time; the release seeks and resumes it.
    fn on_seek_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        let Some(player) = self.player.clone() else {
            return;
        };
        let total = player.transport().total();
        match event {
            SliderEvent::Change(value) => {
                if self.seek_preview.is_none() && player.wants_play() {
                    self.paused_by_seek = true;
                    player.pause();
                }
                self.seek_preview = Some(f64::from(value.end()));
            }
            SliderEvent::Release(value) => {
                self.seek_preview = None;
                player.seek(f64::from(value.end()).clamp(0.0, 1.0) * total.max(0.0));
                if std::mem::take(&mut self.paused_by_seek) {
                    player.play();
                }
            }
        }
        cx.notify();
    }

    /// `volumeControllerUpdate`: the volume follows the line live.
    fn on_volume_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        let (value, dragging) = match event {
            SliderEvent::Change(value) => (value.end(), true),
            SliderEvent::Release(value) => (value.end(), false),
        };
        self.volume_dragging = dragging;
        self.set_volume(value, cx);
    }

    /// Wayland: the compositor resized the window; bring it back to the
    /// aspect (`handleWaylandResize`).
    fn on_bounds_changed(&mut self, window: &mut Window, _: &mut Context<Self>) {
        if !self.wayland {
            return;
        }
        let current = place::to_rect(window.bounds()).size();
        let normalized = wayland_normalized(current, self.ratio);
        if normalized != current {
            window.resize(size(px(normalized.w as f32), px(normalized.h as f32)));
        }
    }

    fn layout(&self, window: &Window) -> ControlsLayout {
        let viewport = window.viewport_size();
        let inner = place::to_rect(Bounds::new(Point::default(), viewport));
        ControlsLayout::new(PipRect::new(0, 0, inner.w, inner.h), self.controls_on_left)
    }

    fn set_over(&mut self, over: OverState, cx: &mut Context<Self>) {
        if self.over != over {
            self.over = over;
            cx.notify();
        }
    }

    /// `handleMousePress` (both the window's and the panel's).
    fn on_mouse_down(
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
        let over = self.layout(window).state_at(local);
        self.set_over(over, cx);
        if event.click_count >= 2 {
            // `handleDoubleClick` takes the second press.
            if let Some(action) = self.presses.double_click(over) {
                self.run(action, window, cx);
            }
            return;
        }
        if self.slide.is_none() {
            self.frame = place::frame(window);
        }
        self.presses.press(over);
        self.dragged = false;
        self.press = Some(Press {
            part: resize_part(local, self.frame.size(), self.attached.with(self.snapped)),
            cursor: place::cursor(window),
            start: self.frame,
            dragging: false,
        });
    }

    /// `handleMouseMove`: the controls' state, the resize cursor, and the
    /// drag once the press moved far enough.
    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let local = local_point(event.position);
        let over = self.layout(window).state_at(local);
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
    fn on_mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        self.finish_press(local_point(event.position), window, cx);
    }

    fn finish_press(&mut self, local: PipPoint, window: &mut Window, cx: &mut Context<Self>) {
        let over = self.layout(window).state_at(local);
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
                self.slide = None;
                self.settle(window, cx);
            }
        }
    }

    /// Start sliding the window's top-left to `to` (`moveAnimated`).
    fn slide_to(&mut self, to: PipPoint, cx: &mut Context<Self>) {
        if self.slide.is_some_and(|slide| slide.to == to) {
            return;
        }
        let from = self.slide.map_or(self.frame.origin(), |slide| {
            self.current_slide_point(slide, cx)
        });
        self.slide = Some(Slide {
            from,
            to,
            size: self.frame.size(),
            started: cx.background_executor().now(),
        });
        cx.notify();
    }

    fn current_slide_point(&self, slide: Slide, cx: &App) -> PipPoint {
        slide_point(slide.from, slide.to, slide_progress(slide, cx))
    }

    /// One frame of the slide (`updatePositionAnimated`); `true` while it
    /// still runs.
    fn step_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
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
        save_position(saved, cx);
    }

    /// The clip's aspect became known or changed (`setAspectRatio`):
    /// re-place the window keeping its sides.
    fn sync_ratio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let ratio = picture_ratio(player, &self.source);
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

    /// Keep the lines on the transport while they are not dragged.
    fn sync_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(transport) = self.player.as_ref().map(VideoPlayer::transport) else {
            return;
        };
        if self.seek_preview.is_none() {
            // The drawn line reads the transport itself; the slider's value
            // (where a drag starts, what a screen reader says) follows in
            // 1 % steps, so a playing clip does not redraw the window on
            // every display frame through the slider's notify.
            let value = position_fraction(transport.position, transport.total());
            let current = self.seek.read(cx).value().end();
            if (current - value).abs() >= SEEK_SYNC_STEP || (value == 0.0 && current != 0.0) {
                self.seek
                    .update(cx, |state, cx| state.set_value(value, window, cx));
            }
        }
        if !self.volume_dragging {
            let value = transport.volume.clamp(0.0, 1.0);
            if self.volume.read(cx).value() != SliderValue::Single(value) {
                self.volume
                    .update(cx, |state, cx| state.set_value(value, window, cx));
            }
        }
    }
}

/// How far the progress slider's value may lag the transport (a fraction
/// of the clip).
const SEEK_SYNC_STEP: f32 = 0.01;

/// Close and enlarge sit where the window title controls are: on the left
/// on macOS, else on the side the system puts its close button.
fn controls_on_left(cx: &App) -> bool {
    if cfg!(target_os = "macos") {
        return true;
    }
    cx.button_layout()
        .is_some_and(|layout| layout.left.contains(&Some(WindowButton::Close)))
}

fn local_point(position: Point<Pixels>) -> PipPoint {
    place::to_rect(Bounds::new(position, Size::default())).origin()
}

fn position_fraction(position: f64, total: f64) -> f32 {
    if total.is_nan() || total <= 0.0 || !position.is_finite() {
        return 0.0;
    }
    (position / total).clamp(0.0, 1.0) as f32
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

/// Save off the UI thread (tdesktop `saveSettingsDelayed`).
#[cfg(not(test))]
fn save_position(saved: SavedPipPosition, cx: &mut App) {
    cx.background_executor()
        .spawn(async move {
            let _ = quill::settings::save_video_pip_position(&saved);
        })
        .detach();
}

/// Tests never write the user's settings.
#[cfg(test)]
fn save_position(_: SavedPipPosition, _: &mut App) {}
