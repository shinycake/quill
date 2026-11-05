//! Screenshot demo for the chat setting dialogs (`chat_setting_dialogs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;

impl QuillApp {
    /// Demo: chat 11 open with one of its setting dialogs:
    /// `QUILL_DEMO_CHAT_DIALOG=ttl` the auto-delete timer, `folders` the
    /// folder picker, anything else the notification settings.
    fn demo_ready_chat_setting_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let chat = ChatId(11);
        match std::env::var("QUILL_DEMO_CHAT_DIALOG").as_deref() {
            Ok("ttl") => self.open_ttl_picker(chat, cx),
            Ok("folders") => self.open_folder_menu(chat, cx),
            _ => self.open_mute_menu(cx),
        }
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-chat-setting-dialog",
        "screenshot demo — a chat's setting dialog"
    )
    .setup(QuillApp::demo_ready_chat_setting_dialog),
];
