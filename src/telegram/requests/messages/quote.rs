//! `inputTextQuote` JSON: the quoted text with the formatting a quote keeps,
//! and for a quote cut from a rich message its `RichText`s.
#![deny(clippy::indexing_slicing)]

use crate::composer::QuoteSelection;
use crate::text::{TextEntity, TextEntityKind, utf8_to_utf16_offset};
use serde_json::{Value, json};

/// `inputTextQuote` JSON (TDLib 1.8.69, `schema/td_api.tl:3488`):
/// `inputTextQuote text:formattedText rich_texts:vector<RichText> position:int32`.
/// `position` is the offset of the quoted text in the original message in
/// UTF-16 code units. `text` keeps only Bold, Italic, Underline,
/// Strikethrough, Spoiler, CustomEmoji and DateTime entities, as the schema
/// requires. A quote cut from a rich message sends one `RichText` per quoted
/// text (TDLib then ignores `text`); an ordinary message sends the empty
/// array.
pub fn input_text_quote_json(quote: &QuoteSelection) -> Value {
    json!({
        "@type": "inputTextQuote",
        "text": {
            "@type": "formattedText",
            "text": quote.text,
            "entities": entities_json(&quote.text, &quote.entities),
        },
        "rich_texts": quote
            .rich_texts
            .iter()
            .map(crate::rich::QuoteRichText::to_json)
            .collect::<Vec<_>>(),
        "position": quote.position,
    })
}

/// `textEntity` JSON with UTF-16 offsets for each entity a quote keeps.
fn entities_json(text: &str, entities: &[TextEntity]) -> Vec<Value> {
    entities
        .iter()
        .filter_map(|entity| {
            let kind = entity_type_json(&entity.kind)?;
            let start = utf8_to_utf16_offset(text, entity.utf8_start).ok()?;
            let end = utf8_to_utf16_offset(text, entity.utf8_end).ok()?;
            (end > start).then(|| {
                json!({
                    "@type": "textEntity",
                    "offset": start,
                    "length": end - start,
                    "type": kind,
                })
            })
        })
        .collect()
}

/// The entity type JSON, for the types a quote keeps
/// ([`crate::text::quote::quote_keeps`]) only.
fn entity_type_json(kind: &TextEntityKind) -> Option<Value> {
    Some(match kind {
        TextEntityKind::Bold => json!({"@type": "textEntityTypeBold"}),
        TextEntityKind::Italic => json!({"@type": "textEntityTypeItalic"}),
        TextEntityKind::Underline => json!({"@type": "textEntityTypeUnderline"}),
        TextEntityKind::Strikethrough => json!({"@type": "textEntityTypeStrikethrough"}),
        TextEntityKind::Spoiler => json!({"@type": "textEntityTypeSpoiler"}),
        TextEntityKind::CustomEmoji { custom_emoji_id } => json!({
            "@type": "textEntityTypeCustomEmoji",
            // int64 ids travel as strings.
            "custom_emoji_id": custom_emoji_id.to_string(),
        }),
        // The date's own format; null when it had none, as TDLib sends it.
        TextEntityKind::DateTime { unix_time, format } => json!({
            "@type": "textEntityTypeDateTime",
            "unix_time": unix_time,
            "formatting_type": format.to_json(),
        }),
        _ => return None,
    })
}
