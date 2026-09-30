//! M2 anniversary rich messages: the page-block model shared by the rich
//! text editor (outgoing `inputPageBlock*`) and the renderer (incoming
//! `pageBlock*`). Schema: TDLib 1.8.67 `schema/td_api.tl` — `richMessage`
//! (:123), `inputRichMessage` (:149), `inputMessageRichMessage` (:6084),
//! `messageRichMessage` (:5143), `pageBlock*` (:4198–:4364),
//! `inputPageBlock*` (:5978–:6071), `richText*` (:4036–:4129).

use crate::composer::ComposerEntity;
use crate::telegram::envelope::{InlineKeyboardButton, parse_inline_keyboard_button};
use crate::text::{TextEntity, TextEntityKind, utf16_to_utf8_offset};
use serde_json::{Value, json};
use std::path::PathBuf;

/// One page block, either parsed from an incoming `pageBlock*` or produced
/// by the rich-text editor. Text is plain; `entities` carry the inline
/// styling so the renderer reuses the normal entity painter.
/// Rich-text composer max length: 32,768 UTF-8 characters (Unicode scalar
/// values) counted over the text of every block, per Telegram's "Rich
/// Message Limits". Entity offsets stay in UTF-16 (`RichWalk::utf16_len`,
/// `src/composer.rs` `out16`); the length limit is a separate unit. Button
/// labels are chrome, not message text, and are not counted.
pub const RICH_TEXT_MAX_CHARS: usize = 32_768;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichBlock {
    Paragraph {
        text: String,
        entities: Vec<TextEntity>,
        buttons: Vec<InlineKeyboardButton>,
    },
    Heading {
        level: u8,
        text: String,
        entities: Vec<TextEntity>,
    },
    List {
        ordered: bool,
        items: Vec<RichListItem>,
    },
    Collapsible {
        header: String,
        body: String,
        open: bool,
    },
    Document {
        file_name: String,
        caption: String,
        /// Set only for editor-created blocks (outgoing `inputDocumentFile`).
        local_path: Option<PathBuf>,
    },
    Photo {
        caption: String,
        /// Set only for editor-created blocks (outgoing `inputPhoto`).
        local_path: Option<PathBuf>,
    },
    Video {
        caption: String,
        /// Set only for editor-created blocks (outgoing `inputVideo`).
        local_path: Option<PathBuf>,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
    ButtonRow {
        buttons: Vec<InlineKeyboardButton>,
    },
    Divider,
    /// Invisible blocks (`pageBlockAnchor`) — parsed, renders nothing.
    Empty,
    Unsupported {
        type_name: String,
    },
}

impl RichBlock {
    /// Character (Unicode scalar value) count of every text payload in this block.
    pub fn text_char_len(&self) -> usize {
        fn len(s: &str) -> usize {
            s.chars().count()
        }
        match self {
            RichBlock::Paragraph { text, .. } | RichBlock::Heading { text, .. } => len(text),
            RichBlock::List { items, .. } => items.iter().map(|item| len(&item.text)).sum(),
            RichBlock::Collapsible { header, body, .. } => len(header) + len(body),
            RichBlock::Document {
                file_name, caption, ..
            } => len(file_name) + len(caption),
            RichBlock::Photo { caption, .. } | RichBlock::Video { caption, .. } => {
                len(caption)
            }
            RichBlock::Table { rows } => rows.iter().flatten().map(|cell| len(cell)).sum(),
            RichBlock::ButtonRow { .. }
            | RichBlock::Divider
            | RichBlock::Empty
            | RichBlock::Unsupported { .. } => 0,
        }
    }

    /// First text-ish content, for chat-list previews.
    pub fn preview_text(&self) -> Option<&str> {
        match self {
            RichBlock::Paragraph { text, .. } | RichBlock::Heading { text, .. } => {
                (!text.is_empty()).then_some(text.as_str())
            }
            RichBlock::Collapsible { header, .. } => {
                (!header.is_empty()).then_some(header.as_str())
            }
            RichBlock::Document { file_name, .. } => {
                (!file_name.is_empty()).then_some(file_name.as_str())
            }
            RichBlock::Photo { caption, .. } | RichBlock::Video { caption, .. } => {
                (!caption.is_empty()).then_some(caption.as_str())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichListItem {
    pub text: String,
    /// `Some` when the item is a checkbox (`has_checkbox`).
    pub checked: Option<bool>,
}

fn field_str(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn block_type(value: &Value) -> &str {
    value.get("@type").and_then(Value::as_str).unwrap_or("")
}

/// Plain text of a `pageBlockCaption` node (`pageBlockPhoto`,
/// `pageBlockVideo`, `pageBlockDocument` share this shape).
fn block_caption(value: &Value) -> String {
    rich_text_to_parts(
        value
            .get("caption")
            .and_then(|c| c.get("text"))
            .unwrap_or(&Value::Null),
    )
    .0
}

#[derive(Default)]
struct RichWalk {
    text: String,
    /// (utf16_start, utf16_end, kind) over the final text.
    entities: Vec<(i32, i32, TextEntityKind)>,
    buttons: Vec<InlineKeyboardButton>,
}

impl RichWalk {
    fn push(&mut self, s: &str) {
        self.text.push_str(s);
    }

    fn utf16_len(&self) -> i32 {
        self.text.encode_utf16().count() as i32
    }

    /// Walk `inner`, then wrap the appended range in `kind`.
    fn styled(&mut self, value: &Value, field: &str, kind: TextEntityKind) {
        let start = self.utf16_len();
        walk_rich_text(value.get(field).unwrap_or(&Value::Null), self);
        let end = self.utf16_len();
        if end > start {
            self.entities.push((start, end, kind));
        }
    }
}

/// Flatten one `RichText` node: plain text plus inline buttons
/// (`richTextButton`). Unknown nodes contribute nothing — never a crash.
fn walk_rich_text(value: &Value, w: &mut RichWalk) {
    match block_type(value) {
        "richTextPlain" => w.push(&field_str(value, "text")),
        "richTexts" => {
            if let Some(texts) = value.get("texts").and_then(Value::as_array) {
                for text in texts {
                    walk_rich_text(text, w);
                }
            }
        }
        "richTextBold" => w.styled(value, "text", TextEntityKind::Bold),
        "richTextItalic" => w.styled(value, "text", TextEntityKind::Italic),
        "richTextUnderline" => w.styled(value, "text", TextEntityKind::Underline),
        "richTextStrikethrough" => w.styled(value, "text", TextEntityKind::Strikethrough),
        "richTextSpoiler" => w.styled(value, "text", TextEntityKind::Spoiler),
        "richTextFixed" => w.styled(value, "text", TextEntityKind::Code),
        "richTextSubscript" | "richTextSuperscript" | "richTextMarked" => {
            walk_rich_text(value.get("text").unwrap_or(&Value::Null), w);
        }
        "richTextUrl" => {
            let url = field_str(value, "url");
            let start_text = w.text.clone();
            walk_rich_text(value.get("text").unwrap_or(&Value::Null), w);
            let label: String = w.text[start_text.len()..].to_string();
            let kind = if url == label {
                TextEntityKind::Url
            } else {
                TextEntityKind::TextUrl { url }
            };
            let start = start_text.encode_utf16().count() as i32;
            let end = w.utf16_len();
            if end > start && !label.is_empty() {
                w.entities.push((start, end, kind));
            }
        }
        "richTextEmailAddress"
        | "richTextPhoneNumber"
        | "richTextMention"
        | "richTextHashtag"
        | "richTextCashtag"
        | "richTextBankCardNumber"
        | "richTextBotCommand"
        | "richTextMentionName" => {
            walk_rich_text(value.get("text").unwrap_or(&Value::Null), w);
        }
        "richTextCustomEmoji" => w.push(&field_str(value, "alternative_text")),
        "richTextIcon" => {}
        "richTextMathematicalExpression" => w.push(&field_str(value, "expression")),
        "richTextButton" => {
            if let Some(button) = value.get("button") {
                let mut label_walk = RichWalk::default();
                walk_rich_text(button.get("text").unwrap_or(&Value::Null), &mut label_walk);
                w.push(&label_walk.text);
                w.buttons.push(parse_inline_button(button));
            }
        }
        "richTextDiff" | "richTextReference" => {
            walk_rich_text(value.get("text").unwrap_or(&Value::Null), w);
        }
        "richTextReferenceLink" | "richTextAnchorLink" => {
            let url = field_str(value, "url");
            w.styled(value, "text", TextEntityKind::TextUrl { url });
        }
        // `richTextAnchor` is an invisible marker — contributes nothing.
        "richTextAnchor" => {}
        "richTextDateTime" => {
            walk_rich_text(value.get("text").unwrap_or(&Value::Null), w);
        }
        _ => {}
    }
}

/// Flatten a `RichText` tree to (plain text, entities, inline buttons).
/// Entity offsets are converted from UTF-16 to UTF-8; unconvertible
/// entities are dropped, never fatal.
pub fn rich_text_to_parts(value: &Value) -> (String, Vec<TextEntity>, Vec<InlineKeyboardButton>) {
    let mut w = RichWalk::default();
    walk_rich_text(value, &mut w);
    let entities = w
        .entities
        .into_iter()
        .filter_map(|(start16, end16, kind)| {
            let start = utf16_to_utf8_offset(&w.text, start16).ok()?;
            let end = utf16_to_utf8_offset(&w.text, end16).ok()?;
            (end > start).then_some(TextEntity {
                utf8_start: start,
                utf8_end: end,
                kind,
            })
        })
        .collect();
    (w.text, entities, w.buttons)
}

/// Parse an `inlineButton` (schema 1.8.67, line 4030 — anniversary in-message
/// buttons). Its `text` is a `RichText`, unlike `inlineKeyboardButton`'s
/// plain string, so the label is flattened first and the shared
/// style/type parser is reused.
pub(crate) fn parse_inline_button(value: &Value) -> InlineKeyboardButton {
    let mut button = parse_inline_keyboard_button(value);
    let (label, _, _) = rich_text_to_parts(value.get("text").unwrap_or(&Value::Null));
    if !label.is_empty() {
        button.text = label;
    }
    button
}

fn paragraph(value: &Value) -> RichBlock {
    let (text, entities, buttons) = rich_text_to_parts(value.get("text").unwrap_or(&Value::Null));
    RichBlock::Paragraph {
        text,
        entities,
        buttons,
    }
}

fn heading(level: u8, value: &Value, field: &str) -> RichBlock {
    let (text, entities, _) = rich_text_to_parts(value.get(field).unwrap_or(&Value::Null));
    RichBlock::Heading {
        level,
        text,
        entities,
    }
}

fn join_blocks_text(blocks: &Value) -> String {
    blocks
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(parse_page_block)
                .filter_map(|block| match block {
                    RichBlock::Paragraph { text, .. }
                    | RichBlock::Heading { text, .. }
                    | RichBlock::Collapsible { header: text, .. } => Some(text),
                    RichBlock::List { items, .. } => Some(
                        items
                            .iter()
                            .map(|item| item.text.clone())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Parse one incoming `pageBlock*` object.
pub fn parse_page_block(value: &Value) -> RichBlock {
    match block_type(value) {
        "pageBlockTitle" => heading(1, value, "title"),
        "pageBlockSubtitle" => heading(2, value, "subtitle"),
        "pageBlockHeader" => heading(2, value, "header"),
        "pageBlockSubheader" => heading(3, value, "subheader"),
        "pageBlockKicker" => heading(3, value, "kicker"),
        "pageBlockSectionHeading" => {
            // Schema :4213 — size 1 is the largest, 6 the smallest; this is
            // what the editor's `inputPageBlockSectionHeading` round-trips to.
            let size = value.get("size").and_then(Value::as_i64).unwrap_or(2);
            heading(size.clamp(1, 3) as u8, value, "text")
        }
        "pageBlockParagraph" => paragraph(value),
        "pageBlockPreformatted" => {
            let (text, mut entities, buttons) =
                rich_text_to_parts(value.get("text").unwrap_or(&Value::Null));
            if !text.is_empty() {
                entities.push(TextEntity {
                    utf8_start: 0,
                    utf8_end: text.len(),
                    kind: TextEntityKind::Pre,
                });
            }
            RichBlock::Paragraph {
                text,
                entities,
                buttons,
            }
        }
        "pageBlockFooter" => {
            let (text, entities, buttons) =
                rich_text_to_parts(value.get("footer").unwrap_or(&Value::Null));
            RichBlock::Paragraph {
                text,
                entities,
                buttons,
            }
        }
        "pageBlockDivider" => RichBlock::Divider,
        "pageBlockAnchor" => RichBlock::Empty,
        "pageBlockList" => {
            let items = value
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| {
                            let mut text = field_str(item, "label");
                            let rest = join_blocks_text(item.get("blocks").unwrap_or(&Value::Null));
                            if !rest.is_empty() {
                                if !text.is_empty() {
                                    text.push('\n');
                                }
                                text.push_str(&rest);
                            }
                            RichListItem {
                                text,
                                checked: item
                                    .get("has_checkbox")
                                    .and_then(Value::as_bool)
                                    .filter(|has| *has)
                                    .map(|_| {
                                        item.get("is_checked")
                                            .and_then(Value::as_bool)
                                            .unwrap_or(false)
                                    }),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Schema `pageBlockList` carries no ordered flag — render bullets.
            RichBlock::List {
                ordered: false,
                items,
            }
        }
        "pageBlockBlockQuote" => {
            let text = join_blocks_text(value.get("blocks").unwrap_or(&Value::Null));
            let quoted = text
                .lines()
                .map(|line| format!("❝ {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            RichBlock::Paragraph {
                text: quoted,
                entities: Vec::new(),
                buttons: Vec::new(),
            }
        }
        "pageBlockExpandableBlockQuote" => {
            let (text, _, _) = rich_text_to_parts(value.get("text").unwrap_or(&Value::Null));
            let mut lines = text.lines();
            let header = lines.next().unwrap_or("Quote").to_string();
            RichBlock::Collapsible {
                header,
                body: lines.collect::<Vec<_>>().join("\n"),
                open: false,
            }
        }
        "pageBlockPullQuote" => paragraph(value),
        "pageBlockDetails" => {
            let (header, _, _) = rich_text_to_parts(value.get("header").unwrap_or(&Value::Null));
            RichBlock::Collapsible {
                header,
                body: join_blocks_text(value.get("blocks").unwrap_or(&Value::Null)),
                open: value
                    .get("is_open")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            }
        }
        "pageBlockDocument" => {
            let document = value.get("document").unwrap_or(&Value::Null);
            RichBlock::Document {
                file_name: field_str(document, "file_name"),
                caption: block_caption(value),
                local_path: None,
            }
        }
        // Schema :4279 (`pageBlockPhoto`) and :4287 (`pageBlockVideo`).
        "pageBlockPhoto" => RichBlock::Photo {
            caption: block_caption(value),
            local_path: None,
        },
        "pageBlockVideo" => RichBlock::Video {
            caption: block_caption(value),
            local_path: None,
        },
        "pageBlockButtonRow" => {
            let buttons = value
                .get("buttons")
                .and_then(Value::as_array)
                .map(|buttons| buttons.iter().map(parse_inline_button).collect())
                .unwrap_or_default();
            RichBlock::ButtonRow { buttons }
        }
        "pageBlockTable" => {
            let rows = value
                .get("cells")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row.as_array()
                                .map(|cells| {
                                    cells
                                        .iter()
                                        .map(|cell| {
                                            rich_text_to_parts(
                                                cell.get("text").unwrap_or(&Value::Null),
                                            )
                                            .0
                                        })
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            RichBlock::Table { rows }
        }
        other => RichBlock::Unsupported {
            type_name: other.to_string(),
        },
    }
}

/// Parse an incoming `richMessage` object → (blocks, is_full).
/// Known simplification (DECISIONS.md): `is_rtl` is ignored — Hebrew/Arabic
/// rich messages render LTR.
pub fn parse_rich_message(value: &Value) -> (Vec<RichBlock>, bool) {
    let blocks = value
        .get("blocks")
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().map(parse_page_block).collect())
        .unwrap_or_default();
    let is_full = value
        .get("is_full")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    (blocks, is_full)
}

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
            crate::composer::FormatKind::TextUrl => {
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

/// Convert composer text into editor blocks. Line markers (same spirit as
/// the M1 markup): `#`/`##`/`###` headings, `-`/`*` unordered lists,
/// `1.` ordered lists, `>>` collapsible blocks, `---` dividers.
pub fn markup_to_blocks(text: &str) -> Vec<RichBlock> {
    let mut blocks: Vec<RichBlock> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut list_items: Vec<RichListItem> = Vec::new();
    let mut list_ordered = false;

    let flush_paragraph = |paragraph: &mut Vec<String>, blocks: &mut Vec<RichBlock>| {
        let text = paragraph.join("\n").trim().to_string();
        paragraph.clear();
        if !text.is_empty() {
            blocks.push(RichBlock::Paragraph {
                text,
                entities: Vec::new(),
                buttons: Vec::new(),
            });
        }
    };
    let flush_list =
        |list_items: &mut Vec<RichListItem>, list_ordered: bool, blocks: &mut Vec<RichBlock>| {
            if !list_items.is_empty() {
                blocks.push(RichBlock::List {
                    ordered: list_ordered,
                    items: std::mem::take(list_items),
                });
            }
        };

    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if let Some(heading) = parse_heading_marker(trimmed) {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, list_ordered, &mut blocks);
            blocks.push(heading);
        } else if trimmed == "---" {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, list_ordered, &mut blocks);
            blocks.push(RichBlock::Divider);
        } else if let Some(item) = parse_list_marker(trimmed) {
            flush_paragraph(&mut paragraph, &mut blocks);
            if list_items.is_empty() {
                list_ordered = item.1;
            }
            list_items.push(item.0);
        } else if let Some(header) = trimmed.strip_prefix(">>") {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, list_ordered, &mut blocks);
            let mut body: Vec<String> = Vec::new();
            while let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.is_empty()
                    || parse_heading_marker(next_trimmed).is_some()
                    || next_trimmed == "---"
                    || parse_list_marker(next_trimmed).is_some()
                    || next_trimmed.starts_with(">>")
                {
                    break;
                }
                body.push(lines.next().unwrap_or("").to_string());
            }
            blocks.push(RichBlock::Collapsible {
                header: header.trim().to_string(),
                body: body.join("\n").trim().to_string(),
                open: false,
            });
        } else if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_list(&mut list_items, list_ordered, &mut blocks);
        } else {
            paragraph.push(line.to_string());
        }
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    flush_list(&mut list_items, list_ordered, &mut blocks);
    blocks
}

/// M2: resolve editor markup to clean text + render entities for the live
/// block preview. `parse_format_markup` yields UTF-16 offsets;
/// `TextEntity` needs UTF-8 byte offsets (converted with
/// `utf16_to_utf8_offset`, as in `rich_text_to_parts`). Block quotes are
/// block-level — inside a paragraph the text survives and the marker is
/// dropped, mirroring `formatted_to_rich_text`.
fn markup_preview_text(text: &str) -> (String, Vec<TextEntity>) {
    let (clean, composer_entities) = crate::composer::parse_format_markup(text);
    let entities = composer_entities
        .into_iter()
        .filter_map(|entity| {
            let start = utf16_to_utf8_offset(&clean, entity.offset).ok()?;
            let end = utf16_to_utf8_offset(&clean, entity.offset + entity.length).ok()?;
            if end <= start {
                return None;
            }
            let kind = match entity.kind {
                crate::composer::FormatKind::Bold => TextEntityKind::Bold,
                crate::composer::FormatKind::Italic => TextEntityKind::Italic,
                crate::composer::FormatKind::Underline => TextEntityKind::Underline,
                crate::composer::FormatKind::Strikethrough => TextEntityKind::Strikethrough,
                crate::composer::FormatKind::Spoiler => TextEntityKind::Spoiler,
                crate::composer::FormatKind::Code => TextEntityKind::Code,
                crate::composer::FormatKind::Pre => {
                    if entity.language.is_empty() {
                        TextEntityKind::Pre
                    } else {
                        TextEntityKind::PreCode {
                            language: entity.language,
                        }
                    }
                }
                crate::composer::FormatKind::TextUrl => TextEntityKind::TextUrl { url: entity.url },
                crate::composer::FormatKind::BlockQuote => return None,
            };
            Some(TextEntity {
                utf8_start: start,
                utf8_end: end,
                kind,
            })
        })
        .collect();
    (clean, entities)
}

/// M2: editor preview — `markup_to_blocks` with inline markup resolved to
/// clean text + `TextEntity` spans. Paragraphs and headings keep their
/// styling (the history renderer paints entities there); list items and
/// collapsible header/body render plain throughout Quill (same as incoming
/// `pageBlock*`, whose list labels are plain strings), so their markers
/// are stripped without entities. The send path is untouched — it
/// re-parses markup itself via `block_rich_text`.
pub fn preview_blocks(text: &str) -> Vec<RichBlock> {
    markup_to_blocks(text)
        .into_iter()
        .map(|block| match block {
            RichBlock::Paragraph {
                text,
                entities: _,
                buttons,
            } => {
                let (clean, styled) = markup_preview_text(&text);
                RichBlock::Paragraph {
                    text: clean,
                    entities: styled,
                    buttons,
                }
            }
            RichBlock::Heading {
                level,
                text,
                entities: _,
            } => {
                let (clean, styled) = markup_preview_text(&text);
                RichBlock::Heading {
                    level,
                    text: clean,
                    entities: styled,
                }
            }
            RichBlock::List { ordered, items } => RichBlock::List {
                ordered,
                items: items
                    .into_iter()
                    .map(|item| RichListItem {
                        text: markup_preview_text(&item.text).0,
                        ..item
                    })
                    .collect(),
            },
            RichBlock::Collapsible { header, body, open } => RichBlock::Collapsible {
                header: markup_preview_text(&header).0,
                body: markup_preview_text(&body).0,
                open,
            },
            other => other,
        })
        .collect()
}

fn parse_heading_marker(trimmed: &str) -> Option<RichBlock> {
    for (marker, level) in [("### ", 3), ("## ", 2), ("# ", 1)] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some(RichBlock::Heading {
                level,
                text: rest.trim().to_string(),
                entities: Vec::new(),
            });
        }
    }
    None
}

fn parse_list_marker(trimmed: &str) -> Option<(RichListItem, bool)> {
    for marker in ["- ", "* ", "• "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            let (checked, text) = parse_checkbox(rest.trim());
            return Some((
                RichListItem {
                    text: text.to_string(),
                    checked,
                },
                false,
            ));
        }
    }
    // `1. ` / `12. ` ordered items.
    let mut digits = 0;
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
        } else {
            break;
        }
    }
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        let rest = trimmed[digits + 2..].trim();
        let (checked, text) = parse_checkbox(rest);
        return Some((
            RichListItem {
                text: text.to_string(),
                checked,
            },
            true,
        ));
    }
    None
}

fn parse_checkbox(rest: &str) -> (Option<bool>, &str) {
    if let Some(text) = rest.strip_prefix("[ ] ") {
        (Some(false), text)
    } else if let Some(text) = rest.strip_prefix("[x] ") {
        (Some(true), text)
    } else {
        (None, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rich_text_flattens_styled_and_buttons() {
        let value = json!({
            "@type": "richTexts",
            "texts": [
                { "@type": "richTextPlain", "text": "Hello " },
                { "@type": "richTextBold", "text": { "@type": "richTextPlain", "text": "bold" } },
                { "@type": "richTextPlain", "text": " " },
                { "@type": "richTextButton", "button": {
                    "@type": "inlineButton",
                    "text": { "@type": "richTextPlain", "text": "Tap" },
                    "style": { "@type": "buttonStylePrimary" },
                    "type": { "@type": "inlineKeyboardButtonTypeCallback", "data": "AQID" },
                }},
            ],
        });
        let (text, entities, buttons) = rich_text_to_parts(&value);
        assert_eq!(text, "Hello bold Tap");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].kind, TextEntityKind::Bold);
        assert_eq!(&text[entities[0].utf8_start..entities[0].utf8_end], "bold");
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].text, "Tap");
        assert!(matches!(
            buttons[0].kind,
            crate::telegram::envelope::InlineKeyboardButtonType::Callback { .. }
        ));
    }

    #[test]
    fn rich_text_url_entity_kinds() {
        let bare = json!({
            "@type": "richTextUrl",
            "text": { "@type": "richTextPlain", "text": "https://example.com" },
            "url": "https://example.com",
            "is_cached": false,
        });
        let (_, entities, _) = rich_text_to_parts(&bare);
        assert_eq!(
            entities,
            vec![TextEntity {
                utf8_start: 0,
                utf8_end: 19,
                kind: TextEntityKind::Url,
            }]
        );
        let labeled = json!({
            "@type": "richTextUrl",
            "text": { "@type": "richTextPlain", "text": "site" },
            "url": "https://example.com",
            "is_cached": false,
        });
        let (_, entities, _) = rich_text_to_parts(&labeled);
        assert_eq!(
            entities[0].kind,
            TextEntityKind::TextUrl {
                url: "https://example.com".into()
            }
        );
    }

    #[test]
    fn parse_page_block_kinds() {
        let heading = json!({ "@type": "pageBlockTitle", "title": { "@type": "richTextPlain", "text": "Hi" } });
        assert!(matches!(
            parse_page_block(&heading),
            RichBlock::Heading { level: 1, .. }
        ));
        let divider = json!({ "@type": "pageBlockDivider" });
        assert_eq!(parse_page_block(&divider), RichBlock::Divider);
        let anchor = json!({ "@type": "pageBlockAnchor", "name": "x" });
        assert_eq!(parse_page_block(&anchor), RichBlock::Empty);
        let unknown = json!({ "@type": "pageBlockSlideshow" });
        assert!(matches!(
            parse_page_block(&unknown),
            RichBlock::Unsupported { .. }
        ));
        let doc = json!({
            "@type": "pageBlockDocument",
            "document": { "@type": "document", "file_name": "notes.txt" },
            "caption": { "@type": "pageBlockCaption",
                "text": { "@type": "richTextPlain", "text": "cap" },
                "credit": { "@type": "richTextPlain", "text": "" } },
        });
        assert_eq!(
            parse_page_block(&doc),
            RichBlock::Document {
                file_name: "notes.txt".into(),
                caption: "cap".into(),
                local_path: None,
            }
        );
        let photo = json!({
            "@type": "pageBlockPhoto",
            "photo": { "@type": "photo", "id": 1 },
            "caption": { "@type": "pageBlockCaption",
                "text": { "@type": "richTextPlain", "text": "sunset" },
                "credit": { "@type": "richTextPlain", "text": "" } },
            "url": "",
            "has_spoiler": false,
        });
        assert_eq!(
            parse_page_block(&photo),
            RichBlock::Photo {
                caption: "sunset".into(),
                local_path: None,
            }
        );
        let video = json!({
            "@type": "pageBlockVideo",
            "video": { "@type": "video", "id": 2 },
            "caption": { "@type": "pageBlockCaption",
                "text": { "@type": "richTextPlain", "text": "clip" },
                "credit": { "@type": "richTextPlain", "text": "" } },
            "need_autoplay": false,
            "is_looped": false,
            "has_spoiler": false,
        });
        assert_eq!(
            parse_page_block(&video),
            RichBlock::Video {
                caption: "clip".into(),
                local_path: None,
            }
        );
    }

    #[test]
    fn parse_page_block_list_and_details() {
        let list = json!({
            "@type": "pageBlockList",
            "items": [
                { "@type": "pageBlockListItem", "label": "one", "blocks": [],
                   "has_checkbox": true, "is_checked": true, "value": 0, "type": "" },
                { "@type": "pageBlockListItem", "label": "two", "blocks": [],
                   "has_checkbox": false, "is_checked": false, "value": 0, "type": "" },
            ],
        });
        let RichBlock::List { items, .. } = parse_page_block(&list) else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].checked, Some(true));
        assert_eq!(items[1].checked, None);

        let details = json!({
            "@type": "pageBlockDetails",
            "header": { "@type": "richTextPlain", "text": "More" },
            "blocks": [{ "@type": "pageBlockParagraph",
                "text": { "@type": "richTextPlain", "text": "body" } }],
            "is_open": true,
        });
        assert_eq!(
            parse_page_block(&details),
            RichBlock::Collapsible {
                header: "More".into(),
                body: "body".into(),
                open: true,
            }
        );
    }

    #[test]
    fn markup_to_blocks_conventions() {
        let blocks = markup_to_blocks(
            "# Title\n\n- a\n- [x] b\n\n1. first\n\n>> More\nbody line\n\n---\n\nplain **bold** tail",
        );
        assert!(matches!(blocks[0], RichBlock::Heading { level: 1, .. }));
        let RichBlock::List { ordered, items } = &blocks[1] else {
            panic!("expected list, got {:?}", blocks[1]);
        };
        assert!(!ordered);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].checked, Some(true));
        let RichBlock::List { ordered, .. } = &blocks[2] else {
            panic!("expected ordered list");
        };
        assert!(ordered);
        assert!(matches!(
            &blocks[3],
            RichBlock::Collapsible { header, open: false, .. } if header == "More"
        ));
        assert_eq!(blocks[4], RichBlock::Divider);
        assert!(
            matches!(&blocks[5], RichBlock::Paragraph { text, .. } if text.contains("**bold**"))
        );
    }

    #[test]
    fn preview_blocks_resolves_markup_to_entities() {
        // M2: the editor preview shows clean text + entity spans for
        // paragraphs/headings (no raw `**` markers); list items and
        // collapsible text is marker-stripped, matching how Quill renders
        // those blocks everywhere.
        let blocks = preview_blocks(
            "# **Club** night\n\nPick **one**:\n\n- **a**\n\n>> **More**\n**body**\n",
        );
        let RichBlock::Heading { text, entities, .. } = &blocks[0] else {
            panic!("expected heading, got {:?}", blocks[0]);
        };
        assert_eq!(text, "Club night");
        assert_eq!(entities.len(), 1);
        assert!(matches!(entities[0].kind, TextEntityKind::Bold));
        let RichBlock::Paragraph { text, entities, .. } = &blocks[1] else {
            panic!("expected paragraph, got {:?}", blocks[1]);
        };
        assert_eq!(text, "Pick one:");
        assert_eq!(entities.len(), 1);
        let RichBlock::List { items, .. } = &blocks[2] else {
            panic!("expected list, got {:?}", blocks[2]);
        };
        assert_eq!(items[0].text, "a");
        let RichBlock::Collapsible { header, body, .. } = &blocks[3] else {
            panic!("expected collapsible, got {:?}", blocks[3]);
        };
        assert_eq!(header, "More");
        assert_eq!(body, "body");
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
    fn section_heading_parses_and_round_trips() {
        // `pageBlockSectionHeading` (schema :4213) is what the editor's
        // `inputPageBlockSectionHeading` round-trips to in TDLib — size 1
        // (the largest) parses to level 1, and the editor's H1 sends size 1.
        let parsed = parse_page_block(&json!({
            "@type": "pageBlockSectionHeading",
            "text": { "@type": "richTextPlain", "text": "Title" },
            "size": 1,
        }));
        assert!(matches!(parsed, RichBlock::Heading { level: 1, .. }));
        let json = input_page_block_json(&RichBlock::Heading {
            level: 1,
            text: "Title".into(),
            entities: Vec::new(),
        })
        .expect("heading");
        assert_eq!(json["@type"], "inputPageBlockSectionHeading");
        assert_eq!(json["size"], 1);
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
