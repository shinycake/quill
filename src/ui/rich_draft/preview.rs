//! The rich editor's live preview. The blocks are parsed once per revision of
//! the text, and a long draft lays out only the blocks in view.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::bg_canvas;
use super::super::message_text::rich_block_element;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::rich::{MarkupBlocks, PreviewBlocks, RichBlock};
use std::cell::{Cell, RefCell};
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
}

pub(in crate::ui) struct RichPreview {
    parsed: RefCell<Parsed>,
    /// The blocks last parsed, kept outside `parsed` so a frame that finds
    /// the cell busy draws these rather than nothing.
    shown: Cell<Rc<Vec<Rc<RichBlock>>>>,
    list: ListState,
}

impl Default for RichPreview {
    fn default() -> Self {
        Self {
            parsed: RefCell::default(),
            shown: Cell::default(),
            list: ListState::new(0, ListAlignment::Top, OVERDRAW),
        }
    }
}

/// The editor's text as blocks, reparsing only what an edit touched. A
/// `Cell`: `parse` takes the cache out, runs and puts it back, so a second
/// caller while one runs parses afresh instead of panicking on a borrow.
#[derive(Default)]
pub(in crate::ui) struct EditorBlocks(std::cell::Cell<MarkupBlocks>);

impl EditorBlocks {
    pub(in crate::ui) fn parse(&self, text: &str) -> Vec<std::sync::Arc<RichBlock>> {
        let mut blocks = self.0.take();
        let parsed = blocks.parse(text);
        self.0.set(blocks);
        parsed
    }
}

impl RichPreview {
    fn blocks(&self, revision: u64, text: &str, editor: &EditorBlocks) -> Rc<Vec<Rc<RichBlock>>> {
        // A busy cell keeps the blocks last shown and parses next frame.
        let changed = self.parsed.try_borrow_mut().ok().and_then(|mut parsed| {
            (parsed.revision != Some(revision)).then(|| {
                let change = parsed.source.update(editor.parse(text));
                self.shown.set(Rc::new(parsed.source.shown().to_vec()));
                parsed.revision = Some(revision);
                change
            })
        });
        if let Some((replaced, count)) = changed {
            self.list.splice(replaced, count);
        }
        self.last_blocks()
    }

    fn last_blocks(&self) -> Rc<Vec<Rc<RichBlock>>> {
        let blocks = self.shown.take();
        self.shown.set(blocks.clone());
        blocks
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
        let revealed = crate::ui::message_text::revealed::Revealed::default();
        let ids = (ChatId(0), MessageId(0));
        rich_block_element(index, block, (0, 0), ids.0, ids.1, &revealed, font, cx)
    }

    /// The parsed blocks of the editor's text, or `None` with nothing to show.
    pub(in crate::ui) fn rich_editor_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let composer = self.composer.read(cx);
        let revision = composer.document_revision();
        let text = composer.value().to_string();
        let draft = &self.composer_ui.rich_draft;
        let preview = &draft.preview;
        let blocks = preview.blocks(revision, &text, &draft.parsed);
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
