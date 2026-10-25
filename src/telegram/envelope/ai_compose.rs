//! AI compose updates and answers (TDLib 1.8.69): the style list, one
//! style, a style example and the `fixTextWithAi` answer. Wrapped as
//! [`MessagesPayload::AiCompose`]; the bare `formattedText` answer of
//! `composeTextWithAi` stays [`CommonPayload::FormattedText`], which many
//! requests share.
use crate::ai_compose::{DiffEntity, DiffKind, DiffText, StyleExample, TextCompositionStyle};
use crate::telegram::envelope::*;
use crate::text::TextEntity;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiComposePayload {
    /// `updateTextCompositionStyles` (`schema/td_api.tl:11945`).
    UpdateStyles { styles: Vec<TextCompositionStyle> },
    /// `textCompositionStyle` (`:181`): the answer of
    /// `searchTextCompositionStyle`, `createTextCompositionStyle` and
    /// `editTextCompositionStyle`.
    Style(TextCompositionStyle),
    /// `textCompositionStyleExample` (`:169`): the
    /// `getTextCompositionStyleExample` answer.
    StyleExample(StyleExample),
    /// `fixedText` (`:163`): the `fixTextWithAi` answer, for the composer's
    /// rich-text tools and the compose box alike.
    FixedText {
        text: String,
        entities: Vec<TextEntity>,
        diff: DiffText,
    },
}

/// The AI compose TDLib types; `Ok(None)` for any other type.
pub(crate) fn parse_ai_compose_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateTextCompositionStyles" => AiComposePayload::UpdateStyles {
            styles: value
                .get("styles")
                .and_then(Value::as_array)
                .map(|styles| styles.iter().filter_map(parse_style).collect())
                .unwrap_or_default(),
        },
        "textCompositionStyle" => {
            AiComposePayload::Style(parse_style(value).ok_or(ParseError::MissingField)?)
        }
        "textCompositionStyleExample" => AiComposePayload::StyleExample(parse_example(value)),
        "fixedText" => {
            let (text, entities) = formatted(value.get("text"));
            AiComposePayload::FixedText {
                text,
                entities,
                diff: parse_diff_text(value.get("diff_text")),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(EnvelopePayload::Messages(MessagesPayload::AiCompose(
        payload,
    ))))
}

fn formatted(value: Option<&Value>) -> (String, Vec<TextEntity>) {
    let text = parse_formatted_text(value);
    let entities = parse_text_entities(&text, value);
    (text, entities)
}

/// A style without a name can't be asked for, so it is dropped.
fn parse_style(value: &Value) -> Option<TextCompositionStyle> {
    let name = json_field_str(value, "name");
    if name.is_empty() {
        return None;
    }
    Some(TextCompositionStyle {
        name,
        custom_emoji_id: json_i64_field(value.get("custom_emoji_id"), 0),
        title: json_field_str(value, "title"),
        is_custom: json_bool(value.get("is_custom"), false),
        is_creator: json_bool(value.get("is_creator"), false),
        install_count: json_i32(value.get("install_count"), 0).max(0),
        prompt: json_field_str(value, "prompt"),
        creator_user_id: int53_or_zero(value.get("creator_user_id")).max(0),
        english_example: value
            .get("english_example")
            .filter(|example| example.is_object())
            .map(parse_example),
    })
}

fn parse_example(value: &Value) -> StyleExample {
    let (source_text, source_entities) = formatted(value.get("source_text"));
    let (result_text, result_entities) = formatted(value.get("result_text"));
    StyleExample {
        source_text,
        source_entities,
        result_text,
        result_entities,
    }
}

/// `diffText text:string entities:vector<diffEntity>` (`:160`). Entities
/// of an unknown type are dropped; the offsets stay UTF-16 for
/// [`crate::ai_compose::diff_segments`].
fn parse_diff_text(value: Option<&Value>) -> DiffText {
    let Some(value) = value else {
        return DiffText::default();
    };
    let edits = value
        .get("entities")
        .and_then(Value::as_array)
        .map(|entities| entities.iter().filter_map(parse_diff_entity).collect())
        .unwrap_or_default();
    DiffText {
        text: json_field_str(value, "text"),
        edits,
    }
}

