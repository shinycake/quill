//! screenshot demo seeds + apply_demo_* mutations.

use super::app::QuillApp;
use super::message_text::interaction_info_update_json;
use super::notifications::notification_settings_json;
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, ComposerAttachment, ComposerEdit, ComposerReplyTo, ForwardDraft,
};
use quill::data_settings::StorageChatStats;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{AccountKey, ChatId, FileId, MessageId};
use quill::state::{RequestPurpose, Session, effective_preview};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    ChatFolderInfo, ChatFolderSpec, ChatNotificationSettings, ConnectionState, MessageContent,
    ParsedSession, ParsedWebsite, PasswordState, StarSubscriptionData, StarSubscriptionPricing,
    StarSubscriptionTypeData, StarSubscriptionsData, StickerFormat, StickerItem,
    StorageFileTypeStats, StorageStats, toggle_chosen_emoji_reaction,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
pub(super) fn seed_ready_chats_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::ReadyChats)
}

pub(super) fn seed_ready_unread_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::UnreadBadge)
}

pub(super) fn seed_ready_unread_read_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::AfterMarkRead)
}

/// Slice parity:platform-offline-indicator — ReadyChats fixture with the
/// client offline (`connectionStateWaitingForNetwork`), so the offline
/// banner renders for screenshots.
pub(super) fn seed_ready_offline_session(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_ready_chats_session(sink);
    session.connection = ConnectionState::WaitingForNetwork;
    session
}

/// Slice parity:platform-reconnect-states — ReadyChats fixture with the
/// client mid-reconnect (`connectionStateUpdating`), so the transitional
/// strip renders with its per-state label for screenshots.
pub(super) fn seed_ready_reconnecting_session(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_ready_chats_session(sink);
    session.connection = ConnectionState::Updating;
    session
}

/// Custom emoji inline rendering — ReadyChats fixture plus a message with a
/// `textEntityTypeCustomEmoji` entity, a resolved `getCustomEmojiStickers`
/// cache entry, and its sticker file downloaded (injected, no live Telegram).
pub(super) fn seed_ready_custom_emoji_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    // Continue the envelope sequence from the ReadyChats seed: `Session::apply`
    // ignores out-of-order envelopes, so restarting at 0 would silently drop
    // every injected update.
    let seq = AtomicU64::new(session.last_seq);
    let apply = |session: &mut Session, json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    // "Custom emoji: 😀 inline" — the emoji is at UTF-16 offset 14, length 2.
    apply(
        &mut session,
        r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":false,"date":1790632300,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Custom emoji: 😀 inline","entities":[{"@type":"textEntity","offset":14,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4242"}}]}}}}"#,
    );
    // The sticker file the custom emoji resolves to (completed download).
    apply(
        &mut session,
        &demo_file_json(61, &demo_thumb_png_path(), true),
    );
    session.emoji.custom_emoji_stickers.push(StickerItem {
        custom_emoji_id: Some(4242),
        id: 4242,
        set_id: 0,
        emoji: "😀".to_string(),
        width: 512,
        height: 512,
        format: StickerFormat::Webp,
        file_id: FileId(61),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
    });
    session.open_chat(ChatId(11));
    session
}

/// Suggest-animated-emoji fixture — an injected `animatedEmoji` answer
/// (with its sticker file downloaded) so the composer suggestion row
/// renders for screenshots without a live Telegram login.
pub(super) fn seed_ready_animated_emoji_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    // Continue the envelope sequence from the ReadyChats seed: `Session::apply`
    // ignores out-of-order envelopes, so restarting at 0 would silently drop
    // every injected update.
    let seq = AtomicU64::new(session.last_seq);
    let apply = |session: &mut Session, json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    // The sticker file the animated emoji resolves to (completed download).
    apply(
        &mut session,
        &demo_file_json(61, &demo_thumb_png_path(), true),
    );
    session.emoji.animated_emoji = Some(StickerItem {
        custom_emoji_id: None,
        id: 9001,
        set_id: 77,
        emoji: "🔥".to_string(),
        width: 512,
        height: 512,
        format: StickerFormat::Tgs,
        file_id: FileId(61),
        // The static preview: a real `animatedEmoji` answer carries the
        // sticker thumbnail, which is what renders (TGS itself is not
        // played — same as the sticker suggestion row).
        thumb_file_id: Some(FileId(61)),
        thumb_width: 512,
        thumb_height: 512,
        requires_premium: false,
    });
    session.emoji.animated_emoji_for = Some("🔥".to_string());
    session.open_chat(ChatId(11));
    session
}

/// Phase C1b: connected-video-call fixture — Zed's incoming video
/// call goes pending → exchanging keys → ready, so the call overlay
/// renders the video-stage placeholder grid. Injected, no live
/// Telegram, no media.
/// Phase C2e: synthetic video-call fixture frames — 320x240 RGBA test
/// patterns generated in code so the two feeds are visually distinct
/// (remote: teal gradient + circle; local: warm gradient + crosshair).
/// NOT a real camera: screenshot demos only.
pub(super) fn demo_video_frame(is_local: bool) -> quill::calls::engine::VideoFrame {
    const W: usize = 320;
    const H: usize = 240;
    let mut rgba = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        for x in 0..W {
            let fx = x as f32 / (W - 1) as f32;
            let fy = y as f32 / (H - 1) as f32;
            let (mut r, mut g, mut b) = if is_local {
                // Warm gradient.
                (
                    (200.0 + 55.0 * fx) as u8,
                    (110.0 + 60.0 * fy) as u8,
                    (60.0 + 40.0 * fx) as u8,
                )
            } else {
                // Teal gradient.
                (
                    (20.0 + 40.0 * fx) as u8,
                    (120.0 + 80.0 * fy) as u8,
                    (140.0 + 60.0 * fx) as u8,
                )
            };
            if is_local {
                // Crosshair.
                if (x as i32 - W as i32 / 2).abs() <= 2 || (y as i32 - H as i32 / 2).abs() <= 2 {
                    (r, g, b) = (255, 255, 255);
                }
            } else {
                // Circle.
                let dx = x as i32 - W as i32 / 2;
                let dy = y as i32 - H as i32 / 2;
                if dx * dx + dy * dy < 50 * 50 {
                    (r, g, b) = (170, 240, 240);
                }
            }
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    quill::calls::engine::VideoFrame {
        seq: 0,
        width: W as u16,
        height: H as u16,
        rgba,
        is_local,
        participant_user_id: None,
        is_screen: false,
    }
}

pub(super) fn seed_ready_media_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::Media)
}

