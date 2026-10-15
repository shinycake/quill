//! `VideoScrubber`: the viewer's seek bar, composed from the kit's base
//! slider parts (`gpui_kit::base::{Slider, SliderTrack, SliderIndicator,
//! SliderThumb}`) so the hit testing, drag and accessibility stay the
//! kit's. On top of them it draws what the packaged `Slider` has no room
//! for: the buffered ranges under the fill, a 3 px line that grows to 5 px
//! under the pointer, a thumb that appears on hover and grows while
//! dragged, and a time bubble at the pointer.
//!
//! The `SliderState` keeps its meaning: `Change` previews the time label,
//! `Release` seeks (`QuillApp::on_viewer_seek_event`). The pure helpers
//! below (fractions, labels) are what the tests cover.
#![deny(clippy::indexing_slicing)]

use super::metrics::{
    BUBBLE_GAP, SCRUBBER_ROW, THUMB, THUMB_DRAG, THUMB_FADE_MS, TRACK, TRACK_HOVER,
};
use crate::ui::chat_theme::{accent, text_on_fill};
use gpui_kit::base::{
    Slider as BaseSlider, SliderIndicator, SliderThumb, SliderTrack, Transition, transition,
};
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::{ActiveTheme, StyledExt as _};
use gpui_kit::*;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

/// Where the pointer is over the scrubber, as a fraction of its width:
/// `Some` while it hovers, `None` when it leaves.
pub(crate) type HoverHandler = Rc<dyn Fn(Option<f32>, &mut Window, &mut App)>;

/// `value` of `total` as a 0–1 fraction (0 while the length is unknown).
pub(crate) fn fraction(value: f64, total: f64) -> f32 {
    if total.is_nan() || total <= 0.0 || !value.is_finite() {
        return 0.0;
    }
    (value / total).clamp(0.0, 1.0) as f32
}

/// The buffered ranges as `(start, end)` fractions of `total`, clipped to
/// the clip and without empty spans.
pub(crate) fn buffered_spans(buffered: &[Range<f64>], total: f64) -> Vec<(f32, f32)> {
    buffered
        .iter()
        .map(|range| (fraction(range.start, total), fraction(range.end, total)))
        .filter(|(start, end)| end > start)
        .collect()
}

/// The pointer's fraction along a track that starts at `left` and is
/// `width` wide; `None` before the track has been laid out. Snapped to
/// whole pixels, so a move inside the same pixel reports the same value
/// (and the hover handler skips the redraw).
pub(crate) fn pointer_fraction(x: Pixels, left: Pixels, width: Pixels) -> Option<f32> {
    let width = f32::from(width);
    if width.is_nan() || width <= 0.0 {
        return None;
    }
    let pixel = (f32::from(x) - f32::from(left)).clamp(0.0, width).round();
    Some((pixel / width).clamp(0.0, 1.0))
}

/// The elapsed label (tdesktop `FormatDurationText` of the played part).
pub(crate) fn elapsed_label(position: f64) -> String {
    quill::message_menu::timecode_text(if position.is_finite() { position } else { 0.0 })
}

/// The remaining label: U+2212 and the whole seconds left, so elapsed and
/// remaining always add up to the length (`PlaybackControls::
/// updatePlayback`: `_toPlayLeft = duration - playedAlready`).
pub(crate) fn remaining_label(position: f64, total: f64) -> String {
    let whole = |secs: f64| {
        if secs.is_finite() {
            secs.max(0.0).floor()
        } else {
            0.0
        }
    };
    let left = (whole(total) - whole(position)).max(0.0);
    format!("\u{2212}{}", quill::message_menu::timecode_text(left))
}

/// What the scrubber's time bubble says at `fraction` of `total`.
pub(crate) fn bubble_label(fraction: f32, total: f64) -> String {
    let fraction = if fraction.is_finite() {
        f64::from(fraction.clamp(0.0, 1.0))
    } else {
        0.0
    };
    elapsed_label(total.max(0.0) * fraction)
}

