//! Screenshot demo of a photo story in the story viewer (injected, no live
//! Telegram): the demo channel's first of three photo stories, posted two
//! hours ago, edited, and shown to contacts only, so the header carries the
//! age, "edited", the position and the privacy badge. The second story
//! reposts the first, so it shows the "Reposted from" link.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::state::unix_ms_now;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(super) const CHANNEL: i64 = 13;
const ORIGIN: i32 = 71;
pub(super) const STORIES: [i32; 3] = [ORIGIN, 72, 73];
const PHOTO_FILE: i32 = 4471;

register_demos![
    DemoSpec::chats(
        "ready-story-photo",
        "screenshot demo — photo story with its age, edited mark and privacy badge"
    )
    .setup(QuillApp::demo_story_photo),
];

impl QuillApp {
    fn demo_story_photo(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
        let path = demo_media_allowlist().join("showcase/p2.jpg");
        let file = demo_file_json(PHOTO_FILE, &path.to_string_lossy(), true);
        let posted = i32::try_from(unix_ms_now() / 1000 - 2 * 3600 - 300).unwrap_or(0);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let seq = &self.demo_ui.seq;
        let mut ingest = |json: String| {
            if let Some(owned) = copy_and_parse(&json, seq, &sink) {
                session.apply(owned);
            }
        };
        ingest(file.clone());
        for (index, id) in STORIES.into_iter().enumerate() {
            let privacy = if index == 0 {
                r#"{"@type":"storyPrivacySettingsContacts","except_user_ids":[]}"#
            } else {
                "null"
            };
            // The second story reposts the first, for the "Reposted from" link.
            let repost = if index == 1 {
                format!(
                    r#"{{"@type":"storyRepostInfo","origin":{{"@type":"storyOriginPublicStory","chat_id":{CHANNEL},"story_id":{ORIGIN}}},"is_content_modified":false}}"#
                )
            } else {
                "null".to_string()
            };
            ingest(format!(
                r#"{{"@type":"story","id":{id},"poster_chat_id":{CHANNEL},"date":{posted},"is_edited":{edited},"privacy_settings":{privacy},"repost_info":{repost},"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"x","photo":{file},"width":1080,"height":1920,"progressive_sizes":[]}}]}}}},"caption":{{"@type":"formattedText","text":"Harbour at dusk","entities":[]}}}}"#,
                edited = index == 0,
            ));
        }
        let keys: Vec<(i64, i32)> = STORIES.iter().map(|id| (CHANNEL, *id)).collect();
        let items = quill::story_viewer::collect_found_story_items(&keys, &session.stories.stories);
        self.begin_story_viewer(items, 0, cx);
        cx.notify();
    }
}
