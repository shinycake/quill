//! Rich text: `RichBlock`s and composer markup into TDLib input JSON.
use super::*;
use std::cmp::Reverse;
use std::iter::Peekable;
use std::ops::Range;

// ---------------------------------------------------------------------------
// Editor (outgoing)
// ---------------------------------------------------------------------------

/// How deep formatting nests in a `RichText`. The composer has fewer kinds of
/// span than this, and TDLib's JSON reader bounds its depth: spans past it
/// lose their formatting, not their text.
const MAX_RICH_NESTING: usize = 16;

#[derive(Clone, Copy)]
struct RichSpan<'a> {
    start: usize,
    end: usize,
    entity: &'a ComposerEntity,
}

/// Spans ordered by start, longest first, each lying wholly inside any span
/// it starts in: one that runs past another's end is cut there, and its rest
/// goes on as a span of its own.
fn nested_spans<'a>(mut spans: Vec<RichSpan<'a>>) -> Vec<RichSpan<'a>> {
    let order = |span: &RichSpan<'_>| (Reverse(span.start), span.end);
    spans.sort_by_key(order);
    let mut out = Vec::with_capacity(spans.len());
    let mut open: Vec<usize> = Vec::new();
    while let Some(mut span) = spans.pop() {
        while open.last().is_some_and(|&end| end <= span.start) {
            open.pop();
        }
        if let Some(&end) = open.last()
            && span.end > end
        {
            let rest = RichSpan { start: end, ..span };
            span.end = end;
            let at = spans.partition_point(|other| order(other) < order(&rest));
            spans.insert(at, rest);
        }
        open.push(span.end);
        out.push(span);
    }
    out
}

/// The `RichText` nodes of `range`, taking the spans that start in it.
fn rich_nodes<'a>(
    text: &str,
    range: Range<usize>,
    spans: &mut Peekable<impl Iterator<Item = RichSpan<'a>>>,
    depth: usize,
) -> Vec<Value> {
    let mut parts = Vec::new();
    let mut pos = range.start;
    while let Some(span) = spans.next_if(|span| span.start < range.end) {
        if span.start > pos {
            parts.push(plain(text.get(pos..span.start).unwrap_or_default()));
        }
        let inner = if depth < MAX_RICH_NESTING {
            rich_nodes(text, span.start..span.end, spans, depth + 1)
        } else {
            while spans.next_if(|inner| inner.start < span.end).is_some() {}
            vec![plain(text.get(span.start..span.end).unwrap_or_default())]
        };
        let run = text.get(span.start..span.end).unwrap_or_default();
        parts.push(styled_node(span.entity, run, joined(inner)));
        pos = span.end;
    }
    if pos < range.end {
        parts.push(plain(text.get(pos..range.end).unwrap_or_default()));
    }
    parts
}

fn joined(mut parts: Vec<Value>) -> Value {
    match parts.len() {
        0 => plain(""),
        1 => parts.pop().unwrap_or_else(|| plain("")),
        _ => json!({ "@type": "richTexts", "texts": parts }),
    }
}

/// `inner`, the text of `run`, under `entity`'s formatting.
fn styled_node(entity: &ComposerEntity, run: &str, inner: Value) -> Value {
    match entity.kind {
        crate::composer::FormatKind::Bold => json!({ "@type": "richTextBold", "text": inner }),
        crate::composer::FormatKind::Italic => json!({ "@type": "richTextItalic", "text": inner }),
        crate::composer::FormatKind::Underline => {
            json!({ "@type": "richTextUnderline", "text": inner })
        }
        crate::composer::FormatKind::Strikethrough => {
            json!({ "@type": "richTextStrikethrough", "text": inner })
        }
        crate::composer::FormatKind::Spoiler => {
            json!({ "@type": "richTextSpoiler", "text": inner })
        }
        // `RichText` has no pre node — fixed-width is the honest mapping.
        crate::composer::FormatKind::Code | crate::composer::FormatKind::Pre => {
            json!({ "@type": "richTextFixed", "text": inner })
        }
        // Rich text has no custom-emoji node: the fallback emoji stays.
        crate::composer::FormatKind::CustomEmoji => inner,
        // Nor a date node: the text the date was written as stays.
        crate::composer::FormatKind::DateTime => inner,
        crate::composer::FormatKind::TextUrl | crate::composer::FormatKind::MentionName => {
            if entity.url == run {
                inner
            } else {
                json!({ "@type": "richTextUrl", "text": inner, "url": entity.url, "is_cached": false })
            }
        }
        // Block quotes are block-level; inside a rich paragraph the text
        // survives and the marker is dropped.
        crate::composer::FormatKind::BlockQuote => inner,
    }
}

