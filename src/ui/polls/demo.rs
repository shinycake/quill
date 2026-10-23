//! Screenshot demo: the poll box with its settings section.
//! `QUILL_DEMO_POLL` is `settings` (a group, Limit Duration on, to the
//! last row), `channel` (a channel: subscribers and country rows),
//! `quiz` (Set Correct Answer on) or `deadline` (the custom deadline box).

use super::*;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use quill::poll::PollClose;

register_demos![DemoSpec::chat_list("ready-poll-settings").setup(QuillApp::demo_poll_settings)];

impl QuillApp {
    fn demo_poll_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_POLL").unwrap_or_default();
        let channel = mode == "channel";
        if let Some(session) = self.demo_session.as_mut() {
            let chat = if channel { 13 } else { 1 };
            if channel && let Some(chat) = session.chats.get_mut(&13) {
                chat.my_member_status =
                    Some(quill::telegram::envelope::ChannelMemberStatus::Creator);
            }
            session.open_chat(quill::ids::ChatId(chat));
        }
        self.open_poll_dialog(window, cx);
        let Some(dialog) = self.composer_ui.poll_dialog.as_mut() else {
            return;
        };
        match mode.as_str() {
            "quiz" => dialog.set_quiz(true),
            "channel" => {
                dialog.limit_by_country = true;
                dialog.country_codes = vec!["US".into(), "GB".into()];
            }
            _ => {
                dialog.limit_duration = true;
                dialog.close = PollClose::Period(8 * 3600);
            }
        }
        if mode == "deadline" {
            self.open_poll_deadline_box(window, cx);
        }
    }
}
