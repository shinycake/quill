//! Screenshot demos: the shared-media gallery.
//!
//! `ready-shared-media` opens the gallery of "Demo chat A" over every tab
//! filled through the real `foundChatMessages` path. `QUILL_DEMO_SHARED_MEDIA_TAB`
//! picks the tab (`files`, `music`, `links`, `voice`, `gifs`, `photos`,
//! `videos`, `media`; default `files`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::demo_thumb_png_path;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::state::{RequestPurpose, SelectedMedia, Session, SharedMediaTab};
use quill::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-shared-media",
        "screenshot demo — shared media gallery",
    )
    .setup(QuillApp::demo_ready_shared_media),
];

const CHAT: i64 = 11;
/// 2026-10-10 12:00 UTC.
const BASE_DATE: i64 = 1_791_633_600;

fn file_json(id: i64, size: i64, path: &str) -> Value {
    json!({
        "@type": "file", "id": id, "size": size, "expected_size": size,
        "local": {
            "@type": "localFile", "path": path, "can_be_downloaded": true,
            "can_be_deleted": false, "is_downloading_active": false,
            "is_downloading_completed": !path.is_empty(), "download_offset": 0,
            "downloaded_prefix_size": 0, "downloaded_size": 0
        },
        "remote": { "@type": "remoteFile", "id": "x", "unique_id": "u", "is_uploading_active": false,
            "is_uploading_completed": true, "uploaded_size": size }
    })
}

fn message(id: i64, age_days: i64, outgoing: bool, content: Value) -> Value {
    json!({
        "id": id, "chat_id": CHAT, "is_outgoing": outgoing,
        "sender_id": { "@type": "messageSenderUser", "user_id": CHAT },
        "date": BASE_DATE - age_days * 86_400,
        "content": content,
    })
}

fn text(text: &str, entities: Value) -> Value {
    json!({ "@type": "formattedText", "text": text, "entities": entities })
}

/// A text message with one URL entity and a page preview.
pub(crate) fn link_message(
    id: i64,
    age_days: i64,
    body: &str,
    url: &str,
    title: &str,
    description: &str,
) -> Value {
    let offset = body.find(url).map_or(0, |at| {
        body.get(..at).map_or(0, |head| head.encode_utf16().count())
    });
    message(
        id,
        age_days,
        false,
        json!({ "@type": "messageText",
            "text": text(body, json!([{ "@type": "textEntity", "offset": offset,
                "length": url.encode_utf16().count(),
                "type": { "@type": "textEntityTypeUrl" } }])),
            "link_preview": { "@type": "linkPreview", "url": url, "display_url": url,
                "site_name": "", "title": title, "description": text(description, json!([])),
                "type": { "@type": "linkPreviewTypeArticle" } } }),
    )
}

/// A text message whose `label` is a text link to `url`, with no preview.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn text_link_message(id: i64, body: &str, label: &str, url: &str) -> Value {
    let offset = body.find(label).map_or(0, |at| {
        body.get(..at).map_or(0, |head| head.encode_utf16().count())
    });
    message(
        id,
        0,
        false,
        json!({ "@type": "messageText",
            "text": text(body, json!([{ "@type": "textEntity", "offset": offset,
                "length": label.encode_utf16().count(),
                "type": { "@type": "textEntityTypeTextUrl", "url": url } }])) }),
    )
}

/// A photo message whose "m" size is the demo thumbnail.
pub(crate) fn photo_message(id: i64, age_days: i64, caption: &str) -> Value {
    message(
        id,
        age_days,
        false,
        json!({ "@type": "messagePhoto", "caption": text(caption, json!([])),
            "photo": { "@type": "photo", "has_stickers": false, "sizes": [
                { "@type": "photoSize", "type": "m", "width": 320, "height": 320,
                  "photo": file_json(id + 1000, 24, &demo_thumb_png_path()) } ] } }),
    )
}

