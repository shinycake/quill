//! Screenshot demo: video userpics. A group whose members are Premium
//! users with video profile photos, open: each sender's userpic beside
//! their messages plays its clip, and so does the chat-list row of a
//! private chat with one of them. `QUILL_DEMO_USERPICS=still` gives the
//! same chat with still photos only (the baseline for measuring the
//! clips' cost).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-group-video-userpics",
        "screenshot demo — group of Premium members with video userpics"
    )
    .setup(QuillApp::demo_group_video_userpics),
];

/// The group (chat and supergroup id).
const USERPIC_GROUP: i64 = 960;
/// The first member; the private chat is with them.
const FIRST_MEMBER: i64 = 9601;

/// `(first name, still, clip)` of each member.
const MEMBERS: [(&str, &str, &str); 10] = [
    ("Noa", "showcase/av01.png", "demo-video-avatar.mp4"),
    ("Eitan", "showcase/av02.png", "demo-gif-loop-square.mp4"),
    ("Maya", "showcase/av03.png", "demo-gif-loop-wide.mp4"),
    ("Yoav", "showcase/av04.png", "demo-video-avatar.mp4"),
    ("Tamar", "showcase/av05.png", "demo-gif-loop-square.mp4"),
    ("Omer", "showcase/av06.png", "demo-gif-loop-wide.mp4"),
    ("Shira", "showcase/av07.png", "demo-video-avatar.mp4"),
    ("Ido", "showcase/av08.png", "demo-gif-loop-square.mp4"),
    ("Lia", "showcase/av09.png", "demo-gif-loop-wide.mp4"),
    ("Ari", "showcase/av10.png", "demo-video-avatar.mp4"),
];

const LINES: [&str; 14] = [
    "Morning! Who is in for the 6 am loop?",
    "Me, if it doesn't rain",
    "Forecast says clear until noon",
    "I'll bring the water",
    "Can we do the river route this time?",
    "Yes please, the hill one nearly killed me",
    "River route it is",
    "Meeting at the bridge?",
    "Bridge, 5:55",
    "I'll be a few minutes late",
    "We'll wait at the first bench",
    "Coffee after?",
    "Always",
    "See you all tomorrow",
];

fn file(id: i64, fixture: &str) -> String {
    let path = demo_media_allowlist().join(fixture);
    demo_file_json(id as i32, &path.to_string_lossy(), true)
}

fn member_jsons(
    session: &mut Session,
    index: i64,
    (name, still, clip): (&str, &str, &str),
) -> Vec<String> {
    let user = FIRST_MEMBER + index;
    let photo = 96_100 + index;
    let files = 96_000 + index * 10;
    let profile_photo = format!(
        r#"{{"@type":"profilePhoto","id":"{photo}","small":{},"big":{},"has_animation":{},"is_personal":false}}"#,
        file(files, still),
        file(files + 1, still),
        !still_only(),
    );
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, user);
    vec![
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user},"first_name":"{name}","last_name":"","is_premium":true,"profile_photo":{profile_photo},"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","photo":{{"@type":"chatPhoto","id":"{photo}","added_date":1700000000,"sizes":[{{"@type":"photoSize","type":"a","photo":{},"width":160,"height":160,"progressive_sizes":[]}}],"animation":{{"@type":"animatedChatPhoto","length":480,"file":{},"main_frame_timestamp":0}},"small_animation":{{"@type":"animatedChatPhoto","length":160,"file":{},"main_frame_timestamp":0}}}},"bio":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
            extra.0,
            file(files + 2, still),
            file(files + 3, clip),
            file(files + 4, clip),
        ),
    ]
}

/// `QUILL_DEMO_USERPICS=still`: the photos are not videos.
fn still_only() -> bool {
    std::env::var("QUILL_DEMO_USERPICS").is_ok_and(|mode| mode == "still")
}

fn apply_group(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let mut jsons = Vec::new();
    for (index, member) in (0..).zip(MEMBERS) {
        jsons.extend(member_jsons(session, index, member));
    }
    jsons.extend([
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{USERPIC_GROUP},"title":"Morning Runners","type":{{"@type":"chatTypeSupergroup","supergroup_id":{USERPIC_GROUP},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{USERPIC_GROUP},"is_channel":false,"status":{{"@type":"chatMemberStatusMember"}},"member_count":10}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{USERPIC_GROUP},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"99","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{FIRST_MEMBER},"title":"Noa","type":{{"@type":"chatTypePrivate","user_id":{FIRST_MEMBER}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{FIRST_MEMBER},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"98","is_pinned":false}}}}"#
        ),
    ]);
    for (id, (line, sender)) in (1..).zip(LINES.iter().zip((0..MEMBERS.len() as i64).cycle())) {
        jsons.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{USERPIC_GROUP},"sender_id":{{"@type":"messageSenderUser","user_id":{}}},"is_outgoing":false,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{line}","entities":[]}}}}}}}}"#,
            FIRST_MEMBER + sender,
            1_700_000_000 + id * 60,
        ));
    }
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in &jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    fn demo_group_video_userpics(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_group(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.open_chat(ChatId(USERPIC_GROUP));
        }
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — group of Premium members with video userpics",
        ));
        cx.notify();
    }
}
