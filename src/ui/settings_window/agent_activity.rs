//! Settings > Advanced > Agent access, the parts about what agents do:
//! "Confirm each send", driving the window and its "Takeover effect", who
//! is connected now, and the activity log
//! (`quill::agent::activity`). Every section reads the process-wide state on
//! each render and keeps no copy of it.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::rows::{group, switch_row};
use crate::ui::agent_labels::{agent_label, now_unix, since_label, tool_label};
use crate::ui::agent_state::{ACTIVITY_PAGE, visible_activity, visible_connected};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::access::{self, TakeoverEffect};
use quill::agent::activity::{self, Action};
use quill::lang::{keys, tr};
use quill::local_time::{CivilTime, chat_list_stamp, civil_local};

impl QuillApp {
    /// "Confirm each send" with its switch.
    pub(super) fn agent_confirm_section(&self, cx: &mut Context<Self>) -> AnyElement {
        group(
            None,
            [
                switch_row(
                    "agent-confirm-switch",
                    tr(keys::Q_AGENT_CONFIRM_SENDS),
                    Some(tr(keys::Q_AGENT_CONFIRM_SENDS_HINT)),
                    access::confirm_sends(),
                    cx.listener(|_, &next: &bool, _, cx| {
                        access::set_confirm_sends(next);
                        cx.notify();
                    }),
                    cx,
                ),
                switch_row(
                    "agent-ui-driver-switch",
                    tr(keys::Q_AGENT_UI_DRIVER),
                    Some(tr(keys::Q_AGENT_UI_DRIVER_HINT)),
                    access::ui_driver(),
                    cx.listener(|_, &next: &bool, _, cx| {
                        access::set_ui_driver(next);
                        cx.notify();
                    }),
                    cx,
                ),
                takeover_effect_row(cx),
            ],
        )
        .into_any_element()
    }

    /// "Connected now": one row per agent, or nothing.
    pub(super) fn agent_connected_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let agents = visible_connected();
        if agents.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let rows: Vec<AnyElement> = agents
            .iter()
            .enumerate()
            .map(|(index, agent)| {
                div()
                    .id(SharedString::from(format!("agent-connected-{}", agent.id)))
                    .debug_selector(move || format!("agent-connected-row-{index}"))
                    .flex()
                    .items_center()
                    .gap_3()
                    .w_full()
                    .min_h(px(40.))
                    .px_3()
                    .py_2()
                    .child(
                        Icon::new(IconName::Bot)
                            .small()
                            .text_color(muted)
                            .flex_none(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(agent_label(&agent.agent)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(muted)
                            .child(since_label(agent.since)),
                    )
                    .into_any_element()
            })
            .collect();
        Some(group(Some(tr(keys::Q_AGENT_CONNECTED_TITLE)), rows).into_any_element())
    }

    /// "Activity": the newest rows for the account Settings edits, or the
    /// empty state, with Show more and Clear underneath.
    pub(super) fn agent_activity_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.agent_settings_account();
        let log = visible_activity(&account);
        let muted = cx.theme().muted_foreground;
        if log.is_empty() {
            return group(
                Some(tr(keys::Q_AGENT_ACTIVITY_TITLE)),
                [div()
                    .id("agent-activity-empty")
                    .debug_selector(|| "agent-activity-empty".to_string())
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_AGENT_ACTIVITY_EMPTY))
                    .into_any_element()],
            )
            .into_any_element();
        }
        let now = civil_local(now_unix());
        let shown = self.agent_ui.activity_shown.max(ACTIVITY_PAGE);
        let more = log.len() > shown;
        let mut rows: Vec<AnyElement> = log
            .iter()
            .take(shown)
            .enumerate()
            .map(|(index, action)| activity_row(index, action, &now, cx))
            .collect();
        rows.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .child(div().when(more, |this| {
                    this.child(
                        Button::new("agent-activity-more")
                            .label(tr(keys::Q_AGENT_ACTIVITY_MORE))
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.agent_ui.activity_shown += ACTIVITY_PAGE;
                                cx.notify();
                            })),
                    )
                }))
                .child(
                    Button::new("agent-activity-clear")
                        .label(tr(keys::Q_AGENT_ACTIVITY_CLEAR))
                        .ghost()
                        .small()
                        .tooltip(tr(keys::Q_AGENT_ACTIVITY_CLEAR_HINT))
                        .on_click(cx.listener(|this, _, _, cx| {
                            activity::clear();
                            this.agent_ui.activity_shown = ACTIVITY_PAGE;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        );
        group(Some(tr(keys::Q_AGENT_ACTIVITY_TITLE)), rows).into_any_element()
    }
}

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

/// One action: what it was and where, who did it and what happened, with
/// the time on the right and a mark when it failed.
fn activity_row(index: usize, action: &Action, now: &CivilTime, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let danger = cx.theme().danger;
    let stamp = chat_list_stamp(&civil_local(action.time), now);
    let what = if action.declined {
        tr(keys::Q_AGENT_ACTIVITY_DECLINED).to_string()
    } else {
        action.summary.clone()
    };
    let detail = if what.is_empty() {
        agent_label(&action.agent)
    } else {
        format!("{}: {what}", agent_label(&action.agent))
    };
    div()
        .id(SharedString::from(format!("agent-activity-{index}")))
        .debug_selector(move || format!("agent-activity-{index}"))
        .flex()
        .items_start()
        .gap_3()
        .w_full()
        .min_h(px(40.))
        .px_3()
        .py_2()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(div().text_sm().flex_none().child(tool_label(&action.tool)))
                        .child(
                            div()
                                .text_sm()
                                .min_w_0()
                                .truncate()
                                .text_color(muted)
                                .child(action.chat_title.clone()),
                        )
                        .when(!action.ok && !action.declined, |this| {
                            this.child(
                                div()
                                    .id(SharedString::from(format!(
                                        "agent-activity-{index}-failed"
                                    )))
                                    .debug_selector(move || {
                                        format!("agent-activity-{index}-failed")
                                    })
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .text_xs()
                                    .text_color(danger)
                                    .child(Icon::new(IconName::TriangleAlert).xsmall())
                                    .child(tr(keys::Q_AGENT_ACTIVITY_FAILED)),
                            )
                        }),
                )
                .child(div().text_xs().text_color(muted).truncate().child(detail)),
        )
        .child(
            div()
                .flex_none()
                .pt_0p5()
                .text_xs()
                .text_color(muted)
                .child(stamp),
        )
        .into_any_element()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_activity_tests.rs"]
mod tests;