/// Build one `RichText` node from clean text + M1 markup entities, nested as
/// the markup nests them. Entities whose bytes are not a range of `text` are
/// dropped.
pub fn formatted_to_rich_text(text: &str, entities: &[ComposerEntity]) -> Value {
    let spans = entities
        .iter()
        .filter_map(|entity| {
            let (start, end) = (entity.bytes.start, entity.bytes.end);
            text.get(start..end)?;
            (end > start).then_some(RichSpan { start, end, entity })
        })
        .collect();
    let mut spans = nested_spans(spans).into_iter().peekable();
    joined(rich_nodes(text, 0..text.len(), &mut spans, 0))
}

fn plain(text: &str) -> Value {
    json!({ "@type": "richTextPlain", "text": text })
}

fn block_rich_text(text: &str) -> Value {
    let (clean, entities) = crate::composer::parse_format_markup(text);
    formatted_to_rich_text(&clean, &entities)
}

/// `pageBlockCaption` for an outgoing media block (schema :4133).
fn page_block_caption(caption: &str) -> Value {
    json!({
        "@type": "pageBlockCaption",
        "text": block_rich_text(caption),
        "credit": plain(""),
    })
}

/// Build one `inputPageBlock*` object. `None` for blocks with no honest
/// input mapping (invisible, unsupported, or a media block without a local
/// path — never a JSON `local.path`).
pub fn input_page_block_json(block: &RichBlock) -> Option<Value> {
    match block {
        RichBlock::Paragraph { text, .. } => {
            // Editor paragraphs are built from markup text, not parsed
            // entities — rebuild the RichText from the markup here.
            Some(json!({ "@type": "inputPageBlockParagraph", "text": block_rich_text(text) }))
        }
        RichBlock::Heading { level, text, .. } => {
            // Schema :5978 documents the semantics: size 1-6, 1 is the
            // largest, 6 the smallest — editor H1/H2/H3 map to sizes 1/2/3.
            let size = (*level as i32).clamp(1, 6);
            Some(json!({
                "@type": "inputPageBlockSectionHeading",
                "text": block_rich_text(text),
                "size": size,
            }))
        }
        RichBlock::List { ordered, items } => {
            let inputs: Vec<Value> = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    json!({
                        "@type": "inputPageBlockListItem",
                        "blocks": [{ "@type": "inputPageBlockParagraph", "text": block_rich_text(&item.text) }],
                        "has_checkbox": item.checked.is_some(),
                        "is_checked": item.checked.unwrap_or(false),
                        "value": if *ordered { index + 1 } else { 0 },
                        "type": if *ordered { "1" } else { "" },
                    })
                })
                .collect();
            Some(json!({ "@type": "inputPageBlockList", "items": inputs }))
        }
        RichBlock::Collapsible {
            header, body, open, ..
        } => Some(json!({
            "@type": "inputPageBlockDetails",
            "header": block_rich_text(header),
            "blocks": [{ "@type": "inputPageBlockParagraph", "text": block_rich_text(body) }],
            "is_open": *open,
        })),
        RichBlock::Document {
            file_name: _,
            caption,
            local_path,
        } => {
            let path = local_path.as_ref()?.to_string_lossy().into_owned();
            Some(json!({
                "@type": "inputPageBlockDocument",
                "document": {
                    "@type": "inputDocument",
                    "document": { "@type": "inputFileLocal", "path": path },
                    "thumbnail": Value::Null,
                    "disable_content_type_detection": false,
                },
                "caption": page_block_caption(caption),
            }))
        }
        RichBlock::Photo {
            caption,
            local_path,
        } => {
            let path = local_path.as_ref()?.to_string_lossy().into_owned();
            Some(json!({
                // Schema :6029 (`inputPageBlockPhoto`); `inputPhoto` is
                // :5837 — thumbnail null uploads from the file, same as
                // `input_message_photo` in telegram/requests/media.rs.
                "@type": "inputPageBlockPhoto",
                "photo": {
                    "@type": "inputPhoto",
                    "photo": { "@type": "inputFileLocal", "path": path },
                    "thumbnail": Value::Null,
                    "video": Value::Null,
                    "added_sticker_file_ids": [],
                    "width": 0,
                    "height": 0,
                },
                "caption": page_block_caption(caption),
                "has_spoiler": false,
            }))
        }
        RichBlock::Video {
            caption,
            local_path,
        } => {
            let path = local_path.as_ref()?.to_string_lossy().into_owned();
            Some(json!({
                // Schema :6035 (`inputPageBlockVideo`); `inputVideo` is
                // :5856 — zeros let TDLib probe the local file, same as
                // `input_message_video` in telegram/requests/media.rs.
                "@type": "inputPageBlockVideo",
                "video": {
                    "@type": "inputVideo",
                    "video": { "@type": "inputFileLocal", "path": path },
                    "thumbnail": Value::Null,
                    "cover": Value::Null,
                    "start_timestamp": 0,
                    "added_sticker_file_ids": [],
                    "duration": 0,
                    "width": 0,
                    "height": 0,
                    "supports_streaming": false,
                },
                "caption": page_block_caption(caption),
                "has_spoiler": false,
            }))
        }
        RichBlock::Table { rows } => {
            let cells: Vec<Value> = rows
                .iter()
                .enumerate()
                .map(|(row_idx, row)| {
                    Value::Array(
                        row.iter()
                            .map(|cell| {
                                json!({
                                    "@type": "pageBlockTableCell",
                                    "text": plain(cell),
                                    "is_header": row_idx == 0,
                                    "colspan": 1,
                                    "rowspan": 1,
                                    "align": { "@type": "pageBlockHorizontalAlignmentLeft" },
                                    "valign": { "@type": "pageBlockVerticalAlignmentTop" },
                                })
                            })
                            .collect(),
                    )
                })
                .collect();
            Some(json!({
                "@type": "inputPageBlockTable",
                "caption": plain(""),
                "cells": cells,
                "is_bordered": true,
                "is_striped": false,
                "is_compact": false,
            }))
        }
        RichBlock::Divider => Some(json!({ "@type": "inputPageBlockDivider" })),
        // The editor never creates button rows (buttons are bot-side), and
        // invisible/unsupported blocks have no input mapping.
        RichBlock::ButtonRow { .. } | RichBlock::Empty | RichBlock::Unsupported { .. } => None,
    }
}

