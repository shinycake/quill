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
//! expanded caption grows over the picture and scrolls with a scrollbar
//! when it outgrows the window. The caption is shaped at the plate's
//! width before anything draws ([`CaptionLayout`]), so the picture makes
//! room for the lines it shows; what it shows under the picture is capped
//! at a quarter of the window in whole lines, and scrolls past that.

use super::*;
use crate::ui::message_text::{RichTextSource, custom_emoji_paths, measure_rich_text};
use crate::ui::motion::Glide;
use gpui_kit::component::scroll::ScrollableElement as _;
use quill::lang::{keys, tr};
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
/// The plate's padding and the toggle row, in rem: the layout and the
/// drawn plate both use these.
const PLATE_PAD_X_REM: f32 = 0.75;
const PLATE_PAD_Y_REM: f32 = 0.5;
const TOGGLE_REM: f32 = 1.25;

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

/// The caption shaped for the viewer: what the picture makes room for and
/// what the plate draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CaptionLayout {
    pub(super) line_h: f32,
    pub(super) content_h: f32,
    /// Collapsed behind "Show more".
    pub(super) cut: bool,
    pub(super) shown_h: f32,
    /// The plate around it: padding, and the "Show more" row when cut.
    pub(super) plate_w: f32,
    pub(super) plate_h: f32,
}

/// Lay out text `content_h` tall in lines of `line_h`, whose widest line
/// is `text_w`, in a window
/// `viewport_h` tall (`rem`: the root font size the plate's padding and
/// toggle are sized in). tdesktop `refreshCaptionGeometry`: at most a
/// quarter of the window, in whole lines.
pub(super) fn caption_layout(
    content_h: f32,
    line_h: f32,
    text_w: f32,
    viewport_h: f32,
    rem: f32,
) -> CaptionLayout {
    let line_h = if line_h.is_finite() {
        line_h.max(0.)
    } else {
        0.
    };
    let content_h = if content_h.is_finite() {
        content_h.max(0.)
    } else {
        0.
    };
    let cut = overflows(content_h, line_h);
    let wanted = if cut {
        collapsed_height(line_h)
    } else {
        content_h
    };
    let quarter_lines = if line_h > 0. {
        (viewport_h / 4. / line_h).floor().max(1.)
    } else {
        0.
    };
    let shown_h = wanted.min(quarter_lines * line_h);
    let toggle_h = if cut { TOGGLE_REM * rem } else { 0. };
    CaptionLayout {
        line_h,
        content_h,
        cut,
        shown_h,
        plate_w: text_w.max(0.).ceil() + 2. * PLATE_PAD_X_REM * rem,
        plate_h: shown_h + 2. * PLATE_PAD_Y_REM * rem + toggle_h,
    }
}

/// The caption's expand state, reset whenever the viewer shows another
/// item.
pub(in crate::ui) struct ViewerCaption {
    /// `(chat, message)` of the item this state belongs to.
    key: Option<(i64, i64)>,
    /// 0 collapsed, 1 expanded.
    glide: Glide,
    scroll: ScrollHandle,
}

