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

mod input;
mod markup;
mod parse;
pub use input::*;
pub use markup::*;
pub use parse::*;

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
            RichBlock::Photo { caption, .. } | RichBlock::Video { caption, .. } => len(caption),
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
