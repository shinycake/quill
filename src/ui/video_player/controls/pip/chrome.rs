//! Drawing Picture-in-Picture (tdesktop `Pip::paint`, `paintButtons`,
//! `paintPlayback`, `paintVolumeController`): the picture, the dark wash
//! and the controls fading in together while the pointer is over the
//! window, each control brightening while it is the one under the
//! pointer.
//!
//! The buttons are kit `Button`s (keyboard and screen readers reach them)
//! in a custom variant with no background, as tdesktop only brightens the
//! icon. The two lines are composed from the kit's base slider parts like
//! the viewer's scrubber, because the packaged `Slider` has no 2 px line
//! that grows on hover and no thumbless look.

use super::super::super::{PlayState, video_frame};
use super::super::{elapsed_label, remaining_label, volume_icon};
use super::{VideoPip, place};
use gpui_kit::assets::IconName;
use gpui_kit::base::{Slider as BaseSlider, SliderIndicator, SliderTrack, Transition, transition};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::media_viewer::pip::layout::{
    Control, FADE_MS, OverState, PLAYBACK_FONT, PLAYBACK_SKIP, PLAYBACK_TEXT_SKIP, PLAYBACK_WIDE,
    PressAction, line_height,
};
use quill::media_viewer::pip::panel::{PipRect, RectPart};
use std::time::Duration;

/// `mediaviewPipControlsFg`: the controls at rest (white at 0xd9).
const CONTROLS_FG: f32 = 0.85;
/// `mediaviewPipPlaybackInactive`: the line's unplayed part (white at 0x26).
const PLAYBACK_INACTIVE: f32 = 0.15;
/// `radialBg`: the wash over the picture while the controls show, and
/// the waiting indicator's disc (black at 0x56).
const WASH: f32 = 0.34;
/// `radialSize`: the waiting indicator's disc.
const RADIAL: Pixels = px(50.);
/// The kit spinner inside it (`radialSize` less `radialLine` on each side,
/// rounded to the kit's sizes).
const SPINNER: Pixels = px(32.);

fn fade() -> Transition {
    // tdesktop fades both linearly (`anim::linear`).
    Transition::new(Duration::from_millis(FADE_MS)).ease(|t| t)
}

/// `anim::color(mediaviewPipControlsFg, ..FgOver, active)`.
fn controls_fg(active: f32) -> Hsla {
    gpui_kit::white().opacity(CONTROLS_FG + (1.0 - CONTROLS_FG) * active)
}

fn place_at(rect: PipRect) -> Div {
    div()
        .absolute()
        .left(px(rect.x as f32))
        .top(px(rect.y as f32))
        .w(px(rect.w as f32))
        .h(px(rect.h as f32))
}

fn cursor_for(edge: RectPart) -> CursorStyle {
    match edge {
        RectPart::Center => CursorStyle::PointingHand,
        RectPart::TopLeft | RectPart::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        RectPart::TopRight | RectPart::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
        RectPart::Left | RectPart::Right => CursorStyle::ResizeLeftRight,
        RectPart::Top | RectPart::Bottom => CursorStyle::ResizeUpDown,
    }
}

impl VideoPip {
    /// One fade per control, toward 1 while it is the control shown as
    /// active (`activeValue`).
    fn active(
        &self,
        name: &'static str,
        state: OverState,
        window: &mut Window,
        cx: &mut App,
    ) -> f32 {
        let shown = self.presses.pressed().unwrap_or(self.over).shown();
        let target = if shown == state.shown() { 1.0_f32 } else { 0.0 };
        transition(("pip-active", name), target, fade(), window, cx)
    }

