//! `AudioScrubber`: the seek control of the active voice note, music row
//! and player bar, composed from the kit's base slider parts
//! (`gpui_kit::base::{Slider, SliderTrack, SliderIndicator}`) so hit
//! testing, drag, touch and accessibility stay the kit's. The face is
//! painted on a canvas with every x edge on a device pixel.
//!
//! A voice note has no separate bar: its waveform is the scrubber, as in
//! Telegram Desktop (`HistoryView::Document`, `PaintWaveform`), with the bar
//! under the progress edge split exactly. Music seeks from a thin line with
//! a small knob that shows on hover (`Media::View::PlaybackProgress` /
//! `MediaSlider`).
//!
//! The `SliderState` keeps its meaning: `Change` previews the time label,
//! `Release` seeks (`QuillApp::on_seek_event`). The pure helpers below
//! (pointer fraction, pixel edges, painted zones) are what the tests cover.
#![deny(clippy::indexing_slicing)]

use crate::ui::history::bubble_width::{WAVEFORM_HEIGHT, waveform_bars};
use gpui_kit::base::{Slider as BaseSlider, SliderIndicator, SliderTrack, Transition, transition};
use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::bubble_layout::WAVEFORM_BAR;
use std::rc::Rc;
use std::time::Duration;

/// Where the pointer is over the scrubber, as a fraction of its width:
/// `Some` while it hovers, `None` when it leaves.
pub(in crate::ui) type HoverHandler = Rc<dyn Fn(Option<f32>, &mut Window, &mut App)>;

/// Opacity of the unplayed waveform (the played part is the full color).
const WAVEFORM_UNPLAYED: f32 = 0.35;
/// Opacity of the line's unplayed track.
const LINE_UNPLAYED: f32 = 0.25;
/// How much of the full color the hovered stretch gains over the unplayed
/// tone. tdesktop uses 30%; at that the stretch barely shows on a dark
/// bubble.
const HOVER_TINT: f32 = 0.5;
/// The line face's row: room for the dragged knob and a comfortable
/// pointer target around the thin track.
const LINE_ROW: f32 = 20.;
const LINE_TRACK: f32 = 3.;
const KNOB: f32 = 10.;
const KNOB_DRAG: f32 = 12.;
const KNOB_FADE_MS: u64 = 120;

/// What the scrubber draws.
pub(in crate::ui) enum ScrubberFace {
    /// A voice note's waveform (5-bit peaks), which doubles as the bar.
    Waveform(Vec<u8>),
    /// A thin line with a knob, for music.
    Line,
}

/// One stretch of the face, painted at `alpha` of the color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Zone {
    pub(super) start: f32,
    pub(super) end: f32,
    pub(super) alpha: f32,
}

/// `value` rounded to the nearest device pixel at `scale`.
pub(super) fn snap(value: f32, scale: f32) -> f32 {
    if scale.is_finite() && scale > 0. {
        (value * scale).round() / scale
    } else {
        value.round()
    }
}

/// The x of `fraction` along `width`, on a device pixel and inside it.
pub(super) fn edge(width: f32, scale: f32, fraction: f32) -> f32 {
    let width = if width.is_finite() { width.max(0.) } else { 0. };
    let fraction = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    snap(width * fraction, scale).clamp(0., width)
}

/// The stretches of a face `width` wide: played to the progress edge,
/// then unplayed. A hover paints the stretch between progress and pointer
/// one tone, whichever side the pointer is on, so it reads as the jump a
/// click would make; the edge at the pointer stays as crisp as the
/// progress edge. Empty stretches are left out.
pub(super) fn zones(
    width: f32,
    scale: f32,
    progress: f32,
    hover: Option<f32>,
    unplayed: f32,
) -> Vec<Zone> {
    let width = if width.is_finite() { width.max(0.) } else { 0. };
    let active = edge(width, scale, progress);
    let (low, high) = match hover.map(|at| edge(width, scale, at)) {
        Some(pointer) => (active.min(pointer), active.max(pointer)),
        None => (active, active),
    };
    let tint = unplayed + HOVER_TINT * (1. - unplayed);
    [
        Zone {
            start: 0.,
            end: low,
            alpha: 1.,
        },
        Zone {
            start: low,
            end: high,
            alpha: tint,
        },
        Zone {
            start: high,
            end: width,
            alpha: unplayed,
        },
    ]
    .into_iter()
    .filter(|zone| zone.end > zone.start)
    .collect()
}

