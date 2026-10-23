//! `ready-story-viewer` screenshot demo: the showcase account's story tray
//! with a photo behind every story, opened the way a tray tap opens it
//! (injected, no live Telegram). `QUILL_DEMO_STORY=gone` instead opens a
//! linked story the server no longer has, which shows the broken-link
//! toast, and `QUILL_DEMO_STORY=card` opens Leo's story the way a story
//! card in a message does: alone, with no neighbours. `composer` opens the
//! story composer instead, and `composer-picked` opens it with a photo
//! already picked.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::showcase_demo::{MAYA, Scene, apply_ready_showcase};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::story_viewer::StoryOpenScope;
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
    fn demo_story_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let variant = std::env::var("QUILL_DEMO_STORY").unwrap_or_default();
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
        let own = variant.starts_with("own");
        for (index, (chat, story, picture, caption)) in STORIES.into_iter().enumerate() {
            let flags = story_flags(chat == LEO && story == 3, own);
            let path = demo_media_allowlist().join("showcase").join(picture);
            let file = demo_file_json(7100 + index as i32, &path.to_string_lossy(), true);
            let caption = serde_json::to_string(caption).unwrap_or_default();
            let json = format!(
                r#"{{"@type":"updateStory","story":{{"@type":"story","id":{story},"poster_chat_id":{chat},"date":{now},{flags}"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"y","photo":{file},"width":1100,"height":740,"progressive_sizes":[]}}]}}}},"caption":{{"@type":"formattedText","text":{caption},"entities":[]}}}}}}"#
            );
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        match variant.as_str() {
            "gone" => {
                session.stories.missing.insert((LEO, 99));
                self.open_story_viewer(ChatId(LEO), 99, StoryOpenScope::Link, cx);
            }
            "own" | "own-viewers" | "own-viewers-loading" => {
                self.open_story_viewer(ChatId(LEO), 3, StoryOpenScope::Single, cx);
                if variant != "own" {
                    self.stories.viewers_open = true;
                    if let Some(session) = self.demo_session.as_mut() {
                        session.stories.viewers =
                            Some(demo_viewers(variant == "own-viewers-loading"));
                    }
                }
            }
            "card" => self.open_story_viewer(ChatId(LEO), 3, StoryOpenScope::Single, cx),
            "composer" | "composer-picked" => {
                self.open_story_composer(window, cx);
                if variant == "composer-picked" {
                    let photo = demo_media_allowlist().join("showcase").join("p3.jpg");
                    self.take_story_media(&[photo], cx);
                }
            }
            _ => self.open_story_viewer(ChatId(LEO), 3, StoryOpenScope::Tray, cx),
        }
    }
}

/// The flags TDLib sets on the story the demo opens: yours (manageable,
/// with views) or someone else's (repliable, shareable).
fn story_flags(opened: bool, own: bool) -> &'static str {
    match (opened, own) {
        (false, _) => "",
        (true, true) => {
            r#""can_be_deleted":true,"can_be_edited":true,"can_be_forwarded":true,"can_set_privacy_settings":true,"can_get_interactions":true,"can_get_statistics":true,"interaction_info":{"@type":"storyInteractionInfo","view_count":128,"forward_count":4,"reaction_count":23,"recent_viewer_user_ids":[]},"#
        }
        (true, false) => r#""can_be_replied":true,"can_be_forwarded":true,"#,
    }
}

/// A viewers list with more pages to come; `loading` shows the spinner row.
fn demo_viewers(loading: bool) -> quill::state::StoryViewersState {
    use quill::telegram::envelope::{MessageSender, StoryInteractionKind, StoryInteractionView};
    let now = i32::try_from(quill::local_time::now_unix()).unwrap_or(0);
    let row = |user_id: i64, minutes: i32, emoji: Option<&str>| StoryInteractionView {
        actor: MessageSender::User { user_id },
        interaction_date: now - minutes * 60,
        reaction_emoji: emoji.map(str::to_string),
        reaction_extra: None,
        kind: StoryInteractionKind::View,
    };
    quill::state::StoryViewersState {
        chat_id: LEO,
        story_id: 3,
        total_count: 128,
        rows: vec![
            row(MAYA, 3, Some("❤")),
            row(106, 9, None),
            row(54, 21, Some("🔥")),
            row(MAYA + 1, 44, None),
        ],
        next_offset: "more".to_string(),
        loading,
        error: None,
    }
}
