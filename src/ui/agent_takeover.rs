//! The takeover state on screen while an agent drives the window: the glow
//! along the edge (`agent_takeover_glow`), a ghost cursor where the agent
//! is about to click or type, and a pill at the top with Stop, or, once
//! the user took control, "Let it continue" and Stop.
//!
//! Everything is derived on each render from
//! `quill::agent::ui_driver::takeover::snapshot`; the window keeps no copy
//! of the phase. The layer is deferred above every popup and has no hitbox
//! except the pill's, so clicks anywhere else land on what is underneath.
//! It asks the frame clock for frames only while something moves (a fade,
//! a glide, a press pulse, a moving glow look); otherwise one timer wakes
//! the window when the phase is due to change, and an idle window does
//! nothing at all.
#![deny(clippy::indexing_slicing)]

use super::agent_labels::agent_label;
use super::agent_takeover_glow::{GLOW_FPS, GlowStyle, glow_element, glow_frame};
use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::agent::ui_driver::guard;
use quill::agent::ui_driver::takeover::{self, Action, Aim, Phase, RealInput};
use quill::lang::{keys, tr, tr_with};
use std::cell::Cell;
use std::time::Duration;

/// Above the kit's popups (100) and tooltips (200).
const TAKEOVER_PRIORITY: usize = 300;
/// The ghost cursor's ring, and how far its press pulse spreads.
const GHOST_RADIUS: f32 = 11.0;
const PULSE_SPREAD: f32 = 18.0;
/// The pointer glyph, and where its tip sits inside it (Lucide's
/// `mouse-pointer-2` tip is at 4.3 of 24).
const POINTER_SIZE: f32 = 20.0;
const POINTER_TIP: f32 = POINTER_SIZE * 4.3 / 24.0;
/// The glow while paused: the agent is still there, the user has control.
const PAUSED_GLOW: f32 = 0.5;

/// The window's own corner radius, so the border follows it: macOS
/// windows are rounded more than Windows 11's and Linux's decorations.
#[cfg(target_os = "macos")]
const WINDOW_CORNER: Pixels = px(16.);
#[cfg(not(target_os = "macos"))]
const WINDOW_CORNER: Pixels = px(8.);

thread_local! {
    /// Where the pill was painted last frame (window pixels), so pointer
    /// input on it answers the pill instead of pausing the agent.
    static PILL: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
}

/// Real input from the window's input probe (`presence::input_probe`).
/// Window tests on other threads (no agent state) stay out of the
/// process-wide record.
pub(super) fn note_real_input(input: RealInput, window: &Window) {
    if !super::agent_state::sees_agents() {
        return;
    }
    let pointer = window.mouse_position();
    let on_controls = !matches!(input, RealInput::Key)
        && PILL.with(|pill| pill.get().is_some_and(|bounds| bounds.contains(&pointer)));
    takeover::note_real_input(input, window.is_window_active(), on_controls);
}

