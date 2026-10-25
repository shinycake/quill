//! Rich message drafts: `draftMessageContentRichMessage` from TDLib, and
//! `draftMessageContentInputRichMessage` back (schema `td_api.tl`
//! `DraftMessageContent`). tdesktop keeps such a draft as a cloud draft
//! with a `richMessage` page (`Data::Draft::richMessage`) and shows it in
//! place of the message field (`HistoryView::Controls::RichDraftPreview`).
use super::*;

/// A rich draft, held as editor blocks: paragraph, heading, list and
/// details text is composer markup, the form the rich editor writes and
/// [`input_page_block_json`] reads. One built from TDLib's blocks has its
/// inline entities written back as markup, so it edits, sends and saves
/// like one typed in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichDraft {
    blocks: Vec<RichBlock>,
}

impl RichDraft {
    /// The draft of a parsed `richMessage`. `None` when no block shows.
    pub fn from_message(blocks: &[RichBlock]) -> Option<Self> {
        Self::from_blocks(blocks.iter().map(editor_block).collect())
    }

    /// The draft the rich editor's markup saves as. `None` when the markup
    /// is plain paragraphs, which save as a text draft instead (tdesktop
    /// `SerializeAsSimple` in `prepareRichDraftForAutosave`).
    pub fn from_markup(markup: &str) -> Option<Self> {
        let blocks = markup_to_blocks(markup);
        if blocks
            .iter()
            .all(|block| matches!(block, RichBlock::Paragraph { .. }))
        {
            return None;
        }
        Self::from_blocks(blocks)
    }

    fn from_blocks(blocks: Vec<RichBlock>) -> Option<Self> {
        blocks
            .iter()
            .any(|block| *block != RichBlock::Empty)
            .then_some(Self { blocks })
    }

    /// The editor blocks, for `inputMessageRichMessage`.
    pub fn blocks(&self) -> &[RichBlock] {
        &self.blocks
    }

    /// The markup the rich editor opens the draft with.
    pub fn markup(&self) -> String {
        blocks_to_markup(&self.blocks)
    }

    /// The blocks as the block renderer paints them.
    pub fn preview_blocks(&self) -> Vec<RichBlock> {
        self.blocks.iter().cloned().map(preview_block).collect()
    }

    /// `inputRichMessage` for `draftMessageContentInputRichMessage`. `None`
    /// when a block has no input form (media, buttons or a block Quill
    /// does not read, from a draft another app saved): sending or saving
    /// it would drop that block.
    pub fn input(&self) -> Option<Value> {
        if self
            .blocks
            .iter()
            .any(|block| *block != RichBlock::Empty && input_page_block_json(block).is_none())
        {
            return None;
        }
        input_rich_message(&self.blocks)
    }

    /// Whether sending needs Premium (tdesktop
    /// `RichPageUsesPremiumFormatting`): anything but paragraphs of plain
    /// text.
    pub fn uses_premium_formatting(&self) -> bool {
        self.blocks.iter().any(|block| match block {
            RichBlock::Paragraph { text, .. } => {
                !crate::composer::parse_format_markup(text).1.is_empty()
            }
            RichBlock::Empty => false,
            _ => true,
        })
    }

    /// The draft flattened to message markup for "Send Without Formatting"
    /// (tdesktop `FlattenRichPageToSimpleText`): inline formatting stays,
    /// block structure becomes text lines. `None` when a block cannot be
    /// flattened (tdesktop `RichPageIsFlattenSafe`: tables, media, buttons).
    pub fn plain_markup(&self) -> Option<String> {
        let mut lines = Vec::new();
        for block in &self.blocks {
            match block {
                RichBlock::Table { .. }
                | RichBlock::Photo { .. }
                | RichBlock::Video { .. }
                | RichBlock::Document { .. }
                | RichBlock::ButtonRow { .. }
                | RichBlock::Unsupported { .. } => return None,
                block => summary_lines(block, &mut lines),
            }
        }
        Some(lines.join("\n"))
    }

    /// The chat list's draft line (tdesktop `FlattenRichPageSummary`).
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        for block in self.preview_blocks() {
            summary_lines(&block, &mut lines);
        }
        lines.join("\n")
    }
}

/// The text lines one block contributes to a summary (tdesktop
/// `AppendSummaryBlock` without icons).
fn summary_lines(block: &RichBlock, lines: &mut Vec<String>) {
    let mut push = |line: String| {
        let line = line.trim();
        if !line.is_empty() {
            lines.push(line.to_string());
        }
    };
    match block {
        RichBlock::Paragraph { text, .. } | RichBlock::Heading { text, .. } => push(text.clone()),
        RichBlock::List { ordered, items } => {
            for (index, item) in items.iter().enumerate() {
                let prefix = match (item.checked, ordered) {
                    (Some(true), _) => "[x] ".to_string(),
                    (Some(false), _) => "[ ] ".to_string(),
                    (None, true) => format!("{}. ", index + 1),
                    (None, false) => "- ".to_string(),
                };
                push(format!("{prefix}{}", item.text));
            }
        }
        RichBlock::Collapsible { header, body, .. } => {
            push(header.clone());
            push(body.clone());
        }
        RichBlock::Table { rows } => {
            for row in rows {
                push(
                    row.iter()
                        .map(|cell| cell.trim())
                        .filter(|cell| !cell.is_empty())
                        .collect::<Vec<_>>()
                        .join(" | "),
                );
            }
        }
        RichBlock::Document {
            file_name, caption, ..
        } => push(if caption.trim().is_empty() {
            file_name.clone()
        } else {
            caption.clone()
        }),
        RichBlock::Photo { caption, .. } | RichBlock::Video { caption, .. } => {
            push(caption.clone())
        }
        RichBlock::ButtonRow { buttons } => {
            for button in buttons {
                push(button.text.clone());
            }
        }
        RichBlock::Divider | RichBlock::Empty | RichBlock::Unsupported { .. } => {}
    }
}

/// A parsed `pageBlock*` as the editor holds it: paragraph and heading
/// text with its entities written as markup. List and details text is
/// plain already.
fn editor_block(block: &RichBlock) -> RichBlock {
    match block {
        RichBlock::Paragraph {
            text,
            entities,
            buttons,
        } => RichBlock::Paragraph {
            text: crate::composer::entities_to_markup(text, entities),
            entities: Vec::new(),
            buttons: buttons.clone(),
        },
        RichBlock::Heading {
            level,
            text,
            entities,
        } => RichBlock::Heading {
            level: *level,
            text: crate::composer::entities_to_markup(text, entities),
            entities: Vec::new(),
        },
        other => other.clone(),
    }
}

/// Whether pasted text has structure the message field cannot show
/// (tdesktop `TextHasMarkdownStructure`): headings, dividers or task
/// lists. Plain lists and quotes read the same in the field.
pub fn text_has_markdown_structure(text: &str) -> bool {
    if text.chars().count() > RICH_TEXT_MAX_CHARS {
        return false;
    }
    markup_to_blocks(text).iter().any(|block| match block {
        RichBlock::Heading { .. } | RichBlock::Divider => true,
        RichBlock::List { items, .. } => items.iter().any(|item| item.checked.is_some()),
        _ => false,
    })
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
