//! The quiet strip at the bottom of the chat list while an agent is
//! connected, with Revoke. It is derived from
//! `quill::agent::activity::connected` on every render; nothing here keeps a
//! second copy of who is connected.
#![deny(clippy::indexing_slicing)]

use super::agent_labels::{agent_label, since_label};
use super::agent_state::visible_connected;
use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::agent::access;
use quill::agent::activity::Connected;
use quill::lang::{keys, tr, tr_with};

/// "Claude Code connected", or "2 agents connected".
fn strip_label(agents: &[Connected]) -> String {
    match agents {
        [one] => tr_with(
            keys::Q_AGENT_CONNECTED_ONE,
            &[("agent", &agent_label(&one.agent))],
        ),
        _ => tr_with(
            keys::Q_AGENT_CONNECTED_MANY,
            &[("count", &agents.len().to_string())],
        ),
    }
}

/// One line per agent: name and since when.
fn tooltip_text(agents: &[Connected]) -> String {
    agents
        .iter()
        .map(|agent| {
            format!(
                "{} ({})",
                agent_label(&agent.agent),
                since_label(agent.since)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl QuillApp {
    /// The strip above the update button, or nothing when no agent is
    /// connected.
    pub(in crate::ui) fn agent_connected_strip(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let agents = visible_connected();
        if agents.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let tip = SharedString::from(tooltip_text(&agents));
        Some(
            div()
                .id("agent-connected")
                .debug_selector(|| "agent-connected".to_string())
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .pl_3()
                .pr_1()
                .py_1()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    Icon::new(IconName::Bot)
                        .small()
                        .text_color(muted)
                        .flex_none(),
                )
                .child(
                    div()
                        .id("agent-connected-label")
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                        .child(strip_label(&agents)),
                )
                .child(
                    Button::new("agent-revoke")
                        .label(tr(keys::Q_AGENT_REVOKE))
                        .ghost()
                        .xsmall()
                        .tooltip(tr(keys::Q_AGENT_REVOKE_HINT))
                        .on_click(cx.listener(|_, _, _, cx| {
                            access::set_enabled(false);
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_presence_tests.rs"]
mod tests;
