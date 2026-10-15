//! The viewer's video controller (`docs/decisions/video-player-controls.md`):
//! one 480 × 72 dark panel with the scrubber and its time labels on top
//! and the buttons under it: volume on the left, play / pause in the
//! middle, speed, quality, Picture-in-Picture and full screen on the
//! right. Geometry is tdesktop's `media_view.style` ([`metrics`]); colors
//! are kit tokens and the viewer's dark overlay.
//!
//! [`VideoController`] is a `RenderOnce` built at render time from the
//! player's [`Transport`], the slider entities in [`VideoControlsState`]
//! (on `ViewerUi`) and callbacks into `QuillApp`, so it never holds state
//! of its own.
#![deny(clippy::indexing_slicing)]

mod metrics;
mod overlay;
mod scrubber;
mod speed;

#[cfg(test)]
mod tests;

pub(crate) use overlay::{PictureOverlay, PictureState};
pub(crate) use scrubber::{HoverHandler, VideoScrubber, elapsed_label, remaining_label};
pub(crate) use speed::{SpeedMenu, speed_trigger};

use super::Transport;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use metrics::{
    BUTTON_ROW_TOP, BUTTONS_RIGHT, BUTTONS_SKIP, PANEL_HEIGHT, PANEL_OPACITY, PANEL_WIDTH,
    PLAY_BUTTON, PROGRESS_INSET, PROGRESS_SKIP, SCRUBBER_ROW_TOP, SIDE_BUTTON, VOLUME_LEFT,
    VOLUME_SKIP, VOLUME_WIDTH,
};
use quill::media_viewer::{BoostState, waiting_wait};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The controller's per-viewer state on `ViewerUi`: the slider entities,
/// what a drag previews, the pointer over the scrubber and the Space-hold
/// boost.
#[derive(Default)]
pub(crate) struct VideoControlsState {
    /// The seek slider, in seconds (`0..=duration`). Created when a clip
    /// starts, cleared when it stops.
    pub(crate) seek: Option<Entity<SliderState>>,
    /// The dragged position while the seek thumb is held.
    pub(crate) seek_preview: Option<f64>,
    /// The volume slider, 0–100.
    pub(crate) volume: Option<Entity<SliderState>>,
    /// The volume thumb is held.
    pub(crate) volume_dragging: bool,
    /// The speed popover's slider (created on first use, kept).
    pub(crate) speed: Option<Entity<SliderState>>,
    /// The speed shown while its slider is dragged.
    pub(crate) speed_preview: Option<f64>,
    /// Screenshot demo: open the speed popover on the first frame.
    pub(crate) demo_speed_open: bool,
    /// The pointer's fraction over the scrubber while it hovers.
    pub(crate) hover: Option<f32>,
    /// Space held to play at 2×.
    pub(crate) boost: BoostState,
    /// The hold check 200 ms after Space went down.
    pub(crate) boost_timer: Option<Task<()>>,
    /// The view the controller was last drawn in (the cached viewer slice,
    /// or a chat window), so a hover redraws only that view.
    pub(crate) panel_view: Option<EntityId>,
    /// Focused while Space is held with nothing else focused, so the
    /// key-up reaches the window's listeners.
    pub(crate) focus: Option<FocusHandle>,
    /// The waiting spinner's delay, for chat windows (the main window's
    /// video layer keeps its own).
    pub(crate) waiting: WaitingGate,
}

impl VideoControlsState {
    /// A slider is being dragged: the controls must not hide.
    pub(crate) fn dragging(&self) -> bool {
        self.seek_preview.is_some() || self.volume_dragging || self.speed_preview.is_some()
    }

    /// The clip stopped: drop its sliders and any pending gesture. Returns
    /// `true` when a boost was on.
    pub(crate) fn clear_clip(&mut self) -> bool {
        self.seek = None;
        self.seek_preview = None;
        self.volume = None;
        self.volume_dragging = false;
        self.hover = None;
        self.boost_timer = None;
        self.waiting = WaitingGate::default();
        self.boost.cancel()
    }
}

/// The delay before a stall shows its spinner (tdesktop
/// `kWaitingShowDelay`): the gate remembers when waiting began and the
/// timer that redraws when the delay is up.
#[derive(Default)]
pub(crate) struct WaitingGate {
    since: Option<Instant>,
    timer: Option<Task<()>>,
}

/// What [`WaitingGate::observe`] decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WaitingCheck {
    /// Not waiting, or the delay is up and the spinner shows.
    Settled { shown: bool },
    /// Waiting, not long enough yet: redraw after this long.
    Pending(Duration),
}

