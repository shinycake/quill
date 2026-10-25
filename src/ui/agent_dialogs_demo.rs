//! `ready-agent-request` and `ready-agent-consent`: the Agent access dialogs
//! over the ready chat list (injected, no live Telegram).
#![deny(clippy::indexing_slicing)]

use super::agent_state::AllChatsConsent;
use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::grants::Scope;
use quill::agent::prompts::Ask;

/// `QUILL_DEMO_AGENT` picks the request: `chat` (default), `write`,
/// `secret`, `all`, `all-write` or `contacts`.
fn demo_request(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    let mode = std::env::var("QUILL_DEMO_AGENT").unwrap_or_default();
    let chat = |chat_id: i64, title: &str, secret: bool, scope: Scope| Ask::Chat {
        chat_id,
        title: title.to_string(),
        secret,
        scope,
    };
    let ask = match mode.as_str() {
        "write" => chat(7, "Ann Lee", false, Scope::ReadWrite),
        "secret" => chat(-1_999_000_000_000, "Ann Lee", true, Scope::Read),
        "all" => Ask::AllChats { scope: Scope::Read },
        "all-write" => Ask::AllChats {
            scope: Scope::ReadWrite,
        },
        "contacts" => Ask::Contacts,
        _ => chat(7, "Ann Lee", false, Scope::Read),
    };
    let _ = quill::agent::prompts::file(
        super::agent_state::active_account(),
        "Claude Code".into(),
        ask,
        "You asked me to summarize last week's trip planning with Ann.".into(),
    );
}

/// `ready-agent-write-request`: an agent asks to send a message.
fn demo_write_request(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    quill::agent::access::set_confirm_sends(true);
    let _ = quill::agent::prompts::file(
        super::agent_state::active_account(),
        "Claude Code".into(),
        Ask::Write {
            chat_id: 7,
            title: "Ann Lee".into(),
            kind: "send_message".into(),
            preview: "Hi Ann, the trip plan is ready. Flights are booked for the 14th, and I put the hotel and the train times in the shared doc. Tell me if the Friday dinner still works for you.".into(),
        },
        // Write confirmations carry no reason: the preview is the point.
        String::new(),
    );
}

/// The consent for "All other chats" over the Agent access page.
fn demo_consent(app: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    app.open_settings(None);
    app.settings.page = Some(quill::settings_catalog::Page::Advanced);
    app.settings.sub = Some(super::settings_window::SubPage::AgentAccess);
    app.agent_ui.consent = Some(AllChatsConsent {
        account: super::agent_state::active_account(),
        scope: Scope::Read,
    });
}

/// `ready-agent-raw-request`: an agent asks to run a deny-listed TDLib call.
fn demo_raw_request(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    quill::fast_buttons::set_persistence(false);
    quill::agent::access::set_enabled(true);
    let _ = quill::agent::prompts::file(
        super::agent_state::active_account(),
        "Claude Code".into(),
        Ask::Raw {
            method: "terminateSession".into(),
            risk: quill::agent::prompts::Risk::Account,
            preview: "{\n  \"@type\": \"terminateSession\",\n  \"session_id\": \"5281930266421836\"\n}".into(),
        },
        "You asked me to sign out the old work laptop.".into(),
    );
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-request").setup(demo_request),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-write-request")
        .setup(demo_write_request),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-consent").setup(demo_consent),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-raw-request")
        .setup(demo_raw_request),
];
