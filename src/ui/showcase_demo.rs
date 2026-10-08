//! `ready-showcase` screenshot demo: a realistic, populated account for the
//! README captures — a dozen chats with generated avatars, a lively group
//! conversation with photos, an album, a voice note, reactions, a reply, a
//! forward and a link preview. All artwork is procedurally generated
//! (`docs/screenshots/fixtures/showcase/`), all text is invented, and
//! everything is injected through the normal reducer (no live Telegram).
//! `QUILL_DEMO_SHOWCASE=<scene>` picks the opened chat (see [`Scene`]).

use super::demo::{demo_file_json, demo_media_allowlist};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(super) const HIKERS: i64 = 50;
pub(super) const LUNCH: i64 = 51;
pub(super) const MAYA: i64 = 52;
pub(super) const PHOTO_CHANNEL: i64 = 56;
const STEP: i64 = 1_048_576;

/// Which conversation the scene opens.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scene {
    Hikers,
    Lunch,
    Maya,
    Channel,
}

impl Scene {
    pub(super) fn chat(self) -> i64 {
        match self {
            Scene::Hikers => HIKERS,
            Scene::Lunch => LUNCH,
            Scene::Maya => MAYA,
            Scene::Channel => PHOTO_CHANNEL,
        }
    }
}

/// Message ids the UI hooks (menu, viewer, player) refer to.
pub(super) const HIKERS_FIRST_MESSAGE: i64 = STEP;
pub(super) const HIKERS_PHOTO_MESSAGE: i64 = 4 * STEP;
pub(super) const MAYA_AUDIO_MESSAGE: i64 = 2 * STEP;
pub(super) const CHANNEL_PHOTO_MESSAGE: i64 = 2 * STEP;

fn fixture(name: &str) -> String {
    demo_media_allowlist()
        .join("showcase")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn text_content(text: &str, entities: &str) -> String {
    let text = serde_json::to_string(text).unwrap_or_default();
    format!(
        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[{entities}]}}}}"#
    )
}

fn photo_content(file_id: i32, name: &str, caption: &str) -> String {
    let file = demo_file_json(file_id, &fixture(name), true);
    let caption = serde_json::to_string(caption).unwrap_or_default();
    format!(
        r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":1100,"height":740,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{caption},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
    )
}

fn chat_photo(file_id: i32, avatar: &str) -> String {
    let small = demo_file_json(file_id, &fixture(avatar), true);
    format!(
        r#"{{"@type":"chatPhotoInfo","small":{small},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}"#
    )
}

fn reactions(items: &[(&str, i32, bool)]) -> String {
    let list = items
        .iter()
        .map(|(emoji, count, chosen)| {
            format!(
                r#"{{"@type":"messageReaction","type":{{"@type":"reactionTypeEmoji","emoji":"{emoji}"}},"total_count":{count},"is_chosen":{chosen},"used_sender_id":null,"recent_sender_ids":[]}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#","interaction_info":{{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{{"@type":"messageReactions","reactions":[{list}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}}"#
    )
}

fn reply_to(chat: i64, message: i64) -> String {
    format!(
        r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{chat},"message_id":{message},"quote":null,"checklist_task_id":0,"poll_option_id":"","origin":null,"origin_send_date":0,"content":null}}"#
    )
}

#[allow(clippy::too_many_arguments)]
fn message(
    chat: i64,
    id: i64,
    sender: i64,
    outgoing: bool,
    date: i64,
    content: &str,
    extra: &str,
) -> String {
    let sender = if sender == 0 {
        String::new()
    } else {
        format!(r#""sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"#)
    };
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},{sender}"is_outgoing":{outgoing},"date":{date},"content":{content}{extra}}}}}"#
    )
}

fn poll_option(id: &str, text: &str, votes: i32, pct: i32, chosen: bool) -> String {
    let text = serde_json::to_string(text).unwrap_or_default();
    format!(
        r#"{{"@type":"pollOption","id":"{id}","text":{{"@type":"formattedText","text":{text},"entities":[]}},"voter_count":{votes},"vote_percentage":{pct},"is_chosen":{chosen}}}"#
    )
}

