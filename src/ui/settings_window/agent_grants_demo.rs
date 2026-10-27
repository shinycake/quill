//! `ready-settings-agent-grants`: the Agent access page with access on.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::*;
use quill::agent::grants::{self, Scope};

/// The page with access on, a second account, a wider default, a blocked
/// chat and a secret chat added. `QUILL_DEMO_AGENT=empty` leaves the added
/// chats empty and the account single.
fn demo_grants(app: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    use quill::agent::grants::ChatGrant;
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    let empty = std::env::var("QUILL_DEMO_AGENT").is_ok_and(|mode| mode == "empty");
    if !empty && let Some(root) = quill::settings::safe_app_root() {
        let _ = quill::settings::add_account(&root, "Work");
    }
    let account = crate::ui::agent_state::active_account();
    grants::update(&account, |g| {
        if empty {
            return;
        }
        g.all_chats = Some(Scope::Read);
        g.contacts = true;
        let mut add = |id: i64, title: &str, scope: Option<Scope>| {
            g.chats.insert(
                id,
                ChatGrant {
                    scope,
                    title: title.to_string(),
                },
            );
        };
        add(7, "Ann Lee", Some(Scope::ReadWrite));
        add(-1_001_234_567_890, "Family", None);
        add(-1_999_000_000_000, "Ben Ortiz", Some(Scope::Read));
    });
    app.open_settings(Some(quill::settings_catalog::Page::AgentAccess));
    super::agent_page::demo_scroll_from_env(app);
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-agent-grants")
        .setup(demo_grants),
];