/// MED3 screenshot fixture: `DemoSeed::Media` (document 24 = notes.txt)
/// plus a mid-download state for it, a completed document (25 =
/// report.pdf, in the recent list) and a failed one (26 = Retry chip),
/// with the downloads panel open.
pub(super) fn seed_ready_downloads_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_demo_session(sink, DemoSeed::Media);
    // Continue the envelope sequence from the Media seed: `Session::apply`
    // ignores out-of-order envelopes (`seq <= last_seq`), so restarting at 0
    // would silently drop every injected update.
    let seq = AtomicU64::new(session.last_seq);
    let apply = |session: &mut Session, json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    let document = |msg_id: i64, file_id: i32, name: &str, size: i64| {
        // serde_json::json! instead of a format! string: the nested
        // braces are no longer hand-counted (and the compiler can't
        // catch a JSON brace typo, only a format-string one).
        serde_json::json!({
            "@type": "updateNewMessage",
            "message": {
                "id": msg_id,
                "chat_id": 11,
                "is_outgoing": false,
                "content": {
                    "@type": "messageDocument",
                    "document": {
                        "@type": "document",
                        "file_name": name,
                        "mime_type": "application/pdf",
                        "document": {
                            "@type": "file",
                            "id": file_id,
                            "size": size,
                            "expected_size": size,
                            "local": {
                                "@type": "localFile",
                                "path": "",
                                "can_be_downloaded": true,
                                "can_be_deleted": false,
                                "is_downloading_active": false,
                                "is_downloading_completed": false,
                                "download_offset": 0,
                                "downloaded_prefix_size": 0,
                                "downloaded_size": 0
                            },
                            "remote": {
                                "@type": "remoteFile",
                                "id": "x",
                                "unique_id": "u",
                                "is_uploading_active": false,
                                "is_uploading_completed": true,
                                "uploaded_size": size
                            }
                        }
                    },
                    "caption": {"@type": "formattedText", "text": "", "entities": []}
                }
            }
        })
        .to_string()
    };
    let file_update = |file_id: i32, size: i64, downloaded: i64, active: bool, path: &str| {
        serde_json::json!({
            "@type": "updateFile",
            "file": {
                "@type": "file",
                "id": file_id,
                "size": size,
                "expected_size": size,
                "local": {
                    "@type": "localFile",
                    "path": path,
                    "can_be_downloaded": true,
                    "can_be_deleted": false,
                    "is_downloading_active": active,
                    "is_downloading_completed": !active && downloaded >= size,
                    "download_offset": 0,
                    "downloaded_prefix_size": downloaded,
                    "downloaded_size": downloaded
                },
                "remote": {
                    "@type": "remoteFile",
                    "id": "x",
                    "unique_id": "u",
                    "is_uploading_active": false,
                    "is_uploading_completed": true,
                    "uploaded_size": size
                }
            }
        })
        .to_string()
    };
    // Completed document 25 → lands in the recent list.
    apply(&mut session, &document(204, 25, "report.pdf", 460_800));
    session.begin_download(FileId(25));
    session.user_downloads.insert(25);
    apply(
        &mut session,
        &file_update(25, 460_800, 460_800, false, "/tmp/report.pdf"),
    );
    // Active document 24 (the Media seed's notes.txt): 42% progress.
    session.begin_download(FileId(24));
    session.user_downloads.insert(24);
    apply(&mut session, &file_update(24, 24, 10, true, ""));
    // Paused document 27 → "paused" state + Resume toggle on the chip and
    // the manager row (slice media-downloads-pause).
    apply(
        &mut session,
        &document(206, 27, "big-video.mp4", 100_000_000),
    );
    session.begin_download(FileId(27));
    session.user_downloads.insert(27);
    apply(
        &mut session,
        &file_update(27, 100_000_000, 30_000_000, true, ""),
    );
    apply(
        &mut session,
        &serde_json::json!({
            "@type": "updateFileDownload",
            "file_id": 27,
            "complete_date": 0,
            "is_paused": true,
            "counts": {
                "@type": "downloadedFileCounts",
                "being_downloaded": 2,
                "recently_downloaded": 1
            }
        })
        .to_string(),
    );
    // Failed document 26 → Retry chip on the row.
    apply(&mut session, &document(205, 26, "archive.zip", 1_048_576));
    session.failed_downloads.insert(26);
    session.downloads_panel_open = true;
    session
}

pub(super) fn seed_ready_send_media_session(sink: Arc<MemorySink>) -> Session {
    seed_demo_session(sink, DemoSeed::SendMedia)
}

#[derive(Clone, Copy)]
pub(super) enum DemoSeed {
    ReadyChats,
    UnreadBadge,
    AfterMarkRead,
    Media,
    SendMedia,
}

/// `QUILL_DEMO_STRESS=<chats>,<messages>` (both optional counts).
pub(super) fn demo_stress_size() -> Option<(usize, usize)> {
    let value = std::env::var("QUILL_DEMO_STRESS").ok()?;
    let (chats, messages) = value.split_once(',').unwrap_or((value.as_str(), "0"));
    Some((
        chats.trim().parse().ok()?,
        messages.trim().parse().unwrap_or(0),
    ))
}

/// Animated content `QUILL_DEMO_HISTORY_ANIM` adds to the open chat.
/// In the order they're added (the newest last, at the bottom).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum HistoryAnim {
    /// A photo and a text behind spoilers (not part of `all`).
    Spoiler,
    /// A video that autoplays inline (macOS).
    Video,
    /// A text message with three animated custom emoji.
    Emoji,
    /// Two animated stickers (TGS and WebM).
    Stickers,
    /// A round video message that autoplays inline (macOS).
    Note,
    /// An animated sticker authored at 60 fps, like most Telegram stickers
    /// (not part of `all`).
    Stickers60,
}

/// `QUILL_DEMO_HISTORY_ANIM=stickers,emoji,video,note,spoiler,stickers60` (any
/// subset; `all` for the first four): the ready-chats fixture's open chat
/// ends with that content, to measure what history animations cost.
pub(super) fn demo_history_anim() -> Vec<HistoryAnim> {
    let Ok(value) = std::env::var("QUILL_DEMO_HISTORY_ANIM") else {
        return Vec::new();
    };
    let all = [
        HistoryAnim::Video,
        HistoryAnim::Emoji,
        HistoryAnim::Stickers,
        HistoryAnim::Note,
    ];
    let mut out: Vec<HistoryAnim> = value
        .split(',')
        .map(str::trim)
        .flat_map(|token| match token {
            "video" => &all[0..1],
            "emoji" => &all[1..2],
            "stickers" => &all[2..3],
            "note" => &all[3..4],
            "all" => &all[..],
            "spoiler" => &[HistoryAnim::Spoiler],
            "stickers60" => &[HistoryAnim::Stickers60],
            _ => &[],
        })
        .copied()
        .collect();
    out.sort();
    out.dedup();
    out
}

