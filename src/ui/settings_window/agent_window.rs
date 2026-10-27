//! Agent access, "Window control" and "Confirmations": the switch that lets
//! agents drive Quill's window, what they see while they do, its state now
//! and the "Takeover effect"; then "Confirm each send". Read from the
//! access summary and `quill::agent::access` on each render.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::agent_access::action_row;
use super::rows::{group, switch_row};
use crate::ui::agent_labels::agent_label;
use gpui_kit::component::button::*;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::access::{self, TakeoverEffect};
use quill::agent::summary::{AccessSummary, Fix, WindowState, WindowView};
use quill::lang::{keys, tr, tr_with};

/// The "Takeover effect" choices, in order.
const EFFECTS: [(TakeoverEffect, &str); 3] = [
    (TakeoverEffect::Automatic, "auto"),
    (TakeoverEffect::GlowAndSweep, "sweep"),
    (TakeoverEffect::StillGlow, "still"),
];

fn effect_label(effect: TakeoverEffect) -> &'static str {
    tr(match effect {
        TakeoverEffect::Automatic => keys::Q_AGENT_TAKEOVER_EFFECT_AUTO,
        TakeoverEffect::GlowAndSweep => keys::Q_AGENT_TAKEOVER_EFFECT_SWEEP,
        TakeoverEffect::StillGlow => keys::Q_AGENT_TAKEOVER_EFFECT_STILL,
    })
}

fn effect_hint(effect: TakeoverEffect) -> &'static str {
    tr(match effect {
        TakeoverEffect::Automatic => keys::Q_AGENT_TAKEOVER_EFFECT_AUTO_HINT,
        TakeoverEffect::GlowAndSweep => keys::Q_AGENT_TAKEOVER_EFFECT_SWEEP_HINT,
        TakeoverEffect::StillGlow => keys::Q_AGENT_TAKEOVER_EFFECT_STILL_HINT,
    })
}

impl QuillApp {
    pub(super) fn agent_window_section(
        &self,
        summary: &AccessSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let window = &summary.window;
        let mut rows = vec![switch_row(
            "agent-ui-driver-switch",
            tr(keys::Q_AGENT_UI_DRIVER),
            Some(tr(keys::Q_AGENT_UI_DRIVER_HINT)),
            window.switch,
            cx.listener(|_, &next: &bool, _, cx| {
                access::set_ui_driver(next);
                cx.notify();
            }),
            cx,
        )];
        let sees = tr(match window.view {
            WindowView::Nothing => keys::Q_AGENT_PAGE_SEES_NOTHING,
            WindowView::GrantedChats => keys::Q_AGENT_PAGE_SEES_GRANTED,
            WindowView::WholeWindow => keys::Q_AGENT_PAGE_SEES_WHOLE,
        });
        rows.push(
            div()
                .id("agent-window-sees")
                .debug_selector(|| "agent-window-sees".to_string())
                .child(action_row(
                    tr(keys::Q_AGENT_PAGE_SEES),
                    Some(sees.into()),
                    div(),
                    cx,
                ))
                .into_any_element(),
        );
        if window.switch {
            rows.push(self.agent_window_state_row(&window.state, cx));
            rows.push(takeover_effect_row(cx));
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(group(None, rows))
            .child(
                div()
                    .id("agent-window-never")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_AGENT_PAGE_WINDOW_NEVER)),
            )
            .into_any_element()
    }

    /// "Now": ready, driving, paused or stopped, with the one button that
    /// changes it.
    fn agent_window_state_row(&self, state: &WindowState, cx: &mut Context<Self>) -> AnyElement {
        let (tag, fix) = match state {
            WindowState::Off => (
                Tag::secondary().child(tr(keys::Q_AGENT_PAGE_STATE_OFF)),
                None,
            ),
            WindowState::Ready => (
                Tag::color(ColorName::Green).child(tr(keys::Q_AGENT_PAGE_WINDOW_READY)),
                None,
            ),
            WindowState::Driving { agent } => (
                Tag::color(ColorName::Blue).child(tr_with(
                    keys::Q_AGENT_TAKEOVER_DRIVING,
                    &[("agent", &agent_label(agent))],
                )),
                None,
            ),
            WindowState::Paused => (
                Tag::color(ColorName::Amber).child(tr(keys::Q_AGENT_TAKEOVER_PAUSED)),
                Some(Fix::LetContinue),
            ),
            WindowState::Stopped => (
                Tag::secondary().child(tr(keys::Q_AGENT_PAGE_WINDOW_STOPPED)),
                Some(Fix::RestartWindowControl),
            ),
        };
        div()
            .id("agent-window-state")
            .debug_selector(|| "agent-window-state".to_string())
            .flex()
            .items_center()
            .gap_3()
            .w_full()
            .min_h(px(40.))
            .px_3()
            .py_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(tr(keys::Q_AGENT_PAGE_WINDOW_NOW)),
            )
            .when_some(fix, |this, fix| {
                let label = match fix {
                    Fix::LetContinue => tr(keys::Q_AGENT_TAKEOVER_CONTINUE),
                    _ => tr(keys::Q_AGENT_PAGE_ALLOW_AGAIN),
                };
                this.child(
                    Button::new("agent-window-fix")
                        .label(label)
                        .outline()
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| this.apply_agent_fix(fix, cx))),
                )
            })
            .child(div().flex_none().child(tag.small()))
            .into_any_element()
    }

    /// "Confirm each send" with what it covers.
    pub(super) fn agent_confirm_section(
        &self,
        summary: &AccessSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(group(
                None,
                [switch_row(
                    "agent-confirm-switch",
                    tr(keys::Q_AGENT_CONFIRM_SENDS),
                    Some(tr(keys::Q_AGENT_CONFIRM_SENDS_HINT)),
                    summary.confirm_each_send,
                    cx.listener(|_, &next: &bool, _, cx| {
                        access::set_confirm_sends(next);
                        cx.notify();
                    }),
                    cx,
                )],
            ))
            .child(
                div()
                    .id("agent-confirm-note")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_AGENT_PAGE_CONFIRM_NOTE)),
            )
            .into_any_element()
    }
}

/// "Takeover effect": how the glow moves while an agent drives, as one
/// segmented control, with what the chosen one does under it. The setter
/// refuses the driver's own clicks (`guard::refuse_if_injecting`).
fn takeover_effect_row(cx: &mut Context<QuillApp>) -> AnyElement {
    let current = access::takeover_effect();
    let segments = EFFECTS.iter().map(|&(effect, name)| {
        Button::new(SharedString::from(format!("agent-takeover-effect-{name}")))
            .label(effect_label(effect))
            .selected(current == effect)
            .when(current == effect, |button| button.primary())
    });
    let control = ButtonGroup::new("agent-takeover-effect")
        .small()
        .children(segments)
        .on_click(cx.listener(|_, picked: &Vec<usize>, _, cx| {
            let effect = picked
                .first()
                .and_then(|&index| EFFECTS.get(index))
                .map(|&(effect, _)| effect);
            if let Some(effect) = effect {
                access::set_takeover_effect(effect);
                cx.notify();
            }
        }));
    div()
        .id("agent-takeover-effect-row")
        .debug_selector(|| "agent-takeover-effect-row".to_string())
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .px_3()
        .py_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .child(tr(keys::Q_AGENT_TAKEOVER_EFFECT)),
                )
                .child(control),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(effect_hint(current)),
        )
        .into_any_element()
}
