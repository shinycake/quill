//! `ready-rendering-leftovers` screenshot demo (codex:rendering-leftovers):
//! grouped bubbles, contact cards, a dice, a location with its map tile,
//! locked paid media and a suggested profile photo, injected through the
//! normal reducer (recorded TDLib JSON shapes, no live Telegram).
//! `QUILL_DEMO_RENDERING_VIEW=bubbles|cards|service|viewer|contact|location`
//! (default `bubbles`) picks the chat and what is open on top of it.

use super::demo::{demo_file_json, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(super) const GROUP: i64 = -3001;
pub(super) const CARDS: i64 = 52;
pub(super) const SERVICE: i64 = 53;
const MAYA: i64 = 51;
const BEN: i64 = 52;
const NEW_PERSON: i64 = 54;

fn user(id: i64, name: &str, accent: i32, phone: &str, contact: bool) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{name}","last_name":"","phone_number":"{phone}","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"accent_color_id":{accent},"is_contact":{contact},"type":{{"@type":"userTypeRegular"}}}}}}"#
    )
}

fn text(text: &str) -> String {
    let text = serde_json::to_string(text).unwrap_or_default();
    format!(
        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}"#
    )
}

fn message(chat: i64, id: i64, sender: i64, outgoing: bool, date: i64, content: &str) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":{outgoing},"date":{date},"content":{content}}}}}"#
    )
}

/// A tiny gradient JPEG: the locked paid-media preview TDLib would send as
/// a `minithumbnail` (about 40 px, blurred on screen by the upscale).
fn blurred_preview() -> String {
    use base64::Engine;
    let mut pixels = image::RgbImage::new(32, 24);
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        let t = x as f32 / 31.0;
        let u = y as f32 / 23.0;
        *pixel = image::Rgb([
            (230.0 - 120.0 * t) as u8,
            (150.0 + 60.0 * u - 40.0 * t) as u8,
            (110.0 + 120.0 * t) as u8,
        ]);
    }
    let mut jpeg = std::io::Cursor::new(Vec::new());
    let _ = image::DynamicImage::ImageRgb8(pixels).write_to(&mut jpeg, image::ImageFormat::Jpeg);
    base64::engine::general_purpose::STANDARD.encode(jpeg.into_inner())
}

