//! Quotes taken from rich messages (TDLib 1.8.69 `rich_texts` of
//! `inputTextQuote` / `textQuote`, `schema/td_api.tl:3477`).
//!
//! A quote from a rich message is not a string with entities: it is a list
//! of `RichText` trees, one per quoted text of the message. TDLib defines
//! the quotable texts of a message (`WebPageBlock::for_each_quote_text`) and
//! the quote position (UTF-16 offset in those texts joined by line feeds),
//! so this module works on the raw `pageBlock*` JSON instead of the
//! flattened [`super::RichBlock`]s, which lose that structure.
#![deny(clippy::indexing_slicing)]

use super::parse::rich_text_to_parts;
use crate::text::{TextEntity, TextEntityKind};
use serde_json::{Value, json};

/// TDLib's server-side limit of quoted texts in one quote.
pub const MAX_QUOTE_TEXTS: usize = 32;

/// Depth limit for walks over received trees.
const MAX_DEPTH: usize = 64;

fn type_of(value: &Value) -> &str {
    value.get("@type").and_then(Value::as_str).unwrap_or("")
}

fn str_field<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or("")
}

/// How a `richText*` node takes part in a quote.
enum Node<'a> {
    /// Text that is quoted by characters.
    Text(&'a str),
    /// Text that is quoted whole or not at all (math, custom emoji).
    Atom(&'a str),
    /// Formatting kept in quotes around one child.
    Wrap(&'a Value),
    /// Formatting quotes do not keep: only the child's text is quoted.
    Transparent(&'a Value),
    /// Several children in a row.
    Many(&'a [Value]),
    /// Nothing to quote (icons, anchors, unknown nodes without text).
    Nothing,
}

fn node(value: &Value) -> Node<'_> {
    match type_of(value) {
        "richTextPlain" => Node::Text(str_field(value, "text")),
        "richTextMathematicalExpression" => Node::Atom(str_field(value, "expression")),
        "richTextCustomEmoji" => Node::Atom(str_field(value, "alternative_text")),
        "richTexts" => Node::Many(
            value
                .get("texts")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        ),
        "richTextBold"
        | "richTextItalic"
        | "richTextUnderline"
        | "richTextStrikethrough"
        | "richTextSpoiler"
        | "richTextSubscript"
        | "richTextSuperscript"
        | "richTextDateTime" => value.get("text").map_or(Node::Nothing, Node::Wrap),
        "richTextIcon" | "richTextAnchor" => Node::Nothing,
        "richTextButton" => value
            .get("button")
            .and_then(|button| button.get("text"))
            .map_or(Node::Nothing, Node::Transparent),
        // Unknown kinds fall back to their plain text.
        _ => value.get("text").map_or(Node::Nothing, Node::Transparent),
    }
}

/// The text a `RichText` contributes to a quote (`RichText::get_quote_text`).
pub fn quote_text_of(value: &Value) -> String {
    let mut out = String::new();
    push_quote_text(value, &mut out, 0);
    out
}

fn push_quote_text(value: &Value, out: &mut String, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match node(value) {
        Node::Text(text) | Node::Atom(text) => out.push_str(text),
        Node::Wrap(child) | Node::Transparent(child) => push_quote_text(child, out, depth + 1),
        Node::Many(children) => {
            for child in children {
                push_quote_text(child, out, depth + 1);
            }
        }
        Node::Nothing => {}
    }
}

/// The part of `value` covering the characters `begin..end` of its quote
/// text (`RichText::get_quote_fragment`), formatting kept as TDLib keeps it.
/// `None` when nothing of it is covered.
pub fn quote_fragment(value: &Value, begin: usize, end: usize) -> Option<Value> {
    let mut pos = 0;
    fragment(value, &mut pos, begin, end, 0)
}

fn fragment(
    value: &Value,
    pos: &mut usize,
    begin: usize,
    end: usize,
    depth: usize,
) -> Option<Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    match node(value) {
        Node::Text(text) => {
            let size = text.chars().count();
            let from = begin.max(*pos);
            let to = end.min(pos.saturating_add(size));
            let start = *pos;
            *pos = pos.saturating_add(size);
            if from >= to {
                return None;
            }
            let part: String = text
                .chars()
                .skip(from.saturating_sub(start))
                .take(to - from)
                .collect();
            Some(json!({ "@type": "richTextPlain", "text": part }))
        }
        Node::Atom(text) => {
            let size = text.chars().count();
            let included = begin <= *pos && pos.saturating_add(size) <= end;
            *pos = pos.saturating_add(size);
            included.then(|| value.clone())
        }
        Node::Transparent(child) => fragment(child, pos, begin, end, depth + 1),
        Node::Wrap(child) => {
            let inner = fragment(child, pos, begin, end, depth + 1)?;
            let mut wrapped = value.clone();
            wrapped.as_object_mut()?.insert("text".into(), inner);
            Some(wrapped)
        }
        Node::Many(children) => {
            let parts: Vec<Value> = children
                .iter()
                .filter_map(|child| fragment(child, pos, begin, end, depth + 1))
                .collect();
            (!parts.is_empty()).then(|| json!({ "@type": "richTexts", "texts": parts }))
        }
        Node::Nothing => None,
    }
}

fn caption_texts(block: &Value, out: &mut Vec<Value>) {
    if let Some(caption) = block.get("caption") {
        out.extend(caption.get("text").cloned());
        out.extend(caption.get("credit").cloned());
    }
}

fn collect_blocks(blocks: &Value, out: &mut Vec<Value>, depth: usize) {
    if let Some(blocks) = blocks.as_array() {
        for block in blocks {
            collect_block(block, out, depth + 1);
        }
    }
}

/// The texts of one `pageBlock*` that can be quoted, in the order of
/// `WebPageBlock::for_each_quote_text`. Buttons, anchors and dividers
/// cannot be quoted.
fn collect_block(block: &Value, out: &mut Vec<Value>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let single = |field: &str, out: &mut Vec<Value>| out.extend(block.get(field).cloned());
    match type_of(block) {
        "pageBlockTitle" => single("title", out),
        "pageBlockSubtitle" => single("subtitle", out),
        "pageBlockAuthorDate" => single("author", out),
        "pageBlockHeader" => single("header", out),
        "pageBlockSubheader" => single("subheader", out),
        "pageBlockKicker" => single("kicker", out),
        "pageBlockFooter" => single("footer", out),
        "pageBlockSectionHeading"
        | "pageBlockParagraph"
        | "pageBlockPreformatted"
        | "pageBlockThinking" => single("text", out),
        "pageBlockMathematicalExpression" => out.push(json!({
            "@type": "richTextMathematicalExpression",
            "expression": str_field(block, "expression"),
        })),
        "pageBlockList" => {
            for item in block
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                collect_blocks(item.get("blocks").unwrap_or(&Value::Null), out, depth);
            }
        }
        "pageBlockBlockQuote" => {
            collect_blocks(block.get("blocks").unwrap_or(&Value::Null), out, depth);
            single("credit", out);
        }
        "pageBlockExpandableBlockQuote" | "pageBlockPullQuote" => {
            single("text", out);
            single("credit", out);
        }
        "pageBlockPhoto" | "pageBlockVideo" | "pageBlockAnimation" | "pageBlockAudio"
        | "pageBlockDocument" | "pageBlockVoiceNote" | "pageBlockEmbedded" | "pageBlockMap" => {
            caption_texts(block, out)
        }
        "pageBlockCover" => {
            if let Some(cover) = block.get("cover") {
                collect_block(cover, out, depth + 1);
            }
        }
        "pageBlockEmbeddedPost" | "pageBlockCollage" | "pageBlockSlideshow" => {
            collect_blocks(block.get("blocks").unwrap_or(&Value::Null), out, depth);
            caption_texts(block, out);
        }
        "pageBlockTable" => {
            single("caption", out);
            for row in block
                .get("cells")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                for cell in row.as_array().map(Vec::as_slice).unwrap_or_default() {
                    out.extend(cell.get("text").cloned());
                }
            }
        }
        "pageBlockDetails" => {
            single("header", out);
            collect_blocks(block.get("blocks").unwrap_or(&Value::Null), out, depth);
        }
        "pageBlockRelatedArticles" => single("header", out),
        _ => {}
    }
}

