//! Rich text: `RichBlock`s and composer markup into TDLib input JSON.
use super::*;

// ---------------------------------------------------------------------------
// Editor (outgoing)
// ---------------------------------------------------------------------------

/// Build one `RichText` node from clean text + M1 markup entities.
/// UTF-16 offsets that don't convert are dropped; overlapping entities keep
/// the longest span (the markup parser doesn't nest).
pub fn formatted_to_rich_text(text: &str, entities: &[ComposerEntity]) -> Value {
    let mut spans: Vec<(usize, usize, &ComposerEntity)> = entities
        .iter()
        .filter_map(|entity| {
            let start = utf16_to_utf8_offset(text, entity.offset).ok()?;
            let end = utf16_to_utf8_offset(text, entity.offset + entity.length).ok()?;
            (end > start).then_some((start, end, entity))
        })
        .collect();
    spans.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
    let mut kept: Vec<(usize, usize, &ComposerEntity)> = Vec::new();
    for span in spans {
        if kept
            .last()
            .is_none_or(|(_, prev_end, _)| span.0 >= *prev_end)
        {
            kept.push(span);
        }
    }
    if kept.is_empty() {
        return json!({ "@type": "richTextPlain", "text": text });
    }
    let mut parts: Vec<Value> = Vec::new();
    let mut pos = 0;
    for (start, end, entity) in kept {
        if start > pos {
            parts.push(json!({ "@type": "richTextPlain", "text": &text[pos..start] }));
        }
        let run = &text[start..end];
        let node = match entity.kind {
            crate::composer::FormatKind::Bold => {
                json!({ "@type": "richTextBold", "text": plain(run) })
            }
            crate::composer::FormatKind::Italic => {
                json!({ "@type": "richTextItalic", "text": plain(run) })
            }
            crate::composer::FormatKind::Underline => {
                json!({ "@type": "richTextUnderline", "text": plain(run) })
            }
            crate::composer::FormatKind::Strikethrough => {
                json!({ "@type": "richTextStrikethrough", "text": plain(run) })
            }
            crate::composer::FormatKind::Spoiler => {
                json!({ "@type": "richTextSpoiler", "text": plain(run) })
            }
            // `RichText` has no pre node — fixed-width is the honest mapping.
            crate::composer::FormatKind::Code | crate::composer::FormatKind::Pre => {
                json!({ "@type": "richTextFixed", "text": plain(run) })
            }
            // Rich text has no custom-emoji node: the fallback emoji stays.
            crate::composer::FormatKind::CustomEmoji => plain(run),
            crate::composer::FormatKind::TextUrl | crate::composer::FormatKind::MentionName => {
                if entity.url == run {
                    plain(run)
                } else {
                    json!({ "@type": "richTextUrl", "text": plain(run), "url": entity.url, "is_cached": false })
                }
            }
            // Block quotes are block-level; inside a rich paragraph the text
            // survives and the marker is dropped.
            crate::composer::FormatKind::BlockQuote => plain(run),
        };
        parts.push(node);
        pos = end;
    }
    if pos < text.len() {
        parts.push(json!({ "@type": "richTextPlain", "text": &text[pos..] }));
    }
    json!({ "@type": "richTexts", "texts": parts })
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
        RichBlock::List { ordered: _, items } => {
            let inputs: Vec<Value> = items
                .iter()
                .map(|item| {
                    json!({
                        "@type": "inputPageBlockListItem",
                        "blocks": [{ "@type": "inputPageBlockParagraph", "text": block_rich_text(&item.text) }],
                        "has_checkbox": item.checked.is_some(),
                        "is_checked": item.checked.unwrap_or(false),
                        "value": 0,
                        "type": "",
                    })
                })
                .collect();
            Some(json!({ "@type": "inputPageBlockList", "items": inputs }))
        }
        RichBlock::Collapsible { header, body, open } => Some(json!({
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
        // Known simplification (DECISIONS.md): always LTR.
        "is_rtl": false,
        "detect_automatic_blocks": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    text: "cd".into(),
                    checked: None,
                }],
            },
            RichBlock::Table {
                rows: vec![vec!["e".into()]],
            },
            RichBlock::Collapsible {
                header: "f".into(),
                body: "g".into(),
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
