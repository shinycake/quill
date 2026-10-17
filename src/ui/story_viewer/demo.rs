//! `ready-story-viewer` screenshot demo: the showcase account's story tray
//! with a photo behind every story, opened the way a tray tap opens it
//! (injected, no live Telegram). `QUILL_DEMO_STORY=gone` instead opens a
//! story the server no longer has, which shows the broken-link toast.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::showcase_demo::{MAYA, Scene, apply_ready_showcase};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Leo's single story: the tray tap the demo replays.
const LEO: i64 = 105;
/// The showcase tray's stories: (poster, story, picture, caption).
const STORIES: [(i64, i32, &str, &str); 5] = [
    (MAYA, 5, "p1.jpg", "Trailhead at six"),
    (MAYA, 6, "p2.jpg", ""),
    (LEO, 3, "p3.jpg", "Ridge at golden hour"),
    (106, 2, "p4.jpg", ""),
    (54, 1, "p5.jpg", "Lunch spot"),
];

register_demos![
    DemoSpec::chats(
        "ready-story-viewer",
        "screenshot demo — story viewer opened from the story tray"
    )
    .setup(QuillApp::demo_story_viewer),
];

impl QuillApp {
    fn demo_story_viewer(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let gone = std::env::var("QUILL_DEMO_STORY").is_ok_and(|v| v == "gone");
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_showcase(
            session,
            &self.demo_ui.sink,
            &self.demo_ui.seq,
            Scene::Hikers,
        );
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let now = quill::local_time::now_unix();
        for (index, (chat, story, picture, caption)) in STORIES.into_iter().enumerate() {
            let path = demo_media_allowlist().join("showcase").join(picture);
            let file = demo_file_json(7100 + index as i32, &path.to_string_lossy(), true);
            let caption = serde_json::to_string(caption).unwrap_or_default();
            let json = format!(
                r#"{{"@type":"updateStory","story":{{"@type":"story","id":{story},"poster_chat_id":{chat},"date":{now},"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"y","photo":{file},"width":1100,"height":740,"progressive_sizes":[]}}]}}}},"caption":{{"@type":"formattedText","text":{caption},"entities":[]}}}}}}"#
            );
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        if gone {
            session.stories.missing.insert((LEO, 99));
            self.open_story_viewer(ChatId(LEO), 99, cx);
        } else {
            self.open_story_viewer(ChatId(LEO), 3, cx);
        }
    }
}
