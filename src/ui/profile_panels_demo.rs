//! B10 screenshot-demo fixtures for the profile and contact panels
//! (`--screenshot-demo ready-profile-panels`, mode from
//! `QUILL_DEMO_PROFILE`). Everything is injected through the real
//! reducers; no live Telegram. English-only text.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_media_allowlist};
use super::dialogs::ProfileDialog;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::profile_forms::PersonalPhotoMode;
use quill::state::{InfoPanelTarget, ProfileChatsKind, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn photo_json(id: i64, thumb_id: i32, full_id: i32, file: &str) -> String {
    let thumb = demo_file_json(thumb_id, file, true);
    let full = demo_file_json(full_id, file, true);
    format!(
        r#"{{"@type":"chatPhoto","id":{id},"added_date":1790000000,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":160,"height":160,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":640,"height":480,"progressive_sizes":[]}}]}}"#
    )
}

/// Ada (31, contact) with a bio, birthday, personal channel, private note,
/// three groups in common and three profile photos; the viewer (777) with
/// no birthday or personal channel; two channels with similar channels.
fn apply_profile_panels(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    session.my_user_id = Some(777);
    let chat = |id: i64, title: &str, sg: i64, channel: bool| {
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{sg},"is_channel":{channel}}},"unread_count":0}}}}"#
        )
    };
    let user = |id: i64, first: &str, last: &str, phone: &str, username: &str, contact: bool| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}","collectible_usernames":[]}},"phone_number":"{phone}","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":{contact},"type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    for json in [
        user(777, "Demo", "Viewer", "15550131", "demoviewer", false),
        user(31, "Ada", "Lovelace", "15550101031", "adalove", true),
        chat(-1001000000001, "Rust Programmers", 1, false),
        chat(-1001000000002, "Weekend Hikers", 2, false),
        chat(-1001000000003, "Analytical Engine Fans", 3, false),
        chat(-1001000000004, "Engine Notes", 4, true),
        chat(-1001000000005, "Compiler Weekly", 5, true),
        chat(-1001000000006, "Systems Digest", 6, true),
        chat(-1001000000007, "Kernel Newsletter", 7, true),
    ] {
        apply(session, sink, seq, &json);
    }
    let fixtures = demo_media_allowlist();
    let file = |name: &str| fixtures.join(name).to_string_lossy().into_owned();
    let main_photo = photo_json(9001, 9101, 9102, &file("demo-thumb.png"));
    let personal_photo = photo_json(9500, 9501, 9502, &file("demo-gif-2.png"));
    let info = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","block_list":null,"photo":{main_photo},"need_phone_number_privacy_exception":true,"uses_unofficial_app":true,"personal_photo":{personal_photo},"group_in_common_count":3,"personal_chat_id":-1001000000004,"birthdate":{{"@type":"birthdate","day":10,"month":12,"year":1815}},"bio":{{"@type":"formattedText","text":"Mathematician. Notes on the Analytical Engine.","entities":[]}},"note":{{"@type":"formattedText","text":"Met at the compilers meetup","entities":[]}},"bot_info":null}}"#,
            info.0
        ),
    );
    let me = session.request_for_user(RequestPurpose::GetUserFullInfo, 777);
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","block_list":null,"photo":{},"group_in_common_count":0,"personal_chat_id":0,"birthdate":null,"bio":{{"@type":"formattedText","text":"","entities":[]}},"bot_info":null}}"#,
            me.0,
            photo_json(9201, 9301, 9302, &file("demo-gif-1.png")),
        ),
    );
    let groups = session.request_for_user(
        RequestPurpose::GetProfileChats(ProfileChatsKind::GroupsInCommon),
        31,
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":3,"chat_ids":[-1001000000001,-1001000000002,-1001000000003]}}"#,
            groups.0
        ),
    );
    let similar = session.request(
        RequestPurpose::GetProfileChats(ProfileChatsKind::SimilarChats),
        Some(ChatId(13)),
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":3,"chat_ids":[-1001000000005,-1001000000006,-1001000000007]}}"#,
            similar.0
        ),
    );
    let suitable = session.request(
        RequestPurpose::GetProfileChats(ProfileChatsKind::SuitablePersonalChats),
        None,
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[-1001000000004,-1001000000005]}}"#,
            suitable.0
        ),
    );
    let photos = session.request_for_user(RequestPurpose::GetUserProfilePhotos, 31);
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chatPhotos","@extra":"{}","total_count":3,"photos":[{},{},{}]}}"#,
            photos.0,
            photo_json(9001, 9101, 9102, &file("demo-thumb.png")),
            photo_json(9002, 9103, 9104, &file("demo-gif-1.png")),
            photo_json(9003, 9105, 9106, &file("demo-gif-2.png")),
        ),
    );
}

register_demos![
    // B10: profile and contact panels (`QUILL_DEMO_PROFILE=contact|self|
    // edit-contact|birthday|channel|share|gallery|similar`; injected data,
    // no live Telegram).
    DemoSpec::chats(
        "ready-profile-panels",
        "screenshot demo — profile and contact panels (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_setup_profile_panels),
];

impl QuillApp {
    /// `QUILL_DEMO_PROFILE=contact|self|edit-contact|birthday|channel|share|
    /// gallery|similar` (default `contact`).
    fn demo_setup_profile_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_PROFILE").unwrap_or_else(|_| "contact".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_profile_panels(session, &self.demo_sink, &self.demo_seq);
            session.open_info_panel = Some(match mode.as_str() {
                "self" | "birthday" | "channel" => InfoPanelTarget::User(777),
                "similar" => {
                    session.open_chat(ChatId(13));
                    InfoPanelTarget::Supergroup(13)
                }
                _ => InfoPanelTarget::User(31),
            });
        }
        match mode.as_str() {
            "edit-contact" => self.open_edit_contact_dialog(31, window, cx),
            "birthday" => self.open_birthday_dialog(window, cx),
            "channel" => self.open_personal_channel_dialog(cx),
            "share" => self.open_share_contact_dialog(31, cx),
            "gallery" => self.open_profile_photos(31, cx),
            "report" => {
                self.profile_dialog = Some(ProfileDialog::ReportPhoto {
                    user_id: 31,
                    file_id: 9502,
                });
            }
            "personal-photo" => {
                self.profile_dialog = Some(ProfileDialog::PersonalPhoto {
                    user_id: 31,
                    mode: PersonalPhotoMode::Set,
                    path: Some("/tmp/ada.png".into()),
                });
            }
            "contact" => {
                self.status_note = "Phone number copied to clipboard".into();
            }
            _ => {}
        }
    }
}