#[allow(clippy::too_many_arguments)]
fn poll_message(
    id: i64,
    poll_id: i64,
    date: i64,
    question: &str,
    options: &[String],
    total: i32,
    closed: bool,
    poll_type: &str,
) -> String {
    let question = serde_json::to_string(question).unwrap_or_default();
    let options = options.join(",");
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{LUNCH},"sender_id":{{"@type":"messageSenderUser","user_id":104}},"is_outgoing":false,"date":{date},"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":{poll_id},"question":{{"@type":"formattedText","text":{question},"entities":[]}},"options":[{options}],"total_voter_count":{total},"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":{closed},"vote_restriction_reason":null,"type":{poll_type}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#
    )
}

/// Applies the whole scene and opens `scene`'s chat.
pub(super) fn apply_ready_showcase(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    scene: Scene,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix();
    let mut jsons: Vec<String> = Vec::new();

    // The base seed's placeholder chats leave the list.
    for id in [11_i64, 12, 13] {
        jsons.push(format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"0","is_pinned":false}}}}"#
        ));
        jsons.push(format!(
            r#"{{"@type":"updateChatRemovedFromList","chat_id":{id},"chat_list":{{"@type":"chatListMain"}}}}"#
        ));
    }

    // People: (id, first, last, avatar file, online, verified).
    let people: [(i64, &str, &str, &str, bool); 12] = [
        (101, "Ben", "Ito", "av05.png", false),
        (102, "Maya", "Chen", "av01.png", true),
        (103, "Dana", "Weiss", "av02.png", false),
        (104, "Priya", "Nair", "av12.png", false),
        (105, "Leo", "Park", "av10.png", false),
        (106, "Sam", "Rivera", "av03.png", false),
        (107, "Chloe", "Martin", "av13.png", false),
        (108, "Tom", "Alvarez", "av08.png", false),
        (109, "Anna", "Kowalski", "av15.png", false),
        (110, "Mom", "", "av11.png", false),
        (111, "Nora", "Fischer", "av16.png", false),
        (112, "Jules", "Bernard", "av14.png", false),
    ];
    for (i, (id, first, last, avatar, online)) in people.iter().enumerate() {
        let status = if *online {
            r#"{"@type":"userStatusOnline","expires":4102444800}"#
        } else {
            r#"{"@type":"userStatusRecently"}"#
        };
        let small = demo_file_json(2000 + i as i32, &fixture(avatar), true);
        jsons.push(format!(r#"{{"@type":"updateFile","file":{small}}}"#));
        jsons.push(format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{status},"profile_photo":{{"@type":"profilePhoto","id":{pid},"small":{small},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}},"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#,
            pid = 9000 + i
        ));
    }

    // Chats: (id, title, kind, avatar, unread, pinned, muted, last sender,
    // last text, outgoing, age seconds, verified channel).
    struct Row {
        id: i64,
        title: &'static str,
        kind: &'static str,
        avatar: &'static str,
        unread: i32,
        pinned: bool,
        muted: bool,
        sender: i64,
        text: &'static str,
        outgoing: bool,
        age: i64,
        verified: bool,
    }
    let private = |id: i64| format!(r#"{{"@type":"chatTypePrivate","user_id":{id}}}"#);
    let group = |id: i64| format!(r#"{{"@type":"chatTypeBasicGroup","basic_group_id":{id}}}"#);
    let channel = |id: i64| {
        format!(r#"{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":true}}"#)
    };
    let supergroup = |id: i64| {
        format!(r#"{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":false}}"#)
    };
    let rows = [
        Row {
            id: HIKERS,
            title: "Weekend Hikers",
            kind: "g",
            avatar: "av07.png",
            unread: 0,
            pinned: true,
            muted: false,
            sender: 103,
            text: "Okay that sunset is unreal",
            outgoing: false,
            age: 140,
            verified: false,
        },
        Row {
            id: MAYA,
            title: "Maya Chen",
            kind: "p",
            avatar: "av01.png",
            unread: 2,
            pinned: true,
            muted: false,
            sender: 102,
            text: "Night Drive. You'll love this one",
            outgoing: false,
            age: 300,
            verified: false,
        },
        Row {
            id: 53,
            title: "Design Club",
            kind: "s",
            avatar: "av09.png",
            unread: 14,
            pinned: true,
            muted: true,
            sender: 103,
            text: "Friday critique moved to 3pm",
            outgoing: false,
            age: 900,
            verified: false,
        },
        Row {
            id: 54,
            title: "Quill News",
            kind: "c",
            avatar: "av05.png",
            unread: 3,
            pinned: false,
            muted: false,
            sender: 0,
            text: "Release 0.9: audio player bar, deep links and tray controls",
            outgoing: false,
            age: 1800,
            verified: true,
        },
        Row {
            id: 105,
            title: "Leo Park",
            kind: "p",
            avatar: "av10.png",
            unread: 0,
            pinned: false,
            muted: false,
            sender: 105,
            text: "Sounds great, see you there!",
            outgoing: true,
            age: 3600,
            verified: false,
        },
        Row {
            id: PHOTO_CHANNEL,
            title: "Photography Weekly",
            kind: "c",
            avatar: "av06.png",
            unread: 0,
            pinned: false,
            muted: false,
            sender: 0,
            text: "Golden hour over the dunes",
            outgoing: false,
            age: 7200,
            verified: true,
        },
        Row {
            id: LUNCH,
            title: "Lunch Crew",
            kind: "g",
            avatar: "av04.png",
            unread: 0,
            pinned: false,
            muted: false,
            sender: 104,
            text: "Voting closes at 1pm",
            outgoing: false,
            age: 9000,
            verified: false,
        },
        Row {
            id: 57,
            title: "Book Club",
            kind: "g",
            avatar: "av12.png",
            unread: 5,
            pinned: false,
            muted: true,
            sender: 104,
            text: "Chapter 7 is wild, no spoilers!",
            outgoing: false,
            age: 100_000,
            verified: false,
        },
        Row {
            id: 58,
            title: "Family",
            kind: "g",
            avatar: "av11.png",
            unread: 1,
            pinned: false,
            muted: false,
            sender: 110,
            text: "Dinner on Sunday at 7?",
            outgoing: false,
            age: 110_000,
            verified: false,
        },
        Row {
            id: 106,
            title: "Sam Rivera",
            kind: "p",
            avatar: "av03.png",
            unread: 0,
            pinned: false,
            muted: false,
            sender: 106,
            text: "Thanks again for the help!",
            outgoing: false,
            age: 200_000,
            verified: false,
        },
        Row {
            id: 107,
            title: "Chloe Martin",
            kind: "p",
            avatar: "av13.png",
            unread: 0,
            pinned: false,
            muted: false,
            sender: 107,
            text: "Stop, I can't breathe",
            outgoing: false,
            age: 300_000,
            verified: false,
        },
        Row {
            id: 61,
            title: "Rust Meetup",
            kind: "s",
            avatar: "av15.png",
            unread: 0,
            pinned: false,
            muted: true,
            sender: 109,
            text: "Slides are in the pinned message",
            outgoing: false,
            age: 400_000,
            verified: false,
        },
    ];
    for (n, row) in rows.iter().enumerate() {
        let kind = match row.kind {
            "p" => private(row.id),
            "g" => group(row.id),
            "c" => channel(row.id),
            _ => supergroup(row.id),
        };
        let photo = chat_photo(3000 + n as i32, row.avatar);
        jsons.push(format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"photo":{photo},"unread_count":{unread},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}}}"#,
            id = row.id,
            title = row.title,
            unread = row.unread,
        ));
        if row.verified {
            jsons.push(format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{id},"is_channel":true,"status":{{"@type":"chatMemberStatusMember"}},"member_count":18400,"verification_status":{{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false}}}}}}"#,
                id = row.id
            ));
        }
        if row.muted {
            jsons.push(format!(
                r#"{{"@type":"updateChatNotificationSettings","chat_id":{id},"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}}"#,
                id = row.id
            ));
        }
        let order = 5000 - n as i64 * 10;
        let date = now - row.age;
        let content = text_content(row.text, "");
        let sender = if row.sender == 0 {
            format!(r#"{{"@type":"messageSenderChat","chat_id":{}}}"#, row.id)
        } else {
            format!(
                r#"{{"@type":"messageSenderUser","user_id":{}}}"#,
                row.sender
            )
        };
        jsons.push(format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{id},"last_message":{{"id":{mid},"chat_id":{id},"sender_id":{sender},"date":{date},"is_outgoing":{out},"is_channel_post":{chan},"content":{content}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":{pinned}}}]}}"#,
            id = row.id,
            mid = 40 * STEP + n as i64 * STEP,
            out = row.outgoing,
            chan = row.kind == "c",
            pinned = row.pinned,
        ));
    }

    // Archive row and story rings.
    for (id, title, avatar, n) in [
        (62_i64, "Old Project", "av16.png", 20_i32),
        (63, "Gym Buddies", "av14.png", 21),
    ] {
        let photo = chat_photo(3000 + n, avatar);
        jsons.push(format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{},"photo":{photo},"unread_count":0}}}}"#,
            group(id)
        ));
        jsons.push(format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{id},"last_message":{{"id":{},"chat_id":{id},"is_outgoing":false,"date":{},"content":{}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"{}","is_pinned":false}}]}}"#,
            STEP,
            now - 500_000,
            text_content("See you at the gym", ""),
            100 + n
        ));
    }
    for (chat, ids, max_read) in [
        (MAYA, &[5_i32, 6][..], 0_i32),
        (105, &[3][..], 0),
        (106, &[2][..], 2),
        (54, &[1][..], 0),
    ] {
        let infos = ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"@type":"storyInfo","story_id":{id},"date":{now},"is_for_close_friends":false,"is_live":false}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        jsons.push(format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat},"list":{{"@type":"storyListMain"}},"order":"{chat}","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{infos}]}}}}"#
        ));
    }

    // --- Weekend Hikers: the lively group conversation. ---
    let t = |mins: i64| now - mins * 60;
    let h = HIKERS;
    jsons.push(message(
        h,
        STEP,
        101,
        false,
        t(190),
        &text_content("Morning all! Forecast looks perfect for Saturday", ""),
        &reactions(&[("👍", 4, false), ("❤", 2, true)]),
    ));
    jsons.push(message(
        h,
        2 * STEP,
        102,
        false,
        t(184),
        &text_content("Finally! I'll bring the good thermos", ""),
        &reply_to(h, STEP),
    ));
    jsons.push(message(
        h,
        3 * STEP,
        101,
        false,
        t(170),
        &text_content("Scouted the ridge last weekend. Look at this light", ""),
        "",
    ));
    let album = |id: i64, file: i32, name: &str, caption: &str| {
        message(
            h,
            id,
            101,
            false,
            t(168),
            &photo_content(file, name, caption),
            r#","media_album_id":"88001""#,
        )
    };
    jsons.push(album(HIKERS_PHOTO_MESSAGE, 4001, "p1.jpg", ""));
    jsons.push(album(5 * STEP, 4002, "p2.jpg", ""));
    jsons.push(album(6 * STEP, 4003, "p3.jpg", "Ridge at golden hour"));
    jsons.push(message(
        h,
        7 * STEP,
        103,
        false,
        t(150),
        &text_content("Okay that is unreal", ""),
        &format!(
            "{}{}",
            reply_to(h, 6 * STEP),
            reactions(&[("🔥", 5, false), ("😍", 3, false)])
        ),
    ));
    jsons.push(message(
        h,
        8 * STEP,
        0,
        true,
        t(140),
        &text_content("Count me in! Leaving at 6am from the north lot", ""),
        "",
    ));
    let wave = quill::voice::waveform_base64(&[
        6, 14, 22, 30, 18, 10, 26, 32, 20, 12, 8, 24, 28, 16, 10, 20,
    ]);
    let wave = serde_json::to_string(&wave).unwrap_or_else(|_| "\"\"".into());
    let voice_file = demo_file_json(
        4010,
        &demo_media_allowlist()
            .join("demo-voice.ogg")
            .to_string_lossy(),
        true,
    );
    jsons.push(message(h, 9 * STEP, 102, false, t(95), &format!(r#"{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":24,"waveform":{wave},"mime_type":"audio/ogg","speech_recognition_result":{{"@type":"speechRecognitionResultText","text":"Quick update, the trailhead parking is full by seven, so let's meet at six"}},"voice":{voice_file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}"#), ""));
    // Link preview with a generated photo.
    let lp_photo = demo_file_json(4011, &fixture("p5.jpg"), true);
    let body = "Trail conditions for tomorrow https://alpinetrails.example/ridge-loop";
    let url = "https://alpinetrails.example/ridge-loop";
    let at = body.find("https").unwrap_or(0);
    jsons.push(message(h, 10 * STEP, 105, false, t(60), &format!(r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{body_json},"entities":[{{"@type":"textEntity","offset":{at},"length":{len},"type":{{"@type":"textEntityTypeUrl"}}}}]}},"link_preview":{{"@type":"linkPreview","url":"{url}","display_url":"alpinetrails.example/ridge-loop","site_name":"Alpine Trails","title":"Ridge Loop: 12 km, moderate","description":{{"@type":"formattedText","text":"Clear skies, light wind at the summit. Trail dry above the tree line.","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{lp_photo},"width":1100,"height":740,"progressive_sizes":[]}}]}}}},"has_large_media":true,"show_large_media":true,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0}},"link_preview_options":null}}"#, body_json = serde_json::to_string(body).unwrap_or_default(), len = url.len()), ""));
    jsons.push(message(
        h,
        11 * STEP,
        0,
        true,
        t(52),
        &text_content(
            "Perfect, bookmarking it. Packing the crampons just in case",
            "",
        ),
        &reply_to(h, 10 * STEP),
    ));
    jsons.push(message(
        h,
        13 * STEP,
        101,
        false,
        t(18),
        &text_content("See everyone at 6. Bring snacks!", ""),
        "",
    ));
    jsons.push(message(
        h,
        14 * STEP,
        103,
        false,
        t(2),
        &text_content("Okay that sunset is unreal", ""),
        "",
    ));
    jsons.push(format!(
        r#"{{"@type":"updateChatReadOutbox","chat_id":{h},"last_read_outbox_message_id":{}}}"#,
        11 * STEP
    ));
    jsons.push(format!(
        r#"{{"@type":"updateMessageIsPinned","chat_id":{h},"message_id":{},"is_pinned":true}}"#,
        STEP
    ));

    // --- Maya Chen: music, voice and a photo. ---
    let m = MAYA;
    jsons.push(message(
        m,
        STEP,
        102,
        false,
        t(30),
        &text_content("Okay, you have to hear this one", ""),
        "",
    ));
    let cover = demo_file_json(4020, &fixture("cover1.png"), true);
    let track = demo_file_json(
        4021,
        &demo_media_allowlist()
            .join("demo-voice.ogg")
            .to_string_lossy(),
        true,
    );
    jsons.push(message(m, MAYA_AUDIO_MESSAGE, 102, false, t(29), &format!(r#"{{"@type":"messageAudio","audio":{{"@type":"audio","duration":214,"title":"Night Drive","performer":"The Midnight Owls","file_name":"night-drive.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":400,"height":400,"file":{cover}}},"external_album_covers":[],"audio":{track}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#), ""));
    jsons.push(message(
        m,
        3 * STEP,
        0,
        true,
        t(25),
        &text_content("Instant favorite. Playing it on repeat", ""),
        &reactions(&[("❤", 1, false)]),
    ));
    jsons.push(message(
        m,
        4 * STEP,
        102,
        false,
        t(20),
        &photo_content(4022, "p6.jpg", "Sunset from my balcony tonight"),
        "",
    ));
    jsons.push(message(
        m,
        5 * STEP,
        0,
        true,
        t(14),
        &text_content("Wow. Where is that?", ""),
        "",
    ));
    jsons.push(message(
        m,
        6 * STEP,
        102,
        false,
        t(8),
        &text_content(
            "Right outside the apartment, I got lucky with the clouds",
            "",
        ),
        "",
    ));
    jsons.push(message(
        m,
        7 * STEP,
        102,
        false,
        t(5),
        &text_content("Sent you the new mockups, tell me what you think!", ""),
        "",
    ));
    jsons.push(format!(
        r#"{{"@type":"updateChatReadOutbox","chat_id":{m},"last_read_outbox_message_id":{}}}"#,
        5 * STEP
    ));

    // --- Lunch Crew: polls. ---
    let l = LUNCH;
    jsons.push(message(
        l,
        STEP,
        104,
        false,
        t(240),
        &text_content("Lunch plans for Friday? Vote below", ""),
        "",
    ));
    jsons.push(poll_message(
        2 * STEP,
        9101,
        t(238),
        "Where should we eat lunch?",
        &[
            poll_option("a", "Sushi place", 12, 55, true),
            poll_option("b", "Pizza", 7, 32, false),
            poll_option("c", "Tacos", 3, 13, false),
        ],
        22,
        false,
        r#"{"@type":"pollTypeRegular"}"#,
    ));
    jsons.push(message(
        l,
        3 * STEP,
        108,
        false,
        t(230),
        &text_content("Sushi gets my vote", ""),
        &reactions(&[("🍣", 4, true)]),
    ));
    jsons.push(poll_message(4 * STEP, 9102, t(200), "Which planet is known as the Red Planet?", &[
        poll_option("m", "Mars", 18, 72, false),
        poll_option("v", "Venus", 7, 28, true),
    ], 25, true, r#"{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"Mars looks red because iron oxide, or rust, coats its surface.","entities":[]}}"#));
    jsons.push(message(
        l,
        5 * STEP,
        0,
        true,
        t(190),
        &text_content("Venus?! I was so sure", ""),
        "",
    ));
    jsons.push(poll_message(
        6 * STEP,
        9103,
        t(150),
        "Coffee or tea after lunch?",
        &[
            poll_option("c", "Coffee", 14, 70, false),
            poll_option("t", "Tea", 6, 30, false),
        ],
        20,
        false,
        r#"{"@type":"pollTypeRegular"}"#,
    ));
    jsons.push(message(
        l,
        7 * STEP,
        104,
        false,
        t(150),
        &text_content("Voting closes at 1pm", ""),
        "",
    ));

    // --- Photography Weekly: channel posts. ---
    let c = PHOTO_CHANNEL;
    let post = |id: i64, file: i32, name: &str, caption: &str, views: i32, age: i64, rx: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{c},"sender_id":{{"@type":"messageSenderChat","chat_id":{c}}},"is_outgoing":false,"is_channel_post":true,"author_signature":"Photography Weekly","date":{date},"interaction_info":{{"@type":"messageInteractionInfo","view_count":{views},"forward_count":42,"reply_info":null,"reactions":{{"@type":"messageReactions","reactions":[{rx}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}},"content":{}}}}}"#,
            photo_content(file, name, caption),
            date = now - age,
        )
    };
    let rx = |items: &[(&str, i32)]| {
        items
            .iter()
            .map(|(e, n)| format!(r#"{{"@type":"messageReaction","type":{{"@type":"reactionTypeEmoji","emoji":"{e}"}},"total_count":{n},"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}}"#))
            .collect::<Vec<_>>()
            .join(",")
    };
    jsons.push(post(
        STEP,
        4030,
        "p3.jpg",
        "Morning mist in the pines. Shot at f/8, 1/60s, tripod essential for the soft look.",
        18_400,
        20_000,
        &rx(&[("😍", 912), ("🔥", 310)]),
    ));
    jsons.push(post(
        CHANNEL_PHOTO_MESSAGE,
        4031,
        "p1.jpg",
        "Last light over the ridge line. Golden hour never gets old.",
        22_100,
        12_000,
        &rx(&[("❤", 1204), ("🔥", 640), ("👏", 211)]),
    ));
    jsons.push(post(
        3 * STEP,
        4032,
        "p5.jpg",
        "Golden hour over the dunes",
        9_800,
        7_200,
        &rx(&[("😍", 488), ("👍", 120)]),
    ));

    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(scene.chat()));
}
