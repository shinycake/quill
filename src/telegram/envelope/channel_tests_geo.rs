use super::*;

// Phase 4.3: `messageLocation` (schema 1.8.67 line 5214), `location`
// (line 646).
#[test]
fn message_location_parses_coordinates() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":108,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageLocation","location":{"@type":"location","latitude":37.7749,"longitude":-122.4194,"horizontal_accuracy":15.6}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Location(content) = &message.content else {
                panic!("{:?}", message.content);
            };
            assert!(content.live.is_none());
            assert_eq!(content.location.lat_e6, 37_774_900);
            assert_eq!(content.location.lon_e6, -122_419_400);
            assert_eq!(content.location.accuracy_m, 16);
            assert_eq!(content.location.coords_label(), "37.7749, -122.4194");
            assert_eq!(
                content.location.open_street_map_url(),
                "https://www.openstreetmap.org/?mlat=37.774900&mlon=-122.419400"
            );
            assert_eq!(message.content.preview(), "📍 Location");
        }
        other => panic!("{other:?}"),
    }
}

// Phase 4.3: `messageLiveLocation` (schema 1.8.67 line 5211) +
// `liveLocation` (line 653). The task's live fields live here, not on
// `messageLocation`.
#[test]
fn message_live_location_parses_live_fields() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":109,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageLiveLocation","location":{"@type":"liveLocation","location":{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0},"live_period":900,"heading":90,"proximity_alert_radius":500},"expires_in":600}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Location(content) = &message.content else {
                panic!("{:?}", message.content);
            };
            let live = content.live.expect("live state");
            assert_eq!(live.live_period, 900);
            assert_eq!(live.expires_in, 600);
            assert_eq!(live.heading, 90);
            assert_eq!(live.proximity_alert_radius, 500);
            assert_eq!(content.location.lat_e6, 48_856_600);
            assert_eq!(content.location.lon_e6, 2_352_200);
            assert_eq!(content.location.accuracy_m, 0);
            assert_eq!(
                live.status_label(),
                "Live · expires in 10 min · heading 90° · proximity alert ≤ 500 m"
            );
            assert_eq!(message.content.preview(), "📍 Live location");
        }
        other => panic!("{other:?}"),
    }
}

// Phase 4.3: `messageVenue` (schema 1.8.67 line 5217), `venue` (line
// 663). Provider `id` / `type` are dropped by design.
#[test]
fn message_venue_parses_all_fields() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":110,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageVenue","venue":{"@type":"venue","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0},"title":"Ferry Building","address":"1 Ferry Building, San Francisco","provider":"foursquare","id":"4a1a2b3c","type":"Food"}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Venue(venue) = &message.content else {
                panic!("{:?}", message.content);
            };
            assert_eq!(venue.title, "Ferry Building");
            assert_eq!(venue.address, "1 Ferry Building, San Francisco");
            assert_eq!(venue.provider, "foursquare");
            assert_eq!(venue.location.coords_label(), "37.7955, -122.3937");
            assert_eq!(message.content.preview(), "📍 Ferry Building");
        }
        other => panic!("{other:?}"),
    }
}

// Phase 4.3: `messageContact` (schema 1.8.67 line 5220), `contact`
// (line 640). The vCard parses without breaking; user_id 0 is
// unknown.
#[test]
fn message_contact_parses_with_vcard() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":111,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageContact","contact":{"@type":"contact","phone_number":"+14155550123","first_name":"Ada","last_name":"Lovelace","vcard":"BEGIN:VCARD\nFN:Ada Lovelace\nEND:VCARD","user_id":123456789}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Contact(contact) = &message.content else {
                panic!("{:?}", message.content);
            };
            assert_eq!(contact.phone_number, "+14155550123");
            assert_eq!(contact.display_name(), "Ada Lovelace");
            assert!(contact.vcard.contains("BEGIN:VCARD"));
            assert_eq!(contact.user_id, 123456789);
            assert_eq!(message.content.preview(), "👤 Ada Lovelace");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn message_contact_without_name_or_phone_is_unsupported() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":112,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageContact","contact":{"@type":"contact","phone_number":"","first_name":"","last_name":"","vcard":"","user_id":0}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            match &message.content {
                MessageContent::Unsupported { type_name } => {
                    assert_eq!(type_name, "messageContact")
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

// Phase 4.4: `messageDice` (schema 1.8.67 line 5231). `initial_state`
// / `final_state` / `success_animation_frame_number` parse without
// breaking and are dropped; `emoji` + `value` are kept.
#[test]
fn message_dice_parses_emoji_and_value() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":115,"chat_id":17,"is_outgoing":false,"content":{"@type":"messageDice","initial_state":{"@type":"diceStickersRegular","sticker":{"@type":"sticker"}},"final_state":{"@type":"diceStickersRegular","sticker":{"@type":"sticker"}},"emoji":"🎲","value":4,"success_animation_frame_number":12}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Dice(dice) = &message.content else {
                panic!("{:?}", message.content);
            };
            assert_eq!(dice.emoji, "🎲");
            assert_eq!(dice.value, 4);
            assert_eq!(dice.face(), "🎲");
            assert_eq!(dice.label(), "🎲 4");
            assert_eq!(message.content.preview(), "🎲 4");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn message_dice_keeps_the_regular_final_state_sticker() {
    let sticker = |id: i32| {
        format!(
            r#"{{"@type":"sticker","id":9,"set_id":3,"width":512,"height":512,"emoji":"🎲","format":{{"@type":"stickerFormatTgs"}},"sticker":{{"@type":"file","id":{id},"size":4000,"local":{{"@type":"localFile","path":"","is_downloading_completed":false}},"remote":{{"@type":"remoteFile","id":"r{id}"}}}}}}"#
        )
    };
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":116,"chat_id":17,"is_outgoing":true,"content":{{"@type":"messageDice","initial_state":{{"@type":"diceStickersRegular","sticker":{}}},"final_state":{{"@type":"diceStickersRegular","sticker":{}}},"emoji":"🎲","value":5,"success_animation_frame_number":0}}}}}}"#,
        sticker(70),
        sticker(71)
    );
    let env = parse_envelope(&json).unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("not a new message");
    };
    let MessageContent::Dice(dice) = &message.content else {
        panic!("{:?}", message.content);
    };
    let sticker = dice.final_sticker.as_ref().expect("final sticker");
    assert_eq!(sticker.file_id.0, 71);
    assert_eq!(sticker.format, StickerFormat::Tgs);
    assert!(message.files.iter().any(|file| file.id.0 == 71));
}

