//! Rich text: TDLib `RichText` / `PageBlock` JSON into `RichBlock`s.
use super::*;

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
}
