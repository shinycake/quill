//! What a quote of a rich message is made of: the message's quotable texts
//! (TDLib `WebPageBlock::for_each_quote_text`) and the part of one that a
//! quote keeps (`RichText::get_quote_fragment`). Kept apart from the display
//! blocks, which add list labels and quote marks that a quote never counts.
#![deny(clippy::indexing_slicing)]

use crate::text::date_time_format::DateTimeFormat;
use crate::text::{TextEntity, TextEntityKind};
use serde_json::{Value, json};

/// Deeper nesting is not quotable (panic-safety rule 13).
const MAX_DEPTH: usize = 64;

/// A style a quote keeps (`schema/td_api.tl:3474`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Spoiler,
    Subscript,
    Superscript,
}

impl QuoteStyle {
    const ALL: [Self; 7] = [
        Self::Bold,
        Self::Italic,
        Self::Underline,
        Self::Strikethrough,
        Self::Spoiler,
        Self::Subscript,
        Self::Superscript,
    ];

    fn type_name(self) -> &'static str {
        match self {
            Self::Bold => "richTextBold",
            Self::Italic => "richTextItalic",
            Self::Underline => "richTextUnderline",
            Self::Strikethrough => "richTextStrikethrough",
            Self::Spoiler => "richTextSpoiler",
            Self::Subscript => "richTextSubscript",
            Self::Superscript => "richTextSuperscript",
        }
    }

    /// The entity that draws the style; Quill has none for sub- and
    /// superscript, which stay plain on screen but keep their `RichText`.
    fn entity(self) -> Option<TextEntityKind> {
        match self {
            Self::Bold => Some(TextEntityKind::Bold),
            Self::Italic => Some(TextEntityKind::Italic),
            Self::Underline => Some(TextEntityKind::Underline),
            Self::Strikethrough => Some(TextEntityKind::Strikethrough),
            Self::Spoiler => Some(TextEntityKind::Spoiler),
            Self::Subscript | Self::Superscript => None,
        }
    }
}

/// A `RichText` with only what a quote may hold (`schema/td_api.tl:3474`):
/// plain text, the [`QuoteStyle`]s, date-times, custom emoji and math.
/// Links, code, mentions and the other wrappers keep their text and lose
/// the wrapper, as TDLib does when it stores a quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuoteRichText {
    Plain(String),
    Styled {
        style: QuoteStyle,
        text: Box<QuoteRichText>,
    },
    DateTime {
        unix_time: i32,
        format: DateTimeFormat,
        text: Box<QuoteRichText>,
    },
    /// Quoted whole or not at all.
    CustomEmoji {
        custom_emoji_id: i64,
        alternative_text: String,
    },
    /// Quoted whole or not at all.
    Math(String),
    Concat(Vec<QuoteRichText>),
}

impl QuoteRichText {
    /// A received `RichText`. `None` for a type Quill does not know, whose
    /// text it could not count.
    pub fn from_json(value: &Value) -> Option<Self> {
        Self::parse(value, 0)
    }