#[test]
fn message_dice_slot_machine_has_no_single_final_sticker() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":117,"chat_id":17,"is_outgoing":false,"content":{"@type":"messageDice","initial_state":{"@type":"diceStickersSlotMachine"},"final_state":{"@type":"diceStickersSlotMachine"},"emoji":"🎰","value":64,"success_animation_frame_number":0}}}"#;
    let env = parse_envelope(json).unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("not a new message");
    };
    let MessageContent::Dice(dice) = &message.content else {
        panic!("{:?}", message.content);
    };
    assert_eq!(dice.value, 64);
    assert!(dice.final_sticker.is_none());
}

fn slot_sticker(id: i32) -> String {
    format!(
        r#"{{"@type":"sticker","id":{id},"set_id":3,"width":512,"height":512,"emoji":"🎰","format":{{"@type":"stickerFormatTgs"}},"sticker":{{"@type":"file","id":{id},"size":4000,"local":{{"@type":"localFile","path":"","is_downloading_completed":false}},"remote":{{"@type":"remoteFile","id":"r{id}"}}}}}}"#
    )
}

fn slot_machine_message(layers: &[&str], value: i32) -> String {
    let parts: Vec<String> = layers
        .iter()
        .zip(80..)
        .map(|(key, id)| format!(r#""{key}":{}"#, slot_sticker(id)))
        .collect();
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":118,"chat_id":17,"is_outgoing":false,"content":{{"@type":"messageDice","initial_state":{{"@type":"diceStickersSlotMachine"}},"final_state":{{"@type":"diceStickersSlotMachine",{}}},"emoji":"🎰","value":{value},"success_animation_frame_number":0}}}}}}"#,
        parts.join(",")
    )
}

const SLOT_KEYS: [&str; 5] = [
    "background",
    "left_reel",
    "center_reel",
    "right_reel",
    "lever",
];

#[test]
fn message_dice_slot_machine_keeps_its_five_layers_in_draw_order() {
    let env = parse_envelope(&slot_machine_message(&SLOT_KEYS, 64)).unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("not a new message");
    };
    let MessageContent::Dice(dice) = &message.content else {
        panic!("{:?}", message.content);
    };
    assert!(dice.is_slot_machine());
    let ids: Vec<i32> = dice.slot_layers.iter().map(|s| s.file_id.0).collect();
    assert_eq!(ids, [80, 81, 82, 83, 84]);
    assert!(
        (80..=84).all(|id| message.files.iter().any(|file| file.id.0 == id)),
        "the layers' files reach the downloader"
    );
}

#[test]
fn message_dice_slot_machine_missing_a_layer_falls_back_to_the_emoji() {
    let env = parse_envelope(&slot_machine_message(&SLOT_KEYS[..4], 3)).unwrap();
    let EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) = env.payload else {
        panic!("not a new message");
    };
    let MessageContent::Dice(dice) = &message.content else {
        panic!("{:?}", message.content);
    };
    assert!(dice.slot_layers.is_empty());
    assert_eq!(dice.face(), "🎰");
}

#[test]
fn slot_machine_values_decode_to_reel_symbols() {
    use SlotSymbol::{Bar, Grapes, Lemon, Seven};
    assert_eq!(slot_symbols(1), Some([Bar, Bar, Bar]));
    assert_eq!(slot_symbols(2), Some([Grapes, Bar, Bar]));
    assert_eq!(slot_symbols(3), Some([Lemon, Bar, Bar]));
    assert_eq!(slot_symbols(4), Some([Seven, Bar, Bar]));
    assert_eq!(slot_symbols(5), Some([Bar, Grapes, Bar]));
    assert_eq!(slot_symbols(22), Some([Grapes, Grapes, Grapes]));
    assert_eq!(slot_symbols(43), Some([Lemon, Lemon, Lemon]));
    assert_eq!(slot_symbols(64), Some([Seven, Seven, Seven]));
    assert_eq!(slot_symbols(0), None);
    assert_eq!(slot_symbols(65), None);
}

