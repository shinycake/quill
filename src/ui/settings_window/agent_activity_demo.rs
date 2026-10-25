//! `ready-settings-agent-activity`: the Agent access page with confirmation
//! on, two agents connected and a log of what they did (fake rows, demo
//! only). `QUILL_DEMO_AGENT=empty` shows the empty state.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::*;
use quill::agent::activity::{self, Action};

fn demo_activity(app: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    quill::agent::access::set_confirm_sends(true);
    let empty = std::env::var("QUILL_DEMO_AGENT").is_ok_and(|mode| mode == "empty");
    let account = crate::ui::agent_state::active_account();
    if !empty {
        let guards: Vec<_> = ["Claude Code", "Cursor"]
            .into_iter()
            .map(activity::connect)
            .collect();
        // The demo window lives until the process ends.
        std::mem::forget(guards);
        let now = crate::ui::agent_labels::now_unix();
        let rows = [
            (
                60,
                "Claude Code",
                "send_message",
                "Ann Lee",
                "Sent \"See you at 6\"",
                true,
            ),
            (
                900,
                "Claude Code",
                "edit_message",
                "Ann Lee",
                "Edited \"See you at 6:30\"",
                true,
            ),
            (
                3_600,
                "Cursor",
                "send_message",
                "Build alerts",
                "Telegram refused it: chat write forbidden",
                false,
            ),
            (
                30_000,
                "Claude Code",
                "delete_message",
                "Saved Messages",
                "Deleted its message",
                true,
            ),
            (
                200_000,
                "Claude Code",
                "send_message",
                "Family",
                "Sent \"Dinner is at 8, bring the good bread\"",
                true,
            ),
        ];
        // The log keeps insertion order, so add the oldest first.
        for (ago, agent, tool, chat, summary, ok) in rows.into_iter().rev() {
            activity::record_at(Action {
                time: now - ago,
                account: account.clone(),
                agent: agent.into(),
                tool: tool.into(),
                chat_id: 7,
                chat_title: chat.into(),
                summary: summary.into(),
                ok,
            });
        }
    }
    app.open_settings(None);
    app.settings.page = Some(quill::settings_catalog::Page::Advanced);
    app.settings.sub = Some(super::SubPage::AgentAccess);
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-agent-activity")
        .setup(demo_activity),
];
