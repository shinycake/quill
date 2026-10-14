//! Long captions in the media viewer: collapsed to two lines, with
//! "Show more" to expand them in place.
//!
//! References (tdesktop, read-only):
//! - `media/view/media_view_overlay_widget.cpp` `refreshCaptionGeometry`:
//!   a media caption shows at most `_maxUsedHeight / 4`, rounded down to
//!   whole lines of `st::mediaviewCaptionStyle`, and elides the rest.
//! - The same function and `paintCaption` for stories: a cut caption gets
//!   a semibold "Show more" at its bottom right
//!   (`st::storiesShowMoreFont`), and clicking it opens
//!   `media/stories/media_stories_caption_full_view.cpp`, which grows the
//!   caption upward to its full height (scrolling past the window) over
//!   `st::fadeWrapDuration` (200 ms) with `anim::sineInOut`; Escape
//!   collapses it again.
//!
//! The viewer follows the stories caption: more than four lines collapse
//! to two (`kMaxShownCaptionLines`, `kCollapsedCaptionLines`), and the
//! expanded caption scrolls with a scrollbar when it outgrows the window. Heights come from the caption's own layout
//! (its scroll handle), measured one frame late and re-rendered until
//! stable, so wrapping, quotes and custom emoji are all accounted for.

use super::*;
use crate::ui::motion::Glide;
use gpui_kit::component::scroll::ScrollableElement as _;
use std::time::Instant;

/// `st::fadeWrapDuration`.
const EXPAND_DURATION: Duration = Duration::from_millis(200);
/// `Stories::kCollapsedCaptionLines`.
const COLLAPSED_LINES: f32 = 2.;
/// `Stories::kMaxShownCaptionLines`: a caption up to this many lines shows
/// in full.
const MAX_SHOWN_LINES: f32 = 4.;
/// Room kept under an expanded caption for the player panel or zoom
/// controls, plus the "Show less" row.
const EXPANDED_BOTTOM_ROOM: f32 = 120.0;

/// `anim::sineInOut`.
fn sine_in_out(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    -((std::f32::consts::PI * t).cos() - 1.) / 2.
}

/// Height of a collapsed caption: two lines.
pub(super) fn collapsed_height(line_h: f32) -> f32 {
    if line_h <= 0. || !line_h.is_finite() {
        return 0.;
    }
    COLLAPSED_LINES * line_h
}

/// Height of an expanded caption: all of it when it fits in `room`,
/// otherwise `room` (it scrolls), never less than the collapsed height.
pub(super) fn expanded_height(content_h: f32, collapsed_h: f32, room: f32) -> f32 {
    content_h.min(room).max(collapsed_h)
}

/// Whether the caption collapses: it runs past four lines (half a pixel
/// of slack for layout rounding). Shorter captions show in full.
pub(super) fn overflows(content_h: f32, line_h: f32) -> bool {
    line_h > 0. && content_h > MAX_SHOWN_LINES * line_h + 0.5
}

/// The caption's expand state, reset whenever the viewer shows another
/// item.
pub(in crate::ui) struct ViewerCaption {
    /// `(chat, message)` of the item this state belongs to.
    key: Option<(i64, i64)>,
    /// 0 collapsed, 1 expanded.
    glide: Glide,
    /// Tracks the caption text box: its laid-out bounds and how far its
    /// content runs past them give the full caption height.
    scroll: ScrollHandle,
    /// The content height the last render used.
    measured: f32,
    /// Not laid out yet for this item: the handle still holds the
    /// previous item's layout.
    fresh: bool,
}

impl Default for ViewerCaption {
    fn default() -> Self {
        Self {
            key: None,
            glide: Glide::settled(0., Instant::now(), sine_in_out),
            scroll: ScrollHandle::new(),
            measured: 0.,
            fresh: true,
        }
    }
}

impl ViewerCaption {
    fn expanded(&self) -> bool {
        self.glide.target() > 0.5
    }

    fn reset(&mut self, key: (i64, i64)) {
        self.key = Some(key);
        self.glide = Glide::settled(0., Instant::now(), sine_in_out);
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.measured = 0.;
        self.fresh = true;
    }

    /// Full height of the caption text from the last layout.
    fn content_height(&self) -> f32 {
        let bounds = self.scroll.bounds();
        f32::from(bounds.size.height + self.scroll.max_offset().y)
    }
}

impl QuillApp {
    /// Expand or collapse the viewer caption.
    pub(in crate::ui) fn toggle_viewer_caption(&mut self, cx: &mut Context<Self>) {
        let animate = !cx.reduce_motion();
        let caption = &mut self.viewer.extra.caption;
        let to = if caption.expanded() { 0. } else { 1. };
        caption
            .glide
            .go(to, EXPAND_DURATION, animate, Instant::now());
        // Either way the text starts from its first line.
        caption.scroll.set_offset(point(px(0.), px(0.)));
        self.viewer_note_activity(false, cx);
        cx.notify();
    }

