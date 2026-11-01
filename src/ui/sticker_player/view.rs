//! A view of a clip: one place it shows, with its own play head.
#![deny(clippy::indexing_slicing)]

use super::{ClipSpec, HeadId, OPENING_FPS, Player, high_rate};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::sticker_playback::player::{PlayMode, edge_for};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One place a clip shows, with its own play head.
#[derive(Clone)]
pub(in crate::ui) struct StickerView {
    pub(super) player: Rc<Player>,
    pub(super) spec: Rc<ClipSpec>,
    pub(super) head: HeadId,
    pub(super) mode: PlayMode,
    /// The edge resolved while rendering (box × last scale factor), for
    /// [`Self::frame_now`].
    pub(super) edge: u32,
    /// Frames a second at most.
    pub(super) max_fps: u32,
    /// The window was active when the view was made.
    pub(super) active: bool,
    /// The window it shows in (`None`: the main window).
    pub(super) window: Option<ChatId>,
}

impl StickerView {
    /// The square of `bounds`' longer side, centred on it: frames are
    /// square, and a WebM sticker's square holds its video letterboxed to
    /// the box's shape.
    fn square(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let side = bounds.size.width.max(bounds.size.height);
        Bounds::new(
            point(
                bounds.origin.x + (bounds.size.width - side) / 2.,
                bounds.origin.y + (bounds.size.height - side) / 2.,
            ),
            size(side, side),
        )
    }

    /// Paint the frame due now into `bounds`, drawn at their size on
    /// this window.
    pub(in crate::ui) fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let square = Self::square(bounds);
        let edge = edge_for(f32::from(square.size.width) * window.scale_factor());
        let active = window.is_window_active() || super::super::frame_clock::assume_active();
        let shown = self.player.frame(
            &self.spec,
            self.head,
            self.mode,
            edge,
            high_rate(active),
            self.window,
            self.player.now(),
        );
        if let Some(image) = shown {
            let _ = window.paint_image(square, square, Corners::default(), image, 0, false);
        }
    }

    /// The frame due now at the edge resolved while rendering, for views
    /// shown as an `img` (panels, the composer).
    pub(in crate::ui) fn frame_now(&self) -> Option<Arc<RenderImage>> {
        self.player.frame(
            &self.spec,
            self.head,
            self.mode,
            self.edge,
            high_rate(self.active),
            self.window,
            self.player.now(),
        )
    }

    /// Frames a second it needs now (0: it rests).
    pub(in crate::ui) fn fps(&self) -> u32 {
        let Ok(inner) = self.player.inner.try_borrow() else {
            return 0;
        };
        let key = self.spec.key(self.edge);
        // The clip drawn at the painted size, else the one asked for while
        // rendering.
        let clip = inner
            .clips
            .iter()
            .filter(|(other, _)| self.spec.same_file(other))
            .max_by_key(|(_, clip)| clip.used)
            .map(|(_, clip)| clip);
        let Some(timeline) = clip.and_then(|clip| clip.timeline) else {
            return if clip.is_none_or(|clip| clip.failed) {
                0
            } else {
                OPENING_FPS.min(self.max_fps)
            };
        };
        let head = inner
            .heads
            .get(&(self.head, key.file))
            .map(|head| head.state)
            .unwrap_or_default();
        let display_high = high_rate(self.active);
        let coarse = self.player.engine.coarse(&key);
        let high = display_high && !coarse;
        let pick = match self.mode {
            PlayMode::At(_) => quill::sticker_playback::player::head::pick(
                Duration::ZERO,
                self.mode,
                timeline,
                high,
            ),
            _ => {
                let mut head = head;
                head.advance(self.player.now(), self.mode, timeline, high)
            }
        };
        match pick {
            Some(pick) if pick.animating => (timeline.tick_fps(display_high, coarse).ceil() as u32)
                .clamp(1, self.max_fps.max(1)),
            _ => 0,
        }
    }

    /// When its clip's first frame arrived (at any size), the clip's
    /// readiness for an effect that picks among ready clips.
    pub(in crate::ui) fn ready_at(&self) -> Option<Instant> {
        let inner = self.player.inner.try_borrow().ok()?;
        inner
            .clips
            .iter()
            .filter(|(key, _)| self.spec.same_file(key))
            .filter_map(|(_, clip)| clip.ready_at)
            .min()
    }

    /// How far its head played and the clip's length, once its first
    /// frame showed.
    pub(in crate::ui) fn progress(&self) -> Option<(Duration, Duration)> {
        let inner = self.player.inner.try_borrow().ok()?;
        let head = &inner.heads.get(&(self.head, self.spec.file.0))?.state;
        if !head.started() {
            return None;
        }
        let total = inner
            .clips
            .iter()
            .filter(|(key, _)| self.spec.same_file(key))
            .find_map(|(_, clip)| clip.timeline)?
            .duration();
        Some((head.pos(), total))
    }
}