impl Default for ViewerCaption {
    fn default() -> Self {
        Self {
            key: None,
            glide: Glide::settled(0., Instant::now(), sine_in_out),
            scroll: ScrollHandle::new(),
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
        self.viewer_note_activity(cx);
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

    /// The caption of `item` laid out as the plate draws it
    /// (`rich_text_line` in the theme font at the message font size),
    /// wrapped at `max_w` less the plate's padding. `None` without a
    /// caption.
    pub(super) fn viewer_caption_layout(
        &self,
        item: &MediaViewerItem,
        max_w: f32,
        window: &mut Window,
        cx: &App,
    ) -> Option<CaptionLayout> {
        if item.caption.is_empty() {
            return None;
        }
        let size = self.msg_font();
        let rem = f32::from(window.rem_size());
        let mut style = window.text_style();
        style.font_family = cx.theme().font_family.clone();
        style.font_size = size.into();
        let line_h = f32::from(
            style
                .line_height
                .to_pixels(size.into(), window.rem_size())
                .round(),
        );
        let emoji_paths = self.viewer_caption_emoji(item);
        let source = RichTextSource {
            text: &item.caption,
            entities: &item.caption_entities,
            msg_key: (item.chat_id.0, item.message_id.0 as u64),
            is_caption: true,
            revealed: &self.message_ui.spoiler_revealed,
            emoji_paths: &emoji_paths,
        };
        let wrap = (max_w - 2. * PLATE_PAD_X_REM * rem).max(1.);
        let text = measure_rich_text(&source, &style, line_h, wrap, window);
        let viewport_h = f32::from(window.viewport_size().height);
        Some(caption_layout(
            text.height,
            line_h,
            text.width,
            viewport_h,
            rem,
        ))
    }

    /// The caption's custom emoji pictures, resolved like message text's.
    pub(super) fn viewer_caption_emoji(&self, item: &MediaViewerItem) -> HashMap<i64, ImageSource> {
        let roots = self.media_display_roots();
        self.session()
            .map(|session| {
                custom_emoji_paths(
                    &item.caption_entities,
                    Some(&session.stickers.emoji.custom_emoji_stickers),
                    &session.media.files,
                    &roots,
                )
            })
            .unwrap_or_default()
    }

    /// The caption plate for `item`: `text` at its shown height, or
    /// growing over the picture to its full height when expanded, with
    /// "Show more" / "Show less" when it is cut.
    pub(super) fn viewer_caption_plate(
        &mut self,
        item: &MediaViewerItem,
        text: AnyElement,
        layout: CaptionLayout,
        viewport_h: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = (item.chat_id.0, item.message_id.0);
        if self.viewer.extra.caption.key != Some(key) {
            self.viewer.extra.caption.reset(key);
        }
        let room = (viewport_h - VIEWER_TOP_BAR - EXPANDED_BOTTOM_ROOM).max(layout.shown_h);
        let now = Instant::now();
        let caption = &self.viewer.extra.caption;
        let shown = caption.glide.value(now).clamp(0., 1.);
        let animating = caption.glide.animating(now);
        let expanded = caption.expanded();
        let full_h = expanded_height(layout.content_h, layout.shown_h, room);
        let max_h = layout.shown_h + (full_h - layout.shown_h) * shown;
        let scroll = caption.scroll.clone();
        if animating {
            self.request_animation_tick(60, cx);
        }
        let clipped = animating || (layout.cut && !expanded);
        let row_id = item.message_id.0 as u64;
        let text_box = div()
            .id(("media-viewer-caption-text", row_id))
            .track_scroll(&scroll)
            .max_h(px(max_h))
            .line_height(px(layout.line_h))
            .when(!clipped, |this| {
                this.overflow_y_scroll().vertical_scrollbar(&scroll)
            })
            .when(clipped, |this| this.overflow_hidden())
            .child(text);
        let toggle = layout.cut.then(|| {
            div().h(rems(TOGGLE_REM)).flex().justify_end().child(
                Button::new(("media-viewer-caption-toggle", row_id))
                    .label(if expanded {
                        tr(keys::Q_AV_SHOW_LESS)
                    } else {
                        tr(keys::Q_AV_SHOW_MORE)
                    })
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
            .w(px(layout.plate_w))
            .max_w_full()
            .px(rems(PLATE_PAD_X_REM))
            .py(rems(PLATE_PAD_Y_REM))
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .child(text_box)
            .children(toggle)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaptionLayout, caption_layout, collapsed_height, expanded_height, overflows, sine_in_out,
    };

    #[test]
    fn a_two_line_caption_shows_both_lines_in_its_plate() {
        let layout = caption_layout(2. * 22., 22., 300.4, 1000., 16.);
        assert_eq!(
            layout,
            CaptionLayout {
                line_h: 22.,
                content_h: 44.,
                cut: false,
                shown_h: 44.,
                plate_w: 301. + 24.,
                plate_h: 44. + 16.,
            }
        );
    }

    #[test]
    fn a_long_caption_shows_two_lines_and_its_toggle() {
        let layout = caption_layout(9. * 22., 22., 600., 1000., 16.);
        assert!(layout.cut);
        assert_eq!(layout.content_h, 9. * 22.);
        assert_eq!(layout.shown_h, 44.);
        assert_eq!(layout.plate_h, 44. + 16. + 20.);
    }

    #[test]
    fn the_shown_caption_is_capped_at_a_quarter_of_the_window_in_whole_lines() {
        let layout = caption_layout(4. * 22., 22., 600., 300., 16.);
        assert!(!layout.cut);
        assert_eq!(layout.shown_h, 66.);
        assert_eq!(caption_layout(2. * 22., 22., 600., 40., 16.).shown_h, 22.);
    }

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