/// The left edge of a `diameter` knob centered on `active_x`, kept inside
/// `width` and on a device pixel.
pub(super) fn knob_left(active_x: f32, width: f32, diameter: f32, scale: f32) -> f32 {
    let radius = diameter / 2.;
    let center = active_x.clamp(radius, (width - radius).max(radius));
    snap(center - radius, scale)
}

/// The pointer's fraction along a track that starts at `left` and is
/// `width` wide; `None` before the track has been laid out. Snapped to
/// whole pixels, so a move inside the same pixel reports the same value
/// (and the hover handler skips the redraw).
pub(super) fn pointer_fraction(x: Pixels, left: Pixels, width: Pixels) -> Option<f32> {
    let width = f32::from(width);
    if width.is_nan() || width <= 0.0 {
        return None;
    }
    let pixel = (f32::from(x) - f32::from(left)).clamp(0.0, width).round();
    Some((pixel / width).clamp(0.0, 1.0))
}

/// Paints `zones` (relative to `left`) with `paint` once per zone, clipped
/// to it horizontally only, so a shape across an edge splits exactly there.
/// The tallest waveform bars reach past the row's bottom (as in tdesktop),
/// so the clip keeps the enclosing mask's height.
fn paint_zones(
    zones: &[Zone],
    left: f32,
    window: &mut Window,
    mut paint: impl FnMut(f32, &mut Window),
) {
    let outer = window.content_mask().bounds;
    for zone in zones {
        let mask = Bounds::new(
            point(px(left + zone.start), outer.origin.y),
            size(px(zone.end - zone.start), outer.size.height),
        );
        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
            paint(zone.alpha, window);
        });
    }
}

/// A voice note's waveform as Telegram Desktop paints it (`PaintWaveform`):
/// `msgWaveformBar` bars `msgWaveformSkip` apart, laid out over the width
/// (`bubble_width::waveform_bars`), as pills.
fn paint_waveform(
    bars: &[u8],
    bounds: Bounds<Pixels>,
    color: Hsla,
    progress: f32,
    hover: Option<f32>,
    window: &mut Window,
) {
    let scale = window.scale_factor();
    let left = snap(f32::from(bounds.origin.x), scale);
    let top = f32::from(bounds.origin.y);
    let width = f32::from(bounds.size.width);
    let shapes = waveform_bars(bars, width.floor() as i32);
    let radius = px(WAVEFORM_BAR as f32 / 2.);
    let zones = zones(width, scale, progress, hover, WAVEFORM_UNPLAYED);
    paint_zones(&zones, left, window, |alpha, window| {
        let fill_color = color.opacity(alpha);
        for bar in &shapes {
            window.paint_quad(
                fill(
                    Bounds::new(
                        point(px(left + bar.left as f32), px(snap(top + bar.top, scale))),
                        size(px(WAVEFORM_BAR as f32), px(bar.height as f32)),
                    ),
                    fill_color,
                )
                .corner_radii(radius),
            );
        }
    });
}