impl WaitingGate {
    /// Feed whether the clip waits now. A pending delay that already has
    /// its timer reports `Settled { shown: false }`.
    pub(crate) fn observe(&mut self, waiting: bool, now: Instant) -> WaitingCheck {
        if !waiting {
            self.since = None;
            self.timer = None;
            return WaitingCheck::Settled { shown: false };
        }
        let since = *self.since.get_or_insert(now);
        match waiting_wait(since, now) {
            None => {
                self.timer = None;
                WaitingCheck::Settled { shown: true }
            }
            Some(_) if self.timer.is_some() => WaitingCheck::Settled { shown: false },
            Some(wait) => WaitingCheck::Pending(wait),
        }
    }

    /// Keep the timer that redraws once the delay is up.
    pub(crate) fn arm(&mut self, timer: Task<()>) {
        self.timer = Some(timer);
    }
}

/// The seek slider's range: the clip length, at least one second (a
/// zero-width range has no drag).
pub(crate) fn seek_range(total: f64) -> f64 {
    total.max(1.0)
}

/// The seek slider was built for another length: its range has no setter,
/// so it is made again. Compared against [`seek_range`], so a clip under a
/// second keeps its one slider.
pub(crate) fn seek_range_stale(max: f32, total: f64) -> bool {
    total > 0.0 && (f64::from(max) - seek_range(total)).abs() > 0.5
}

/// The volume icon's tier (tdesktop `VolumeController`: off at 0, the
/// small speaker under half, the full one above).
pub(crate) fn volume_icon(level: f32) -> IconName {
    if level.is_nan() || level <= 0.0 {
        IconName::VolumeX
    } else if level < 0.5 {
        IconName::Volume1
    } else {
        IconName::Volume2
    }
}

/// The panel's background: black at `mediaviewSaveMsgBg`'s 0.75 in both
/// themes (the viewer is dark in both, as in tdesktop).
pub(crate) fn panel_background() -> Hsla {
    gpui_kit::black().opacity(PANEL_OPACITY)
}

/// The controller's buttons: white on the dark panel in both themes, with
/// a white wash on hover and press (a ghost button's light-theme wash
/// would leave white icons on light gray).
pub(crate) fn overlay_button(cx: &App) -> ButtonCustomVariant {
    let white = gpui_kit::white();
    ButtonCustomVariant::new(cx)
        .color(gpui_kit::transparent_black())
        .foreground(white)
        .hover(white.opacity(0.14))
        .active(white.opacity(0.22))
}

/// A press handler from the controller into `QuillApp`.
pub(crate) type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// What the controller's buttons do.
pub(crate) struct ControllerActions {
    pub(crate) toggle_play: Handler,
    pub(crate) toggle_mute: Handler,
    pub(crate) fullscreen: Handler,
    /// `None` hides the button (no Picture-in-Picture backend here).
    pub(crate) pip: Option<Handler>,
    pub(crate) hover: HoverHandler,
}

/// The viewer's controller panel (see the module docs).
#[derive(IntoElement)]
pub(crate) struct VideoController {
    row_id: u64,
    transport: Transport,
    seek: Option<Entity<SliderState>>,
    seek_preview: Option<f64>,
    hover: Option<f32>,
    volume: Option<Entity<SliderState>>,
    level: f32,
    fullscreen: bool,
    speed_dial: Option<AnyElement>,
    quality: Option<AnyElement>,
    actions: ControllerActions,
}

impl VideoController {
    /// `row_id` keys the element ids (the viewed message).
    pub(crate) fn new(
        row_id: u64,
        transport: &Transport,
        state: &VideoControlsState,
        actions: ControllerActions,
    ) -> Self {
        Self {
            row_id,
            transport: transport.clone(),
            seek: state.seek.clone(),
            seek_preview: state.seek_preview,
            hover: state.hover,
            volume: state.volume.clone(),
            level: transport.volume,
            fullscreen: false,
            speed_dial: None,
            quality: None,
            actions,
        }
    }

    /// The volume the icon tier shows (0–1).
    pub(crate) fn level(mut self, level: f32) -> Self {
        self.level = level;
        self
    }

    /// The viewer is in video full screen.
    pub(crate) fn fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    /// The speed popover (its trigger is the speed button).
    pub(crate) fn speed_dial(mut self, speed_dial: AnyElement) -> Self {
        self.speed_dial = Some(speed_dial);
        self
    }

    /// The quality picker, for clips with alternative qualities.
    pub(crate) fn quality(mut self, quality: Option<AnyElement>) -> Self {
        self.quality = quality;
        self
    }
}

/// A kit tooltip naming the button and its key.
fn tooltip_with_key(
    label: &'static str,
    key: &'static str,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |window, cx| {
        Tooltip::new(label)
            .key_binding(Keystroke::parse(key).ok().map(Kbd::new))
            .build(window, cx)
    }
}