    /// Escape collapses an expanded caption before it closes the viewer.
    pub(in crate::ui) fn collapse_viewer_caption_on_escape(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.viewer.extra.caption.expanded() {
            return false;
        }
        self.toggle_viewer_caption(cx);
        true
    }

    /// The caption plate for `item`: `text` clipped to its collapsed or
    /// animating height, with "Show more" / "Show less" when it is cut.
    pub(super) fn viewer_caption_plate(
        &mut self,
        item: &MediaViewerItem,
        text: AnyElement,
        plate_w: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = (item.chat_id.0, item.message_id.0);
        if self.viewer.extra.caption.key != Some(key) {
            self.viewer.extra.caption.reset(key);
        }
        let font = self.msg_font();
        let line_h = f32::from(
            window
                .text_style()
                .line_height
                .to_pixels(font.into(), window.rem_size())
                .round(),
        );
        let viewport_h = f32::from(window.viewport_size().height);
        let collapsed_h = collapsed_height(line_h);
        let room = (viewport_h - VIEWER_TOP_BAR - EXPANDED_BOTTOM_ROOM).max(collapsed_h);

        let now = Instant::now();
        let caption = &mut self.viewer.extra.caption;
        // Right after a reset the handle still describes the previous
        // item: show this one collapsed, untoggled, until it is laid out.
        let fresh = std::mem::take(&mut caption.fresh);
        let content_h = if fresh { 0. } else { caption.content_height() };
        // The text box was laid out at another width or for another item:
        // draw again so the toggle and heights follow the new layout.
        let remeasure = fresh || (content_h - caption.measured).abs() > 0.5;
        caption.measured = content_h;
        let cut = overflows(content_h, line_h);
        let shown = caption.glide.value(now).clamp(0., 1.);
        let animating = caption.glide.animating(now);
        let expanded = caption.expanded();
        let full_h = expanded_height(content_h, collapsed_h, room);
        let max_h = if cut {
            collapsed_h + (full_h - collapsed_h) * shown
        } else {
            // Up to four lines show in full.
            MAX_SHOWN_LINES * line_h
        };
        let scroll = caption.scroll.clone();
        if remeasure || animating {
            self.request_animation_tick(60, cx);
        }

        let row_id = item.message_id.0 as u64;
        let text_box = div()
            .id(("media-viewer-caption-text", row_id))
            .track_scroll(&scroll)
            .max_h(px(max_h))
            .line_height(px(line_h))
            .when(expanded && !animating, |this| {
                this.overflow_y_scroll().vertical_scrollbar(&scroll)
            })
            .when(!expanded || animating, |this| this.overflow_hidden())
            .child(text);
        let toggle = cut.then(|| {
            div().flex().justify_end().child(
                Button::new(("media-viewer-caption-toggle", row_id))
                    .label(if expanded { "Show less" } else { "Show more" })
                    .ghost()
                    .xsmall()
                    .text_color(gpui_kit::white())
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_viewer_caption(cx);
                    })),
            )
        });
        div()
            .id(("media-viewer-caption", row_id))
            .w(px(plate_w))
            .max_w_full()
            .px_3()
            .py_2()
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .child(text_box)
            .children(toggle)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{collapsed_height, expanded_height, overflows, sine_in_out};

    #[test]
    fn collapsed_caption_is_two_lines() {
        assert_eq!(collapsed_height(20.), 40.);
        // No line height (not laid out): nothing to clip to.
        assert_eq!(collapsed_height(0.), 0.);
        assert_eq!(collapsed_height(f32::NAN), 0.);
    }

    #[test]
    fn expanded_caption_fits_its_content_or_the_room() {
        assert_eq!(expanded_height(300., 40., 500.), 300.);
        assert_eq!(expanded_height(900., 40., 500.), 500.);
        // A cramped window never shrinks below the collapsed height.
        assert_eq!(expanded_height(900., 40., 20.), 40.);
    }

    #[test]
    fn only_a_caption_past_four_lines_collapses() {
        assert!(!overflows(80., 20.));
        assert!(!overflows(80.4, 20.));
        assert!(overflows(100., 20.));
        assert!(!overflows(100., 0.));
    }

    #[test]
    fn sine_in_out_runs_from_zero_to_one() {
        assert_eq!(sine_in_out(0.), 0.);
        assert!((sine_in_out(0.5) - 0.5).abs() < 1e-6);
        assert!((sine_in_out(1.) - 1.).abs() < 1e-6);
        assert_eq!(sine_in_out(2.), sine_in_out(1.));
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "caption_ui_tests.rs"]
mod ui_tests;
