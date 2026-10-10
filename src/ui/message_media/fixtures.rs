//! Screenshot-demo fixtures that inject location and dice messages.

use super::*;

/// `ReadyLocation` fixture (Phase 4.3): open a dedicated "Demo places" chat
/// (id 16) and inject four messages through the normal reducer — a plain
/// `messageLocation` (coordinates + accuracy), a `messageLiveLocation`
/// (live period / expires / heading / proximity alert), a `messageVenue`
/// (title + address + provider), and a `messageContact` (name + phone +
/// vCard + user_id). All data is synthetic; no live Telegram.
pub(in crate::ui) fn apply_ready_location(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 16;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo places","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let message = |message_id: i32, content: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{content}}}}}"#
        )
    };

    // Plain location: San Francisco, ±15 m accuracy.
    let location = message(
        108,
        r#"{"@type":"messageLocation","location":{"@type":"location","latitude":37.7749,"longitude":-122.4194,"horizontal_accuracy":15}}"#,
    );
    // Live location: Paris, 15-minute live period, 10 minutes left,
    // heading 90°, 500 m proximity alert.
    let live_location = message(
        109,
        r#"{"@type":"messageLiveLocation","location":{"@type":"liveLocation","location":{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0},"live_period":900,"heading":90,"proximity_alert_radius":500},"expires_in":600}"#,
    );
    // Venue: Ferry Building, via foursquare.
    let venue = message(
        110,
        r#"{"@type":"messageVenue","venue":{"@type":"venue","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0},"title":"Ferry Building","address":"1 Ferry Building, San Francisco","provider":"foursquare","id":"4a1a2b3c","type":"Food"}}"#,
    );
    // Contact: Ada Lovelace with a vCard and a known Telegram user id.
    let contact = message(
        111,
        r#"{"@type":"messageContact","contact":{"@type":"contact","phone_number":"+14155550123","first_name":"Ada","last_name":"Lovelace","vcard":"BEGIN:VCARD\nFN:Ada Lovelace\nEND:VCARD","user_id":123456789}}"#,
    );

    for json in [location, live_location, venue, contact] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Phase 4.4: inject a "Demo dice" chat with three `messageDice` rolls —
/// an incoming 🎲 = 4, an outgoing 🎲 = 6, and an incoming 🎯 = 5.
/// All data is synthetic; no live Telegram.
pub(in crate::ui) fn apply_ready_dice(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 17;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo dice","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let message = |message_id: i32, outgoing: bool, content: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":{outgoing},"content":{content}}}}}"#
        )
    };

    // Incoming 🎲 = 4.
    let roll_one = message(
        112,
        false,
        r#"{"@type":"messageDice","emoji":"🎲","value":4,"success_animation_frame_number":0}"#,
    );
    // Outgoing 🎲 = 6.
    let roll_two = message(
        113,
        true,
        r#"{"@type":"messageDice","emoji":"🎲","value":6,"success_animation_frame_number":0}"#,
    );
    // Incoming 🎯 = 5.
    let roll_three = message(
        114,
        false,
        r#"{"@type":"messageDice","emoji":"🎯","value":5,"success_animation_frame_number":0}"#,
    );

    for json in [roll_one, roll_two, roll_three] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}