fn tab_messages(tab: SharedMediaTab) -> Vec<Value> {
    let thumb = demo_thumb_png_path();
    let document = |id: i64, age: i64, name: &str, size: i64| {
        message(
            id,
            age,
            false,
            json!({ "@type": "messageDocument", "caption": text("", json!([])),
                "document": { "@type": "document", "file_name": name,
                    "mime_type": "application/octet-stream",
                    "document": file_json(id + 1000, size, "") } }),
        )
    };
    let audio = |id: i64, age: i64, performer: &str, title: &str, secs: i64, size: i64| {
        message(
            id,
            age,
            false,
            json!({ "@type": "messageAudio", "caption": text("", json!([])),
                "audio": { "@type": "audio", "duration": secs, "title": title,
                    "performer": performer, "file_name": "track.mp3",
                    "mime_type": "audio/mpeg", "audio": file_json(id + 1000, size, "") } }),
        )
    };
    let voice = |id: i64, age: i64, outgoing: bool, secs: i64| {
        message(
            id,
            age,
            outgoing,
            json!({ "@type": "messageVoiceNote", "caption": text("", json!([])),
                "voice_note": { "@type": "voiceNote", "duration": secs, "waveform": "",
                    "mime_type": "audio/ogg", "voice": file_json(id + 1000, 2048, "") } }),
        )
    };
    let link = |id: i64, age: i64, body: &str, url: &str, title: &str, description: &str| {
        link_message(id, age, body, url, title, description)
    };
    let photo = photo_message;
    let video = |id: i64, age: i64, secs: i64| {
        message(
            id,
            age,
            false,
            json!({ "@type": "messageVideo", "caption": text("", json!([])),
                "video": { "@type": "video", "duration": secs, "width": 320, "height": 320,
                    "file_name": "clip.mp4", "mime_type": "video/mp4",
                    "supports_streaming": true,
                    "thumbnail": { "@type": "thumbnail", "width": 320, "height": 320,
                        "format": { "@type": "thumbnailFormatJpeg" },
                        "file": file_json(id + 2000, 24, &thumb) },
                    "video": file_json(id + 1000, 4_000_000, "") } }),
        )
    };
    let gif = |id: i64, age: i64| {
        message(
            id,
            age,
            false,
            json!({ "@type": "messageAnimation", "caption": text("", json!([])),
                "animation": { "@type": "animation", "duration": 3, "width": 320, "height": 320,
                    "file_name": "loop.gif", "mime_type": "image/gif",
                    "thumbnail": { "@type": "thumbnail", "width": 320, "height": 320,
                        "format": { "@type": "thumbnailFormatJpeg" },
                        "file": file_json(id + 2000, 24, &thumb) },
                    "animation": file_json(id + 1000, 400_000, "") } }),
        )
    };
    match tab {
        SharedMediaTab::Gifs => vec![gif(260, 0), gif(261, 2), gif(262, 9)],
        SharedMediaTab::Files => vec![
            document(210, 0, "Quarterly report.pdf", 4_404_019),
            document(209, 3, "budget-2026.xlsx", 182_272),
            document(208, 9, "photos.zip", 31_457_280),
            document(207, 40, "notes.txt", 900),
        ],
        SharedMediaTab::Music => vec![
            audio(220, 1, "Daft Punk", "Around the World", 429, 6_815_744),
            audio(219, 5, "", "Untitled demo", 95, 1_500_000),
            audio(218, 31, "Nina Simone", "Feeling Good", 177, 3_250_585),
        ],
        SharedMediaTab::Voice => vec![
            voice(230, 0, false, 12),
            voice(229, 2, true, 47),
            voice(228, 35, false, 3),
        ],
        SharedMediaTab::Links => vec![
            link(
                240,
                0,
                "Worth a read: https://blog.example.com/posts/rust-ui",
                "https://blog.example.com/posts/rust-ui",
                "Building desktop UIs in Rust",
                "A walk through retained and immediate mode toolkits, and why GPU rendering changes the trade-offs.",
            ),
            link(
                241,
                4,
                "https://docs.example.org/",
                "https://docs.example.org/",
                "",
                "",
            ),
        ],
        SharedMediaTab::Photos | SharedMediaTab::Media => vec![
            photo(250, 0, "Sunset"),
            video(251, 1, 83),
            photo(252, 2, ""),
            video(253, 8, 3725),
            photo(254, 9, ""),
            photo(255, 9, ""),
            photo(256, 40, ""),
        ],
        SharedMediaTab::Videos => vec![video(251, 1, 83), video(253, 8, 3725)],
        _ => Vec::new(),
    }
}

