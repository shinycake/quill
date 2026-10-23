//! Screenshot demos of the story album picker and the video story cover
//! picker (injected session, no live Telegram): the channel's page with eight
//! stories to pick from, and a video story whose cover strip is cut from the
//! demo clip by the real frame path.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::story_page::StoryPage;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::state::unix_ms_now;
use quill::story_album_picker::AlbumPickerTarget;
use quill::story_page::{ArchivedStories, ChatPageStories};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope_story::ParsedStoryAlbum;
use std::sync::Arc;
use std::sync::atomic::Ordering;

const CHANNEL: i64 = 13;
const ALBUM: i32 = 4;
const PHOTOS: [&str; 6] = ["p1", "p2", "p3", "p4", "p5", "p6"];
const FIRST_FILE: i32 = 5100;
const CLIP_FILE: i32 = 5190;

register_demos![
    DemoSpec::chats(
        "ready-story-album-picker",
        "screenshot demo — the story album picker over eight stories, two ticked beyond the album"
    )
    .setup(QuillApp::demo_story_album_picker),
    DemoSpec::chats(
        "ready-story-cover-strip",
        "screenshot demo — a video story's cover picker: preview over the frame strip"
    )
    .setup(QuillApp::demo_story_cover_strip)
    .timing(4500, 6500),
];

fn story_json(id: i32, posted: i32, content: String) -> String {
    format!(
        r#"{{"@type":"story","id":{id},"poster_chat_id":{CHANNEL},"date":{posted},"is_edited":false,"can_be_added_to_album":true,"can_be_deleted":true,"can_be_edited":true,"can_be_forwarded":true,"can_be_replied":false,"can_set_privacy_settings":true,"content":{content},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#
    )
}

impl QuillApp {
    /// Stories 1 to 8: six photos, then two videos.
    fn demo_album_stories(&mut self) -> Vec<i32> {
        let Some(session) = self.demo_session.as_mut() else {
            return Vec::new();
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
        let posted = i32::try_from(unix_ms_now() / 1000 - 3 * 3600).unwrap_or(0);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let seq = &self.demo_ui.seq;
        let mut ingest = |json: String| {
            if let Some(owned) = copy_and_parse(&json, seq, &sink) {
                session.apply(owned);
            }
        };
        let root = demo_media_allowlist();
        let mut ids = Vec::new();
        for (index, name) in PHOTOS.iter().enumerate() {
            let id = index as i32 + 1;
            let file_id = FIRST_FILE + id;
            let path = root.join(format!("showcase/{name}.jpg"));
            let file = demo_file_json(file_id, &path.to_string_lossy(), true);
            ingest(file.clone());
            ingest(story_json(
                id,
                posted - id * 60,
                format!(
                    r#"{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"x","photo":{file},"width":1080,"height":1920,"progressive_sizes":[]}}]}}}}"#
                ),
            ));
            ids.push(id);
        }
        let clip = root.join("demo-clip-12s.mp4");
        let clip_json = demo_file_json(CLIP_FILE, &clip.to_string_lossy(), true);
        ingest(clip_json.clone());
        for (offset, thumb) in ["p3", "p5"].into_iter().enumerate() {
            let id = 7 + offset as i32;
            let thumb_file = FIRST_FILE + 20 + id;
            let path = root.join(format!("showcase/{thumb}.jpg"));
            let thumb_json = demo_file_json(thumb_file, &path.to_string_lossy(), true);
            ingest(thumb_json.clone());
            ingest(story_json(
                id,
                posted - id * 60,
                format!(
                    r#"{{"@type":"storyContentVideo","video":{{"@type":"storyVideo","duration":12.0,"width":720,"height":1280,"has_stickers":false,"is_animation":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":720,"height":1280,"file":{thumb_json}}},"preload_prefix_size":0,"cover_frame_timestamp":0.0,"video":{clip_json}}},"alternative_video":null}}"#
                ),
            ));
            ids.push(id);
        }
        ids
    }

    fn demo_story_album_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.demo_album_stories();
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.stories.albums.insert(
            CHANNEL,
            vec![ParsedStoryAlbum {
                id: ALBUM,
                name: "Harbour".into(),
            }],
        );
        session
            .stories
            .album_stories
            .insert((CHANNEL, ALBUM), vec![2, 5]);
        session.stories.archived_stories.insert(
            CHANNEL,
            ArchivedStories {
                story_ids: ids.clone(),
                total_count: ids.len() as i32,
                next_from_story_id: None,
            },
        );
        session.stories.chat_page_stories.insert(
            CHANNEL,
            ChatPageStories {
                story_ids: ids,
                pinned_story_ids: Vec::new(),
                total_count: 8,
            },
        );
        let mut page = StoryPage::new(ChatId(CHANNEL), window, cx);
        page.open_album = Some(ALBUM);
        self.stories.page = Some(page);
        self.open_album_picker(AlbumPickerTarget::Album(ALBUM), cx);
        if let Some(picker) = self
            .stories
            .page
            .as_mut()
            .and_then(|page| page.picker.as_mut())
        {
            picker.toggle(4);
            picker.toggle(7);
        }
        cx.notify();
    }

    fn demo_story_cover_strip(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.demo_album_stories();
        let Some(session) = self.demo_session.as_ref() else {
            return;
        };
        let items = quill::story_viewer::collect_found_story_items(
            &[(CHANNEL, 7)],
            &session.stories.stories,
        );
        self.begin_story_viewer(items, 0, cx);
        self.toggle_story_cover_edit(CHANNEL, 7, cx);
        cx.notify();
    }
}
