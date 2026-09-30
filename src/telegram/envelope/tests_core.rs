use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

#[test]
fn unknown_variant_does_not_keep_raw_json() {
    let json = r#"{"@type":"updateSomethingSecret","secret":"CANARY_PHONE_+1555","@extra":"1"}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Unknown(ref kind) => {
            assert_eq!(kind.type_name, "updateSomethingSecret")
        }
        other => panic!("unexpected {other:?}"),
    }
    let debug = format!("{env:?}");
    assert!(!debug.contains("CANARY_PHONE"));
    assert!(!debug.contains("+1555"));
}

/// Phase B1: `updateSecretChat` parses the full `secretChat` record —
/// all three states plus the base64 `key_hash` bytes, `is_outbound`,
/// and `layer`.
#[test]
fn secret_chat_states_parsed_with_key_hash() {
    // Sequential-bytes fixture (0x00..0x24), built at runtime so the
    // literal never trips the secret scanner (gitleaks false positive).
    let key_hash_b64 = STANDARD.encode((0u8..36).collect::<Vec<u8>>());
    for (state_type, expected) in [
        ("secretChatStatePending", SecretChatState::Pending),
        ("secretChatStateReady", SecretChatState::Ready),
        ("secretChatStateClosed", SecretChatState::Closed),
    ] {
        let json = format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":7,"user_id":41,"state":{{"@type":"{state_type}"}},"is_outbound":true,"key_hash":"{key_hash_b64}","layer":144}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateSecretChat { secret_chat } => {
                assert_eq!(secret_chat.id, 7);
                assert_eq!(secret_chat.user_id, 41);
                assert_eq!(secret_chat.state, expected);
                assert!(secret_chat.is_outbound);
                assert_eq!(secret_chat.key_hash.len(), 36);
                assert_eq!(secret_chat.key_hash[0], 0x00);
                assert_eq!(secret_chat.key_hash[35], 0x23);
                assert_eq!(secret_chat.layer, 144);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// Phase B2: `Debug` on `ParsedSecretChat` redacts the key bytes
/// (length only) — a stray `{env:?}` in a log can never leak key
/// material.
#[test]
fn secret_chat_debug_redacts_key_hash() {
    let secret_chat = ParsedSecretChat {
        id: 7,
        user_id: 41,
        state: SecretChatState::Ready,
        is_outbound: true,
        key_hash: vec![0xAB; 36],
        layer: 144,
    };
    let debug = format!("{secret_chat:?}");
    assert!(debug.contains("key_hash_len: 36"));
    // 0xAB = 171; a full-bytes Debug would print it 36 times.
    assert!(!debug.contains("171"));
}

/// Phase B1: an unknown `SecretChatState` constructor degrades to
/// `SecretChatState::Unknown` instead of failing the envelope parse.
#[test]
fn secret_chat_unknown_state_degrades() {
    let env = parse_envelope(
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":7,"user_id":41,"state":{"@type":"secretChatStateFuture"},"is_outbound":false,"key_hash":"","layer":144}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateSecretChat { secret_chat } => {
            assert_eq!(
                secret_chat.state,
                SecretChatState::Unknown("secretChatStateFuture".to_string())
            );
            assert!(secret_chat.key_hash.is_empty());
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Phase B3: `message.self_destruct_type` / `message.self_destruct_in`
/// parse (schema 1.8.67 lines 3146–3147 / 3165 / 5915 / 5918); unknown
/// future variants degrade to `None`; absent/null fields mean no timer.
#[test]
fn self_destruct_type_and_in_parsed() {
    let base = |sd_type: &str, sd_in: &str| {
        format!(
            r#"{{"id":1,"chat_id":41,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}},"self_destruct_type":{sd_type},"self_destruct_in":{sd_in}}}"#
        )
    };
    let parse = |json: &str| {
        let value: Value = serde_json::from_str(json).unwrap();
        parse_message(&value).unwrap()
    };

    let timer = parse(&base(
        r#"{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}"#,
        "42.5",
    ));
    let sd = timer.self_destruct.expect("timer parsed");
    assert_eq!(sd.kind, SelfDestructKind::Timer { secs: 60 });
    assert_eq!(sd.expires_in_ms, 42_500);

    let immediate = parse(&base(
        r#"{"@type":"messageSelfDestructTypeImmediately"}"#,
        "0",
    ));
    let sd = immediate.self_destruct.expect("immediately parsed");
    assert_eq!(sd.kind, SelfDestructKind::Immediately);
    assert_eq!(sd.remaining_secs(sd.fetched_at_ms), None);

    let plain = parse(&base("null", "0"));
    assert_eq!(plain.self_destruct, None);

    let future = parse(&base(
        r#"{"@type":"messageSelfDestructTypeFuture"}"#,
        "10.0",
    ));
    assert_eq!(future.self_destruct, None);

    // `remaining_secs` decays locally; garbage `self_destruct_in`
    // values degrade to "not scheduled".
    let sd = timer.self_destruct.unwrap();
    assert_eq!(sd.remaining_secs(sd.fetched_at_ms), Some(43));
    assert_eq!(sd.remaining_secs(sd.fetched_at_ms + 42_500), Some(0));
    assert_eq!(sd.badge_label(sd.fetched_at_ms), "⏱ 43s left");
    let never = parse(&base(
        r#"{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}"#,
        "0",
    ));
    assert_eq!(
        never
            .self_destruct
            .unwrap()
            .badge_label(never.self_destruct.unwrap().fetched_at_ms),
        "⏱ 60s"
    );
    let nan = parse(&base(
        r#"{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60}"#,
        "-3.0",
    ));
    assert_eq!(nan.self_destruct.unwrap().remaining_secs(u64::MAX), None);
}

#[test]
fn int64_order_is_not_float() {
    let json = r#"{"@type":"updateChatPosition","chat_id":42,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9223372036854775806","is_pinned":false}}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatPosition(pos) => {
            assert_eq!(pos.order, 9223372036854775806);
        }
        other => panic!("{other:?}"),
    }
}
