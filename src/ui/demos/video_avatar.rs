//! Screenshot demos: video profile photos. A group and a contact whose
//! current photo is a video photo, their info panel open: the cover plays
//! the clip from its main frame (one second in, zoomed toward the sun)
//! over the still. A click on the avatar opens the viewer, where the clip
//! plays too and the earlier still photo stays still in the strip.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
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
        "ready-info-video-avatar",
        "screenshot demo — group info panel, video group photo"
    )
    .setup(QuillApp::demo_info_video_avatar_group),
    DemoSpec::chats(
        "ready-info-video-avatar-user",
        "screenshot demo — contact info panel, video profile photo"
    )
    .setup(QuillApp::demo_info_video_avatar_user),
];

/// Used as both the chat and the supergroup id, like the other demos.
pub(in crate::ui) const VIDEO_GROUP: i64 = 94;
pub(in crate::ui) const VIDEO_USER: i64 = 9401;
/// The clip of both video photos.
pub(in crate::ui) const CLIP_FILE: i32 = 9490;
const STILL: &str = "demo-video-avatar-still.jpg";
const CLIP: &str = "demo-video-avatar.mp4";
const EARLIER: &str = "showcase/p3.jpg";

fn fixture_file(id: i32, fixture: &str) -> String {
    let path = demo_media_allowlist().join(fixture);
    demo_file_json(id, &path.to_string_lossy(), true)
}

/// A `chatPhoto` with a grid size and a full size, and a clip that starts
/// one second in when `animated`.
fn chat_photo_json(photo_id: i64, fixture: &str, file: i32, animated: bool) -> String {
    let animation = if animated {
        format!(
            r#","animation":{{"@type":"animatedChatPhoto","length":480,"file":{},"main_frame_timestamp":1.0}}"#,
            fixture_file(CLIP_FILE, CLIP)
        )
    } else {
        String::new()
    };
    format!(
        r#"{{"@type":"chatPhoto","id":"{photo_id}","added_date":1700000000,"sizes":[{{"@type":"photoSize","type":"a","photo":{},"width":160,"height":160,"progressive_sizes":[]}},{{"@type":"photoSize","type":"c","photo":{},"width":640,"height":640,"progressive_sizes":[]}}]{animation}}}"#,
        fixture_file(file, fixture),
        fixture_file(file + 1, fixture),
    )
}

fn apply_all(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_group(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let mut jsons = vec![
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{VIDEO_GROUP},"title":"Desert Sunrise Club","type":{{"@type":"chatTypeSupergroup","supergroup_id":{VIDEO_GROUP},"is_channel":false}},"photo":{{"@type":"chatPhotoInfo","small":{},"big":{},"has_animation":true,"is_personal":false}},"unread_count":0}}}}"#,
            fixture_file(9405, STILL),
            fixture_file(9406, STILL),
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{VIDEO_GROUP},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"95","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{VIDEO_GROUP},"is_channel":false,"status":{{"@type":"chatMemberStatusMember"}},"member_count":58}}}}"#
        ),
    ];
    let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, VIDEO_GROUP);
    jsons.push(format!(
        r#"{{"@type":"supergroupFullInfo","@extra":"{}","photo":{},"description":"Up before the sun, every Sunday.","member_count":58}}"#,
        extra.0,
        chat_photo_json(9403, STILL, 9410, true),
    ));
    let changes = [(9403, 40, STILL, 9410, true), (9402, 30, EARLIER, 9420, false)]
        .iter()
        .map(|(photo_id, message_id, fixture, file, animated)| {
            format!(
                r#"{{"@type":"message","id":{message_id},"chat_id":{VIDEO_GROUP},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageChatChangePhoto","photo":{}}}}}"#,
                chat_photo_json(*photo_id, fixture, *file, *animated),
            )
        })
        .collect::<Vec<_>>();
    let extra = session.request(
        RequestPurpose::GetChatPhotoHistory,
        Some(ChatId(VIDEO_GROUP)),
    );
    jsons.push(format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":{},"next_from_message_id":0,"messages":[{}]}}"#,
        extra.0,
        changes.len(),
        changes.join(","),
    ));
    apply_all(session, sink, seq, &jsons);
}

fn apply_user(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let mut jsons = vec![
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{VIDEO_USER},"first_name":"Noa","last_name":"Adler","phone_number":"15550101094","is_contact":true,"profile_photo":{{"@type":"profilePhoto","id":"9413","small":{},"big":{},"has_animation":true,"is_personal":false}},"usernames":{{"@type":"usernames","active_usernames":["noaadler"],"disabled_usernames":[],"editable_username":"noaadler"}},"type":{{"@type":"userTypeRegular"}}}}}}"#,
            fixture_file(9407, STILL),
            fixture_file(9408, STILL),
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{VIDEO_USER},"title":"Noa Adler","type":{{"@type":"chatTypePrivate","user_id":{VIDEO_USER}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{VIDEO_USER},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"96","is_pinned":false}}}}"#
        ),
    ];
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, VIDEO_USER);
    jsons.push(format!(
        r#"{{"@type":"userFullInfo","@extra":"{}","photo":{},"bio":{{"@type":"formattedText","text":"Morning person.","entities":[]}},"can_be_called":true}}"#,
        extra.0,
        chat_photo_json(9413, STILL, 9430, true),
    ));
    let extra = session.request_for_user(RequestPurpose::GetUserProfilePhotos, VIDEO_USER);
    jsons.push(format!(
        r#"{{"@type":"chatPhotos","@extra":"{}","total_count":2,"photos":[{},{}]}}"#,
        extra.0,
        chat_photo_json(9413, STILL, 9430, true),
        chat_photo_json(9412, EARLIER, 9440, false),
    ));
    apply_all(session, sink, seq, &jsons);
}

impl QuillApp {
    fn demo_info_video_avatar_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_group(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(VIDEO_GROUP), window, cx);
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — group info panel, video group photo",
        ));
        cx.notify();
    }

    fn demo_info_video_avatar_user(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_user(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::User(VIDEO_USER), window, cx);
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — contact info panel, video profile photo",
        ));
        cx.notify();
    }
}