/// The music line: a pill track, played in the full color, and the knob
/// (`diameter`, `opacity`) on the progress edge.
fn paint_line(
    bounds: Bounds<Pixels>,
    color: Hsla,
    progress: f32,
    hover: Option<f32>,
    knob: (f32, f32),
    window: &mut Window,
) {
    let scale = window.scale_factor();
    let left = snap(f32::from(bounds.origin.x), scale);
    let top = f32::from(bounds.origin.y);
    let width = snap(f32::from(bounds.size.width), scale);
    let height = f32::from(bounds.size.height);
    let track = Bounds::new(
        point(px(left), px(snap(top + (height - LINE_TRACK) / 2., scale))),
        size(px(width), px(LINE_TRACK)),
    );
    let zones = zones(width, scale, progress, hover, LINE_UNPLAYED);
    paint_zones(&zones, left, window, |alpha, window| {
        window.paint_quad(fill(track, color.opacity(alpha)).corner_radii(px(LINE_TRACK / 2.)));
    });
    let (diameter, opacity) = knob;
    if opacity <= 0. {
        return;
    }
    let x = left + knob_left(edge(width, scale, progress), width, diameter, scale);
    let y = snap(top + (height - diameter) / 2., scale);
    window.paint_quad(
        fill(
            Bounds::new(point(px(x), px(y)), size(px(diameter), px(diameter))),
            color.opacity(opacity),
        )
        .corner_radii(px(diameter / 2.)),
    );
}

/// A voice note's waveform outside the active row: the same paint as the
/// scrubber's face, without the slider. `progress` (0..=1) is the played
/// part.
pub(super) fn waveform_canvas(bars: Vec<u8>, color: Hsla, progress: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_waveform(&bars, bounds, color, progress, None, window),
    )
    .w_full()
    .h(px(WAVEFORM_HEIGHT as f32))
}

/// The active audio row's (or the player bar's) seek control (see the
/// module docs).
#[derive(IntoElement)]
pub(in crate::ui) struct AudioScrubber {
    id: ElementId,
    state: Entity<SliderState>,
    face: ScrubberFace,
    color: Hsla,
    progress: f32,
    hover: Option<f32>,
    dragging: bool,
    on_hover: HoverHandler,
}

impl AudioScrubber {
    /// `state` is the seek slider (seconds); `on_hover` hears where the
    /// pointer is.
    pub(in crate::ui) fn new(
        id: impl Into<ElementId>,
        state: &Entity<SliderState>,
        face: ScrubberFace,
        color: impl Into<Hsla>,
        on_hover: HoverHandler,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            face,
            color: color.into(),
            progress: 0.,
            hover: None,
            dragging: false,
            on_hover,
        }
    }

    /// The played fraction, 0..=1, already following a drag.
    pub(in crate::ui) fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }

    /// The pointer's fraction while it hovers.
    pub(in crate::ui) fn hover(mut self, hover: Option<f32>) -> Self {
        self.hover = hover;
        self
    }

    /// Whether the slider is being dragged.
    pub(in crate::ui) fn dragging(mut self, dragging: bool) -> Self {
        self.dragging = dragging;
        self
    }
}

