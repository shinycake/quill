//! Screenshot demos of the Edit box (injected, no live Telegram): a
//! supergroup the viewer administers, a channel and a basic group the
//! viewer created, each with a description, and the photo crop over the
//! group's box.
#![deny(clippy::indexing_slicing)]

use super::super::profile_photo_sources::PhotoTarget;
use crate::ui::app::QuillApp;
use crate::ui::groups::apply_ready_group_manage;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats("ready-group-edit-box", "screenshot demo — group Edit box")
        .setup(QuillApp::demo_group_edit_box),
    DemoSpec::chats(
        "ready-channel-edit-box",
        "screenshot demo — channel Edit box"
    )
    .setup(QuillApp::demo_channel_edit_box),
    DemoSpec::chats(
        "ready-basic-group-edit-box",
        "screenshot demo — basic group Edit box"
    )
    .setup(QuillApp::demo_basic_group_edit_box),
    DemoSpec::chats(
        "ready-group-photo-crop",
        "screenshot demo — group photo crop"
    )
    .setup(QuillApp::demo_group_photo_crop),
];

const GROUP: i64 = 61;
const CHANNEL: i64 = 62;
const BASIC: i64 = 63;
const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn apply_edit_box_chats(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    apply_ready_group_manage(session, sink, seq);
    let position = |id: i64| {
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"79","is_pinned":false}}}}"#
        )
    };
    apply(
        session,
        sink,
        seq,
        &[
            format!(
                r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{GROUP},"supergroup_full_info":{{"description":"Weekend hikes around the bay. Routes, carpools and photos.","member_count":5}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{CHANNEL},"title":"Quill Releases","type":{{"@type":"chatTypeSupergroup","supergroup_id":{CHANNEL},"is_channel":true}},"unread_count":0,"can_be_deleted_for_all_users":true}}}}"#
            ),
            position(CHANNEL),
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{CHANNEL},"is_channel":true,"status":{CREATOR},"member_count":1830}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{CHANNEL},"supergroup_full_info":{{"description":"Release notes for every Quill build.","member_count":1830}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{BASIC},"title":"Book club","type":{{"@type":"chatTypeBasicGroup","basic_group_id":{BASIC}}},"unread_count":0,"can_be_deleted_for_all_users":true}}}}"#
            ),
            position(BASIC),
            format!(
                r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":{BASIC},"member_count":4,"status":{CREATOR},"is_active":true}}}}"#
            ),
            format!(
                r#"{{"@type":"updateBasicGroupFullInfo","basic_group_id":{BASIC},"basic_group_full_info":{{"@type":"basicGroupFullInfo","description":"One book a month, we meet on Thursdays.","creator_user_id":777,"members":[]}}}}"#
            ),
        ],
    );
}

/// A picture for the crop demo: a diagonal gradient, wider than tall.
fn demo_picture() -> image::RgbaImage {
    image::RgbaImage::from_fn(900, 600, |x, y| {
        let r = (x * 255 / 900) as u8;
        let g = (y * 255 / 600) as u8;
        image::Rgba([r, g, 200, 255])
    })
}

impl QuillApp {
    fn seed_edit_box(&mut self) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_edit_box_chats(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
    }

    fn demo_edit_box(
        &mut self,
        chat: i64,
        note: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.seed_edit_box();
        self.open_group_edit_box(ChatId(chat), window, cx);
        self.connection.set_note(StatusNote::silent(note));
        cx.notify();
    }

    fn demo_group_edit_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_edit_box(GROUP, "screenshot demo — group Edit box", window, cx);
    }

    fn demo_channel_edit_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_edit_box(CHANNEL, "screenshot demo — channel Edit box", window, cx);
    }

    fn demo_basic_group_edit_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_edit_box(BASIC, "screenshot demo — basic group Edit box", window, cx);
    }

    fn demo_group_photo_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_edit_box(GROUP, "screenshot demo — group photo crop", window, cx);
        self.open_profile_photo_crop(PhotoTarget::Chat(ChatId(GROUP)), demo_picture(), cx);
    }
}