fn history_anim_fixture(wanted: &[HistoryAnim]) -> Vec<String> {
    let root = demo_media_allowlist();
    let file = |id: i32, name: &str| demo_file_json(id, &root.join(name).to_string_lossy(), true);
    let mut out = Vec::new();
    let mut id = 90_000;
    let mut message = |content: String, outgoing: bool| {
        id += 1;
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":{outgoing},"date":1790633000,"content":{content}}}}}"#
        )
    };
    for item in wanted {
        match item {
            HistoryAnim::Spoiler => {
                let photo = demo_file_json(48, &demo_thumb_png_path(), true);
                out.push(message(
                    format!(
                        r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":{{"@type":"minithumbnail","width":40,"height":30,"data":"{DEMO_MINITHUMB}"}},"sizes":[{{"@type":"photoSize","type":"m","photo":{photo},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":true,"is_secret":false}}"#
                    ),
                    true,
                ));
                out.push(message(
                    r#"{"@type":"messageText","text":{"@type":"formattedText","text":"The answer is forty-two, of course.","entities":[{"@type":"textEntity","offset":14,"length":10,"type":{"@type":"textEntityTypeSpoiler"}}]}}"#
                        .to_string(),
                    false,
                ));
            }
            HistoryAnim::Stickers => {
                for (file_id, name, format, outgoing) in [
                    (44, "demo-sticker.tgs", "stickerFormatTgs", false),
                    (45, "demo-sticker.webm", "stickerFormatWebm", true),
                ] {
                    let sticker = file(file_id, name);
                    out.push(message(
                        format!(
                            r#"{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"{file_id}","set_id":"77","width":128,"height":128,"emoji":"😀","format":{{"@type":"{format}"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{sticker}}}}}"#
                        ),
                        outgoing,
                    ));
                }
            }
            HistoryAnim::Stickers60 => {
                let sticker = file(4601, "demo-sticker-60.tgs");
                out.push(message(
                    format!(
                        r#"{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"4601","set_id":"77","width":128,"height":128,"emoji":"😀","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{sticker}}}}}"#
                    ),
                    false,
                ));
            }
            HistoryAnim::Emoji => {
                // Three custom emoji (UTF-16 offsets 0, 3 and 6).
                let entity = |offset: u32| {
                    format!(
                        r#"{{"@type":"textEntity","offset":{offset},"length":2,"type":{{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4343"}}}}"#
                    )
                };
                out.push(message(
                    format!(
                        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":"😀 😀 😀 animated custom emoji","entities":[{},{},{}]}}}}"#,
                        entity(0),
                        entity(3),
                        entity(6)
                    ),
                    false,
                ));
            }
            HistoryAnim::Video => {
                let clip = file(46, "demo-clip-12s.mp4");
                out.push(message(
                    format!(
                        r#"{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
                    ),
                    true,
                ));
            }
            HistoryAnim::Note => {
                let clip = file(47, "demo-video-note.mp4");
                out.push(message(
                    format!(
                        r#"{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":8,"waveform":"","length":240,"minithumbnail":null,"thumbnail":null,"speech_recognition_result":null,"video":{clip}}},"is_viewed":true,"is_secret":false}}"#
                    ),
                    false,
                ));
            }
        }
    }
    if !out.is_empty() {
        // Read up to the end, so the chat opens at its newest messages.
        out.push(
            r#"{"@type":"updateChatReadInbox","chat_id":11,"last_read_inbox_message_id":99999,"unread_count":0}"#
                .to_string(),
        );
    }
    out
}

/// The first animated sticker message `QUILL_DEMO_HISTORY_ANIM=all` adds.
pub(super) const HISTORY_ANIM_STICKER: i64 = 90_003;

/// Whether `QUILL_DEMO_HISTORY_ANIM` lists `token` (`panel`, `menu`,
/// `select`: something over the animated history).
pub(super) fn demo_history_extra(token: &str) -> bool {
    std::env::var("QUILL_DEMO_HISTORY_ANIM")
        .is_ok_and(|value| value.split(',').any(|item| item.trim() == token))
}

/// `QUILL_DEMO_STRESS_AVATARS=<dir>`: a directory of stress-chat photos.
pub(super) fn demo_stress_avatar_dir() -> Option<PathBuf> {
    std::env::var_os("QUILL_DEMO_STRESS_AVATARS").map(PathBuf::from)
}

fn demo_stress_avatar(i: usize) -> Option<String> {
    let path = demo_stress_avatar_dir()?.join(format!("avatar-{i}.jpg"));
    path.exists().then(|| path.to_string_lossy().into_owned())
}

fn stress_fixture(chats: usize, messages: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(chats * 3 + messages);
    for i in 0..chats {
        let id = 10_000 + i as i64;
        let file_id = 50_000 + i as i64;
        // `QUILL_DEMO_STRESS_AVATARS=<dir>`: chat `i`'s photo is
        // `<dir>/avatar-<i>.jpg` (distinct files, as real avatars are).
        let avatar = demo_stress_avatar(i);
        let (avatar_path, avatar_done) = match &avatar {
            Some(path) => (path.as_str(), true),
            None => ("", false),
        };
        let avatar_path = serde_json::to_string(avatar_path).unwrap_or_default();
        out.push(format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"Stress chat {i}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":{unread},"photo":{{"@type":"chatPhotoInfo","small":{{"@type":"file","id":{file_id},"size":1000,"expected_size":1000,"local":{{"@type":"localFile","path":{avatar_path},"can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{avatar_done},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r{file_id}","unique_id":"u{file_id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":1000}}}},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}}}}}"#,
            unread = i % 7
        ));
        out.push(format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{id},"last_message":{{"id":1,"chat_id":{id},"is_outgoing":{out},"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"😀 Message number {i} with some preview text","entities":[{entities}]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}]}}"#,
            out = i % 3 == 0,
            date = 1_790_000_000 + i as i64 * 60,
            order = 1_000_000 + i,
            // The newest stress chat previews one animated custom emoji
            // (see `seed_demo_session`): the idle-animation case.
            entities = if i + 1 == chats {
                r#"{"@type":"textEntity","offset":0,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4343"}}"#
            } else {
                ""
            }
        ));
    }
    for i in 0..messages {
        let id = 1_000 + i as i64;
        out.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":{out},"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Stress message {i}: some bold words, a link https://example.com/{i}, and enough text to wrap onto a second line in a typical window width.","entities":[{{"@type":"textEntity","offset":20,"length":10,"type":{{"@type":"textEntityTypeBold"}}}}]}}}}}}}}"#,
            out = i % 2 == 0,
            date = 1_790_000_000 + i as i64 * 600
        ));
    }
    out
}

