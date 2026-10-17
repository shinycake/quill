//! Screenshot demos: the action rows of a bot's and a channel's info panel.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::bots::apply_ready_bot_chat;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{InfoPanelTarget, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    // The bot of `ready-bot-chat` with /help, /settings and a privacy
    // link, its info panel open: Bot Help, Bot Settings, Bot Privacy
    // Policy, Report and Stop and block bot.
    DemoSpec::chats("ready-info-bot", "screenshot demo — bot info panel actions")
        .setup(QuillApp::demo_info_bot),
    // A public channel the viewer has not joined, info panel open: Join
    // Channel and Report.
    DemoSpec::chats(
        "ready-info-channel-left",
        "screenshot demo — channel info panel, not joined"
    )
    .setup(QuillApp::demo_info_channel_left),
];

const BOT: i64 = 21;
const CHANNEL: i64 = 73;

fn apply_all(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    fn demo_info_bot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let extra = session.request(RequestPurpose::GetUserFullInfo, Some(ChatId(BOT)));
            let info = format!(
                r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"Demo Bot answers questions.","privacy_policy_url":"https://example.com/privacy","commands":[{{"@type":"botCommand","command":"start","description":"Start the bot","is_ephemeral":false}},{{"@type":"botCommand","command":"help","description":"Show help","is_ephemeral":false}},{{"@type":"botCommand","command":"settings","description":"Settings","is_ephemeral":false}}]}}}}"#,
                extra.0
            );
            apply_all(session, &self.demo_ui.sink, &self.demo_ui.seq, &[info]);
        }
        self.open_info_panel_target(InfoPanelTarget::User(BOT), window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — bot info panel"));
        cx.notify();
    }

    fn demo_info_channel_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let jsons = [
                format!(
                    r#"{{"@type":"updateNewChat","chat":{{"id":{CHANNEL},"title":"Open Channel","type":{{"@type":"chatTypeSupergroup","supergroup_id":{CHANNEL},"is_channel":true}},"unread_count":0}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{CHANNEL},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"80","is_pinned":false}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{CHANNEL},"is_channel":true,"status":{{"@type":"chatMemberStatusLeft"}},"member_count":900}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{CHANNEL},"supergroup_full_info":{{"description":"A public channel.","member_count":900}}}}"#
                ),
            ];
            apply_all(session, &self.demo_ui.sink, &self.demo_ui.seq, &jsons);
            session.open_chat(ChatId(CHANNEL));
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(CHANNEL), window, cx);
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — channel info, not joined",
        ));
        cx.notify();
    }
}
