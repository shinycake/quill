//! Screenshot demos: the buttons under a profile's name and its More
//! menu, for a contact, a location-based group and a channel.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
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
    DemoSpec::chats(
        "ready-profile-user",
        "screenshot demo — contact profile actions"
    )
    .setup(QuillApp::demo_profile_user),
    DemoSpec::chats(
        "ready-profile-group",
        "screenshot demo — group profile actions"
    )
    .setup(QuillApp::demo_profile_group),
    DemoSpec::chats(
        "ready-profile-channel",
        "screenshot demo — channel profile actions"
    )
    .setup(QuillApp::demo_profile_channel),
];

const PROFILE_USER: i64 = 81;
const PROFILE_GROUP: i64 = 82;
const PROFILE_CHANNEL: i64 = 83;

fn apply_all(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn listed_chat(id: i64, title: &str, kind: &str) -> [String; 2] {
    [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"can_be_reported":true,"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"70","is_pinned":false}}}}"#
        ),
    ]
}

fn apply_group(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let mut jsons: Vec<String> = listed_chat(
        PROFILE_GROUP,
        "Quill Berlin",
        &format!(
            r#"{{"@type":"chatTypeSupergroup","supergroup_id":{PROFILE_GROUP},"is_channel":false}}"#
        ),
    )
    .into();
    jsons.extend([
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{PROFILE_GROUP},"is_channel":false,"has_location":true,"usernames":{{"@type":"usernames","active_usernames":["quillberlin","quill_de","berlinquill"],"disabled_usernames":[],"editable_username":"quillberlin","collectible_usernames":[]}},"status":{{"@type":"chatMemberStatusMember"}},"member_count":1284}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{PROFILE_GROUP},"supergroup_full_info":{{"description":"Quill users in Berlin: meetups, tips and help.","member_count":1284,"location":{{"@type":"chatLocation","location":{{"latitude":52.5219,"longitude":13.4132}},"address":"Alexanderplatz, Berlin"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatOnlineMemberCount","chat_id":{PROFILE_GROUP},"online_member_count":37}}"#
        ),
    ]);
    apply_all(session, sink, seq, &jsons);
}

impl QuillApp {
    fn demo_profile_user(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, PROFILE_USER);
            let mut jsons = vec![format!(
                r#"{{"@type":"updateUser","user":{{"id":{PROFILE_USER},"first_name":"Lena","last_name":"Vogel","phone_number":"15550101081","is_contact":true,"usernames":{{"@type":"usernames","active_usernames":["lenavogel"],"disabled_usernames":[],"editable_username":"lenavogel"}},"type":{{"@type":"userTypeRegular"}}}}}}"#
            )];
            jsons.extend(listed_chat(
                PROFILE_USER,
                "Lena Vogel",
                &format!(r#"{{"@type":"chatTypePrivate","user_id":{PROFILE_USER}}}"#),
            ));
            jsons.push(format!(
                r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"Product designer, Berlin.","entities":[]}},"can_be_called":true}}"#,
                extra.0
            ));
            apply_all(session, &self.demo_ui.sink, &self.demo_ui.seq, &jsons);
        }
        self.open_info_panel_target(InfoPanelTarget::User(PROFILE_USER), window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — contact profile"));
        cx.notify();
    }

    fn demo_profile_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_group(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.open_chat(ChatId(PROFILE_GROUP));
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(PROFILE_GROUP), window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — group profile"));
        cx.notify();
    }

    fn demo_profile_channel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_group(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let mut jsons: Vec<String> = listed_chat(
                PROFILE_CHANNEL,
                "Quill Releases",
                &format!(
                    r#"{{"@type":"chatTypeSupergroup","supergroup_id":{PROFILE_CHANNEL},"is_channel":true}}"#
                ),
            )
            .into();
            jsons.extend([
                format!(
                    r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{PROFILE_CHANNEL},"is_channel":true,"usernames":{{"@type":"usernames","active_usernames":["quillreleases"],"disabled_usernames":[],"editable_username":"quillreleases","collectible_usernames":[]}},"status":{{"@type":"chatMemberStatusMember"}},"member_count":20480}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{PROFILE_CHANNEL},"supergroup_full_info":{{"description":"New Quill versions and what changed.","member_count":20480,"linked_chat_id":{PROFILE_GROUP},"can_send_gift":true}}}}"#
                ),
            ]);
            apply_all(session, &self.demo_ui.sink, &self.demo_ui.seq, &jsons);
            session.open_chat(ChatId(PROFILE_CHANNEL));
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(PROFILE_CHANNEL), window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — channel profile"));
        cx.notify();
    }
}