fn demo_tab() -> SharedMediaTab {
    match std::env::var("QUILL_DEMO_SHARED_MEDIA_TAB")
        .unwrap_or_default()
        .as_str()
    {
        "music" => SharedMediaTab::Music,
        "links" => SharedMediaTab::Links,
        "voice" => SharedMediaTab::Voice,
        "gifs" => SharedMediaTab::Gifs,
        "photos" => SharedMediaTab::Photos,
        "videos" => SharedMediaTab::Videos,
        "media" => SharedMediaTab::Media,
        _ => SharedMediaTab::Files,
    }
}

/// Fill `tab` the way TDLib's answer would.
fn fill_tab(
    session: &mut Session,
    sink: &std::sync::Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
    tab: SharedMediaTab,
    messages: Vec<Value>,
) {
    let generation = session.media.shared_media.begin_fetch(tab);
    let next_from = i64::from(std::env::var_os("QUILL_DEMO_SHARED_MEDIA_LOADING_MORE").is_some());
    let extra = session.request(
        RequestPurpose::GetSharedMedia { tab, generation },
        Some(ChatId(CHAT)),
    );
    let answer = json!({ "@type": "foundChatMessages", "@extra": extra.0,
        "total_count": messages.len(), "next_from_message_id": next_from, "messages": messages });
    if let Some(owned) = copy_and_parse(&answer.to_string(), seq, sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    fn demo_ready_shared_media(
        &mut self,
        _window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let sink: std::sync::Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let tab = demo_tab();
        if let Some(session) = self.demo_session.as_mut() {
            let seq = AtomicU64::new(session.last_seq);
            let user = json!({ "@type": "updateUser", "user": { "@type": "user", "id": CHAT,
                "first_name": "Ada", "last_name": "Lovelace", "usernames": null,
                "phone_number": "", "status": null, "profile_photo": null,
                "is_contact": true, "is_mutual_contact": true, "is_close_friend": false,
                "is_verified": false, "is_premium": false, "is_scam": false, "is_fake": false,
                "is_bot": false, "type": { "@type": "userTypeRegular" } } });
            if let Some(owned) = copy_and_parse(&user.to_string(), &seq, &sink) {
                session.apply(owned);
            }
            session.open_chat(ChatId(CHAT));
            session.media.shared_media.open_for_tab(ChatId(CHAT), tab);
            fill_tab(session, &sink, &seq, tab, tab_messages(tab));
            // QUILL_DEMO_SHARED_MEDIA_LOADING_MORE=1 shows the next page on its way.
            if std::env::var_os("QUILL_DEMO_SHARED_MEDIA_LOADING_MORE").is_some() {
                let _ = session.media.shared_media.begin_fetch_more(tab);
            }
            // QUILL_DEMO_SHARED_MEDIA_SELECT=2 ticks the first two rows.
            let ticked: usize = std::env::var("QUILL_DEMO_SHARED_MEDIA_SELECT")
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let ids: Vec<_> = session
                .media
                .shared_media
                .tab_at(tab)
                .map(|state| {
                    state
                        .items
                        .iter()
                        .map(|i| i.message_id)
                        .take(ticked)
                        .collect()
                })
                .unwrap_or_default();
            let allowed = quill::telegram::envelope::MessageActions {
                can_be_forwarded: true,
                can_be_deleted_only_for_self: true,
                ..Default::default()
            };
            for id in ids {
                session
                    .media
                    .shared_media
                    .toggle_selected(SelectedMedia::from_actions(id, Some(&allowed)));
            }
            self.demo_ui
                .seq
                .store(seq.load(Ordering::SeqCst), Ordering::SeqCst);
        }
        cx.notify();
    }

    /// Open `tab` of the gallery filled with `messages` (UI tests).
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn demo_fill_shared_media(
        &mut self,
        tab: SharedMediaTab,
        messages: Vec<Value>,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let sink: std::sync::Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(session) = self.demo_session.as_mut() {
            let seq = AtomicU64::new(session.last_seq);
            session.media.shared_media.select_tab(tab);
            fill_tab(session, &sink, &seq, tab, messages);
            self.demo_ui
                .seq
                .store(seq.load(Ordering::SeqCst), Ordering::SeqCst);
        }
        cx.notify();
    }
}