/// Total characters (Unicode scalar values) of text across all blocks in
/// the composer.
pub fn rich_blocks_char_len(blocks: &[RichBlock]) -> usize {
    blocks.iter().map(RichBlock::text_char_len).sum()
}

/// Whether the message reads right to left, as Telegram Desktop decides it
/// (`DetermineRichPageRtl`): by the first text that is not blank, in block
/// order, taking the direction of its first strong character.
fn blocks_are_rtl(blocks: &[RichBlock]) -> bool {
    blocks
        .iter()
        .flat_map(block_texts)
        .find(|text| !text.trim().is_empty())
        .is_some_and(crate::text::is_rtl_text)
}

/// The texts of a block in reading order.
fn block_texts(block: &RichBlock) -> Vec<&str> {
    match block {
        RichBlock::Paragraph { text, .. } | RichBlock::Heading { text, .. } => vec![text],
        RichBlock::List { items, .. } => items.iter().map(|item| item.text.as_str()).collect(),
        RichBlock::Collapsible { header, body, .. } => vec![header, body],
        RichBlock::Document {
            file_name, caption, ..
        } => vec![caption, file_name],
        RichBlock::Photo { caption, .. } | RichBlock::Video { caption, .. } => vec![caption],
        RichBlock::Table { rows } => rows.iter().flatten().map(String::as_str).collect(),
        RichBlock::ButtonRow { .. }
        | RichBlock::Divider
        | RichBlock::Empty
        | RichBlock::Unsupported { .. } => Vec::new(),
    }
}