impl RenderOnce for AudioScrubber {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // While dragging, the progress is the drag; a hover tint would
        // only repeat it.
        let hover = self.hover.filter(|_| !self.dragging);
        let (color, progress) = (self.color, self.progress);
        let (height, face): (f32, Box<dyn Fn(Bounds<Pixels>, &mut Window)>) = match self.face {
            ScrubberFace::Waveform(bars) => (
                WAVEFORM_HEIGHT as f32,
                Box::new(move |bounds, window| {
                    paint_waveform(&bars, bounds, color, progress, hover, window)
                }),
            ),
            ScrubberFace::Line => {
                let fade = || Transition::new(Duration::from_millis(KNOB_FADE_MS));
                let shown = self.dragging || hover.is_some();
                let opacity = transition(
                    (self.id.clone(), "knob-opacity"),
                    if shown { 1.0_f32 } else { 0. },
                    fade(),
                    window,
                    cx,
                );
                let diameter = transition(
                    (self.id.clone(), "knob-size"),
                    if self.dragging { KNOB_DRAG } else { KNOB },
                    fade(),
                    window,
                    cx,
                );
                (
                    LINE_ROW,
                    Box::new(move |bounds, window| {
                        paint_line(bounds, color, progress, hover, (diameter, opacity), window)
                    }),
                )
            }
        };
        let on_move = self.on_hover.clone();
        let on_leave = self.on_hover;
        let state = self.state.clone();
        div()
            .id(self.id)
            .relative()
            .w_full()
            .h(px(height))
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                // This frame's track, not the one at render time.
                let bounds = state.read(cx).bounds();
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
                BaseSlider::new(&self.state).size_full().child(
                    SliderTrack::new(&self.state)
                        .size_full()
                        .cursor_pointer()
                        .child(
                            SliderIndicator::new(&self.state).size_full().child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, _| face(bounds, window),
                                )
                                .size_full(),
                            ),
                        ),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Zone, edge, knob_left, pointer_fraction, snap, zones};
    use gpui_kit::px;

    #[test]
    fn edges_land_on_device_pixels() {
        assert_eq!(snap(10.3, 1.), 10.);
        assert_eq!(snap(10.3, 2.), 10.5);
        assert_eq!(snap(10.2, 2.), 10.);
        assert_eq!(edge(101., 1., 0.5), 51.);
        assert_eq!(edge(101., 2., 0.5), 50.5);
        // Out of range or broken fractions stay inside the face.
        assert_eq!(edge(100., 1., 1.7), 100.);
        assert_eq!(edge(100., 1., -0.2), 0.);
        assert_eq!(edge(100., 1., f32::NAN), 0.);
        assert_eq!(edge(f32::NAN, 1., 0.5), 0.);
    }

    #[test]
    fn progress_splits_the_face_at_one_pixel() {
        let zone = |start, end, alpha| Zone { start, end, alpha };
        assert_eq!(
            zones(120., 1., 0.4, None, 0.35),
            vec![zone(0., 48., 1.), zone(48., 120., 0.35)]
        );
        // A bar under the edge (bars sit 3 px apart) is split, not rounded
        // to a whole bar.
        let split = zones(120., 2., 0.4021, None, 0.35);
        assert_eq!(split.first().map(|z| z.end), Some(48.5));
        // Nothing played, or all of it: one stretch.
        assert_eq!(zones(120., 1., 0., None, 0.35), vec![zone(0., 120., 0.35)]);
        assert_eq!(zones(120., 1., 1., None, 0.35), vec![zone(0., 120., 1.)]);
    }

    #[test]
    fn hover_tints_the_stretch_between_progress_and_pointer() {
        let tint = 0.25 + 0.3 * 0.75;
        // Pointer ahead: the unplayed part up to it brightens.
        let ahead = zones(100., 1., 0.25, Some(0.6), 0.25);
        assert_eq!(
            ahead,
            vec![
                Zone {
                    start: 0.,
                    end: 25.,
                    alpha: 1.
                },
                Zone {
                    start: 25.,
                    end: 60.,
                    alpha: tint
                },
                Zone {
                    start: 60.,
                    end: 100.,
                    alpha: 0.25
                },
            ]
        );
        // Pointer behind: the played part back to it dims to the same tone.
        let behind = zones(100., 1., 0.6, Some(0.25), 0.25);
        assert_eq!(
            behind.iter().map(|z| (z.start, z.end)).collect::<Vec<_>>(),
            vec![(0., 25.), (25., 60.), (60., 100.)]
        );
        assert_eq!(behind.get(1).map(|z| z.alpha), Some(tint));
        // Pointer on the edge: nothing extra.
        assert_eq!(zones(100., 1., 0.5, Some(0.5), 0.25).len(), 2);
    }

    #[test]
    fn knob_stays_inside_the_track() {
        assert_eq!(knob_left(50., 140., 10., 1.), 45.);
        assert_eq!(knob_left(0., 140., 10., 1.), 0.);
        assert_eq!(knob_left(140., 140., 12., 1.), 128.);
        // An odd center rounds to a whole pixel, never a half one at 1x.
        assert_eq!(knob_left(50.5, 140., 10., 1.), 46.);
        assert_eq!(knob_left(50.5, 140., 10., 2.), 45.5);
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
        assert_ne!(at(150.2), at(151.0));
        // Not laid out yet.
        assert_eq!(pointer_fraction(px(10.), px(0.), px(0.)), None);
    }
}