fn parse_diff_entity(value: &Value) -> Option<DiffEntity> {
    let kind = value.get("type")?;
    let kind = match kind.get("@type").and_then(Value::as_str)? {
        "diffEntityTypeInsert" => DiffKind::Insert,
        "diffEntityTypeReplace" => DiffKind::Replace {
            old_text: json_field_str(kind, "old_text"),
        },
        "diffEntityTypeDelete" => DiffKind::Delete,
        _ => return None,
    };
    Some(DiffEntity {
        offset: json_i32(value.get("offset"), 0),
        length: json_i32(value.get("length"), 0),
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(value: Value) -> Option<AiComposePayload> {
        let type_name = value["@type"].as_str().unwrap().to_string();
        match parse_ai_compose_payload(&type_name, &value) {
            Ok(Some(EnvelopePayload::Messages(MessagesPayload::AiCompose(payload)))) => {
                Some(payload)
            }
            _ => None,
        }
    }

    fn style_json(name: &str) -> Value {
        json!({
            "@type": "textCompositionStyle",
            "name": name,
            "custom_emoji_id": "5368324170671202286",
            "title": "Pirate",
            "is_custom": true,
            "is_creator": false,
            "install_count": 12,
            "prompt": "talk like a pirate",
            "creator_user_id": 42,
            "english_example": {
                "@type": "textCompositionStyleExample",
                "source_text": {"@type": "formattedText", "text": "Hello", "entities": []},
                "result_text": {"@type": "formattedText", "text": "Ahoy", "entities": [
                    {"@type": "textEntity", "offset": 0, "length": 4, "type": {"@type": "textEntityTypeBold"}}
                ]}
            }
        })
    }

    #[test]
    fn style_list_parses_every_field() {
        let Some(AiComposePayload::UpdateStyles { styles }) = parse(json!({
            "@type": "updateTextCompositionStyles",
            "styles": [style_json("pirate"), style_json(""), {
                "@type": "textCompositionStyle", "name": "formal", "title": "Formal",
                "custom_emoji_id": 0, "is_custom": false, "is_creator": false,
                "install_count": 0, "prompt": "", "creator_user_id": 0, "english_example": null
            }]
        })) else {
            panic!("styles");
        };
        assert_eq!(styles.len(), 2, "a nameless style is dropped");
        let pirate = &styles[0];
        assert_eq!(pirate.name, "pirate");
        assert_eq!(pirate.custom_emoji_id, 5368324170671202286);
        assert_eq!(pirate.title, "Pirate");
        assert!(pirate.is_custom && !pirate.is_creator);
        assert_eq!(pirate.install_count, 12);
        assert_eq!(pirate.prompt, "talk like a pirate");
        assert_eq!(pirate.creator_user_id, 42);
        let example = pirate.english_example.as_ref().expect("example");
        assert_eq!(example.source_text, "Hello");
        assert_eq!(example.result_text, "Ahoy");
        assert_eq!(example.result_entities.len(), 1);
        assert!(styles[1].english_example.is_none());
    }

    #[test]
    fn single_style_and_example_answers() {
        assert!(matches!(
            parse(style_json("pirate")),
            Some(AiComposePayload::Style(style)) if style.name == "pirate"
        ));
        assert!(parse_ai_compose_payload("textCompositionStyle", &style_json("")).is_err());
        assert!(matches!(
            parse(json!({
                "@type": "textCompositionStyleExample",
                "source_text": {"@type": "formattedText", "text": "a", "entities": []},
                "result_text": {"@type": "formattedText", "text": "b", "entities": []}
            })),
            Some(AiComposePayload::StyleExample(example)) if example.result_text == "b"
        ));
    }

    #[test]
    fn fixed_text_keeps_the_diff() {
        let Some(AiComposePayload::FixedText {
            text,
            entities,
            diff,
        }) = parse(json!({
            "@type": "fixedText",
            "text": {"@type": "formattedText", "text": "the draft", "entities": [
                {"@type": "textEntity", "offset": 4, "length": 5, "type": {"@type": "textEntityTypeItalic"}}
            ]},
            "diff_text": {"@type": "diffText", "text": "the very draft", "entities": [
                {"@type": "diffEntity", "offset": 0, "length": 3,
                 "type": {"@type": "diffEntityTypeReplace", "old_text": "teh"}},
                {"@type": "diffEntity", "offset": 4, "length": 5,
                 "type": {"@type": "diffEntityTypeDelete"}},
                {"@type": "diffEntity", "offset": 0, "length": 1,
                 "type": {"@type": "diffEntityTypeFuture"}}
            ]}
        }))
        else {
            panic!("fixedText");
        };
        assert_eq!(text, "the draft");
        assert_eq!(entities.len(), 1);
        assert_eq!(diff.text, "the very draft");
        assert_eq!(
            diff.edits,
            [
                DiffEntity {
                    offset: 0,
                    length: 3,
                    kind: DiffKind::Replace {
                        old_text: "teh".into()
                    }
                },
                DiffEntity {
                    offset: 4,
                    length: 5,
                    kind: DiffKind::Delete
                },
            ]
        );
    }

    #[test]
    fn other_types_are_left_alone() {
        assert!(matches!(
            parse_ai_compose_payload("formattedText", &json!({})),
            Ok(None)
        ));
    }
}
