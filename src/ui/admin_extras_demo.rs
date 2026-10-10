//! Screenshot-demo fixtures for the admin extras (`--screenshot-demo
//! ready-admin-extras`, mode from `QUILL_DEMO_ADMIN_EXTRAS`). Everything is
//! injected through the real reducers; no live Telegram. English-only text.

use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::group_invites::apply_ready_admin_log;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::state::{InfoPanelTarget, RequestPurpose};
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::Ordering;

register_demos![
    // Admin extras (`QUILL_DEMO_ADMIN_EXTRAS=log|title|broadcast|warning|
    // delete`; injected data, no live Telegram).
    DemoSpec::chats(
        "ready-admin-extras",
        "screenshot demo — admin extras (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_setup_admin_extras),
];

impl QuillApp {
    /// `QUILL_DEMO_ADMIN_EXTRAS=log|title|broadcast|warning|delete`
    /// (default `log`).
    fn demo_setup_admin_extras(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_ADMIN_EXTRAS").unwrap_or_else(|_| "log".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_admin_log(session, &self.demo_ui.sink, &self.demo_ui.seq);
            // The administrator list feeds the "Recent actions" admin chips.
            let admins = session.request(RequestPurpose::GetChatAdministrators, Some(ChatId(13)));
            let json = format!(
                r#"{{"@type":"chatAdministrators","@extra":"{}","administrators":[{{"@type":"chatAdministrator","user_id":1,"custom_title":"Founder","is_owner":true,"can_be_edited":false}},{{"@type":"chatAdministrator","user_id":777,"custom_title":"","is_owner":false,"can_be_edited":true}},{{"@type":"chatAdministrator","user_id":2,"custom_title":"","is_owner":false,"can_be_edited":true}}]}}"#,
                admins.0
            );
            let sink: std::sync::Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
            if mode == "log" {
                session.event_log_users.insert(13, vec![2]);
            }
        }
        match mode.as_str() {
            "title" => {
                self.open_custom_title_dialog(ChatId(13), 2, "Head of Moderation", window, cx)
            }
            "broadcast" => {
                self.open_group_confirm(ChatId(13), GroupConfirmAction::BroadcastIntro, cx)
            }
            "warning" => {
                self.open_group_confirm(ChatId(13), GroupConfirmAction::BroadcastUpgrade, cx)
            }
            "delete" => self.open_group_confirm(ChatId(13), GroupConfirmAction::DeleteChat, cx),
            _ => self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx),
        }
        self.connection.status_note =
            "screenshot demo — admin extras (injected, no live Telegram)".into();
        cx.notify();
    }
}
