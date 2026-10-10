//! Rich text: plain-text markup to and from `RichBlock`s, and the live preview.
use super::*;

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

/// Serialize blocks to the composer markup [`markup_to_blocks`] parses.
///
/// Inverse of the editor's block markers (`#` / `##` / `###`, `-` and
/// `1.` lists, `[ ]` / `[x]` checkboxes, `>>` collapsible blocks, `---`).
/// Rich AI answers are written back through this so the rich send path
/// rebuilds the same structure. Clipboard
/// [`crate::telegram::envelope::RichMessageContent::copy_text`] drops those
/// markers, and the send path then sees flat paragraph(s).
///
/// Inline `TextEntity` spans stay plain text: the editor stores styling as
/// markup characters inside the block text, and incoming entity spans have
/// no marker here. Tables, captions, and document names have no block
/// marker either — their text is kept as paragraphs so the words are not
/// dropped. Button rows, anchors, and unsupported blocks contribute nothing.
pub fn blocks_to_markup(blocks: &[RichBlock]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        if let Some(chunk) = block_to_markup(block)
            && !chunk.is_empty()
        {
            parts.push(chunk);
        }
    }
    parts.join("\n\n")
}

fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn block_to_markup(block: &RichBlock) -> Option<String> {
    match block {
        RichBlock::Paragraph { text, .. } => {
            let text = text.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }
        RichBlock::Heading { level, text, .. } => {
            let level = (*level).clamp(1, 3) as usize;
            Some(format!("{} {}", "#".repeat(level), one_line(text)))
        }
        RichBlock::List { ordered, items } => {
            if items.is_empty() {
                return None;
            }
            let lines: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let marker = if *ordered {
                        format!("{}. ", index + 1)
                    } else {
                        String::from("- ")
                    };
                    let check = match item.checked {
                        Some(true) => "[x] ",
                        Some(false) => "[ ] ",
                        None => "",
                    };
                    format!("{marker}{check}{}", one_line(&item.text))
                })
                .collect();
            Some(lines.join("\n"))
        }
        RichBlock::Collapsible { header, body, .. } => {
            let mut lines = vec![format!(">> {}", one_line(header))];
            let body = body
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !body.is_empty() {
                lines.push(body);
            }
            Some(lines.join("\n"))
        }
        RichBlock::Divider => Some("---".to_string()),
        RichBlock::Table { rows } => {
            let lines: Vec<String> = rows
                .iter()
                .filter_map(|row| {
                    let line = row
                        .iter()
                        .map(String::as_str)
                        .map(str::trim)
                        .filter(|cell| !cell.is_empty())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    if line.is_empty() { None } else { Some(line) }
                })
                .collect();
            if lines.is_empty() {
                None
            } else {
                Some(lines.join("\n"))
            }
        }
        RichBlock::Document {
            file_name, caption, ..
        } => nonempty_lines(&[file_name, caption]),
        RichBlock::Photo { caption, .. } | RichBlock::Video { caption, .. } => {
            nonempty_lines(&[caption])
        }
        RichBlock::ButtonRow { .. } | RichBlock::Empty | RichBlock::Unsupported { .. } => None,
    }
}

fn nonempty_lines(parts: &[&str]) -> Option<String> {
    let lines: Vec<&str> = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
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
                crate::composer::FormatKind::MentionName => TextEntityKind::MentionName {
                    user_id: crate::composer::mention_user_id(&entity.url)?,
                },
                crate::composer::FormatKind::CustomEmoji => TextEntityKind::CustomEmoji {
                    custom_emoji_id: entity
                        .url
                        .strip_prefix("tg://emoji?id=")
                        .and_then(|id| id.parse().ok())?,
                },
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
    fn blocks_to_markup_round_trips_structure_copy_text_flattens() {
        // Rich AI answers must land as editor markup the send path
        // re-parses. `copy_text` drops the markers, so the same blocks
        // become flat paragraph(s).
        let source = "# Title\n\n## Sub\n\n### Small\n\n- a\n- [ ] b\n- [x] c\n\n1. first\n2. second\n\n>> More\nbody line\nstill body\n\n---\n\nplain **bold** tail";
        let blocks = markup_to_blocks(source);
        let markup = blocks_to_markup(&blocks);
        assert_eq!(markup_to_blocks(&markup), blocks);

        let sent = input_rich_message(&markup_to_blocks(&markup)).expect("sendable");
        let types: Vec<&str> = sent["source"]["blocks"]
            .as_array()
            .expect("blocks")
            .iter()
            .map(|block| block["@type"].as_str().unwrap_or(""))
            .collect();
        for kind in [
            "inputPageBlockSectionHeading",
            "inputPageBlockList",
            "inputPageBlockDetails",
            "inputPageBlockDivider",
            "inputPageBlockParagraph",
        ] {
            assert!(types.contains(&kind), "missing {kind} in {types:?}");
        }

        let flat = crate::telegram::envelope::RichMessageContent {
            blocks: blocks.clone(),
            is_full: true,
        }
        .copy_text();
        let flat_blocks = markup_to_blocks(&flat);
        assert!(
            flat_blocks
                .iter()
                .all(|block| matches!(block, RichBlock::Paragraph { .. })),
            "copy_text must flatten, got {flat_blocks:?}"
        );
        assert!(flat_blocks.len() < blocks.len());
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
}