#[test]
fn dice_result_lines_state_the_roll() {
    let dice = |emoji: &str, value| DiceContent {
        emoji: emoji.into(),
        value,
        final_sticker: None,
        slot_layers: Vec::new(),
    };
    assert_eq!(dice("🎲", 4).result_line(), "Rolled 4");
    assert_eq!(dice("🎯", 6).result_line(), "Rolled 6");
    assert_eq!(
        dice("🎰", 64).result_line(),
        "Seven · Seven · Seven, jackpot!"
    );
    assert_eq!(dice("🎰", 22).result_line(), "Grapes · Grapes · Grapes");
    assert_eq!(dice("🎰", 3).result_line(), "Lemon · Bar · Bar");
    // A value outside the machine's range is shown as a plain number.
    assert_eq!(dice("🎰", 99).result_line(), "Rolled 99");
}

// Phase 4.4 safe rule: `value` is required — a missing (or
// non-integer) value can't be displayed honestly, so the message
// becomes `Unsupported` instead of inventing a number.
#[test]
fn message_dice_without_value_is_unsupported() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":116,"chat_id":17,"is_outgoing":false,"content":{"@type":"messageDice","emoji":"🎲"}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            match &message.content {
                MessageContent::Unsupported { type_name } => {
                    assert_eq!(type_name, "messageDice")
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

// Phase 4.4: an empty `emoji` falls back to the plain die rather than
// rendering nothing (the value still parses).
#[test]
fn message_dice_empty_emoji_falls_back_to_die() {
    // Parser-level: emoji present but empty in the JSON (not constructed
    // directly) — the row falls back to 🎲.
    let json = r#"{"@type":"updateNewMessage","message":{"id":116,"chat_id":17,"is_outgoing":false,"content":{"@type":"messageDice","emoji":"","value":3}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            let MessageContent::Dice(dice) = &message.content else {
                panic!("{message:?}");
            };
            assert_eq!(dice.face(), "🎲");
            assert_eq!(dice.label(), "🎲 3");
        }
        other => panic!("{other:?}"),
    }
}

// Phase 4.3 safe rule: coordinates must be finite and in range; the
// location is dropped otherwise (the message renders as
// `Unsupported`). Note `serde_json` already rejects out-of-range
// float literals (`1e999`) at the parse boundary, so infinities
// can't reach `geo_location` from text; a huge-but-finite value
// still fails the `|lat| <= 90` range check below.
#[test]
fn location_rejects_non_finite_coordinates() {
    assert!(
        serde_json::from_str::<serde_json::Value>(r#"{"latitude":1e999,"longitude":0.0}"#).is_err()
    );
    let value: serde_json::Value =
        serde_json::from_str(r#"{"latitude":1e308,"longitude":0.0}"#).unwrap();
    assert!(geo_location(Some(&value)).is_none());
    assert!(geo_location(None).is_none());
}

#[test]
fn location_rejects_out_of_range_coordinates() {
    assert!(
        geo_location(Some(
            &serde_json::json!({"latitude": 95.0, "longitude": 0.0})
        ))
        .is_none()
    );
    assert!(
        geo_location(Some(
            &serde_json::json!({"latitude": 0.0, "longitude": -190.0})
        ))
        .is_none()
    );
}

#[test]
fn message_location_with_bad_coordinates_is_unsupported() {
    let json = r#"{"@type":"updateNewMessage","message":{"id":113,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageLocation","location":{"@type":"location","latitude":95.0,"longitude":200.0,"horizontal_accuracy":0}}}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
            match &message.content {
                MessageContent::Unsupported { type_name } => {
                    assert_eq!(type_name, "messageLocation")
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn add_contact_shape_uses_imported_contact() {
    // The `addContact` JSON shape lives in requests.rs, but the schema
    // contract it must match is pinned here: `addContact
    // user_id:int53 contact:importedContact share_phone_number:Bool =
    // Ok` with `importedContact phone_number:string first_name:string
    // last_name:string note:formattedText` (schema 1.8.67 lines 14513 /
    // 7382). This test guards the field list, not the builder.
    let line = include_str!("../../../schema/td_api.tl")
        .lines()
        .find(|l| l.starts_with("addContact "))
        .expect("addContact in schema");
    assert!(
        line.contains("contact:importedContact"),
        "unexpected addContact signature: {line}"
    );
    let imported = include_str!("../../../schema/td_api.tl")
        .lines()
        .find(|l| l.starts_with("importedContact "))
        .expect("importedContact in schema");
    for field in [
        "phone_number:string",
        "first_name:string",
        "last_name:string",
        "note:formattedText",
    ] {
        assert!(
            imported.contains(field),
            "importedContact missing {field}: {imported}"
        );
    }
}