impl QuillApp {
    /// The takeover layer, or nothing when no agent drives.
    pub(super) fn takeover_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // The pill paints again below (and records itself) if still shown.
        PILL.with(|pill| pill.set(None));
        if !super::agent_state::sees_agents() {
            return None;
        }
        let (phase, facts, now) = takeover::snapshot();
        if phase == Phase::Idle {
            return None;
        }
        let style = GlowStyle::chosen().shown(cx.reduce_motion());
        let paused = matches!(phase, Phase::Paused { .. });
        if takeover::moving(&phase, now) || (style.moves() && !paused) {
            self.request_background_animation_tick(GLOW_FPS, cx);
        } else if let Some(at) = takeover::next_change_ms(&phase, &facts) {
            self.wake_takeover_at(at, now, cx);
        }
        let presence = phase.presence(now);
        let started = facts.session.as_ref().map_or(now, |s| s.started_ms);
        let mut frame = glow_frame(style, presence, now.saturating_sub(started));
        if paused {
            frame.strength *= PAUSED_GLOW;
        }
        let theme = cx.theme();
        let (accent, on_accent, radius) = (theme.primary, theme.primary_foreground, theme.radius);
        let viewport = window.viewport_size();
        let layer = div()
            .absolute()
            .top_0()
            .left_0()
            .w(viewport.width)
            .h(viewport.height)
            .child(glow_element(frame, accent, viewport, WINDOW_CORNER))
            .children(
                phase
                    .action()
                    .map(|action| ghost(action, now, presence, accent, on_accent, radius)),
            )
            .child(self.takeover_pill(&phase, presence, viewport.width, cx));
        Some(
            deferred(anchored().position(point(px(0.), px(0.))).child(layer))
                .with_priority(TAKEOVER_PRIORITY)
                .into_any_element(),
        )
    }

    /// One timer for the next phase change nothing animates towards
    /// (driving ends after the hold, the fade ends).
    fn wake_takeover_at(&self, at: u64, now: u64, cx: &mut Context<Self>) {
        let pending = self.takeover.wake_at.get();
        if pending > now && pending <= at {
            return;
        }
        self.takeover.wake_at.set(at);
        let delay = Duration::from_millis(at.saturating_sub(now).saturating_add(5));
        QuillApp::spawn_main(cx, async move |app, cx| {
            cx.background_executor().timer(delay).await;
            let _ = app.update(cx, |app, cx| {
                if app.takeover.wake_at.get() == at {
                    app.takeover.wake_at.set(0);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The pill at the top centre: who drives and Stop; paused, what the
    /// agent waits for and the two choices. Fading out it keeps the label
    /// only (the session is over; there is nothing left to stop).
    fn takeover_pill(
        &self,
        phase: &Phase,
        presence: f32,
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agent = agent_label(phase.agent().unwrap_or_default());
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let pill = div()
            .id("agent-takeover-pill")
            .debug_selector(|| "agent-takeover-pill".to_string())
            .occlude()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .max_w(width - px(32.))
            .h_8()
            .pl_3()
            .pr_0p5()
            .rounded_full()
            .bg(theme.popover)
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .text_sm()
            .text_color(theme.popover_foreground)
            .opacity(presence)
            .child(
                canvas(
                    |bounds, _, _| PILL.with(|pill| pill.set(Some(bounds))),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        let pill = match phase {
            Phase::Paused { .. } => pill
                .child(
                    Icon::new(IconName::Hand)
                        .small()
                        .text_color(muted)
                        .flex_none(),
                )
                .child(
                    div()
                        .flex_none()
                        .font_semibold()
                        .child(tr(keys::Q_AGENT_TAKEOVER_PAUSED)),
                )
                .child(div().min_w_0().truncate().text_color(muted).child(tr_with(
                    keys::Q_AGENT_TAKEOVER_PAUSED_LINE,
                    &[("agent", &agent)],
                )))
                .child(
                    Button::new("agent-takeover-continue")
                        .label(tr(keys::Q_AGENT_TAKEOVER_CONTINUE))
                        .primary()
                        .small()
                        .rounded_full()
                        .flex_none()
                        .on_click(cx.listener(|_, _, _, cx| {
                            if guard::refuse_if_injecting("let the agent continue") {
                                return;
                            }
                            takeover::let_continue();
                            cx.notify();
                        })),
                )
                .child(stop_button(cx)),
            Phase::Driving { .. } => pill
                .child(live_dot(cx.theme().primary))
                .child(div().min_w_0().truncate().child(tr_with(
                    keys::Q_AGENT_TAKEOVER_DRIVING,
                    &[("agent", &agent)],
                )))
                .child(stop_button(cx)),
            Phase::FadingOut { .. } | Phase::Idle => pill
                .pr_3()
                .child(live_dot(cx.theme().primary))
                .child(div().min_w_0().truncate().child(tr_with(
                    keys::Q_AGENT_TAKEOVER_DRIVING,
                    &[("agent", &agent)],
                ))),
        };
        div()
            .absolute()
            .top(px(1.))
            .left_0()
            .w(width)
            .flex()
            .justify_center()
            .child(pill)
            .into_any_element()
    }
}

fn stop_button(cx: &mut Context<QuillApp>) -> Button {
    Button::new("agent-takeover-stop")
        .label(tr(keys::Q_AGENT_TAKEOVER_STOP))
        .ghost()
        .small()
        .rounded_full()
        .flex_none()
        .tooltip(tr(keys::Q_AGENT_TAKEOVER_STOP_HINT))
        .on_click(cx.listener(|_, _, _, cx| {
            if guard::refuse_if_injecting("stop the agent") {
                return;
            }
            takeover::stop_driving();
            cx.notify();
        }))
}

/// The small accent dot that says the agent is acting now.
fn live_dot(accent: Hsla) -> Div {
    div().flex_none().size_2().rounded_full().bg(accent)
}

/// The ghost cursor: a ring with a pointer whose tip is where the agent is
/// about to click, gliding there from its last spot, with a pulse when the
/// click goes in. Typing shows a keyboard badge at the text box's caret.
fn ghost(
    action: &Action,
    now: u64,
    presence: f32,
    accent: Hsla,
    on_accent: Hsla,
    radius: Pixels,
) -> AnyElement {
    let (x, y) = takeover::ghost_point(action, now);
    let shown = (presence * takeover::ghost_arrival(action, now)).clamp(0.0, 1.0);
    if action.aim == Aim::Type {
        return div()
            .absolute()
            .left(px(x + 4.))
            .top(px(y - 22.))
            .flex()
            .items_center()
            .justify_center()
            .size_5()
            .rounded(radius)
            .bg(accent)
            .opacity(shown)
            .child(Icon::new(IconName::Keyboard).xsmall().text_color(on_accent))
            .into_any_element();
    }
    let ring = |spread: f32, alpha: f32, fill: f32| {
        let r = GHOST_RADIUS + spread;
        div()
            .absolute()
            .left(px(x - r))
            .top(px(y - r))
            .size(px(2.0 * r))
            .rounded_full()
            .border_2()
            .border_color(accent.opacity(alpha))
            .bg(accent.opacity(fill))
    };
    let pulse = takeover::press_pulse(action, now).map(|t| {
        let eased = takeover::ease_out_cubic(t);
        ring(PULSE_SPREAD * eased, 0.8 * (1.0 - t) * shown, 0.0)
    });
    div()
        .absolute()
        .top_0()
        .left_0()
        .children(pulse)
        .child(ring(0.0, 0.9 * shown, 0.18 * shown))
        .child(
            Icon::new(IconName::MousePointer2)
                .absolute()
                .left(px(x - POINTER_TIP))
                .top(px(y - POINTER_TIP))
                .size(px(POINTER_SIZE))
                .text_color(accent.opacity(shown)),
        )
        .into_any_element()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_takeover_tests.rs"]
mod tests;