/// The viewer's seek bar (see the module docs).
#[derive(IntoElement)]
pub(crate) struct VideoScrubber {
    id: ElementId,
    state: Entity<SliderState>,
    total: f64,
    buffered: Vec<(f32, f32)>,
    hover: Option<f32>,
    preview: Option<f64>,
    on_hover: HoverHandler,
}

impl VideoScrubber {
    /// `state` is the seek slider (seconds, `0..=total`).
    pub(crate) fn new(
        id: impl Into<ElementId>,
        state: &Entity<SliderState>,
        total: f64,
        on_hover: HoverHandler,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            total,
            buffered: Vec::new(),
            hover: None,
            preview: None,
            on_hover,
        }
    }

    /// The time ranges the player has data for, in seconds.
    pub(crate) fn buffered(mut self, buffered: &[Range<f64>]) -> Self {
        self.buffered = buffered_spans(buffered, self.total);
        self
    }

    /// The pointer's fraction while it hovers (last reported through
    /// `on_hover`).
    pub(crate) fn hover(mut self, hover: Option<f32>) -> Self {
        self.hover = hover;
        self
    }

    /// The dragged position while the thumb is held (seconds).
    pub(crate) fn preview(mut self, preview: Option<f64>) -> Self {
        self.preview = preview;
        self
    }
}

