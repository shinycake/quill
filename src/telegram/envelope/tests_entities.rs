//! `parse_text_entities`: every interactive TDLib entity type survives the
//! JSON → `TextEntity` conversion, with UTF-16 offsets mapped to bytes.

use super::message_content::parse_text_entities;
use crate::text::{LinkTarget, TextEntityKind as K, styled_runs};
use serde_json::{Value, json};

fn formatted(text: &str, entities: Vec<Value>) -> Value {
    json!({"@type": "formattedText", "text": text, "entities": entities})
}

fn entity(offset: usize, length: usize, kind: Value) -> Value {
    json!({"@type": "textEntity", "offset": offset, "length": length, "type": kind})
}

fn kinds(text: &str, entities: Vec<Value>) -> Vec<K> {
    let value = formatted(text, entities);
    parse_text_entities(text, Some(&value))
        .into_iter()
        .map(|entity| entity.kind)
        .collect()
}

#[test]
fn each_interactive_type_is_parsed() {
    let text = "@ann #tag $USD /start a@b.co +1 555 0100 4111111111111111 1:30 x";
    let cases = [
        (0, 4, json!({"@type": "textEntityTypeMention"}), K::Mention),
        (5, 4, json!({"@type": "textEntityTypeHashtag"}), K::Hashtag),
        (10, 4, json!({"@type": "textEntityTypeCashtag"}), K::Cashtag),
        (
            15,
            6,
            json!({"@type": "textEntityTypeBotCommand"}),
            K::BotCommand,
        ),
        (
            22,
            6,
            json!({"@type": "textEntityTypeEmailAddress"}),
            K::EmailAddress,
        ),
        (
            29,
            11,
            json!({"@type": "textEntityTypePhoneNumber"}),
            K::PhoneNumber,
        ),
        (
            41,
            16,
            json!({"@type": "textEntityTypeBankCardNumber"}),
            K::BankCardNumber,
        ),
        (
            58,
            4,
            json!({"@type": "textEntityTypeMediaTimestamp", "media_timestamp": 90}),
            K::MediaTimestamp { seconds: 90 },
        ),
        (
            0,
            4,
            json!({"@type": "textEntityTypeMentionName", "user_id": 42}),
            K::MentionName { user_id: 42 },
        ),
        (
            0,
            4,
            json!({"@type": "textEntityTypeDateTime", "unix_time": 1700000000, "formatting_type": null}),
            K::DateTime {
                unix_time: 1_700_000_000,
            },
        ),
    ];
    for (offset, length, kind, expected) in cases {
        assert_eq!(
            kinds(text, vec![entity(offset, length, kind.clone())]),
            vec![expected],
            "{kind}"
        );
    }
}

#[test]
fn malformed_interactive_entities_are_dropped() {
    let text = "@ann";
    for kind in [
        json!({"@type": "textEntityTypeMentionName"}),
        json!({"@type": "textEntityTypeMentionName", "user_id": 0}),
        json!({"@type": "textEntityTypeMediaTimestamp", "media_timestamp": -1}),
        json!({"@type": "textEntityTypeMediaTimestamp"}),
        json!({"@type": "textEntityTypeDateTime", "unix_time": -5}),
        json!({"@type": "textEntityTypeSomethingNew"}),
    ] {
        assert!(
            kinds(text, vec![entity(0, 4, kind.clone())]).is_empty(),
            "{kind}"
        );
    }
}

#[test]
fn offsets_count_utf16_units_past_emoji_and_scripts() {
    // "😀" is two UTF-16 units; "שלום" four.
    let text = "😀 שלום @ann";
    let value = formatted(
        text,
        vec![entity(8, 4, json!({"@type": "textEntityTypeMention"}))],
    );
    let parsed = parse_text_entities(text, Some(&value));
    assert_eq!(parsed.len(), 1);
    assert_eq!(&text[parsed[0].utf8_start..parsed[0].utf8_end], "@ann");
}

#[test]
fn runs_carry_the_link_target_of_each_entity() {
    let text = "hi @ann #tag t.me";
    let value = formatted(
        text,
        vec![
            entity(3, 4, json!({"@type": "textEntityTypeMention"})),
            entity(8, 4, json!({"@type": "textEntityTypeHashtag"})),
            entity(
                13,
                4,
                json!({"@type": "textEntityTypeTextUrl", "url": "https://t.me/x"}),
            ),
        ],
    );
    let entities = parse_text_entities(text, Some(&value));
    let links: Vec<_> = styled_runs(text, &entities)
        .into_iter()
        .filter_map(|run| run.link)
        .collect();
    assert_eq!(
        links,
        vec![
            LinkTarget::Mention("@ann".into()),
            LinkTarget::Hashtag("#tag".into()),
            LinkTarget::Url {
                url: "https://t.me/x".into(),
                label: Some("t.me".into())
            },
        ]
    );
}

#[test]
fn adjacent_mentions_do_not_merge_into_one_run() {
    let text = "@a@b";
    let value = formatted(
        text,
        vec![
            entity(0, 2, json!({"@type": "textEntityTypeMention"})),
            entity(2, 2, json!({"@type": "textEntityTypeMention"})),
        ],
    );
    let entities = parse_text_entities(text, Some(&value));
    assert_eq!(styled_runs(text, &entities).len(), 2);
}

#[test]
fn a_scheme_less_url_entity_opens_over_https() {
    let text = "see example.com/a now";
    let value = formatted(
        text,
        vec![entity(4, 13, json!({"@type": "textEntityTypeUrl"}))],
    );
    let entities = parse_text_entities(text, Some(&value));
    let run = styled_runs(text, &entities)
        .into_iter()
        .find(|run| run.href.is_some())
        .expect("a link run");
    assert_eq!(run.href.as_deref(), Some("https://example.com/a"));
    assert_eq!(run.text, "example.com/a");
}