    fn parse(value: &Value, depth: usize) -> Option<Self> {
        if depth > MAX_DEPTH {
            return None;
        }
        let inner = |field: &str| Self::parse(value.get(field)?, depth + 1);
        let string = |field: &str| {
            value
                .get(field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let kind = value.get("@type").and_then(Value::as_str)?;
        if let Some(style) = QuoteStyle::ALL
            .into_iter()
            .find(|style| style.type_name() == kind)
        {
            return Some(Self::Styled {
                style,
                text: Box::new(inner("text")?),
            });
        }
        Some(match kind {
            "richTextPlain" => Self::Plain(string("text")),
            "richTexts" => Self::Concat(
                value
                    .get("texts")?
                    .as_array()?
                    .iter()
                    .map(|text| Self::parse(text, depth + 1))
                    .collect::<Option<_>>()?,
            ),
            "richTextDateTime" => Self::DateTime {
                unix_time: i32::try_from(value.get("unix_time")?.as_i64()?).ok()?,
                format: DateTimeFormat::from_json(value.get("formatting_type"))?,
                text: Box::new(inner("text")?),
            },
            "richTextCustomEmoji" => Self::CustomEmoji {
                custom_emoji_id: crate::telegram::envelope::int64(value.get("custom_emoji_id"))?,
                alternative_text: string("alternative_text"),
            },
            "richTextMathematicalExpression" => Self::Math(string("expression")),
            // Formatting a quote cannot keep: only the text is quoted. A diff
            // quotes its new text.
            "richTextFixed"
            | "richTextUrl"
            | "richTextEmailAddress"
            | "richTextPhoneNumber"
            | "richTextMarked"
            | "richTextMention"
            | "richTextHashtag"
            | "richTextCashtag"
            | "richTextBankCardNumber"
            | "richTextTonAddress"
            | "richTextBotCommand"
            | "richTextMentionName"
            | "richTextReference"
            | "richTextReferenceLink"
            | "richTextAnchorLink"
            | "richTextDiff" => inner("text")?,
            "richTextButton" => Self::parse(value.get("button")?.get("text")?, depth + 1)?,
            // Icons and anchors have no text.
            "richTextIcon" | "richTextAnchor" => Self::Plain(String::new()),
            _ => return None,
        })
    }

    /// The plain text a quote position counts in
    /// (`RichText::get_quote_text`).
    pub fn quote_text(&self) -> String {
        self.parts().0
    }

    /// The plain text with the entities Quill draws it with.
    pub fn parts(&self) -> (String, Vec<TextEntity>) {
        let mut text = String::new();
        let mut entities = Vec::new();
        self.walk(&mut text, &mut entities);
        (text, entities)
    }

    fn walk(&self, text: &mut String, entities: &mut Vec<TextEntity>) {
        let start = text.len();
        let kind = match self {
            Self::Plain(plain) | Self::Math(plain) => {
                text.push_str(plain);
                None
            }
            Self::CustomEmoji {
                custom_emoji_id,
                alternative_text,
            } => {
                text.push_str(alternative_text);
                Some(TextEntityKind::CustomEmoji {
                    custom_emoji_id: *custom_emoji_id,
                })
            }
            Self::Styled { style, text: inner } => {
                inner.walk(text, entities);
                style.entity()
            }
            Self::DateTime {
                unix_time,
                format,
                text: inner,
            } => {
                inner.walk(text, entities);
                Some(TextEntityKind::DateTime {
                    unix_time: *unix_time,
                    format: *format,
                })
            }
            Self::Concat(texts) => {
                for inner in texts {
                    inner.walk(text, entities);
                }
                None
            }
        };
        if let Some(kind) = kind.filter(|_| text.len() > start) {
            entities.push(TextEntity {
                utf8_start: start,
                utf8_end: text.len(),
                kind,
            });
        }
    }

    /// The characters `begin..end` of [`Self::quote_text`] with their
    /// formatting, as TDLib cuts a quote: custom emoji and math only when
    /// wholly inside. `None` when nothing is left.
    pub fn fragment(&self, begin: usize, end: usize) -> Option<Self> {
        let mut pos = 0;
        self.fragment_at(&mut pos, begin, end)
    }

    fn fragment_at(&self, pos: &mut usize, begin: usize, end: usize) -> Option<Self> {
        match self {
            Self::Plain(text) => {
                let start = *pos;
                let size = text.chars().count();
                *pos = start.saturating_add(size);
                let from = begin.max(start);
                let to = end.min(*pos);
                (from < to).then(|| {
                    Self::Plain(
                        text.chars()
                            .skip(from.saturating_sub(start))
                            .take(to.saturating_sub(from))
                            .collect(),
                    )
                })
            }
            Self::CustomEmoji {
                alternative_text: text,
                ..
            }
            | Self::Math(text) => {
                let start = *pos;
                *pos = start.saturating_add(text.chars().count());
                (begin <= start && *pos <= end).then(|| self.clone())
            }
            Self::Styled { style, text } => {
                let inner = text.fragment_at(pos, begin, end)?;
                Some(Self::Styled {
                    style: *style,
                    text: Box::new(inner),
                })
            }
            Self::DateTime {
                unix_time,
                format,
                text,
            } => {
                let inner = text.fragment_at(pos, begin, end)?;
                Some(Self::DateTime {
                    unix_time: *unix_time,
                    format: *format,
                    text: Box::new(inner),
                })
            }
            Self::Concat(texts) => {
                let kept: Vec<Self> = texts
                    .iter()
                    .filter_map(|text| text.fragment_at(pos, begin, end))
                    .collect();
                (!kept.is_empty()).then_some(Self::Concat(kept))
            }
        }
    }

    /// The `RichText` JSON TDLib takes.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Plain(text) => json!({"@type": "richTextPlain", "text": text}),
            Self::Styled { style, text } => {
                json!({"@type": style.type_name(), "text": text.to_json()})
            }
            Self::DateTime {
                unix_time,
                format,
                text,
            } => json!({
                "@type": "richTextDateTime",
                "text": text.to_json(),
                "unix_time": unix_time,
                "formatting_type": format.to_json(),
            }),
            Self::CustomEmoji {
                custom_emoji_id,
                alternative_text,
            } => json!({
                "@type": "richTextCustomEmoji",
                // int64 ids travel as strings.
                "custom_emoji_id": custom_emoji_id.to_string(),
                "alternative_text": alternative_text,
            }),
            Self::Math(expression) => {
                json!({"@type": "richTextMathematicalExpression", "expression": expression})
            }
            Self::Concat(texts) => json!({
                "@type": "richTexts",
                "texts": texts.iter().map(Self::to_json).collect::<Vec<_>>(),
            }),
        }
    }
}

