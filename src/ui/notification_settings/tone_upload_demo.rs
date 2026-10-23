//! Screenshot demo of the sound picker with saved sounds, one file
//! uploading and the "Upload Sound" button (injected, no live Telegram).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use quill::tone_upload::ToneUpload;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-notify-upload-sound",
        "screenshot demo — sound picker with Upload Sound and a file uploading"
    )
    .setup(QuillApp::demo_notify_upload_sound),
];

fn saved_sound(id: i64, title: &str, duration: i32) -> String {
    format!(
        r#"{{"@type":"notificationSound","id":"{id}","duration":{duration},"date":1,"title":"{title}","data":"","sound":{{"@type":"file","id":{id},"size":4000,"expected_size":4000,"local":{{"@type":"localFile","path":"","can_download":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r{id}","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4000}}}}}}"#
    )
}

impl QuillApp {
    fn demo_notify_upload_sound(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat_id) = self.demo_session.as_ref().map(|_| ChatId(11)) else {
            return;
        };
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            session.open_chat(chat_id);
            let extra = session.request(RequestPurpose::GetSavedNotificationSounds, None);
            let json = format!(
                r#"{{"@type":"notificationSounds","@extra":"{}","notification_sounds":[{},{}]}}"#,
                extra.as_extra(),
                saved_sound(901, "Soft Bell", 2),
                saved_sound(902, "Marimba", 4),
            );
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_ui.sink.clone();
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
            session.settings.tone_upload = ToneUpload::Uploading {
                name: "Glass Chime".into(),
            };
        }
        self.open_mute_menu(cx);
        self.notify.notif_sound_picker_open = true;
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — sound picker · Upload Sound",
        ));
    }
}
