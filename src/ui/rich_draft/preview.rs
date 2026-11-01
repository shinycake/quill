//! The rich editor's live preview. The blocks are parsed once per revision of
//! the text, and a long draft lays out only the blocks in view.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::bg_canvas;
use super::super::message_text::rich_block_element;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::rich::{PreviewBlocks, RichBlock};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// The height the preview stops growing at, so a long draft can't squeeze the
/// history to zero height on short windows.
const PREVIEW_HEIGHT: Pixels = px(320.);

/// From this many blocks the preview is sure to fill [`PREVIEW_HEIGHT`], so
/// it can be a list of that height.
const LISTED_BLOCKS: usize = 32;

const OVERDRAW: Pixels = px(200.);

#[derive(Default)]
struct Parsed {
    revision: Option<u64>,
    source: PreviewBlocks,
    blocks: Rc<Vec<Rc<RichBlock>>>,
}

pub(in crate::ui) struct RichPreview {
    parsed: RefCell<Parsed>,
    list: ListState,
}

impl Default for RichPreview {
    fn default() -> Self {
        Self {
            parsed: RefCell::default(),
            list: ListState::new(0, ListAlignment::Top, OVERDRAW),
        }
    }
}

impl RichPreview {
    fn blocks(&self, revision: u64, text: &str) -> Rc<Vec<Rc<RichBlock>>> {
        let changed = {
            let mut parsed = self.parsed.borrow_mut();
            (parsed.revision != Some(revision)).then(|| {
                let change = parsed.source.update(text);
                parsed.blocks = Rc::new(parsed.source.shown().to_vec());
                parsed.revision = Some(revision);
                change
            })
        };
        if let Some((replaced, count)) = changed {
            self.list.splice(replaced, count);
        }
        self.parsed.borrow().blocks.clone()
    }
}

impl QuillApp {
    fn rich_preview_block(
        &self,
        index: usize,
        block: &RichBlock,
        font: Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Editor blocks never carry buttons, so the callback ids are unused.
        let revealed = HashSet::new();
        let ids = (ChatId(0), MessageId(0));
        rich_block_element(index, block, (0, 0), ids.0, ids.1, &revealed, font, cx)
    }

    /// The parsed blocks of the editor's text, or `None` with nothing to show.
    pub(in crate::ui) fn rich_editor_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let composer = self.composer.read(cx);
        let revision = composer.document_revision();
        let text = composer.value().to_string();
        let preview = &self.composer_ui.rich_draft.preview;
        let blocks = preview.blocks(revision, &text);
        let shows = |block: &Rc<RichBlock>| {
            !matches!(**block, RichBlock::Empty | RichBlock::Unsupported { .. })
        };
        if !blocks.iter().any(shows) {
            return None;
        }
        let font = self.msg_font();
        let frame = div()
            .id("rich-editor-preview")
            .px_2()
            .py_1()
            .rounded_md()
            .bg(bg_canvas());
        if blocks.len() >= LISTED_BLOCKS {
            let app = cx.weak_entity();
            let rows = list(preview.list.clone(), move |index, _, cx| {
                let block = blocks.get(index);
                let row = app.update(cx, |app, cx| {
                    block.and_then(|block| app.rich_preview_block(index, block, font, cx))
                });
                div().pb_1().children(row.ok().flatten()).into_any_element()
            })
            .size_full();
            return Some(frame.h(PREVIEW_HEIGHT).child(rows).into_any_element());
        }
        let mut column = frame
            .flex()
            .flex_col()
            .gap_1()
            .max_h(PREVIEW_HEIGHT)
            .overflow_y_scroll();
        for (index, block) in blocks.iter().enumerate() {
            if let Some(child) = self.rich_preview_block(index, block, font, cx) {
                column = column.child(child);
            }
        }
        Some(column.into_any_element())
    }
}