/// The text of a quote made of `texts`, with its formatting: the texts
/// joined by line feeds, as a quote's position counts them.
pub fn join_quote_texts(texts: &[QuoteRichText]) -> (String, Vec<TextEntity>) {
    let mut text = String::new();
    let mut entities = Vec::new();
    for (index, rich) in texts.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        rich.walk(&mut text, &mut entities);
    }
    (text, entities)
}

/// What to show for a received quote's `rich_texts` (`textQuote`, whose
/// `text` TDLib leaves empty for a quote from a rich message): the texts
/// joined by line feeds. A type Quill does not know still shows its text.
pub fn quote_rich_texts_display(rich_texts: &[Value]) -> (String, Vec<TextEntity>) {
    if let Some(texts) = rich_texts
        .iter()
        .map(QuoteRichText::from_json)
        .collect::<Option<Vec<_>>>()
    {
        return join_quote_texts(&texts);
    }
    let mut text = String::new();
    let mut entities = Vec::new();
    for (index, value) in rich_texts.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        let (part, part_entities, _) = super::rich_text_to_parts(value);
        let shift = text.len();
        entities.extend(part_entities.into_iter().map(|entity| TextEntity {
            utf8_start: entity.utf8_start + shift,
            utf8_end: entity.utf8_end + shift,
            kind: entity.kind,
        }));
        text.push_str(&part);
    }
    (text, crate::text::quote::quote_entities(&entities))
}

/// The quotable texts of a received `richMessage` in order of appearance,
/// without the empty ones (`MessageQuote::create_automatic_quote` and
/// tdesktop's quote model skip those too). Empty when a block or text has a
/// type Quill does not know: every position after it would be off.
pub fn rich_message_quote_texts(message: &Value) -> Vec<QuoteRichText> {
    let mut texts = Vec::new();
    let blocks = message.get("blocks").and_then(Value::as_array);
    if blocks
        .and_then(|blocks| push_blocks(blocks, 0, &mut texts))
        .is_none()
    {
        return Vec::new();
    }
    texts.retain(|text| !text.quote_text().is_empty());
    texts
}

fn push_blocks(blocks: &[Value], depth: usize, out: &mut Vec<QuoteRichText>) -> Option<()> {
    blocks
        .iter()
        .try_for_each(|block| push_block(block, depth, out))
}

