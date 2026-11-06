//! Agent access's master switch while it is off: one row on Advanced, the
//! only trace of agents anywhere in Quill. Turning it on brings back the
//! Agent access page and every agent surface and opens the page; turning
//! it off on that page comes back here. The row reads
//! `quill::agent::access::enabled` on each render, the one source of truth
//! (`docs/decisions/agent-access.md`, "Master switch").
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::*;
use quill::agent::access;
use quill::lang::{keys, tr};
use quill::settings_catalog::{AGENT_SWITCH, Page};

impl QuillApp {
    /// Advanced's "Agent access" row, shown only while access is off.
    pub(in crate::ui) fn agent_access_advanced_row(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if access::enabled() {
            return None;
        }
        let title = tr(keys::Q_AGENT_ACCESS);
        let row = div()
            .id("advanced-agent-access")
            .debug_selector(|| "advanced-agent-access".into())
            .child(super::rows::switch_row(
                "advanced-agent-access-switch",
                title,
                Some(tr(keys::Q_AGENT_ACCESS_SWITCH_HINT)),
                false,
                cx.listener(|this, &on: &bool, _, cx| this.set_agent_access(on, cx)),
                cx,
            ))
            .into_any_element();
        Some(self.setting_spot(&[AGENT_SWITCH], row, cx))
    }

    /// The master switch, from Advanced or the Agent access page. Settings
    /// follows it to where the switch now is: the page once access is on,
    /// Advanced once it is off. An agent driving the window can't flip it
    /// (`access::set_enabled` refuses), so nothing moves then.
    pub(super) fn set_agent_access(&mut self, on: bool, cx: &mut Context<Self>) {
        access::set_enabled(on);
        if access::enabled() != on {
            cx.notify();
            return;
        }
        let page = if on {
            Page::AgentAccess
        } else {
            Page::Advanced
        };
        self.show_settings_page(page, cx);
    }
}

crate::ui::screenshot_demo::register_demos![
    // Advanced with Agent access off: its switch, and no Agent access page
    // in the sidebar.
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-agent-off").setup(
        |app, _, _| {
            quill::fast_buttons::set_persistence(false);
            quill::agent::access::set_enabled(false);
            app.demo_settings(Page::Advanced);
        }
    ),
];

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_switch_tests.rs"]
mod tests;
