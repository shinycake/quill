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
//! [`open`] opens the window, [`input`] handles the pointer, [`place`]
//! moves the native window and [`chrome`] draws. The window owns the
//! player: closing it drops the player (the sound stops); "back to the
//! viewer" asks the owner to reopen the viewer, which takes the same
//! player and then [`VideoPip::detach`]es the window.
#![deny(clippy::indexing_slicing)]

mod chrome;
mod input;
mod open;
mod place;

pub(crate) use open::open_pip_window;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) use place::tests::take_frames;

use super::super::{PlayState, VideoPlayer};
use super::WaitingGate;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::media_viewer::pip::layout::{OverState, PipKey, PressAction, Presses, pip_key};
use quill::media_viewer::pip::panel::{
    PipRect, PipSize, Placement, RectPart, Sides, count_position,
};
use std::rc::Rc;
use std::time::Instant;

/// What the window tells its owner. Each runs after the window's own
/// event returned (deferred), so the owner may touch the window again.
#[derive(Clone)]
pub(crate) struct PipHooks {
    /// The window closed and its player was dropped.
    pub(crate) closed: Rc<dyn Fn(&mut App)>,
    /// "Back to the viewer": reopen the viewer on the clip; it takes the
    /// window's player (and detaches the window) when it can.
    pub(crate) enlarge: Rc<dyn Fn(&mut App)>,
    /// The volume changed (0–1).
    pub(crate) volume: Rc<dyn Fn(f32, &mut App)>,
}

