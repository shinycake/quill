//! Screenshot demo: a group with three photos, its info panel open. A
//! click on the avatar opens the photo viewer on the current photo, with
//! the two earlier ones after it ("Photo 1 of 3").
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
    // A group you created, with a current photo and two earlier ones in
    // its photo history: click the avatar for the viewer, where Delete and
    // Set as Main are offered.
    DemoSpec::chats(
        "ready-info-group-photos",
        "screenshot demo — group info panel, three group photos"
    )
    .setup(QuillApp::demo_info_group_photos),
];

/// Used as both the chat and the supergroup id, like the other demos.
pub(in crate::ui) const PHOTO_GROUP: i64 = 93;

/// `(photo id, change message id, fixture, first file id)`, newest first.
const PHOTOS: [(i64, i64, &str, i32); 3] = [
    (9303, 30, "showcase/p1.jpg", 9310),
    (9302, 20, "showcase/p3.jpg", 9320),
    (9301, 10, "showcase/p4.jpg", 9330),
];

fn fixture_file(id: i32, fixture: &str) -> String {
    let path = demo_media_allowlist().join(fixture);
    demo_file_json(id, &path.to_string_lossy(), true)
}

/// A `chatPhoto` with a grid size and a full size, both on disk.
fn chat_photo_json(photo_id: i64, fixture: &str, file: i32) -> String {
    format!(
        r#"{{"@type":"chatPhoto","id":"{photo_id}","added_date":1700000000,"sizes":[{{"@type":"photoSize","type":"a","photo":{},"width":160,"height":107,"progressive_sizes":[]}},{{"@type":"photoSize","type":"c","photo":{},"width":1100,"height":739,"progressive_sizes":[]}}]}}"#,
        fixture_file(file, fixture),
        fixture_file(file + 1, fixture),
    )
}

fn apply_group_photos(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let Some((current_id, _, current_fixture, current_file)) = PHOTOS.first().copied() else {
        return;
    };
    let mut jsons = vec![
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{PHOTO_GROUP},"title":"Trail Runners","type":{{"@type":"chatTypeSupergroup","supergroup_id":{PHOTO_GROUP},"is_channel":false}},"photo":{{"@type":"chatPhotoInfo","small":{},"big":{},"has_animation":false,"is_personal":false}},"unread_count":0}}}}"#,
            fixture_file(current_file + 5, current_fixture),
            fixture_file(current_file + 6, current_fixture),
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{PHOTO_GROUP},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"90","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{PHOTO_GROUP},"is_channel":false,"status":{{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}},"member_count":42}}}}"#
        ),
    ];
    let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, PHOTO_GROUP);
    jsons.push(format!(
        r#"{{"@type":"supergroupFullInfo","@extra":"{}","photo":{},"description":"Saturday long runs and the occasional hill repeat.","member_count":42}}"#,
        extra.0,
        chat_photo_json(current_id, current_fixture, current_file),
    ));
    let changes: Vec<String> = PHOTOS
        .iter()
        .map(|(photo_id, message_id, fixture, file)| {
            format!(
                r#"{{"@type":"message","id":{message_id},"chat_id":{PHOTO_GROUP},"is_outgoing":true,"date":1700000000,"content":{{"@type":"messageChatChangePhoto","photo":{}}}}}"#,
                chat_photo_json(*photo_id, fixture, *file),
            )
        })
        .collect();
    let extra = session.request(
        RequestPurpose::GetChatPhotoHistory,
        Some(ChatId(PHOTO_GROUP)),
    );
    jsons.push(format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":{},"next_from_message_id":0,"messages":[{}]}}"#,
        extra.0,
        PHOTOS.len(),
        changes.join(","),
    ));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    fn demo_info_group_photos(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_group_photos(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(PHOTO_GROUP), window, cx);
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — group info panel, three group photos",
        ));
        cx.notify();
    }
}