/// Every quotable text of a `richMessage`'s `blocks`, in order of
/// appearance, without the empty ones. Kept next to the message so a quote
/// can be cut out of it later.
pub fn quote_texts(blocks: &Value) -> Vec<Value> {
    let mut texts = Vec::new();
    collect_blocks(blocks, &mut texts, 0);
    texts.retain(|text| !quote_text_of(text).is_empty());
    texts
}

/// The text quotes are picked from and positioned in: the plain quote text
/// of every quotable text, separated by line feeds.
pub fn quote_full_text(texts: &[Value]) -> String {
    texts
        .iter()
        .map(quote_text_of)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `rich_texts` of an `inputTextQuote` for `quote`, a verbatim part of
/// [`quote_full_text`] starting at `position` (UTF-16 units there): one
/// element per quoted text, the first and the last possibly partial.
/// `None` when the span cannot be cut into at most [`MAX_QUOTE_TEXTS`]
/// whole elements (the caller then quotes plain text or the whole message).
pub fn quote_rich_texts(texts: &[Value], quote: &str, position: i32) -> Option<Vec<Value>> {
    if quote.is_empty() || position < 0 {
        return None;
    }
    let full = quote_full_text(texts);
    let start = utf16_to_char_index(&full, usize::try_from(position).ok()?)?;
    let end = start.checked_add(quote.chars().count())?;
    let quoted: String = full.chars().skip(start).take(end - start).collect();
    if quoted != quote {
        return None;
    }
    let mut result = Vec::new();
    let mut at = 0usize;
    for text in texts {
        let size = quote_text_of(text).chars().count();
        let (from, to) = (start.max(at), end.min(at + size));
        if from < to
            && let Some(part) = quote_fragment(text, from - at, to - at)
        {
            result.push(part);
        }
        at += size + 1;
        if at > end {
            break;
        }
    }
    (!result.is_empty() && result.len() <= MAX_QUOTE_TEXTS).then_some(result)
}

fn utf16_to_char_index(text: &str, units: usize) -> Option<usize> {
    let mut seen = 0usize;
    for (index, ch) in text.chars().enumerate() {
        if seen == units {
            return Some(index);
        }
        seen += ch.len_utf16();
        if seen > units {
            return None;
        }
    }
    (seen == units).then(|| text.chars().count())
}

/// Entity kinds a quote shows (tdesktop `ParseRichQuote` keeps bold, italic,
/// underline, strikethrough, spoiler, custom emoji and dates).
fn keeps_in_quote(kind: &TextEntityKind) -> bool {
    matches!(
        kind,
        TextEntityKind::Bold
            | TextEntityKind::Italic
            | TextEntityKind::Underline
            | TextEntityKind::Strikethrough
            | TextEntityKind::Spoiler
    )
}

/// The text and entities a received quote shows: one line per quoted text
/// (tdesktop joins them with line feeds), kinds the quote does not keep
/// reduced to plain text.
pub fn quote_display(rich_texts: &[Value]) -> (String, Vec<TextEntity>) {
    let mut text = String::new();
    let mut entities = Vec::new();
    for rich in rich_texts {
        let (line, line_entities, _) = rich_text_to_parts(rich);
        if line.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        let shift = text.len();
        entities.extend(
            line_entities
                .into_iter()
                .filter(|entity| keeps_in_quote(&entity.kind))
                .map(|entity| TextEntity {
                    utf8_start: entity.utf8_start.saturating_add(shift),
                    utf8_end: entity.utf8_end.saturating_add(shift),
                    kind: entity.kind,
                }),
        );
        text.push_str(&line);
    }
    (text, entities)
}

/// `text` on one line (control and separator characters become spaces,
/// as `one_line_preview` does) with `entities` moved to the new offsets.
pub fn one_line(text: &str, entities: &[TextEntity]) -> (String, Vec<TextEntity>) {
    let mut line = String::with_capacity(text.len());
    // New byte offset of every char boundary of `text`, by old offset.
    let mut moved = std::collections::HashMap::with_capacity(text.len() + 1);
    for (old, ch) in text.char_indices() {
        moved.insert(old, line.len());
        line.push(
            if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                ch
            },
        );
    }
    moved.insert(text.len(), line.len());
    let entities = entities
        .iter()
        .filter_map(|entity| {
            let start = *moved.get(&entity.utf8_start)?;
            let end = *moved.get(&entity.utf8_end)?;
            (start < end).then(|| TextEntity {
                utf8_start: start,
                utf8_end: end,
                kind: entity.kind.clone(),
            })
        })
        .collect();
    (line, entities)
}

