//! The emoji pattern behind the card, one frame at a time: the tiles of
//! [`quill::peer_qr::pattern`] for the time since the box opened, with
//! the swapped tiles shrinking out and popping in, painted from the
//! emoji stills on the frame clock. Reduced motion and power saving
//! freeze it on its first frame.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::PatternSwap;
use gpui_kit::*;
use quill::peer_qr::pattern::{
    ENTRY_STAGGER_MS, Falloff, LAYER_BAND, Metrics, POP_DELAY_MS, Rect, SWAP_MS, SWAP_STAGGER_MS,
    card_cull, covered, drift_offset, pick_index, pop_scale, shrink_scale, tiles,
};
use std::sync::Arc;
use std::time::Instant;

/// One emoji to paint, centered on `(x, y)` in the box.
pub(super) struct DrawnTile {
    x: f64,
    y: f64,
    size: f64,
    still: Arc<RenderImage>,
}

/// A frame of the pattern and whether the next one differs.
pub(super) struct PatternFrame {
    pub(super) tiles: Vec<DrawnTile>,
    pub(super) moving: bool,
}

/// What a frame is made of.
pub(super) struct PatternArgs<'a> {
    pub(super) picks: &'a [String],
    pub(super) swap: Option<&'a PatternSwap>,
    pub(super) opened: Instant,
    pub(super) now: Instant,
    pub(super) animate: bool,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) card: Rect,
    /// The buttons and an open panel. Their glass has no blur, so no
    /// tiles go under it.
    pub(super) glass: &'a [Rect],
}

fn ms(from: Instant, till: Instant) -> i64 {
    i64::try_from(till.saturating_duration_since(from).as_millis()).unwrap_or(i64::MAX)
}

impl QuillApp {
    /// The tiles of one frame (tdesktop `PeerQrEmojiField::paint`).
    pub(super) fn qr_pattern_frame(
        &mut self,
        args: PatternArgs<'_>,
        cx: &mut Context<Self>,
    ) -> PatternFrame {
        let previous: &[String] = args.swap.map_or(&[], |swap| &swap.previous);
        let count = args.picks.len().max(previous.len());
        if count == 0 {
            return PatternFrame {
                tiles: Vec::new(),
                moving: false,
            };
        }
        let elapsed = if args.animate {
            ms(args.opened, args.now)
        } else {
            0
        };
        let swap_elapsed = args
            .swap
            .filter(|_| args.animate)
            .map(|swap| ms(swap.started, args.now))
            .filter(|elapsed| *elapsed < SWAP_MS);
        let (dx, dy) = drift_offset(elapsed);
        let (cx0, cy0) = args.card.center();
        let falloff = Falloff {
            area: Rect::new(0., 0., args.width, args.height),
            band: LAYER_BAND,
        };
        let cull = card_cull(args.card);
        let layout = tiles(
            &falloff,
            (cx0 + dx, cy0 + dy),
            &Metrics::LIVE,
            count,
            elapsed as f64 / 1000.,
        );
        let mut out = Vec::new();
        for tile in layout {
            let half = tile.size / 2.;
            let under_glass = args.glass.iter().any(|glass| {
                tile.x + half > glass.x
                    && tile.x - half < glass.x + glass.width
                    && tile.y + half > glass.y
                    && tile.y - half < glass.y + glass.height
            });
            if under_glass || covered(&cull, tile.x, tile.y, tile.size) {
                continue;
            }
            let shown = |list: &[String]| {
                (!list.is_empty())
                    .then(|| list.get(pick_index(tile.row, tile.column, list.len())))
                    .flatten()
                    .cloned()
            };
            let new = shown(args.picks);
            let old = swap_elapsed.and_then(|_| shown(previous));
            let mut draw = |emoji: &str, scale: f64, out: &mut Vec<DrawnTile>| {
                if scale <= 0. {
                    return;
                }
                if let Some(still) = self.qr_emoji_still(emoji, cx) {
                    out.push(DrawnTile {
                        x: tile.x,
                        y: tile.y,
                        size: tile.size * scale,
                        still,
                    });
                }
            };
            let Some(elapsed) = swap_elapsed.filter(|_| old != new) else {
                if let Some(new) = &new {
                    draw(new, 1., &mut out);
                }
                continue;
            };
            let delay = |stagger: i64| (tile.delay * stagger as f64) as i64;
            if let Some(old) = &old {
                draw(old, shrink_scale(elapsed, delay(SWAP_STAGGER_MS)), &mut out);
            }
            if let Some(new) = &new {
                let start = if old.is_some() {
                    delay(SWAP_STAGGER_MS) + POP_DELAY_MS
                } else {
                    delay(ENTRY_STAGGER_MS)
                };
                draw(new, pop_scale(elapsed, start), &mut out);
            }
        }
        PatternFrame {
            tiles: out,
            moving: args.animate && (!args.picks.is_empty() || swap_elapsed.is_some()),
        }
    }
}

/// The frame as a canvas filling the box, clipped to it.
pub(super) fn pattern_canvas(frame: PatternFrame) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            for tile in frame.tiles {
                let half = tile.size / 2.;
                let at =
                    bounds.origin + point(px((tile.x - half) as f32), px((tile.y - half) as f32));
                let side = px(tile.size as f32);
                let _ = window.paint_image(
                    bounds,
                    Bounds::new(at, size(side, side)),
                    Corners::default(),
                    tile.still,
                    0,
                    false,
                );
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