    /// The waiting indicator shows after tdesktop's delay; a redraw is
    /// armed for when it is up.
    fn waiting_shown(&mut self, waiting: bool, cx: &mut Context<Self>) -> bool {
        use super::super::WaitingCheck;
        match self
            .waiting
            .observe(waiting, cx.background_executor().now())
        {
            WaitingCheck::Settled { shown } => shown,
            WaitingCheck::Pending(wait) => {
                let timer = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                });
                self.waiting.arm(timer);
                false
            }
        }
    }

    pub(super) fn icon_button(
        &self,
        id: &'static str,
        control: Control,
        icon: IconName,
        label: &'static str,
        action: PressAction,
        active: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let variant = ButtonCustomVariant::new(cx)
            .color(gpui_kit::transparent_black())
            .hover(gpui_kit::transparent_black())
            .active(gpui_kit::transparent_black())
            .foreground(controls_fg(active));
        place_at(control.area).child(
            Button::new(id)
                .custom(variant)
                .with_size(px(control.area.w as f32))
                .icon(Icon::new(icon).with_size(px(control.icon.w as f32)))
                .accessibility_label(label)
                .on_click(cx.listener(move |this, _, window, cx| this.button(action, window, cx))),
        )
    }

    /// A 2 px line growing to `full` px while active, filled to
    /// `fraction` (`paintProgressBar`), over the base slider parts so
    /// presses and drags on its whole area set `state`.
    fn line(
        &self,
        control: Control,
        state: &Entity<SliderState>,
        fraction: f32,
        active: f32,
    ) -> Div {
        let height = line_height(active, PLAYBACK_WIDE);
        let inset = PLAYBACK_SKIP as f32;
        // `mediaviewPipControlsFg` (0xd9) toward `mediaviewPipPlaybackActive`
        // (0xda): the same white at 85 %.
        let done = gpui_kit::white().opacity(CONTROLS_FG);
        place_at(control.area).child(
            BaseSlider::new(state).size_full().child(
                SliderTrack::new(state)
                    .relative()
                    .h_full()
                    .mx(px(inset))
                    .cursor_pointer()
                    .child(
                        SliderIndicator::new(state)
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(px(inset))
                            .h(px(height))
                            .rounded_full()
                            .bg(gpui_kit::white().opacity(PLAYBACK_INACTIVE))
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left_0()
                                    .w(relative(fraction.clamp(0.0, 1.0)))
                                    .rounded_full()
                                    .bg(done),
                            ),
                    ),
            ),
        )
    }
}