/// A `RichText` field of a block; a missing or null one has no text.
fn push_text(value: Option<&Value>, out: &mut Vec<QuoteRichText>) -> Option<()> {
    match value {
        None | Some(Value::Null) => Some(()),
        Some(value) => {
            out.push(QuoteRichText::from_json(value)?);
            Some(())
        }
    }
}

/// A `pageBlockCaption`: its text, then its credit.
fn push_caption(value: Option<&Value>, out: &mut Vec<QuoteRichText>) -> Option<()> {
    let caption = value.unwrap_or(&Value::Null);
    push_text(caption.get("text"), out)?;
    push_text(caption.get("credit"), out)
}

fn push_children(block: &Value, depth: usize, out: &mut Vec<QuoteRichText>) -> Option<()> {
    match block.get("blocks") {
        None | Some(Value::Null) => Some(()),
        Some(blocks) => push_blocks(blocks.as_array()?, depth + 1, out),
    }
}

/// TDLib `for_each_quote_text` of each block type, over the `td_api` form.
fn push_block(block: &Value, depth: usize, out: &mut Vec<QuoteRichText>) -> Option<()> {
    if depth > MAX_DEPTH {
        return None;
    }
    let field = |name: &str| block.get(name);
    match block.get("@type").and_then(Value::as_str)? {
        "pageBlockTitle" => push_text(field("title"), out),
        "pageBlockSubtitle" => push_text(field("subtitle"), out),
        "pageBlockAuthorDate" => push_text(field("author"), out),
        "pageBlockHeader" => push_text(field("header"), out),
        "pageBlockSubheader" => push_text(field("subheader"), out),
        "pageBlockKicker" => push_text(field("kicker"), out),
        "pageBlockFooter" => push_text(field("footer"), out),
        "pageBlockSectionHeading"
        | "pageBlockParagraph"
        | "pageBlockPreformatted"
        | "pageBlockThinking" => push_text(field("text"), out),
        "pageBlockDivider"
        | "pageBlockAnchor"
        | "pageBlockChatLink"
        | "pageBlockButtonRow"
        | "pageBlockUnsupported" => Some(()),
        // The whole expression is one quotable text.
        "pageBlockMathematicalExpression" => {
            let expression = field("expression").and_then(Value::as_str)?;
            out.push(QuoteRichText::Math(expression.to_string()));
            Some(())
        }
        // An item's blocks; its label is not text of the message.
        "pageBlockList" => field("items")?
            .as_array()?
            .iter()
            .try_for_each(|item| push_children(item, depth, out)),
        "pageBlockBlockQuote" => {
            push_children(block, depth, out)?;
            push_text(field("credit"), out)
        }
        "pageBlockExpandableBlockQuote" | "pageBlockPullQuote" => {
            push_text(field("text"), out)?;
            push_text(field("credit"), out)
        }
        "pageBlockAnimation" | "pageBlockAudio" | "pageBlockDocument" | "pageBlockPhoto"
        | "pageBlockVideo" | "pageBlockVoiceNote" | "pageBlockEmbedded" | "pageBlockMap" => {
            push_caption(field("caption"), out)
        }
        "pageBlockCover" => push_block(field("cover")?, depth + 1, out),
        "pageBlockEmbeddedPost" | "pageBlockCollage" | "pageBlockSlideshow" => {
            push_children(block, depth, out)?;
            push_caption(field("caption"), out)
        }
        "pageBlockTable" => {
            push_text(field("caption"), out)?;
            field("cells")?.as_array()?.iter().try_for_each(|row| {
                row.as_array()?
                    .iter()
                    .try_for_each(|cell| push_text(cell.get("text"), out))
            })
        }
        "pageBlockDetails" => {
            push_text(field("header"), out)?;
            push_children(block, depth, out)
        }
        "pageBlockRelatedArticles" => push_text(field("header"), out),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{QuoteRichText, QuoteStyle, rich_message_quote_texts};
    use serde_json::json;

    fn plain(text: &str) -> serde_json::Value {
        json!({"@type": "richTextPlain", "text": text})
    }

    #[test]
    fn a_fragment_keeps_the_styles_a_quote_may_hold_and_drops_the_rest() {
        // A bold link, a superscript, code, a custom emoji and math.
        let text = QuoteRichText::from_json(&json!({"@type": "richTexts", "texts": [
            {"@type": "richTextUrl", "url": "https://example.com", "is_cached": false,
             "text": {"@type": "richTextBold", "text": plain("x")}},
            {"@type": "richTextSuperscript", "text": plain("2")},
            {"@type": "richTextFixed", "text": plain(" = 4")},
            {"@type": "richTextCustomEmoji", "custom_emoji_id": "5", "alternative_text": "🙂"},
            {"@type": "richTextMathematicalExpression", "expression": "a+b"},
        ]}))
        .expect("known types");
        assert_eq!(text.quote_text(), "x2 = 4🙂a+b");
        // A cut through the math leaves it out; the emoji is wholly inside.
        let cut = text.fragment(1, 9).expect("something left");
        assert_eq!(cut.quote_text(), "2 = 4🙂");
        assert_eq!(
            cut.to_json(),
            json!({"@type": "richTexts", "texts": [
                {"@type": "richTextSuperscript", "text": plain("2")},
                plain(" = 4"),
                {"@type": "richTextCustomEmoji", "custom_emoji_id": "5", "alternative_text": "🙂"},
            ]})
        );
        let whole = text.fragment(0, 10).expect("everything");
        assert_eq!(whole.quote_text(), text.quote_text());
        assert!(matches!(
            whole,
            QuoteRichText::Concat(ref parts) if matches!(
                parts.first(),
                Some(QuoteRichText::Styled { style: QuoteStyle::Bold, .. })
            )
        ));
        assert!(text.fragment(7, 8).is_none());
    }

    #[test]
    fn a_date_time_keeps_its_format_and_round_trips() {
        let source = json!({"@type": "richTextDateTime", "text": plain("today"),
            "unix_time": 1_700_000_000,
            "formatting_type": {"@type": "dateTimeFormattingTypeRelative"}});
        let text = QuoteRichText::from_json(&source).expect("date-time");
        assert_eq!(text.to_json(), source);
        assert_eq!(
            text.fragment(0, 3).map(|cut| cut.to_json()),
            Some(json!({"@type": "richTextDateTime", "text": plain("tod"),
                "unix_time": 1_700_000_000,
                "formatting_type": {"@type": "dateTimeFormattingTypeRelative"}}))
        );
    }

    #[test]
    fn quotable_texts_follow_tdlib_block_order() {
        let caption = json!({"@type": "pageBlockCaption", "text": plain("photo"),
            "credit": plain("")});
        let texts = rich_message_quote_texts(&json!({"blocks": [
            {"@type": "pageBlockTitle", "title": plain("Title")},
            {"@type": "pageBlockDivider"},
            {"@type": "pageBlockPhoto", "caption": caption},
            {"@type": "pageBlockDetails", "header": plain("More"), "is_open": true,
             "blocks": [{"@type": "pageBlockParagraph", "text": plain("inside")}]},
            {"@type": "pageBlockTable", "caption": plain(""), "cells": [[
                {"@type": "pageBlockTableCell", "text": plain("cell")}]]},
            {"@type": "pageBlockMathematicalExpression", "expression": "e=mc^2"},
            {"@type": "pageBlockPullQuote", "text": plain("pull"), "credit": plain("by")},
        ]}));
        let plain: Vec<_> = texts.iter().map(QuoteRichText::quote_text).collect();
        assert_eq!(
            plain,
            [
                "Title", "photo", "More", "inside", "cell", "e=mc^2", "pull", "by"
            ]
        );
    }

    #[test]
    fn an_unknown_type_makes_nothing_quotable() {
        let unknown_block = json!({"blocks": [
            {"@type": "pageBlockParagraph", "text": plain("ok")},
            {"@type": "pageBlockHologram"},
        ]});
        assert!(rich_message_quote_texts(&unknown_block).is_empty());
        let unknown_text = json!({"blocks": [
            {"@type": "pageBlockParagraph", "text": {"@type": "richTextHologram"}},
        ]});
        assert!(rich_message_quote_texts(&unknown_text).is_empty());
    }
}
