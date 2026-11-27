//! The glass tuning preview's pieces can be dragged over its backdrop, to
//! see each one's glass over stripes, text, rings and photos. Where each
//! piece sits and the drag in progress live here, with the panel
//! (`GlassTuner`). Nothing is written to disk: a new launch starts every
//! piece in its place.
//!
//! A drag is one value (`PreviewDrag`): it starts on a press on a piece and
//! ends on the release, inside the preview or outside it, or on the first
//! move that finds the button up (a release the preview never saw).
#![deny(clippy::indexing_slicing)]

use gpui_kit::{Bounds, Pixels, Point, point, px};
use std::cell::Cell;
use std::rc::Rc;

/// One of the surfaces in the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewPiece {
    Tabs,
    Date,
    Slab,
    Menu,
    Emoji,
    Jump,
}

impl PreviewPiece {
    /// Every piece, in the order they are first drawn (the last on top).
    pub(crate) const ALL: [Self; 6] = [
        Self::Tabs,
        Self::Date,
        Self::Slab,
        Self::Menu,
        Self::Emoji,
        Self::Jump,
    ];

    fn index(self) -> usize {
        match self {
            Self::Tabs => 0,
            Self::Date => 1,
            Self::Slab => 2,
            Self::Menu => 3,
            Self::Emoji => 4,
            Self::Jump => 5,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Tabs => "glass-preview-piece-tabs",
            Self::Date => "glass-preview-piece-date",
            Self::Slab => "glass-preview-piece-slab",
            Self::Menu => "glass-preview-piece-menu",
            Self::Emoji => "glass-preview-piece-emoji",
            Self::Jump => "glass-preview-piece-jump",
        }
    }
}

/// The drag in progress, if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PreviewDrag {
    /// Every piece rests where it was left.
    Resting,
    /// `piece` follows the pointer: it was at `from` (its offset) when the
    /// button went down at `grab`, in window coordinates.
    Dragging {
        piece: PreviewPiece,
        grab: Point<Pixels>,
        from: Point<Pixels>,
    },
}

/// Where the pieces sit, which is on top, and the drag.
#[derive(Debug)]
pub(crate) struct PreviewLayout {
    /// Each piece's offset from where it starts, by `PreviewPiece::index`.
    offsets: [Point<Pixels>; 6],
    /// Paint order: the piece moved last draws on top.
    order: [PreviewPiece; 6],
    drag: PreviewDrag,
    /// Where the preview was last drawn (layout output, written as it
    /// prepaints): a dragged piece's grab point stays inside it.
    area: PreviewArea,
}

/// The preview's bounds as last drawn, in window coordinates.
#[derive(Clone, Debug, Default)]
pub(crate) struct PreviewArea(Rc<Cell<Option<Bounds<Pixels>>>>);

impl PreviewArea {
    pub(crate) fn record(&self, bounds: Bounds<Pixels>) {
        self.0.set(Some(bounds));
    }

    /// `at`, moved inside the preview when it was drawn.
    fn clamp(&self, at: Point<Pixels>) -> Point<Pixels> {
        match self.0.get() {
            Some(area) => point(
                at.x.clamp(area.left(), area.right()),
                at.y.clamp(area.top(), area.bottom()),
            ),
            None => at,
        }
    }
}

impl Default for PreviewLayout {
    fn default() -> Self {
        Self {
            offsets: [point(px(0.), px(0.)); 6],
            order: PreviewPiece::ALL,
            drag: PreviewDrag::Resting,
            area: PreviewArea::default(),
        }
    }
}

impl PreviewLayout {
    /// How far `piece` was moved from where it starts.
    pub(crate) fn offset(&self, piece: PreviewPiece) -> Point<Pixels> {
        self.offsets.get(piece.index()).copied().unwrap_or_default()
    }

    /// The pieces in paint order.
    pub(crate) fn order(&self) -> [PreviewPiece; 6] {
        self.order
    }

    /// The piece being dragged.
    pub(crate) fn dragging(&self) -> Option<PreviewPiece> {
        match self.drag {
            PreviewDrag::Resting => None,
            PreviewDrag::Dragging { piece, .. } => Some(piece),
        }
    }

    pub(crate) fn area(&self) -> &PreviewArea {
        &self.area
    }

    /// The button went down on `piece` at `at`: it follows the pointer
    /// from here, drawn over the others.
    pub(crate) fn press(&mut self, piece: PreviewPiece, at: Point<Pixels>) {
        self.drag = PreviewDrag::Dragging {
            piece,
            grab: at,
            from: self.offset(piece),
        };
        let mut order = self.order.into_iter().filter(|other| *other != piece);
        self.order = std::array::from_fn(|_| order.next().unwrap_or(piece));
    }

    /// The pointer moved to `at`, the left button `held` or not. Answers
    /// whether anything moved. A move with the button up ends the drag:
    /// the release happened where the preview could not see it.
    pub(crate) fn pointer_moved(&mut self, at: Point<Pixels>, held: bool) -> bool {
        let PreviewDrag::Dragging { piece, grab, from } = self.drag else {
            return false;
        };
        if !held {
            self.drag = PreviewDrag::Resting;
            return true;
        }
        let at = self.area.clamp(at);
        if let Some(offset) = self.offsets.get_mut(piece.index()) {
            *offset = from + (at - grab);
        }
        true
    }

    /// The button came up, anywhere: the dragged piece stays where it is.
    /// Answers whether a drag ended.
    pub(crate) fn release(&mut self) -> bool {
        let was_dragging = self.drag != PreviewDrag::Resting;
        self.drag = PreviewDrag::Resting;
        was_dragging
    }
}

#[cfg(test)]
#[path = "preview_drag_tests.rs"]
mod tests;
