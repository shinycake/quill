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
    ParsedSession, ParsedWebsite, StickerFormat, StickerItem, StorageFileTypeStats, StorageStats,
    toggle_chosen_emoji_reaction,
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
    apply(&mut session, &rich_post_message_json(106));
    apply(&mut session, &rtl_emoji_message_json(107));
    // The sticker file the custom emoji resolves to (completed download).
    apply(
        &mut session,
        &demo_file_json(61, &demo_thumb_png_path(), true),
    );
    session
        .stickers
        .emoji
        .custom_emoji_stickers
        .push(StickerItem {
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

/// A channel-post style message mixing custom emoji (at the start of
/// paragraphs and mid-line), bold amounts, and inline links followed by
/// punctuation, as seen in a real announcement post. Every paragraph must
/// flow as one wrapped block of text.
pub(super) fn rich_post_message_json(id: u64) -> String {
    #[derive(Clone, Copy)]
    enum Kind {
        Plain,
        Emoji,
        Bold,
        Link,
    }
    use Kind::*;
    let parts: [(&str, Kind); 24] = [
        ("\u{1F91D}", Emoji),
        (" In just one month, Telegram has awarded over ", Plain),
        ("$2,222,000", Bold),
        (" to some of the brightest minds on our planet.\n\n", Plain),
        ("\u{1F3A8}", Emoji),
        (" This week alone we distributed ", Plain),
        ("$222,000", Bold),
        (
            " among the winners of our two latest competitions \u{2014} the ",
            Plain,
        ),
        ("Design Contest", Link),
        (" and the ", Plain),
        ("Digital Freedom Contest", Link),
        (", ", Plain),
        ("worth more", Link),
        (" than ", Plain),
        ("$2 million", Bold),
        (
            ".\n\nOn top of that, over the past month we awarded prizes to the winners of the 2026 International Olympiads in ",
            Plain,
        ),
        ("Informatics", Link),
        (" and ", Plain),
        ("AI", Link),
        (". ", Plain),
        ("\u{1F3C6}", Emoji),
        (" We're proud to support the best! ", Plain),
        ("\u{1F680}", Emoji),
        (" Keep building.", Plain),
    ];
    let mut text = String::new();
    let mut entities = Vec::new();
    for (part, kind) in parts {
        let offset = text.encode_utf16().count();
        text.push_str(part);
        let length = part.encode_utf16().count();
        let ty = match kind {
            Plain => continue,
            Emoji => {
                r#"{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4242"}"#.to_string()
            }
            Bold => r#"{"@type":"textEntityTypeBold"}"#.to_string(),
            Link => r#"{"@type":"textEntityTypeTextUrl","url":"https://example.com/contest"}"#
                .to_string(),
        };
        entities.push(format!(
            r#"{{"@type":"textEntity","offset":{offset},"length":{length},"type":{ty}}}"#
        ));
    }
    let text = serde_json::to_string(&text).unwrap_or_default();
    let entities = entities.join(",");
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"date":1790632400,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[{entities}]}}}}}}}}"#
    )
}

/// A right-to-left message with a custom emoji mid-text, long enough to
/// wrap: the emoji must stay inline in the right-aligned lines.
pub(super) fn rtl_emoji_message_json(id: u64) -> String {
    let before = "שלום עולם, זהו טקסט ארוך שמכיל אימוג'י מובנה ";
    let after = " ועוד כמה מילים כדי שהשורה תישבר במקום כלשהו באמצע הפסקה הזאת.";
    let offset = before.encode_utf16().count();
    let text = serde_json::to_string(&format!("{before}\u{1F3A8}{after}")).unwrap_or_default();
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"date":1790632500,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[{{"@type":"textEntity","offset":{offset},"length":2,"type":{{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4242"}}}}]}}}}}}}}"#
    )
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
    session.media.user_downloads.insert(25);
    apply(
        &mut session,
        &file_update(25, 460_800, 460_800, false, "/tmp/report.pdf"),
    );
    // Active document 24 (the Media seed's notes.txt): 42% progress.
    session.begin_download(FileId(24));
    session.media.user_downloads.insert(24);
    apply(&mut session, &file_update(24, 24, 10, true, ""));
    // Paused document 27 → "paused" state + Resume toggle on the chip and
    // the manager row (slice media-downloads-pause).
    apply(
        &mut session,
        &document(206, 27, "big-video.mp4", 100_000_000),
    );
    session.begin_download(FileId(27));
    session.media.user_downloads.insert(27);
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
    session.media.failed_downloads.insert(26);
    session.media.downloads_panel_open = true;
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

/// `QUILL_DEMO_STRESS_PHOTOS=<dir>`: a directory of `photo-<i>.jpg` files
/// (any size, e.g. 2560 px) that become photo messages at the bottom of the
/// open chat, one per file, for profiling media-heavy history.
pub(super) fn demo_stress_photo_dir() -> Option<PathBuf> {
    std::env::var_os("QUILL_DEMO_STRESS_PHOTOS").map(PathBuf::from)
}

/// One photo message per `photo-<i>.jpg`, `i` from 0 until a file is missing.
fn stress_photos_fixture(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0.. {
        let path = dir.join(format!("photo-{i}.jpg"));
        let Ok((width, height)) = image::image_dimensions(&path) else {
            break;
        };
        let file = demo_file_json(60_000 + i, &path.to_string_lossy(), true);
        out.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":{out},"date":{date},"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{file},"width":{width},"height":{height},"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Photo {i}","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#,
            id = 50_000 + i as i64,
            out = i % 2 == 0,
            date = 1_791_000_000 + i as i64 * 600,
        ));
    }
    out
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
    let mut stress: Vec<String> = match (kind, demo_stress_size()) {
        (DemoSeed::ReadyChats, Some((chats, messages))) => stress_fixture(chats, messages),
        _ => Vec::new(),
    };
    if matches!(kind, DemoSeed::ReadyChats)
        && let Some(dir) = demo_stress_photo_dir()
    {
        stress.extend(stress_photos_fixture(&dir));
    }
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
        session
            .stickers
            .emoji
            .custom_emoji_stickers
            .push(StickerItem {
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
            is_unconfirmed: false,
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
            log_in_date: now - 40 * 86_400,
            is_official_application: true,
        },
        ParsedSession {
            id: 123456789,
            is_current: false,
            is_password_pending: false,
            is_unconfirmed: false,
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
            log_in_date: now - 40 * 86_400,
            is_official_application: true,
        },
        ParsedSession {
            id: 555111222,
            is_current: false,
            is_password_pending: false,
            is_unconfirmed: false,
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
            log_in_date: now - 40 * 86_400,
            is_official_application: true,
        },
        ParsedSession {
            id: 999888777,
            is_current: false,
            is_password_pending: true,
            is_unconfirmed: false,
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
            log_in_date: now - 40 * 86_400,
            is_official_application: true,
        },
    ]
}

impl QuillApp {}

mod demo_websites;
#[allow(unused_imports)]
pub use demo_websites::*;
mod apply_demo_album;