/// What [`open_pip_window`] needs.
pub(crate) struct PipOpen {
    pub(crate) player: VideoPlayer,
    /// The video's size from the message, for the aspect until the first
    /// picture is known.
    pub(crate) dimensions: Option<(u32, u32)>,
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

/// A press in progress: where it went down and, once it moved far enough,
/// the drag (tdesktop `_pressState`, `_pressPoint`, `_dragState`,
/// `_dragStartGeometry`).
#[derive(Clone, Copy, Debug)]
struct Press {
    part: RectPart,
    cursor: quill::media_viewer::pip::panel::PipPoint,
    start: PipRect,
    dragging: bool,
}

/// The window sliding to a snap position (`moveAnimated`).
#[derive(Clone, Copy, Debug)]
struct Slide {
    from: quill::media_viewer::pip::panel::PipPoint,
    to: quill::media_viewer::pip::panel::PipPoint,
    size: PipSize,
    started: Instant,
}

/// The Picture-in-Picture window's view (see the module docs).
pub(crate) struct VideoPip {
    player: Option<VideoPlayer>,
    dimensions: Option<(u32, u32)>,
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
    /// The value the window last gave the line (anything else came from
    /// the keyboard or a screen reader).
    seek_synced: f32,
    seek_preview: Option<f64>,
    paused_by_seek: bool,
    /// The volume line, 0–1.
    volume: Entity<SliderState>,
    volume_synced: f32,
    volume_dragging: bool,
    last_volume: f32,
    waiting: WaitingGate,
    _subscriptions: Vec<Subscription>,
}

impl VideoPip {
    #[allow(clippy::too_many_arguments)]
    fn new(
        player: VideoPlayer,
        dimensions: Option<(u32, u32)>,
        hooks: PipHooks,
        last_volume: f32,
        ratio: PipSize,
        placement: Placement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let transport = player.transport();
        let seek_value = position_fraction(transport.position, transport.total());
        let volume_value = transport.volume.clamp(0.0, 1.0);
        let seek = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.001)
                .default_value(seek_value)
                .label(tr(keys::Q_MEDIA_PIP_SEEK))
        });
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(volume_value)
                .label(tr(keys::Q_MEDIA_PIP_VOLUME))
        });
        let subscriptions = vec![
            cx.subscribe(&seek, |this, _, event: &SliderEvent, cx| {
                this.on_seek_event(event, cx)
            }),
            cx.subscribe(&volume, |this, _, event: &SliderEvent, cx| {
                this.on_volume_event(event, cx)
            }),
            // Keyboard and screen-reader steps set the value without an
            // event.
            cx.observe(&seek, |this, seek, cx| {
                let value = seek.read(cx).value().end();
                this.on_seek_adjusted(value, cx);
            }),
            cx.observe(&volume, |this, volume, cx| {
                let value = volume.read(cx).value().end();
                this.on_volume_adjusted(value, cx);
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
            dimensions,
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
            seek_synced: seek_value,
            seek_preview: None,
            paused_by_seek: false,
            volume,
            volume_synced: volume_value,
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

    /// The viewer took the player: let go of it (the sound goes on there)
    /// and close the window.
    pub(crate) fn detach(&mut self, window: &mut Window, _: &mut Context<Self>) {
        self.player = None;
        window.remove_window();
    }

    /// Back to the viewer with the same player (`closeAndContinue`). The
    /// owner reopens the viewer and detaches this window once the viewer
    /// holds the player; if it can't, the window keeps playing.
    pub(crate) fn enlarge(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.player.is_none() {
            return;
        }
        let enlarge = self.hooks.enlarge.clone();
        cx.defer(move |cx| enlarge(cx));
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
    /// clip and previews the time; the release seeks and resumes it (an
    /// ended clip plays from there: `_startPaused = !_pausedBySeek &&
    /// !finished`).
    fn on_seek_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        let Some(player) = self.player.clone() else {
            return;
        };
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
                self.seek_synced = value.end();
                let resume =
                    std::mem::take(&mut self.paused_by_seek) || player.state() == PlayState::Ended;
                self.seek_to(&player, value.end());
                if resume {
                    player.play();
                }
            }
        }
        cx.notify();
    }

    fn seek_to(&self, player: &VideoPlayer, fraction: f32) {
        let total = player.transport().total().max(0.0);
        player.seek(f64::from(fraction).clamp(0.0, 1.0) * total);
    }

    /// The line was stepped from the keyboard or a screen reader.
    fn on_seek_adjusted(&mut self, value: f32, cx: &mut Context<Self>) {
        if self.seek_preview.is_some() || (value - self.seek_synced).abs() < f32::EPSILON {
            return;
        }
        self.seek_synced = value;
        if let Some(player) = self.player.clone() {
            self.seek_to(&player, value);
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
        self.volume_synced = value;
        self.set_volume(value, cx);
    }

    fn on_volume_adjusted(&mut self, value: f32, cx: &mut Context<Self>) {
        if self.volume_dragging || (value - self.volume_synced).abs() < f32::EPSILON {
            return;
        }
        self.volume_synced = value;
        self.set_volume(value, cx);
    }

    /// The clip's aspect: the picture's when there is one, else the
    /// message's.
    fn picture_ratio(&self) -> Option<PipSize> {
        let player = self.player.as_ref()?;
        Some(open::picture_ratio(player, self.dimensions))
    }

    /// Keep the lines on the transport while they are not dragged.
    fn sync_lines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(transport) = self.player.as_ref().map(VideoPlayer::transport) else {
            return;
        };
        if self.seek_preview.is_none() {
            // The drawn line reads the transport itself; the slider's value
            // (where a drag or a keyboard step starts, what a screen reader
            // says) follows in 1 % steps, so a playing clip does not redraw
            // the window on every display frame through the slider's
            // notify.
            let value = position_fraction(transport.position, transport.total());
            let current = self.seek.read(cx).value().end();
            if (current - value).abs() >= SEEK_SYNC_STEP || (value == 0.0 && current != 0.0) {
                self.seek_synced = value;
                self.seek
                    .update(cx, |state, cx| state.set_value(value, window, cx));
            }
        }
        if !self.volume_dragging {
            let value = transport.volume.clamp(0.0, 1.0);
            if self.volume.read(cx).value() != SliderValue::Single(value) {
                self.volume_synced = value;
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

fn position_fraction(position: f64, total: f64) -> f32 {
    if total.is_nan() || total <= 0.0 || !position.is_finite() {
        return 0.0;
    }
    (position / total).clamp(0.0, 1.0) as f32
}
