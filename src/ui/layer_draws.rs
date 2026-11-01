//! What each animation layer of the main window drew since the frame
//! clock (`frame_clock`) last ticked it.
#![deny(clippy::indexing_slicing)]

use gpui_kit::EntityId;
use std::time::Instant;

/// What each animation layer of the main window drew since the clock last
/// ticked it: a layer a clip's picture redrew (with its emoji) needs no
/// tick of its own until one content frame after that draw.
#[derive(Debug, Default)]
pub(super) struct LayerDraws(Vec<LayerDraw>);

#[derive(Debug)]
struct LayerDraw {
    painter: EntityId,
    /// The clock ticked it and it has not drawn since: its next draw is
    /// the tick's own.
    ticked: bool,
    /// The vsync it last drew on after its tick's own draw.
    redrawn: Option<Instant>,
}

impl LayerDraws {
    /// The clock ticked `painter` (any other view is no layer: ignored).
    pub(super) fn ticked(&mut self, painter: EntityId) {
        if let Some(draw) = self.0.iter_mut().find(|draw| draw.painter == painter) {
            draw.ticked = true;
            draw.redrawn = None;
        }
    }

    /// `painter` drew a frame shown on the vsync at `at`. Only a draw that
    /// `counts` as a redraw puts its next tick off: one by a clip off the
    /// grid shows no new emoji frame (`frame_clock::clip_redraws_off_grid`).
    pub(super) fn drawn(&mut self, painter: EntityId, at: Instant, counts: bool) {
        if let Some(draw) = self.0.iter_mut().find(|draw| draw.painter == painter) {
            if std::mem::take(&mut draw.ticked) {
                draw.redrawn = None;
            } else if counts {
                draw.redrawn = Some(at);
            }
            return;
        }
        if !counts {
            return;
        }
        // The chat list's and the conversation's; a replaced one ages out.
        if self.0.len() >= 4 {
            self.0.remove(0);
        }
        self.0.push(LayerDraw {
            painter,
            ticked: false,
            redrawn: Some(at),
        });
    }

    /// The oldest of the vsyncs `targets` were last redrawn on since their
    /// ticks: `None` unless every target is a layer redrawn since.
    pub(super) fn redrawn(
        &self,
        targets: impl IntoIterator<Item = Option<EntityId>>,
    ) -> Option<Instant> {
        let mut oldest: Option<Instant> = None;
        for target in targets {
            let painter = target?;
            let at = self
                .0
                .iter()
                .find(|draw| draw.painter == painter)?
                .redrawn?;
            oldest = Some(oldest.map_or(at, |oldest| oldest.min(at)));
        }
        oldest
    }
}