pub(super) fn apply_ready_rendering_leftovers(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 2400;
    let apply = |session: &mut Session, json: String| {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    apply(session, user(MAYA, "Maya Cohen", 0, "+14155550101", true));
    apply(session, user(BEN, "Ben Ito", 5, "+14155550102", false));
    apply(session, user(NEW_PERSON, "Noor Haddad", 2, "", false));
    session.my_user_id = Some(9);
    apply(session, user(9, "You", 1, "+14155550109", true));
    session.contacts = Some(vec![MAYA]);
    for (id, title, kind) in [
        (
            GROUP,
            "Trail Crew",
            r#"{"@type":"chatTypeBasicGroup","basic_group_id":3001}"#.to_string(),
        ),
        (
            CARDS,
            "Ben Ito",
            format!(r#"{{"@type":"chatTypePrivate","user_id":{BEN}}}"#),
        ),
        (
            SERVICE,
            "Maya Cohen",
            format!(r#"{{"@type":"chatTypePrivate","user_id":{MAYA}}}"#),
        ),
    ] {
        apply(
            session,
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"unread_count":0}}}}"#
            ),
        );
        apply(
            session,
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{}","is_pinned":false}}}}"#,
                3000 + id.abs() % 100
            ),
        );
    }

    // Grouped bubbles: runs from one sender join with a small corner and
    // the avatar sits beside the last bubble of a run.
    let run = [
        (MAYA, false, "Anyone up for the ridge trail on Saturday?"),
        (MAYA, false, "Starting 7:30 at the trailhead."),
        (MAYA, false, "I'll bring a map just in case."),
        (9, true, "Count me in."),
        (9, true, "Is 7:30 sharp or flexible?"),
        (BEN, false, "Flexible, but the car park fills up fast."),
        (BEN, false, "I'm in, bringing the good thermos."),
    ];
    for (index, (sender, outgoing, body)) in run.into_iter().enumerate() {
        apply(
            session,
            message(
                GROUP,
                201 + index as i64,
                sender,
                outgoing,
                now + 60 * index as i64,
                &text(body),
            ),
        );
    }

    // Cards: contacts, a dice, a location with its tile, locked media.
    let contact = |user_id: i64, phone: &str, first: &str, last: &str| {
        format!(
            r#"{{"@type":"messageContact","contact":{{"@type":"contact","phone_number":"{phone}","first_name":"{first}","last_name":"{last}","vcard":"","user_id":{user_id}}}}}"#
        )
    };
    let tile = demo_file_json(95, &demo_thumb_png_path(), true);
    let cards = [
        (
            BEN,
            false,
            contact(MAYA, "+14155550101", "Maya", "Cohen"),
        ),
        (
            BEN,
            false,
            contact(0, "+14155550177", "Noor", "Haddad"),
        ),
        (
            BEN,
            false,
            r#"{"@type":"messageDice","emoji":"🎲","value":4,"success_animation_frame_number":0}"#
                .to_string(),
        ),
        (
            BEN,
            false,
            r#"{"@type":"messageLocation","location":{"@type":"location","latitude":37.7749,"longitude":-122.4194,"horizontal_accuracy":15}}"#
                .to_string(),
        ),
        (
            BEN,
            false,
            format!(
                r#"{{"@type":"messagePaidMedia","star_count":25,"media":[{{"@type":"paidMediaPreview","width":800,"height":600,"duration":0,"minithumbnail":{{"@type":"minithumbnail","width":32,"height":24,"data":"{}"}}}}],"caption":{{"@type":"formattedText","text":"Behind the scenes from the summit","entities":[]}},"show_caption_above_media":false}}"#,
                blurred_preview()
            ),
        ),
    ];
    for (index, (sender, outgoing, content)) in cards.into_iter().enumerate() {
        apply(
            session,
            message(
                CARDS,
                301 + index as i64,
                sender,
                outgoing,
                now + 60 * index as i64,
                &content,
            ),
        );
    }
    // The map tile TDLib would answer `getMapThumbnailFile` with.
    session.map_thumbs.expect(
        quill::ids::RequestId(900_001),
        quill::state::MapKey::of(&quill::telegram::envelope::GeoLocation {
            lat_e6: 37_774_900,
            lon_e6: -122_419_400,
            accuracy_m: 15,
        }),
    );
    session
        .map_thumbs
        .answered(quill::ids::RequestId(900_001), 95);
    apply(session, tile);

    // A photo for the media viewer scene.
    let photo = demo_file_json(97, &demo_thumb_png_path(), true);
    apply(
        session,
        message(
            CARDS,
            310,
            BEN,
            false,
            now + 600,
            &format!(
                r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{photo},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"Sunset from the summit","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
            ),
        ),
    );

    // A suggested profile photo.
    let photo_file = demo_file_json(96, &demo_thumb_png_path(), true);
    let suggested = format!(
        r#"{{"@type":"messageSuggestProfilePhoto","photo":{{"@type":"chatPhoto","id":"77","added_date":1,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"a","photo":{photo_file},"width":160,"height":160,"progressive_sizes":[]}}]}}}}"#
    );
    apply(
        session,
        message(
            SERVICE,
            401,
            MAYA,
            false,
            now,
            &text("Hey! I took a new photo of you."),
        ),
    );
    apply(
        session,
        message(SERVICE, 402, MAYA, false, now + 60, &suggested),
    );

    let open = match view {
        "cards" | "contact" | "location" | "viewer" => CARDS,
        "service" => SERVICE,
        _ => GROUP,
    };
    session.open_chat(ChatId(open));
}