/// Build the `inputRichMessage` object for `inputMessageRichMessage`.
/// `None` when no block survives (the send is then invalid), or when the
/// blocks exceed the rich-text max length.
pub fn input_rich_message(blocks: &[RichBlock]) -> Option<Value> {
    if rich_blocks_char_len(blocks) > RICH_TEXT_MAX_CHARS {
        return None;
    }
    let inputs: Vec<Value> = blocks.iter().filter_map(input_page_block_json).collect();
    if inputs.is_empty() {
        return None;
    }
    Some(json!({
        "@type": "inputRichMessage",
        "source": { "@type": "richMessageSourceBlocks", "blocks": inputs },
        "is_rtl": blocks_are_rtl(blocks),
        "detect_automatic_blocks": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(text: &str) -> RichBlock {
        RichBlock::Paragraph {
            text: text.into(),
            entities: Vec::new(),
            buttons: Vec::new(),
        }
    }

    #[test]
    fn the_first_text_that_is_not_blank_sets_the_direction() {
        let rtl = |blocks: &[RichBlock]| input_rich_message(blocks).map(|m| m["is_rtl"].clone());
        assert_eq!(rtl(&[paragraph("שלום עולם")]), Some(json!(true)));
        assert_eq!(rtl(&[paragraph("مرحبا")]), Some(json!(true)));
        assert_eq!(rtl(&[paragraph("hello שלום")]), Some(json!(false)));
        // A blank first block is skipped, and a divider holds no text.
        assert_eq!(
            rtl(&[paragraph("  "), RichBlock::Divider, paragraph("שלום")]),
            Some(json!(true))
        );
        assert_eq!(
            rtl(&[
                RichBlock::Heading {
                    level: 1,
                    text: "Title".into(),
                    entities: Vec::new(),
                },
                paragraph("שלום")
            ]),
            Some(json!(false))
        );
    }

    #[test]
    fn formatting_inside_bold_is_sent() {
        let link = block_rich_text("**see [docs](https://example.com) now**").to_string();
        assert!(link.contains("https://example.com"), "{link}");
        let italic = block_rich_text("**a *b* c**").to_string();
        assert!(italic.contains("richTextItalic"), "{italic}");
    }

    #[test]
    fn spans_that_cross_are_both_kept_in_order() {
        use crate::composer::{ComposerEntity, FormatKind};
        let entity = |bytes, kind| ComposerEntity {
            offset: 0,
            length: 0,
            bytes,
            kind,
            url: String::new(),
            language: String::new(),
        };
        let crossing = [
            entity(0..4, FormatKind::Bold),
            entity(2..6, FormatKind::Italic),
        ];
        assert_eq!(
            formatted_to_rich_text("abcdef", &crossing),
            json!({"@type": "richTexts", "texts": [
                {"@type": "richTextBold", "text": {"@type": "richTexts", "texts": [
                    {"@type": "richTextPlain", "text": "ab"},
                    {"@type": "richTextItalic", "text": {"@type": "richTextPlain", "text": "cd"}},
                ]}},
                {"@type": "richTextItalic", "text": {"@type": "richTextPlain", "text": "ef"}},
            ]})
        );
    }

    #[test]
    fn formatting_past_the_depth_limit_keeps_its_text() {
        use crate::composer::{ComposerEntity, FormatKind};
        let text = "x".repeat(40);
        let spans: Vec<ComposerEntity> = (0..20)
            .map(|at| ComposerEntity {
                offset: 0,
                length: 0,
                bytes: at..40 - at,
                kind: FormatKind::Bold,
                url: String::new(),
                language: String::new(),
            })
            .collect();
        let sent = formatted_to_rich_text(&text, &spans).to_string();
        let plain_text: usize = sent
            .split("\"text\":\"")
            .skip(1)
            .map(|rest| rest.chars().take_while(|ch| *ch == 'x').count())
            .sum();
        assert_eq!(plain_text, 40, "{sent}");
    }

    #[test]
    fn input_page_block_json_shapes() {
        let paragraph = RichBlock::Paragraph {
            text: "hi **there**".into(),
            entities: Vec::new(),
            buttons: Vec::new(),
        };
        let json = input_page_block_json(&paragraph).expect("paragraph");
        assert_eq!(json["@type"], "inputPageBlockParagraph");
        assert_eq!(json["text"]["@type"], "richTexts");

        let heading = RichBlock::Heading {
            level: 1,
            text: "T".into(),
            entities: Vec::new(),
        };
        let json = input_page_block_json(&heading).expect("heading");
        assert_eq!(json["@type"], "inputPageBlockSectionHeading");

        // Schema :5978 documents the semantics: size 1-6, 1 is the largest.
        // Editor H1/H2/H3 map to sizes 1/2/3 (regression: H1 once sent size 3).
        for (level, size) in [(1u8, 1), (2u8, 2), (3u8, 3)] {
            let json = input_page_block_json(&RichBlock::Heading {
                level,
                text: "T".into(),
                entities: Vec::new(),
            })
            .expect("heading");
            assert_eq!(json["size"], size);
        }

        // No local path → no honest input mapping.
        let doc = RichBlock::Document {
            file_name: "n.txt".into(),
            caption: String::new(),
            local_path: None,
        };
        assert!(input_page_block_json(&doc).is_none());

        let doc = RichBlock::Document {
            file_name: "n.txt".into(),
            caption: String::new(),
            local_path: Some(PathBuf::from("/tmp/n.txt")),
        };
        let json = input_page_block_json(&doc).expect("document");
        assert_eq!(json["@type"], "inputPageBlockDocument");
        assert_eq!(json["document"]["@type"], "inputDocument");
        assert_eq!(json["document"]["document"]["@type"], "inputFileLocal");

        // Photo/video mirror the document mapping: no local path → no
        // honest input mapping.
        for (kind, type_name) in [
            (
                RichBlock::Photo {
                    caption: "cap".into(),
                    local_path: None,
                },
                "inputPageBlockPhoto",
            ),
            (
                RichBlock::Video {
                    caption: "cap".into(),
                    local_path: None,
                },
                "inputPageBlockVideo",
            ),
        ] {
            assert!(input_page_block_json(&kind).is_none(), "{type_name}");
        }
        let photo = RichBlock::Photo {
            caption: "cap".into(),
            local_path: Some(PathBuf::from("/tmp/p.jpg")),
        };
        let json = input_page_block_json(&photo).expect("photo");
        assert_eq!(json["@type"], "inputPageBlockPhoto");
        assert_eq!(json["photo"]["@type"], "inputPhoto");
        assert_eq!(json["photo"]["photo"]["@type"], "inputFileLocal");
        assert_eq!(json["photo"]["photo"]["path"], "/tmp/p.jpg");
        assert_eq!(json["caption"]["@type"], "pageBlockCaption");
        assert_eq!(json["has_spoiler"], false);

        let video = RichBlock::Video {
            caption: String::new(),
            local_path: Some(PathBuf::from("/tmp/v.mp4")),
        };
        let json = input_page_block_json(&video).expect("video");
        assert_eq!(json["@type"], "inputPageBlockVideo");
        assert_eq!(json["video"]["@type"], "inputVideo");
        assert_eq!(json["video"]["video"]["@type"], "inputFileLocal");
        assert_eq!(json["video"]["video"]["path"], "/tmp/v.mp4");
        assert_eq!(json["caption"]["@type"], "pageBlockCaption");
        assert_eq!(json["has_spoiler"], false);
    }

    #[test]
    fn input_rich_message_needs_blocks() {
        assert!(input_rich_message(&[]).is_none());
        assert!(input_rich_message(&[RichBlock::Divider]).is_some());
        let message = input_rich_message(&[RichBlock::Divider]).unwrap();
        assert_eq!(message["@type"], "inputRichMessage");
        assert_eq!(message["source"]["@type"], "richMessageSourceBlocks");
    }

    #[test]
    fn rich_text_max_length_is_chars_across_blocks() {
        let paragraph = |text: String| RichBlock::Paragraph {
            text,
            entities: Vec::new(),
            buttons: Vec::new(),
        };
        // Under the limit across two blocks: accepted.
        let half = "x".repeat(RICH_TEXT_MAX_CHARS / 2);
        let blocks = vec![paragraph(half.clone()), paragraph(half.clone())];
        assert_eq!(rich_blocks_char_len(&blocks), RICH_TEXT_MAX_CHARS);
        assert!(input_rich_message(&blocks).is_some());
        // One char over: rejected.
        let blocks = vec![paragraph(half.clone()), paragraph(format!("{half}x"))];
        assert!(rich_blocks_char_len(&blocks) > RICH_TEXT_MAX_CHARS);
        assert!(input_rich_message(&blocks).is_none());
        // The limit counts Unicode scalar values, not UTF-16 code units:
        // 32,768 emoji accepted, one more refused (a UTF-16 count would
        // have refused at 16,385 emoji).
        let emoji = "\u{1F600}".repeat(RICH_TEXT_MAX_CHARS);
        assert!(input_rich_message(&[paragraph(emoji.clone())]).is_some());
        assert!(input_rich_message(&[paragraph(format!("{emoji}\u{1F600}"))]).is_none());
        // Headings, list items, table cells, collapsibles, and document
        // fields all contribute; chrome (buttons/dividers) does not.
        let mixed = vec![
            RichBlock::Heading {
                level: 1,
                text: "ab".into(),
                entities: Vec::new(),
            },
            RichBlock::List {
                ordered: false,
                items: vec![RichListItem {
                    label: String::new(),
                    text: "cd".into(),
                    entities: Vec::new(),
                    checked: None,
                }],
            },
            RichBlock::Table {
                rows: vec![vec!["e".into()]],
            },
            RichBlock::Collapsible {
                header: "f".into(),
                header_entities: Vec::new(),
                body: "g".into(),
                body_entities: Vec::new(),
                open: true,
            },
            RichBlock::Document {
                file_name: "h".into(),
                caption: "i".into(),
                local_path: None,
            },
            RichBlock::Divider,
        ];
        assert_eq!(rich_blocks_char_len(&mixed), 9);
        assert_eq!(rich_blocks_char_len(&[RichBlock::Divider]), 0);
    }

    #[test]
    fn formatted_to_rich_text_wraps_markup() {
        let (clean, entities) = crate::composer::parse_format_markup("a **b** c");
        let json = formatted_to_rich_text(&clean, &entities);
        assert_eq!(json["@type"], "richTexts");
        let texts = json["texts"].as_array().unwrap();
        assert_eq!(texts.len(), 3);
        assert_eq!(texts[1]["@type"], "richTextBold");
        assert_eq!(texts[1]["text"]["text"], "b");
    }
}