/// 40×30 JPEG minithumbnail for the pending demo photo (base64, as TDLib
/// sends `bytes`).
const DEMO_MINITHUMB: &str = "/9j/4AAQSkZJRgABAQAASABIAAD/4QBMRXhpZgAATU0AKgAAAAgAAYdpAAQAAAABAAAAGgAAAAAAA6ABAAMAAAABAAEAAKACAAQAAAABAAAAKKADAAQAAAABAAAAHgAAAAD/7QA4UGhvdG9zaG9wIDMuMAA4QklNBAQAAAAAAAA4QklNBCUAAAAAABDUHYzZjwCyBOmACZjs+EJ+/8AAEQgAHgAoAwEiAAIRAQMRAf/EAB8AAAEFAQEBAQEBAAAAAAAAAAABAgMEBQYHCAkKC//EALUQAAIBAwMCBAMFBQQEAAABfQECAwAEEQUSITFBBhNRYQcicRQygZGhCCNCscEVUtHwJDNicoIJChYXGBkaJSYnKCkqNDU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6g4SFhoeIiYqSk5SVlpeYmZqio6Slpqeoqaqys7S1tre4ubrCw8TFxsfIycrS09TV1tfY2drh4uPk5ebn6Onq8fLz9PX29/j5+v/EAB8BAAMBAQEBAQEBAQEAAAAAAAABAgMEBQYHCAkKC//EALURAAIBAgQEAwQHBQQEAAECdwABAgMRBAUhMQYSQVEHYXETIjKBCBRCkaGxwQkjM1LwFWJy0QoWJDThJfEXGBkaJicoKSo1Njc4OTpDREVGR0hJSlNUVVZXWFlaY2RlZmdoaWpzdHV2d3h5eoKDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uLj5OXm5+jp6vLz9PX29/j5+v/bAEMAAgICAgICBAICBAYEBAQGCAYGBgYICggICAgICgwKCgoKCgoMDAwMDAwMDA4ODg4ODhAQEBAQEhISEhISEhISEv/bAEMBAwMDBQQFCAQECBMNCw0TExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTE//dAAQAA//aAAwDAQACEQMRAD8A8Pooor+yD+egooooAKKKKAP/0PD6KKK/sg/noKKKKACiiigD/9k=";