/// `entities` of `text` re-based onto `text.trim()`; entities that fall
/// outside the trimmed text are dropped.
pub fn trimmed_entities(text: &str, entities: &[TextEntity]) -> Vec<TextEntity> {
    let lead = text.len().saturating_sub(text.trim_start().len());
    let len = text.trim().len();
    entities
        .iter()
        .filter_map(|entity| {
            let start = entity.utf8_start.checked_sub(lead)?;
            let end = entity.utf8_end.checked_sub(lead)?.min(len);
            (start < end).then(|| TextEntity {
                utf8_start: start,
                utf8_end: end,
                kind: entity.kind.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, reason = "test code")]
    use super::*;

    fn plain(text: &str) -> Value {
        json!({ "@type": "richTextPlain", "text": text })
    }

    fn bold(text: &str) -> Value {
        json!({ "@type": "richTextBold", "text": plain(text) })
    }

    fn paragraph(text: Value) -> Value {
        json!({ "@type": "pageBlockParagraph", "text": text })
    }

    fn sample_blocks() -> Value {
        json!([
            { "@type": "pageBlockTitle", "title": plain("Plan") },
            { "@type": "pageBlockDivider" },
            paragraph(json!({
                "@type": "richTexts",
                "texts": [plain("Ship the "), bold("quote"), plain(" feature")]
            })),
            { "@type": "pageBlockButtonRow", "buttons": [] },
            paragraph(plain("")),
            { "@type": "pageBlockList", "items": [
                { "label": "1", "blocks": [paragraph(plain("then rest"))] }
            ] },
            { "@type": "pageBlockPhoto", "caption": { "text": plain("cap"), "credit": plain("by me") } }
        ])
    }

    #[test]
    fn collects_quotable_texts_in_order() {
        let texts = quote_texts(&sample_blocks());
        assert_eq!(
            quote_full_text(&texts),
            "Plan\nShip the quote feature\nthen rest\ncap\nby me"
        );
    }

    #[test]
    fn quote_inside_one_text_keeps_formatting() {
        let texts = quote_texts(&sample_blocks());
        let full = quote_full_text(&texts);
        let quote = "the quote fea";
        let position = full.find(quote).unwrap() as i32;
        let rich = quote_rich_texts(&texts, quote, position).unwrap();
        assert_eq!(
            rich,
            vec![json!({ "@type": "richTexts", "texts": [
                plain("the "), bold("quote"), plain(" fea")
            ] })]
        );
    }

    #[test]
    fn quote_across_texts_is_partial_at_the_edges_only() {
        let texts = quote_texts(&sample_blocks());
        let quote = "feature\nthen rest\nca";
        let full = quote_full_text(&texts);
        let position = full.find(quote).unwrap() as i32;
        let rich = quote_rich_texts(&texts, quote, position).unwrap();
        assert_eq!(rich.len(), 3);
        assert_eq!(quote_text_of(&rich[0]), "feature");
        assert_eq!(quote_text_of(&rich[1]), "then rest");
        assert_eq!(quote_text_of(&rich[2]), "ca");
    }

    #[test]
    fn position_counts_utf16_units() {
        let blocks = json!([paragraph(plain("😀 one")), paragraph(plain("two"))]);
        let texts = quote_texts(&blocks);
        // "😀 one\ntwo": the emoji is two UTF-16 units.
        let rich = quote_rich_texts(&texts, "one", 3).unwrap();
        assert_eq!(rich, vec![plain("one")]);
        assert!(quote_rich_texts(&texts, "one", 2).is_none());
        assert!(quote_rich_texts(&texts, "one", 1).is_none());
    }

    #[test]
    fn unsupported_formatting_falls_back_to_text() {
        let url = json!({ "@type": "richTextUrl", "text": bold("site"), "url": "https://x.y", "is_cached": false });
        let unknown = json!({ "@type": "richTextFromTheFuture", "text": plain("odd") });
        let texts = quote_texts(&json!([paragraph(json!({
            "@type": "richTexts", "texts": [url, plain(" "), unknown]
        }))]));
        let rich = quote_rich_texts(&texts, "site odd", 0).unwrap();
        assert_eq!(
            rich,
            vec![
                json!({ "@type": "richTexts", "texts": [bold("site"), plain(" "), plain("odd")] })
            ]
        );
    }

    #[test]
    fn math_is_quoted_whole_or_not_at_all() {
        let math = json!({ "@type": "richTextMathematicalExpression", "expression": "x+y" });
        let texts = quote_texts(&json!([paragraph(json!({
            "@type": "richTexts", "texts": [plain("a "), math.clone(), plain(" b")]
        }))]));
        let whole = quote_rich_texts(&texts, "a x+y b", 0).unwrap();
        assert_eq!(whole.len(), 1);
        let split = quote_rich_texts(&texts, "a x", 0).unwrap();
        assert_eq!(
            split,
            vec![json!({ "@type": "richTexts", "texts": [plain("a ")] })]
        );
    }

    #[test]
    fn rejects_text_that_is_not_in_the_message() {
        let texts = quote_texts(&sample_blocks());
        assert!(quote_rich_texts(&texts, "nope", 0).is_none());
        assert!(quote_rich_texts(&texts, "", 0).is_none());
        assert!(quote_rich_texts(&texts, "Plan", -1).is_none());
        assert!(quote_rich_texts(&[], "Plan", 0).is_none());
    }

    #[test]
    fn too_many_texts_are_refused() {
        let blocks: Vec<Value> = (0..40)
            .map(|i| paragraph(plain(&format!("l{i}"))))
            .collect();
        let texts = quote_texts(&Value::Array(blocks));
        let full = quote_full_text(&texts);
        assert!(quote_rich_texts(&texts, &full, 0).is_none());
    }

    #[test]
    fn display_joins_lines_and_keeps_only_quote_styles() {
        let code = json!({ "@type": "richTextFixed", "text": plain("mono") });
        let rich = vec![
            json!({ "@type": "richTexts", "texts": [plain("a "), bold("b")] }),
            plain(""),
            json!({ "@type": "richTexts", "texts": [code, plain(" "), json!({
                "@type": "richTextItalic", "text": plain("é")
            })] }),
        ];
        let (text, entities) = quote_display(&rich);
        assert_eq!(text, "a b\nmono é");
        assert_eq!(
            entities,
            vec![
                TextEntity {
                    utf8_start: 2,
                    utf8_end: 3,
                    kind: TextEntityKind::Bold
                },
                TextEntity {
                    utf8_start: 9,
                    utf8_end: 11,
                    kind: TextEntityKind::Italic
                },
            ]
        );
    }

    #[test]
    fn one_line_moves_entities_over_wider_replacements() {
        let text = "a\u{2028}é b";
        let entities = vec![TextEntity {
            utf8_start: 7,
            utf8_end: 8,
            kind: TextEntityKind::Bold,
        }];
        let (line, moved) = one_line(text, &entities);
        assert_eq!(line, "a é b");
        assert_eq!(line.get(moved[0].utf8_start..moved[0].utf8_end), Some("b"));
    }

    #[test]
    fn trimmed_entities_follow_the_trim() {
        let text = "  hi there ";
        let entities = vec![TextEntity {
            utf8_start: 5,
            utf8_end: 10,
            kind: TextEntityKind::Italic,
        }];
        let moved = trimmed_entities(text, &entities);
        assert_eq!(
            text.trim().get(moved[0].utf8_start..moved[0].utf8_end),
            Some("there")
        );
    }

    #[test]
    fn deep_trees_stop_instead_of_overflowing() {
        let mut value = plain("deep");
        for _ in 0..500 {
            value = json!({ "@type": "richTextBold", "text": value });
        }
        assert_eq!(quote_text_of(&value), "");
        assert!(quote_fragment(&value, 0, 10).is_none());
    }
}