impl RenderOnce for VideoController {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let row_id = self.row_id;
        let variant = overlay_button(cx);
        let transport = &self.transport;
        let total = transport.total();
        let playing = transport.is_playing();
        let elapsed = self.seek_preview.unwrap_or(transport.position);
        let white = gpui_kit::white();
        let label_color = white.opacity(0.9);
        let time = |text: String| {
            div()
                .flex_none()
                .text_xs()
                .font_semibold()
                .text_color(label_color)
                .child(text)
        };
        let actions = self.actions;
        let scrubber = self.seek.as_ref().map(|seek| {
            VideoScrubber::new(
                ("viewer-scrubber", row_id),
                seek,
                total,
                actions.hover.clone(),
            )
            .buffered(&transport.buffered)
            .hover(self.hover)
            .preview(self.seek_preview)
        });
        let scrubber_row = div()
            .absolute()
            .top(SCRUBBER_ROW_TOP)
            .left(PROGRESS_INSET)
            .right(PROGRESS_INSET)
            .flex()
            .items_center()
            .gap(PROGRESS_SKIP)
            .child(time(elapsed_label(elapsed)))
            .child(div().flex_1().min_w_0().flex().children(scrubber))
            .child(time(remaining_label(elapsed, total)));

        let toggle_play = actions.toggle_play.clone();
        let (play_icon, play_label) = if playing {
            (IconName::Pause, "Pause")
        } else {
            (IconName::Play, "Play")
        };
        let play = div()
            .id(("viewer-play-tip", row_id))
            .tooltip(tooltip_with_key(play_label, "space"))
            .child(
                Button::new(("media-viewer-play", row_id))
                    .icon(play_icon)
                    .custom(variant)
                    .with_size(PLAY_BUTTON)
                    .accessibility_label(play_label)
                    .on_click(move |_, window, cx| toggle_play(window, cx)),
            );

        let toggle_mute = actions.toggle_mute.clone();
        let muted = self.level <= 0.0;
        let mute_label = if muted { "Unmute" } else { "Mute" };
        let volume = div()
            .flex()
            .items_center()
            .gap(VOLUME_SKIP)
            .child(
                Button::new(("media-viewer-mute", row_id))
                    .icon(volume_icon(self.level))
                    .custom(variant)
                    .with_size(SIDE_BUTTON)
                    .tooltip(mute_label)
                    .accessibility_label(mute_label)
                    .on_click(move |_, window, cx| toggle_mute(window, cx)),
            )
            .when_some(self.volume, |this, slider| {
                this.child(
                    div().w(VOLUME_WIDTH).child(
                        Slider::new(&slider)
                            .bg(crate::ui::chat_theme::accent())
                            .text_color(crate::ui::chat_theme::text_on_fill()),
                    ),
                )
            });

        let fullscreen = actions.fullscreen.clone();
        let (fs_icon, fs_label, fs_key) = if self.fullscreen {
            (IconName::Minimize, "Exit full screen", "escape")
        } else {
            (IconName::Maximize, "Full screen", "alt-enter")
        };
        let pip = actions.pip.clone().map(|pip| {
            Button::new(("media-viewer-pip", row_id))
                .icon(IconName::PictureInPicture2)
                .custom(variant)
                .with_size(SIDE_BUTTON)
                .tooltip("Picture-in-Picture")
                .accessibility_label("Picture-in-Picture")
                .on_click(move |_, window, cx| pip(window, cx))
        });
        let right = div()
            .flex()
            .items_center()
            .gap(BUTTONS_SKIP)
            .children(self.speed_dial)
            .children(self.quality)
            .children(pip)
            .child(
                div()
                    .id(("viewer-fullscreen-tip", row_id))
                    .tooltip(tooltip_with_key(fs_label, fs_key))
                    .child(
                        Button::new(("media-viewer-fullscreen", row_id))
                            .icon(fs_icon)
                            .custom(variant)
                            .with_size(SIDE_BUTTON)
                            .accessibility_label(fs_label)
                            .on_click(move |_, window, cx| fullscreen(window, cx)),
                    ),
            );
        let button_row = div()
            .absolute()
            .top(BUTTON_ROW_TOP)
            .left_0()
            .right_0()
            .h(PLAY_BUTTON)
            .child(
                div()
                    .absolute()
                    .left(VOLUME_LEFT)
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(volume),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .justify_center()
                    .child(play),
            )
            .child(
                div()
                    .absolute()
                    .right(BUTTONS_RIGHT)
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(right),
            );
        div()
            .id(("media-viewer-player", row_id))
            .test_support()
            .role(Role::Group)
            .aria_label("Video controls")
            .relative()
            .w(PANEL_WIDTH)
            .max_w_full()
            .h(PANEL_HEIGHT)
            .rounded(cx.theme().radius_lg)
            .bg(panel_background())
            .child(scrubber_row)
            .child(button_row)
    }
}