pub(super) fn seed_demo_session(sink: Arc<MemorySink>, kind: DemoSeed) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink.clone());
    // Slice parity:platform-offline-indicator — the fixtures model a
    // connected client, so the offline indicator stays hidden in every
    // existing demo.
    session.connection = ConnectionState::Ready;
    let seq = AtomicU64::new(0);
    let a_unread = match kind {
        DemoSeed::ReadyChats | DemoSeed::Media | DemoSeed::SendMedia => 1,
        DemoSeed::UnreadBadge => 3,
        DemoSeed::AfterMarkRead => 3,
    };
    let jsons = [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#
            .to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":11,"title":"Demo chat A","type":{{"@type":"chatTypePrivate","user_id":11}},"unread_count":{a_unread},"last_read_inbox_message_id":100,"last_read_outbox_message_id":0}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":12,"title":"Demo chat B","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0,"last_read_inbox_message_id":40,"last_read_outbox_message_id":0}}"#
            .to_string(),
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"20","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatPosition","chat_id":13,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#
            .to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":101,"chat_id":11,"is_outgoing":false,"date":1790631720,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#
            .to_string(),
        r#"{"@type":"updateChatLastMessage","chat_id":12,"last_message":{"id":40,"chat_id":12,"is_outgoing":false,"date":1790632080,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Later.","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"20","is_pinned":false}]}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"date":1790631720,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":102,"chat_id":11,"is_outgoing":true,"date":1790631840,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":103,"chat_id":11,"is_outgoing":false,"date":1790631960,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Two more waiting.","entities":[]}}}}"#
            .to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":40,"chat_id":12,"is_outgoing":false,"date":1790632080,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Later.","entities":[]}}}}"#
            .to_string(),
    ];
    // Performance fixture: `QUILL_DEMO_STRESS=<chats>,<messages>` adds that
    // many chats (each with an avatar file) and that many formatted
    // messages in the open chat, for profiling realistic volumes.
    let stress: Vec<String> = match (kind, demo_stress_size()) {
        (DemoSeed::ReadyChats, Some((chats, messages))) => stress_fixture(chats, messages),
        _ => Vec::new(),
    };
    for json in jsons.into_iter().chain(stress) {
        if matches!(kind, DemoSeed::Media | DemoSeed::SendMedia)
            && (json.contains(r#""id":101"#)
                || json.contains(r#""id":102"#)
                || json.contains(r#""id":103"#))
        {
            continue;
        }
        if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let history_anim = match kind {
        DemoSeed::ReadyChats => demo_history_anim(),
        _ => Vec::new(),
    };
    for json in history_anim_fixture(&history_anim) {
        if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    if matches!(kind, DemoSeed::ReadyChats)
        && (demo_stress_size().is_some() || history_anim.contains(&HistoryAnim::Emoji))
    {
        // The animated custom emoji the newest stress chat previews.
        let path = demo_media_allowlist().join("demo-sticker.tgs");
        if let Some(owned) = copy_and_parse(
            &demo_file_json(4343, &path.to_string_lossy(), true),
            &seq,
            &dyn_sink,
        ) {
            session.apply(owned);
        }
        session.emoji.custom_emoji_stickers.push(StickerItem {
            custom_emoji_id: Some(4343),
            id: 4343,
            set_id: 0,
            emoji: "😀".to_string(),
            width: 512,
            height: 512,
            format: StickerFormat::Tgs,
            file_id: FileId(4343),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            requires_premium: false,
        });
    }
    match kind {
        DemoSeed::ReadyChats => {
            session.open_chat(ChatId(11));
        }
        DemoSeed::UnreadBadge => {
            session.open_chat(ChatId(12));
        }
        DemoSeed::AfterMarkRead => {
            let follow = [
                r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"date":1790632200,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Still waiting on a receipt.","entities":[]}}}}"#,
                r#"{"@type":"updateChatReadInbox","chat_id":11,"last_read_inbox_message_id":103,"unread_count":0}"#,
                r#"{"@type":"updateChatReadOutbox","chat_id":11,"last_read_outbox_message_id":102}"#,
            ];
            for json in follow {
                if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.open_chat(ChatId(11));
        }
        DemoSeed::Media => {
            let thumb_path = demo_thumb_png_path();
            let loaded = demo_file_json(21, &thumb_path, true);
            let pending = demo_file_json(22, "", false);
            let full_pending = demo_file_json(23, "", false);
            let doc = demo_file_json(24, "", false);
            let follow = [
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":201,"chat_id":11,"date":1790631000,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{loaded},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Loaded photo","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":202,"chat_id":11,"date":1790631060,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":{{"@type":"minithumbnail","width":40,"height":30,"data":"{DEMO_MINITHUMB}"}},"sizes":[{{"@type":"photoSize","type":"m","photo":{pending},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full_pending},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Pending photo","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":203,"chat_id":11,"date":1790631120,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{doc}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
                ),
                r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":203,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageDocument","document":{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{"@type":"file","id":24,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}},"caption":{"@type":"formattedText","text":"","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#
                    .to_string(),
            ];
            for json in follow {
                if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.open_chat(ChatId(11));
        }
        DemoSeed::SendMedia => {
            let thumb_path = demo_thumb_png_path();
            let photo_file = demo_file_json(31, &thumb_path, true);
            let doc_path = demo_media_allowlist()
                .join("demo-notes.txt")
                .to_string_lossy()
                .into_owned();
            let doc_file = demo_file_json(32, &doc_path, true);
            let follow = [
                r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Send me a photo?","entities":[]}}}}"#
                    .to_string(),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":302,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{photo_file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Outgoing photo","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":303,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"demo-notes.txt","mime_type":"text/plain","document":{doc_file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
                ),
                r#"{"@type":"updateChatLastMessage","chat_id":11,"last_message":{"id":303,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageDocument","document":{"@type":"document","file_name":"demo-notes.txt","mime_type":"text/plain","document":{"@type":"file","id":32,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":true,"download_offset":0,"downloaded_prefix_size":24,"downloaded_size":24},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}},"caption":{"@type":"formattedText","text":"","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"30","is_pinned":true}]}"#
                    .to_string(),
            ];
            for json in follow {
                if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            session.open_chat(ChatId(11));
        }
    }
    session
}

pub(super) fn demo_thumb_png_path() -> String {
    demo_media_allowlist()
        .join("demo-thumb.png")
        .to_string_lossy()
        .into_owned()
}

pub(super) fn demo_media_allowlist() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures")
}

pub(super) fn demo_file_json(id: i32, path: &str, completed: bool) -> String {
    format!(
        r#"{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#,
        path = serde_json::to_string(path).unwrap(),
        completed = completed,
    )
}

/// Phase S2: storage-stats fixture for the screenshot demo (injected,
/// no live Telegram) — includes a nonzero `fileTypeSecret` entry so
/// the "Secret media and files" category is visible.
/// Slice A3: `getActiveSessions` fixture for the `ready-sessions`
/// screenshot demo — the current device, two other sessions, and one
/// incomplete login attempt (injected, no live Telegram).
pub(super) fn demo_sessions() -> Vec<ParsedSession> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(0);
    vec![
        ParsedSession {
            id: 987654321,
            is_current: true,
            is_password_pending: false,
            can_accept_secret_chats: false,
            can_accept_calls: true,
            device_model: "ThinkPad X1 Carbon".into(),
            application_name: "Quill".into(),
            application_version: "0.1.0".into(),
            platform: "Linux".into(),
            system_version: "6.8.0".into(),
            last_active_date: now,
            ip_address: "192.168.1.42".into(),
            location: "Austin, United States".into(),
        },
        ParsedSession {
            id: 123456789,
            is_current: false,
            is_password_pending: false,
            can_accept_secret_chats: true,
            can_accept_calls: false,
            device_model: "iPhone 15 Pro".into(),
            application_name: "Quill".into(),
            application_version: "0.1.0".into(),
            platform: "iOS".into(),
            system_version: "18.4".into(),
            last_active_date: now - 3600,
            ip_address: "203.0.113.7".into(),
            location: "Austin, United States".into(),
        },
        ParsedSession {
            id: 555111222,
            is_current: false,
            is_password_pending: false,
            can_accept_secret_chats: false,
            can_accept_calls: true,
            device_model: "Pixel 8".into(),
            application_name: "Quill".into(),
            application_version: "0.0.9".into(),
            platform: "Android".into(),
            system_version: "15".into(),
            last_active_date: now - 2 * 86400,
            ip_address: "198.51.100.23".into(),
            location: "Dallas, United States".into(),
        },
        ParsedSession {
            id: 999888777,
            is_current: false,
            is_password_pending: true,
            can_accept_secret_chats: false,
            can_accept_calls: false,
            device_model: "".into(),
            application_name: "".into(),
            application_version: "".into(),
            platform: "".into(),
            system_version: "".into(),
            last_active_date: now - 600,
            ip_address: "203.0.113.99".into(),
            location: "Unknown".into(),
        },
    ]
}

/// Slice A4: `getConnectedWebsites` fixture for the `ready-web-sessions`
/// screenshot demo — three connected websites (injected, no live
/// Telegram).
pub(super) fn demo_websites() -> Vec<ParsedWebsite> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(0);
    vec![
        ParsedWebsite {
            id: 1000000001,
            domain_name: "fragment.com".into(),
            bot_user_id: 999888111,
            browser: "Chrome".into(),
            platform: "Web".into(),
            log_in_date: now - 90 * 86400,
            last_active_date: now - 1800,
            ip_address: "203.0.113.42".into(),
            location: "Austin, United States".into(),
        },
        ParsedWebsite {
            id: 1000000002,
            domain_name: "t.me".into(),
            bot_user_id: 777666555,
            browser: "Safari".into(),
            platform: "Web".into(),
            log_in_date: now - 40 * 86400,
            last_active_date: now - 2 * 86400,
            ip_address: "198.51.100.7".into(),
            location: "Dallas, United States".into(),
        },
        ParsedWebsite {
            id: 1000000003,
            domain_name: "wallet.bot".into(),
            bot_user_id: 444333222,
            browser: "Firefox".into(),
            platform: "Web".into(),
            log_in_date: now - 10 * 86400,
            last_active_date: now - 86400,
            ip_address: "192.0.2.19".into(),
            location: "Unknown".into(),
        },
    ]
}

pub(super) fn demo_storage_stats() -> StorageStats {
    StorageStats {
        total_size: 1_234_567_890,
        by_chat: vec![
            StorageChatStats {
                chat_id: 11,
                size: 600_000_000,
                count: 800,
            },
            StorageChatStats {
                chat_id: 13,
                size: 400_000_000,
                count: 300,
            },
            StorageChatStats {
                chat_id: 12,
                size: 150_000_000,
                count: 143,
            },
        ],
        by_file_type: vec![
            StorageFileTypeStats {
                file_type: "fileTypePhoto".to_string(),
                size: 800_000_000,
                count: 1200,
            },
            StorageFileTypeStats {
                file_type: "fileTypeVideo".to_string(),
                size: 300_000_000,
                count: 45,
            },
            StorageFileTypeStats {
                file_type: "fileTypeSecret".to_string(),
                size: 96_000_000,
                count: 210,
            },
            StorageFileTypeStats {
                file_type: "fileTypeDocument".to_string(),
                size: 30_000_000,
                count: 88,
            },
            StorageFileTypeStats {
                file_type: "fileTypeSticker".to_string(),
                size: 8_000_000,
                count: 640,
            },
        ],
    }
}

/// Slice A2: `passwordState` fixture for the `Ready2faManage` screenshot
/// demo — password set, hint, recovery email set, no pending
/// confirmation (injected, no live Telegram).
pub(super) fn demo_password_state_manage() -> PasswordState {
    PasswordState {
        has_password: true,
        password_hint: "favorite street".to_string(),
        has_recovery_email_address: true,
        has_passport_data: false,
        pending_email_pattern: None,
        pending_email_code_length: 0,
        login_email_address_pattern: String::new(),
        pending_reset_date: 0,
    }
}

/// Slice A2: `passwordState` fixture for the `ReadyRecoveryEmail`
/// screenshot demo — recovery email change pending confirmation
/// (TGX `PendingEmailText` wording, injected, no live Telegram).
pub(super) fn demo_password_state_pending() -> PasswordState {
    PasswordState {
        has_password: true,
        password_hint: String::new(),
        has_recovery_email_address: true,
        has_passport_data: false,
        pending_email_pattern: Some("n***@example.com".to_string()),
        pending_email_code_length: 6,
        login_email_address_pattern: String::new(),
        pending_reset_date: 0,
    }
}

impl QuillApp {
    pub(super) fn apply_demo_album(
        &mut self,
        text: &str,
        attachments: &[ComposerAttachment],
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let caption = text.trim();
        let album_id = "88001";
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        for (index, att) in attachments.iter().enumerate() {
            let id = -((index as i64) + 20);
            let item_caption = if index + 1 == attachments.len() {
                caption
            } else {
                ""
            };
            let path = att.path.to_string_lossy();
            let file = demo_file_json(910 + index as i32, &path, true);
            let json = match att.kind {
                AttachmentKind::Photo => format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"media_album_id":"{album_id}","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(item_caption).unwrap_or_else(|_| "\"\"".into()),
                ),
                AttachmentKind::Video => {
                    let probe = quill::video::probe_local_video(&att.path).unwrap_or(
                        quill::video::VideoProbe {
                            duration: 0,
                            width: 320,
                            height: 180,
                            supports_streaming: false,
                        },
                    );
                    format!(
                        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"media_album_id":"{album_id}","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":{},"width":{},"height":{},"file_name":{},"mime_type":"video/mp4","has_stickers":false,"supports_streaming":{},"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":{},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                        chat_id.0,
                        probe.duration,
                        probe.width,
                        probe.height,
                        serde_json::to_string(&att.file_name)
                            .unwrap_or_else(|_| "\"clip.mp4\"".into()),
                        probe.supports_streaming,
                        serde_json::to_string(item_caption).unwrap_or_else(|_| "\"\"".into()),
                    )
                }
                AttachmentKind::Document | AttachmentKind::VideoNote => continue,
            };
            if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    }

    pub(super) fn apply_demo_outgoing(
        &mut self,
        text: &str,
        attachment: Option<&ComposerAttachment>,
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let caption = text.trim();
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        let json = match attachment {
            Some(att) if att.kind == AttachmentKind::Photo => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(900, &path, true);
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"has_spoiler":false,"is_secret":false}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            Some(att) if att.kind == AttachmentKind::VideoNote => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(903, &path, true);
                let probe = quill::video::probe_local_video_note(&att.path).unwrap_or(
                    quill::video::VideoNoteProbe {
                        duration: 1,
                        length: 240,
                    },
                );
                let thumb_path = quill::video::write_video_note_thumbnail(&att.path)
                    .map(|thumb| thumb.path.to_string_lossy().into_owned());
                let thumb = thumb_path
                    .as_deref()
                    .map(|path| demo_file_json(904, path, true))
                    .unwrap_or_else(|| "null".into());
                let thumb_obj = if thumb == "null" {
                    "null".to_string()
                } else {
                    format!(
                        r#"{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":240,"file":{thumb}}}"#
                    )
                };
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":{},"waveform":"","length":{},"minithumbnail":null,"thumbnail":{thumb_obj},"speech_recognition_result":null,"video":{file}}},"is_viewed":true,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0, probe.duration, probe.length,
                )
            }
            Some(att) if att.kind == AttachmentKind::Video => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(902, &path, true);
                let probe = quill::video::probe_local_video(&att.path).unwrap_or(
                    quill::video::VideoProbe {
                        duration: 0,
                        width: 0,
                        height: 0,
                        supports_streaming: false,
                    },
                );
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":{},"width":{},"height":{},"file_name":{},"mime_type":"video/mp4","has_stickers":false,"supports_streaming":{},"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":{},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    probe.duration,
                    probe.width,
                    probe.height,
                    serde_json::to_string(&att.file_name).unwrap_or_else(|_| "\"clip.mp4\"".into()),
                    probe.supports_streaming,
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            Some(att) => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(901, &path, true);
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":{},"mime_type":"text/plain","document":{file}}},"caption":{{"@type":"formattedText","text":{},"entities":[]}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(&att.file_name).unwrap_or_else(|_| "\"file\"".into()),
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            None => format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}{reply_json}}}}}"#,
                chat_id.0,
                serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into()),
            ),
        };
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(super) fn apply_demo_sticker(
        &mut self,
        chat_id: ChatId,
        emoji: &str,
        file_id: FileId,
        reply: Option<quill::telegram::SendReply>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = session
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .unwrap_or("");
        let file = demo_file_json(file_id.0, path, !path.is_empty());
        let reply_json = reply
            .map(|reply| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    chat_id.0, reply.message_id.0
                )
            })
            .unwrap_or_default();
        let emoji = serde_json::to_string(emoji).unwrap_or_else(|_| "\"\"".into());
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"0","set_id":"0","width":512,"height":512,"emoji":{emoji},"format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatPng"}},"width":128,"height":128,"file":{file}}},"sticker":{file}}}}}{reply_json}}}}}"#,
            chat_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(super) fn apply_demo_gif(
        &mut self,
        chat_id: ChatId,
        file_id: FileId,
        duration: i32,
        width: i32,
        height: i32,
        reply: Option<quill::telegram::SendReply>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = session
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .unwrap_or("");
        let file = demo_file_json(file_id.0, path, !path.is_empty());
        let reply_json = reply
            .map(|reply| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    chat_id.0, reply.message_id.0
                )
            })
            .unwrap_or_default();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":{duration},"width":{width},"height":{height},"file_name":"gif.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":{width},"height":{height},"file":{file}}},"animation":{file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
            chat_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(super) fn apply_demo_edit(&mut self, edit: &ComposerEdit, text: &str) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        match edit.kind {
            quill::composer::ComposerEditKind::Text => {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                let body = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
                let json = format!(
                    r#"{{"@type":"updateMessageContent","chat_id":{},"message_id":{},"new_content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[]}}}}}}"#,
                    edit.chat_id.0, edit.message_id.0
                );
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            quill::composer::ComposerEditKind::Caption => {
                if let Some(message) = session
                    .histories
                    .get_mut(&edit.chat_id.0)
                    .and_then(|history| history.messages.get_mut(&edit.message_id.0))
                {
                    match &mut message.content {
                        MessageContent::Photo(photo) => photo.caption = text.to_string(),
                        MessageContent::Document(doc) => doc.caption = text.to_string(),
                        MessageContent::Text(body) => {
                            body.text = text.to_string();
                            body.entities.clear();
                            body.link_preview = None;
                        }
                        MessageContent::VoiceNote(note) => note.caption = text.to_string(),
                        MessageContent::Animation(animation) => {
                            animation.caption = text.to_string()
                        }
                        MessageContent::Video(video) => video.caption = text.to_string(),
                        MessageContent::Audio(audio) => audio.caption = text.to_string(),
                        MessageContent::VideoNote(_)
                        | MessageContent::Sticker(_)
                        | MessageContent::Poll(_)
                        | MessageContent::Location(_)
                        | MessageContent::Venue(_)
                        | MessageContent::Contact(_)
                        | MessageContent::Dice(_)
                        | MessageContent::GroupCallInvitation { .. }
                        | MessageContent::Call { .. }
                        | MessageContent::ChatTtlChanged { .. }
                        | MessageContent::ScreenshotTaken
                        | MessageContent::Unsupported { .. }
                        // M2: rich messages are not editable through the
                        // legacy text/caption path.
                        | MessageContent::RichMessage(_)
                        // B1: games carry no editable caption.
                        | MessageContent::Game(_)
                        // Slice P1: invoices and payment notices carry no
                        // editable caption.
                        | MessageContent::Invoice(_)
                        | MessageContent::PaymentSuccessful(_)
                        | MessageContent::PaymentReceived(_)
                        // Slice C2k: community service rows carry no
                        // editable caption.
                        | MessageContent::ChatAddedToCommunity { .. }
                        | MessageContent::ChatRemovedFromCommunity
                        // Slice G9: the join-from-community service row
                        // carries no editable caption either.
                        | MessageContent::ChatJoinFromCommunity { .. }
                        | MessageContent::Service(_) => {}
                    }
                }
            }
        }
    }

    pub(super) fn apply_demo_delete(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        let json = format!(
            r#"{{"@type":"updateDeleteMessages","chat_id":{},"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#,
            chat_id.0, message_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(super) fn apply_demo_forward(&mut self, dest: ChatId, draft: &ForwardDraft) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let extra = session.request(RequestPurpose::ForwardMessages, Some(dest));
        session.in_flight_forward = Some(quill::state::ForwardFlight {
            extra,
            dest_chat_id: dest,
            from_chat_id: draft.from_chat_id,
            requested: draft.message_ids.len(),
        });
        let origin_user = session
            .chats
            .get(&draft.from_chat_id.0)
            .and_then(|chat| match chat.kind {
                quill::telegram::envelope::ChatKind::Private { user_id } => Some(user_id.0),
                _ => None,
            })
            .unwrap_or(draft.from_chat_id.0);
        let mut copies = Vec::new();
        for (offset, id) in draft.message_ids.iter().enumerate() {
            let preview = session
                .histories
                .get(&draft.from_chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
                .map(|message| effective_preview(message))
                .unwrap_or_else(|| "Message".into());
            let body = serde_json::to_string(&preview).unwrap_or_else(|_| "\"\"".into());
            copies.push(format!(
                r#"{{"id":{},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":{origin_user}}},"date":1}}}}"#,
                80 + offset as i64,
                dest.0
            ));
        }
        let json = format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            extra.0,
            copies.len(),
            copies.join(",")
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(super) fn apply_demo_reaction_toggle(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let current = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| message.interaction_info.clone());
        let next = toggle_chosen_emoji_reaction(current.as_ref(), emoji);
        let json = interaction_info_update_json(chat_id, message_id, &next);
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Demo-only vote: resolve the tap with the same `poll_answer_for_tap`
    /// semantics as the live driver, then flip the chosen marks in place
    /// (counts stay as the fixture set them).
    pub(super) fn apply_demo_poll_vote(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let answer = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            })
            .and_then(|poll| quill::poll::poll_answer_for_tap(&poll, option_index));
        let Some(answer) = answer else {
            return;
        };
        if let Some(history) = session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::Poll(poll_content) = &mut message.content
        {
            let chosen: HashSet<i32> = answer.into_iter().collect();
            for (index, option) in poll_content.poll.options.iter_mut().enumerate() {
                option.is_chosen = chosen.contains(&(index as i32));
            }
        }
    }

    pub(super) fn apply_demo_pin_toggle(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let currently_pinned = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_pinned);
        let json = format!(
            r#"{{"@type":"updateMessageIsPinned","chat_id":{},"message_id":{},"is_pinned":{}}}"#,
            chat_id.0, message_id.0, !currently_pinned
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Parity slice: apply arbitrary notification-settings edits to the demo
    /// session (screenshot demos have no TDLib), via the same
    /// `updateChatNotificationSettings` reducer path live updates take.
    pub(super) fn apply_demo_notification_settings(
        &mut self,
        chat_id: ChatId,
        edit: impl FnOnce(&mut ChatNotificationSettings),
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let mut current = session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.notification_settings.clone())
            .unwrap_or_default();
        edit(&mut current);
        let json = format!(
            r#"{{"@type":"updateChatNotificationSettings","chat_id":{},"notification_settings":{}}}"#,
            chat_id.0,
            notification_settings_json(&current)
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
        cx.notify();
    }

    /// Parity slice: screenshot-demo folder create/edit (no live Telegram —
    /// apply to the demo session directly).
    pub(super) fn apply_demo_folder_save(&mut self, folder_id: Option<i32>, spec: ChatFolderSpec) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        match folder_id {
            Some(id) => {
                if let Some(info) = session.chat_folders.iter_mut().find(|f| f.id == id) {
                    info.name = spec.name.clone();
                }
                session.folder_specs.insert(id, spec);
            }
            None => {
                let id = session.chat_folders.iter().map(|f| f.id).max().unwrap_or(0) + 1;
                session.chat_folders.push(ChatFolderInfo {
                    id,
                    name: spec.name.clone(),
                    icon_name: String::new(),
                    color_id: -1,
                });
                session.folder_specs.insert(id, spec);
            }
        }
    }

    /// Parity slice: screenshot-demo folder delete.
    pub(super) fn apply_demo_folder_delete(&mut self, folder_id: i32) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.chat_folders.retain(|f| f.id != folder_id);
        session.folder_specs.remove(&folder_id);
        session.folder_chats_to_leave.remove(&folder_id);
        session.folder_chats_exhausted.remove(&folder_id);
    }

    /// Parity slice: screenshot-demo folder reorder.
    pub(super) fn apply_demo_folder_reorder(&mut self, ids: &[i32]) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let order: HashMap<i32, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        session
            .chat_folders
            .sort_by_key(|f| order.get(&f.id).copied().unwrap_or(usize::MAX));
    }

    pub(super) fn apply_demo_archive(&mut self, chat_id: ChatId, archive: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let order = session
            .chats
            .get(&chat_id.0)
            .map(|chat| {
                if archive {
                    chat.order.max(1)
                } else {
                    chat.archive_order.max(1)
                }
            })
            .unwrap_or(1);
        let jsons = if archive {
            vec![
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"0","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatRemovedFromList","chat_id":{},"chat_list":{{"@type":"chatListMain"}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"{order}","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatAddedToList","chat_id":{},"chat_list":{{"@type":"chatListArchive"}}}}"#,
                    chat_id.0
                ),
            ]
        } else {
            vec![
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"0","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatRemovedFromList","chat_id":{},"chat_list":{{"@type":"chatListArchive"}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatAddedToList","chat_id":{},"chat_list":{{"@type":"chatListMain"}}}}"#,
                    chat_id.0
                ),
            ]
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(session) = self.demo_session.as_mut() {
            for json in jsons {
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
        }
    }

    /// Slice CL1: demo pin/unpin through the real `updateChatPosition`
    /// reducer (screenshot demos have no TDLib).
    pub(super) fn apply_demo_pin(&mut self, chat_id: ChatId, pin: bool, archived: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let list = if archived {
            "chatListArchive"
        } else {
            "chatListMain"
        };
        let order = session
            .chats
            .get(&chat_id.0)
            .map(|chat| {
                if archived {
                    chat.archive_order
                } else {
                    chat.order
                }
            })
            .unwrap_or(1)
            .max(1);
        let json = format!(
            r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"{list}"}},"order":"{order}","is_pinned":{pin}}}}}"#,
            chat_id.0
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Slice CL1: demo mark-as-unread through the real
    /// `updateChatIsMarkedAsUnread` reducer.
    pub(super) fn apply_demo_marked_as_unread(&mut self, chat_id: ChatId, marked: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        // Mark-as-read also clears real unread, like TDLib's
        // `updateChatReadInbox` would after `viewMessages`.
        let jsons = if marked {
            vec![format!(
                r#"{{"@type":"updateChatIsMarkedAsUnread","chat_id":{},"is_marked_as_unread":{marked}}}"#,
                chat_id.0
            )]
        } else {
            vec![
                format!(
                    r#"{{"@type":"updateChatIsMarkedAsUnread","chat_id":{},"is_marked_as_unread":false}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatReadInbox","chat_id":{},"last_read_inbox_message_id":0,"unread_count":0}}"#,
                    chat_id.0
                ),
            ]
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
        for json in jsons {
            if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    }
}

/// Slice `parity:bots-payment-recurring`: subscriptions dialog fixture —
/// one active channel subscription ("Demo channel", chat 13 in the
/// `ReadyChats` seed), one canceled bot subscription with its own title,
/// and one expired channel subscription to show the Renew row.
/// Injected, no live Telegram.
pub(super) fn demo_star_subscriptions() -> StarSubscriptionsData {
    // Far-future / fixed dates so the demo is stable: sub1 renews,
    // sub2 was canceled, sub3 already expired.
    StarSubscriptionsData {
        star_amount: 500,
        required_star_count: 100,
        next_offset: String::new(),
        subscriptions: vec![
            StarSubscriptionData {
                id: "demo-sub-1".into(),
                chat_id: 13,
                expiration_date: 1893456000, // 2030-01-01
                is_canceled: false,
                is_expiring: false,
                pricing: StarSubscriptionPricing {
                    period: 2_592_000,
                    star_count: 100,
                },
                sub_type: StarSubscriptionTypeData::Channel {
                    can_reuse: true,
                    invite_link: "https://t.me/+demo".into(),
                },
            },
            StarSubscriptionData {
                id: "demo-sub-2".into(),
                chat_id: 42,
                expiration_date: 1893456000,
                is_canceled: true,
                is_expiring: false,
                pricing: StarSubscriptionPricing {
                    period: 604_800,
                    star_count: 25,
                },
                sub_type: StarSubscriptionTypeData::Bot {
                    is_canceled_by_bot: false,
                    title: "Demo Poll Bot".into(),
                    invoice_link: "https://t.me/$demo-invoice".into(),
                },
            },
            StarSubscriptionData {
                id: "demo-sub-3".into(),
                chat_id: 13,
                expiration_date: 1700000000, // 2023-11-14, expired
                is_canceled: false,
                is_expiring: true,
                pricing: StarSubscriptionPricing {
                    period: 2_592_000,
                    star_count: 50,
                },
                // `can_reuse` is false: it implies an ACTIVE subscription
                // (schema 1.8.67), so an expired channel sub renews through
                // `invite_link` instead.
                sub_type: StarSubscriptionTypeData::Channel {
                    can_reuse: false,
                    invite_link: "https://t.me/+demo-renew".into(),
                },
            },
        ],
    }
}