impl RenderOnce for VideoScrubber {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);
        // While dragging the fill follows the drag preview.
        let percentage = match self.preview {
            Some(secs) => fraction(secs, self.total),
            None => state.percentage().end.clamp(0.0, 1.0),
        };
        let bounds = state.bounds();
        let dragging = self.preview.is_some();
        let active = dragging || self.hover.is_some();
        let fade = || Transition::new(Duration::from_millis(THUMB_FADE_MS));
        let id = self.id.clone();
        let track = transition(
            (id.clone(), "track"),
            if active { TRACK_HOVER } else { TRACK },
            fade(),
            window,
            cx,
        );
        let thumb_opacity = transition(
            (id.clone(), "thumb-opacity"),
            if active { 1.0_f32 } else { 0.0 },
            fade(),
            window,
            cx,
        );
        let thumb = transition(
            (id.clone(), "thumb-size"),
            if dragging { THUMB_DRAG } else { THUMB },
            fade(),
            window,
            cx,
        );
        // The bubble follows the drag, else the pointer.
        let bubble_at = self
            .preview
            .map(|secs| fraction(secs, self.total))
            .or(self.hover);
        let bubble = bubble_at.filter(|_| self.total > 0.0).map(|at| {
            let label = bubble_label(at, self.total);
            let theme = cx.theme();
            // Deferred so it paints above the picture layer; laid out at
            // the pointer in a zero-width box that centers it. Styled as
            // the kit `Tooltip` view (which is a view, built per show).
            deferred(
                div()
                    .absolute()
                    .left(relative(at))
                    .bottom(SCRUBBER_ROW / 2. + BUBBLE_GAP)
                    .w(px(0.))
                    .flex()
                    .justify_center()
                    .child(
                        gpui_kit::base::Tooltip::new((id.clone(), "bubble"))
                            .flex_none()
                            .px_2()
                            .py_0p5()
                            .rounded(theme.radius)
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.popover)
                            .text_color(theme.popover_foreground)
                            .shadow_md()
                            .text_xs()
                            .font_semibold()
                            .child(label),
                    ),
            )
            .with_priority(1)
        });
        let on_move = self.on_hover.clone();
        let on_leave = self.on_hover;
        let line = gpui_kit::white();
        div()
            .id(self.id)
            .relative()
            .flex_1()
            .min_w_0()
            .h(SCRUBBER_ROW)
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                if let Some(at) =
                    pointer_fraction(event.position.x, bounds.left(), bounds.size.width)
                {
                    on_move(Some(at), window, cx);
                }
            })
            .on_hover(move |hovered, window, cx| {
                if !*hovered {
                    on_leave(None, window, cx);
                }
            })
            .child(
                BaseSlider::new(&self.state)
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(
                        SliderTrack::new(&self.state)
                            .w_full()
                            .h_full()
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .child(
                                SliderIndicator::new(&self.state)
                                    .relative()
                                    .w_full()
                                    .h(track)
                                    .rounded_full()
                                    .bg(line.opacity(0.2))
                                    .children(self.buffered.iter().map(|(start, end)| {
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left(relative(*start))
                                            .w(relative(end - start))
                                            .rounded_full()
                                            .bg(line.opacity(0.35))
                                    }))
                                    .child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .bottom_0()
                                            .left_0()
                                            .w(relative(percentage))
                                            .rounded_full()
                                            .bg(accent()),
                                    )
                                    .child(
                                        SliderThumb::new(&self.state)
                                            .absolute()
                                            .top((track - thumb) / 2.)
                                            .left(relative(percentage))
                                            .ml(-thumb / 2.)
                                            .size(thumb)
                                            .rounded_full()
                                            .bg(text_on_fill())
                                            .shadow_sm()
                                            .opacity(thumb_opacity),
                                    ),
                            ),
                    ),
            )
            .children(bubble)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bubble_label, buffered_spans, elapsed_label, fraction, pointer_fraction, remaining_label,
    };
    use gpui_kit::px;

    #[test]
    fn fractions_stay_inside_the_clip() {
        assert_eq!(fraction(5.0, 10.0), 0.5);
        assert_eq!(fraction(-1.0, 10.0), 0.0);
        assert_eq!(fraction(12.0, 10.0), 1.0);
        // Unknown or broken lengths draw an empty bar.
        assert_eq!(fraction(5.0, 0.0), 0.0);
        assert_eq!(fraction(5.0, f64::NAN), 0.0);
        assert_eq!(fraction(f64::INFINITY, 10.0), 0.0);
    }

    #[test]
    fn buffered_ranges_become_clipped_spans() {
        let spans = buffered_spans(&[0.0..2.5, 4.0..4.0, 8.0..14.0, -3.0..-1.0], 10.0);
        assert_eq!(spans, vec![(0.0, 0.25), (0.8, 1.0)]);
        assert!(buffered_spans(&[0.0..5.0], 0.0).is_empty());
    }

    #[test]
    fn pointer_maps_onto_the_track() {
        let at = |x: f32| pointer_fraction(px(x), px(100.), px(200.));
        assert_eq!(at(100.), Some(0.0));
        assert_eq!(at(200.), Some(0.5));
        assert_eq!(at(350.), Some(1.0));
        assert_eq!(at(20.), Some(0.0));
        // Moves inside one pixel report the same value (no redraw).
        assert_eq!(at(150.2), at(149.8));
        assert_eq!(at(150.2), Some(0.25));
        assert_ne!(at(150.2), at(151.0));
        // Not laid out yet.
        assert_eq!(pointer_fraction(px(10.), px(0.), px(0.)), None);
    }

    #[test]
    fn labels_add_up_to_the_length() {
        assert_eq!(elapsed_label(0.0), "0:00");
        assert_eq!(elapsed_label(7.9), "0:07");
        assert_eq!(elapsed_label(f64::NAN), "0:00");
        assert_eq!(remaining_label(7.9, 12.4), "\u{2212}0:05");
        assert_eq!(remaining_label(0.0, 3725.0), "\u{2212}1:02:05");
        // Past the end, or an unknown length: nothing left.
        assert_eq!(remaining_label(20.0, 12.0), "\u{2212}0:00");
        assert_eq!(remaining_label(f64::NAN, f64::NAN), "\u{2212}0:00");
    }

    #[test]
    fn bubble_names_the_time_under_the_pointer() {
        assert_eq!(bubble_label(0.5, 120.0), "1:00");
        assert_eq!(bubble_label(1.5, 120.0), "2:00");
        assert_eq!(bubble_label(f32::NAN, 120.0), "0:00");
    }
}
