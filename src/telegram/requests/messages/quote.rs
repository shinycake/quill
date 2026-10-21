//! `inputTextQuote` JSON: the quoted text with its formatting, and for a
//! quote cut from a rich message the same text as `RichText`.
#![deny(clippy::indexing_slicing)]

use crate::composer::QuoteSelection;
use crate::text::{TextEntity, TextEntityKind, utf8_to_utf16_offset};
use serde_json::{Value, json};

/// `inputTextQuote` JSON (TDLib 1.8.69, `schema/td_api.tl:3488`):
/// `inputTextQuote text:formattedText rich_texts:vector<RichText> position:int32`.
/// `position` is the offset of the quoted text in the original message in
/// UTF-16 code units. The quote keeps the formatting of the part it was cut
/// from. A quote cut from a rich message also sends its text as one
/// `RichText` (the schema wants one element per quoted text; a quote of a
/// single paragraph is one), and an ordinary message sends the empty array.
pub fn input_text_quote_json(quote: &QuoteSelection) -> Value {
    json!({
        "@type": "inputTextQuote",
        "text": {
            "@type": "formattedText",
            "text": quote.text,
            "entities": entities_json(&quote.text, &quote.entities),
        },
        "rich_texts": if quote.from_rich {
            vec![rich_text_json(&quote.text, &quote.entities)]
        } else {
            Vec::new()
        },
        "position": quote.position,
    })
}

/// `textEntity` JSON for each entity, with UTF-16 offsets.
fn entities_json(text: &str, entities: &[TextEntity]) -> Vec<Value> {
    entities
        .iter()
        .filter_map(|entity| {
            let start = utf8_to_utf16_offset(text, entity.utf8_start).ok()?;
            let end = utf8_to_utf16_offset(text, entity.utf8_end).ok()?;
            (end > start).then(|| {
                json!({
                    "@type": "textEntity",
                    "offset": start,
                    "length": end - start,
                    "type": entity_type_json(&entity.kind),
                })
            })
        })
        .collect()
}

fn entity_type_json(kind: &TextEntityKind) -> Value {
    match kind {
        TextEntityKind::Url => json!({"@type": "textEntityTypeUrl"}),
        TextEntityKind::TextUrl { url } => json!({"@type": "textEntityTypeTextUrl", "url": url}),
        TextEntityKind::Bold => json!({"@type": "textEntityTypeBold"}),
        TextEntityKind::Italic => json!({"@type": "textEntityTypeItalic"}),
        TextEntityKind::Underline => json!({"@type": "textEntityTypeUnderline"}),
        TextEntityKind::Strikethrough => json!({"@type": "textEntityTypeStrikethrough"}),
        TextEntityKind::Spoiler => json!({"@type": "textEntityTypeSpoiler"}),
        TextEntityKind::Code => json!({"@type": "textEntityTypeCode"}),
        TextEntityKind::Pre => json!({"@type": "textEntityTypePre"}),
        TextEntityKind::PreCode { language } => {
            json!({"@type": "textEntityTypePreCode", "language": language})
        }
        TextEntityKind::BlockQuote => json!({"@type": "textEntityTypeBlockQuote"}),
        TextEntityKind::ExpandableBlockQuote => {
            json!({"@type": "textEntityTypeExpandableBlockQuote"})
        }
        TextEntityKind::CustomEmoji { custom_emoji_id } => json!({
            "@type": "textEntityTypeCustomEmoji",
            // int64 ids travel as strings.
            "custom_emoji_id": custom_emoji_id.to_string(),
        }),
        TextEntityKind::Mention => json!({"@type": "textEntityTypeMention"}),
        TextEntityKind::MentionName { user_id } => {
            json!({"@type": "textEntityTypeMentionName", "user_id": user_id})
        }
        TextEntityKind::Hashtag => json!({"@type": "textEntityTypeHashtag"}),
        TextEntityKind::Cashtag => json!({"@type": "textEntityTypeCashtag"}),
        TextEntityKind::BotCommand => json!({"@type": "textEntityTypeBotCommand"}),
        TextEntityKind::EmailAddress => json!({"@type": "textEntityTypeEmailAddress"}),
        TextEntityKind::PhoneNumber => json!({"@type": "textEntityTypePhoneNumber"}),
        TextEntityKind::BankCardNumber => json!({"@type": "textEntityTypeBankCardNumber"}),
        TextEntityKind::TonAddress => json!({"@type": "textEntityTypeTonAddress"}),
        TextEntityKind::MediaTimestamp { seconds } => {
            json!({"@type": "textEntityTypeMediaTimestamp", "media_timestamp": seconds})
        }
        TextEntityKind::DateTime { unix_time } => json!({
            "@type": "textEntityTypeDateTime",
            "unix_time": unix_time,
            "formatting_type": null,
        }),
    }
}

/// A `RichText` for `text`. A rich quote may hold plain text and these
/// styles only: bold, italic, underline, strikethrough and spoiler
/// (`schema/td_api.tl:3472`). Other entities (code, links, mentions) stay as
/// plain text, as they are plain in the quote's `text` too.
fn rich_text_json(text: &str, entities: &[TextEntity]) -> Value {
    let styles: Vec<(&TextEntity, &str)> = entities
        .iter()
        .filter_map(|entity| {
            let name = match entity.kind {
                TextEntityKind::Bold => "richTextBold",
                TextEntityKind::Italic => "richTextItalic",
                TextEntityKind::Underline => "richTextUnderline",
                TextEntityKind::Strikethrough => "richTextStrikethrough",
                TextEntityKind::Spoiler => "richTextSpoiler",
                _ => return None,
            };
            (entity.utf8_start < entity.utf8_end).then_some((entity, name))
        })
        .collect();
    let mut cuts: Vec<usize> = vec![0, text.len()];
    for (entity, _) in &styles {
        cuts.push(entity.utf8_start);
        cuts.push(entity.utf8_end);
    }
    cuts.retain(|cut| text.is_char_boundary(*cut));
    cuts.sort_unstable();
    cuts.dedup();
    let mut parts: Vec<Value> = Vec::new();
    for pair in cuts.windows(2) {
        let (Some(&from), Some(&to)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let Some(piece) = text.get(from..to).filter(|piece| !piece.is_empty()) else {
            continue;
        };
        let mut node = json!({"@type": "richTextPlain", "text": piece});
        for (entity, name) in &styles {
            if entity.utf8_start <= from && to <= entity.utf8_end {
                node = json!({"@type": name, "text": node});
            }
        }
        parts.push(node);
    }
    match parts.len() {
        0 => json!({"@type": "richTextPlain", "text": ""}),
        1 => parts.pop().unwrap_or(Value::Null),
        _ => json!({"@type": "richTexts", "texts": parts}),
    }
}