impl Render for VideoPip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(player) = self.player.clone() else {
            return div().size_full().bg(gpui_kit::black()).into_any_element();
        };
        // A clip that fails closes the window (`handleStreamingError`).
        // A live picture that ended (the stream, or the call) closes too.
        let ended = self.live.is_some() && player.state() == PlayState::Ended;
        if player.state() == PlayState::Failed || ended {
            self.close(window, cx);
            return div().size_full().bg(gpui_kit::black()).into_any_element();
        }
        self.sync_ratio(window, cx);
        if self.step_slide(window, cx) {
            window.request_animation_frame();
        }
        if self.live.is_none() {
            self.sync_lines(window, cx);
        }
        // Transport changes (an end, a stall) redraw this window.
        player.watch(cx.entity_id());
        let transport = player.transport();
        let layout = self.layout(window);
        let shown = transition(
            "pip-controls",
            if self.over == OverState::None {
                0.0_f32
            } else {
                1.0
            },
            fade(),
            window,
            cx,
        );
        let waiting = self.waiting_shown(
            matches!(transport.state, PlayState::Opening | PlayState::Buffering),
            cx,
        );
        let close_active = self.active("close", OverState::Close, window, cx);
        let enlarge_active = self.active("enlarge", OverState::Enlarge, window, cx);
        let volume_active = self.active("volume", OverState::VolumeToggle, window, cx);
        let play_active = self.active("play", OverState::Other, window, cx);
        let playback_active = self.active("playback", OverState::Playback, window, cx);

        let weak = cx.entity().downgrade();
        let input = canvas(
            |_, _, _| {},
            move |_, (), window, _| {
                let down = weak.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = down.update(cx, |this, cx| this.on_mouse_down(event, window, cx));
                    }
                });
                let moved = weak.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moved.update(cx, |this, cx| this.on_mouse_move(event, window, cx));
                    }
                });
                let up = weak.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = up.update(cx, |this, cx| this.on_mouse_up(event, window, cx));
                    }
                });
                let exit = weak.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        // `handleLeave`.
                        let _ = exit.update(cx, |this, cx| this.set_over(OverState::None, cx));
                    }
                });
            },
        )
        .absolute()
        .inset_0();

        let (controls, badge) = if let Some(live) = self.live.clone() {
            let active = (close_active, enlarge_active, volume_active);
            let (controls, badge) = self.live_controls(&live, &layout, shown, active, cx);
            (controls, Some(badge))
        } else {
            let playing = transport.is_playing();
            let total = transport.total();
            let position = self
                .seek_preview
                .map_or(transport.position, |fraction| fraction * total.max(0.0));
            let play_icon = if playing {
                IconName::Pause
            } else {
                IconName::Play
            };
            let muted = transport.volume <= 0.0;
            let volume_label = if muted {
                tr(keys::Q_MISC_UNMUTE)
            } else {
                tr(keys::Q_MISC_MUTE)
            };
            let fraction = |position: f64| {
                if total > 0.0 && position.is_finite() {
                    (position / total).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                }
            };
            let playback = layout.playback;
            let font = px(PLAYBACK_FONT);
            let line_box = px(PLAYBACK_FONT + 2.0);
            let viewport = place::to_rect(Bounds::new(Point::default(), window.viewport_size()));
            let times = div()
                .absolute()
                .left(px((playback.area.x + PLAYBACK_TEXT_SKIP) as f32))
                .right(px(
                    (viewport.w - playback.area.right() + PLAYBACK_TEXT_SKIP) as f32,
                ))
                .top(px(playback.icon.y as f32) - line_box)
                .h(line_box)
                .flex()
                .justify_between()
                .text_size(font)
                .line_height(line_box)
                .text_color(gpui_kit::white())
                .child(elapsed_label(position))
                .child(remaining_label(position, total));

            let mut controls = div()
                .absolute()
                .inset_0()
                .opacity(shown)
                .child(
                    place_at(layout.play).child(
                        Icon::new(play_icon)
                            .with_size(px(layout.play.w as f32))
                            .text_color(controls_fg(play_active)),
                    ),
                )
                .child(self.icon_button(
                    "pip-close",
                    layout.close,
                    IconName::X,
                    tr(keys::CLOSE),
                    PressAction::Close,
                    close_active,
                    cx,
                ))
                .child(self.icon_button(
                    "pip-enlarge",
                    layout.enlarge,
                    IconName::Maximize2,
                    tr(keys::Q_MEDIA_PIP_BACK_TO_VIEWER),
                    PressAction::Enlarge,
                    enlarge_active,
                    cx,
                ))
                .child(self.icon_button(
                    "pip-volume",
                    layout.volume_toggle,
                    volume_icon(transport.volume),
                    volume_label,
                    PressAction::ToggleVolume,
                    volume_active,
                    cx,
                ))
                .child(times.debug_selector(|| "pip-times".into()))
                .child(
                    self.line(playback, &self.seek, fraction(position), playback_active)
                        .debug_selector(|| "pip-playback".into()),
                );
            if !layout.volume_controller.icon.is_empty() {
                controls = controls.child(self.line(
                    layout.volume_controller,
                    &self.volume,
                    transport.volume.clamp(0.0, 1.0),
                    volume_active,
                ));
            }
            (controls, None)
        };
        let spinner = waiting.then(|| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(RADIAL)
                        .rounded_full()
                        .bg(gpui_kit::black().opacity(WASH))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Spinner::new().with_size(SPINNER).color(gpui_kit::white())),
                )
        });

        div()
            .id("video-pip")
            .role(Role::Group)
            .aria_label(tr(keys::Q_MISC_PICTURE_IN_PICTURE))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(gpui_kit::black())
            .cursor(cursor_for(self.edge))
            .child(input)
            .child(
                div().absolute().inset_0().child(
                    video_frame(&player)
                        .object_fit(ObjectFit::Fill)
                        .keep_alive(true),
                ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black().opacity(WASH))
                    .opacity(shown),
            )
            .children(spinner)
            .child(controls)
            .children(badge)
            .into_any_element()
    }
}
