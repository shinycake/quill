//! Mouse wheel notches glide (`gpui::WheelGlider`, the Quill patch in
//! `third_party/gpui-pre/src/wheel_glide.rs`): the glide's curve, and the
//! scroll areas that use it, a `div` scroll handle, a `gpui::list` and the
//! kit's `ScrollBounce`, stepped frame by frame on the test clock.
#![cfg(test)]
#![cfg(feature = "demo-capture")]

use gpui_base::ScrollBounce;
use gpui_kit::{
    Context, InteractiveElement, IntoElement, ListAlignment, ListOffset, ListState, ParentElement,
    Pixels, Render, ScrollDelta, ScrollEvent, ScrollHandle, StatefulInteractiveElement, Styled,
    TestAppContext, TouchPhase, VisualTestContext, WHEEL_GLIDE_DURATION, WheelGlide, Window, div,
    list, point, px,
};
use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_millis(16);

fn ms(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

fn down(y: f32) -> gpui_kit::Point<Pixels> {
    point(px(0.), px(y))
}

#[test]
fn a_notch_eases_out_to_its_distance() {
    let start = Instant::now();
    let mut glide = WheelGlide::Resting.push(down(-60.), start);
    let mut moved = Vec::new();
    for frame in 1..=9 {
        let (due, next) = glide.step(start + FRAME * frame);
        moved.push(due.y);
        glide = next;
    }
    assert!(
        moved
            .windows(2)
            .all(|pair| pair[0] < px(0.) && pair[0] < pair[1]),
        "each frame moves less than the one before: {moved:?}"
    );
    let (rest, glide) = glide.step(start + WHEEL_GLIDE_DURATION);
    let total = moved.iter().fold(rest.y, |sum, step| sum + *step);
    assert!((total - px(-60.)).abs() < px(0.001), "covered {total:?}");
    assert_eq!(glide, WheelGlide::Resting);
}

#[test]
fn a_notch_mid_glide_adds_to_what_is_left() {
    let start = Instant::now();
    let glide = WheelGlide::Resting.push(down(-60.), start);
    let (first, glide) = glide.step(start + ms(50));
    let glide = glide.push(down(-60.), start + ms(50));
    assert_eq!(glide.left().y, px(-120.) - first.y);
    assert_eq!(glide.since(), Some(start), "still the same glide");
    // The new notch starts a fresh leg, so it is not done at the old end.
    let (_, glide) = glide.step(start + WHEEL_GLIDE_DURATION);
    assert!(glide.left().y < px(0.));
    let (_, glide) = glide.step(start + ms(50) + WHEEL_GLIDE_DURATION);
    assert_eq!(glide, WheelGlide::Resting);
}

#[test]
fn a_notch_the_other_way_drops_what_is_left() {
    let start = Instant::now();
    let glide = WheelGlide::Resting.push(down(-60.), start);
    let (_, glide) = glide.step(start + ms(30));
    let glide = glide.push(down(20.), start + ms(30));
    assert_eq!(glide.left(), down(20.));
}

#[test]
fn a_notch_that_moves_nothing_leaves_the_glide_at_rest() {
    let now = Instant::now();
    assert_eq!(WheelGlide::Resting.push(down(0.), now), WheelGlide::Resting);
    assert_eq!(
        WheelGlide::Resting.push(down(f32::NAN), now),
        WheelGlide::Resting
    );
}

/// A 200 px area over 2,000 px of content, scrolled by a `div`.
struct DivArea(ScrollHandle);

impl Render for DivArea {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("area")
            .w(px(200.))
            .h(px(200.))
            .overflow_y_scroll()
            .track_scroll(&self.0)
            .child(div().w_full().h(px(2_000.)))
    }
}

/// The same area with the kit's bounce taking the wheel, as kit
/// `Scrollable`s and the history do.
struct BounceArea(ScrollHandle);

impl Render for BounceArea {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ScrollBounce::new(
            "bounce",
            &self.0,
            div()
                .id("area")
                .w(px(200.))
                .h(px(200.))
                .overflow_y_scroll()
                .track_scroll(&self.0)
                .child(div().w_full().h(px(2_000.))),
        )
    }
}

/// Fifty 40 px rows in a 200 px `gpui::list`.
struct ListArea(ListState);

impl Render for ListArea {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).h(px(200.)).child(
            list(self.0.clone(), |_, _, _| {
                div().w_full().h(px(40.)).into_any_element()
            })
            .size_full(),
        )
    }
}

fn open<V: Render + 'static>(
    cx: &mut TestAppContext,
    view: impl FnOnce() -> V + 'static,
) -> &mut VisualTestContext {
    let (_, vcx) = cx.add_window_view(move |_, _| view());
    draw(vcx);
    vcx
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| window.draw(cx).clear(cx));
}

fn wheel(vcx: &mut VisualTestContext, delta: ScrollDelta) {
    vcx.simulate_event(ScrollEvent {
        position: point(px(100.), px(100.)),
        delta,
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
}

fn notch(vcx: &mut VisualTestContext, lines: f32) {
    wheel(vcx, ScrollDelta::Lines(point(0., lines)));
}

/// Move the clock by `by` and deliver a frame; returns how many next-frame
/// callbacks ran (zero once nothing animates).
fn frame(vcx: &mut VisualTestContext, by: Duration) -> usize {
    vcx.executor().advance_clock(by);
    let ran = vcx.update(|window, cx| window.simulate_next_frame(cx));
    draw(vcx);
    ran
}

/// Equal but for the rounding of the steps' sum.
fn assert_near(got: Pixels, want: Pixels) {
    assert!((got - want).abs() < px(0.01), "{got:?}, not {want:?}");
}

fn line_height(vcx: &mut VisualTestContext) -> Pixels {
    vcx.update(|window, _| window.line_height())
}

#[gpui_kit::test]
fn a_div_glides_a_notch_over_frames(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let vcx = open(cx, {
        let handle = handle.clone();
        move || DivArea(handle)
    });
    let target = line_height(vcx) * -3.;
    notch(vcx, -3.);
    assert_eq!(handle.offset().y, px(0.), "nothing moves before a frame");
    let mut seen = vec![px(0.)];
    for _ in 0..8 {
        frame(vcx, FRAME);
        seen.push(handle.offset().y);
    }
    assert!(
        seen.windows(2).all(|pair| pair[1] < pair[0]),
        "every frame moves on: {seen:?}"
    );
    assert!(handle.offset().y > target, "still gliding at 128 ms");
    frame(vcx, ms(40));
    assert_near(handle.offset().y, target);
    assert_eq!(frame(vcx, FRAME), 0, "a finished glide asks for no frames");
}

#[gpui_kit::test]
fn notches_mid_glide_add_up_and_a_reverse_turns_at_once(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let vcx = open(cx, {
        let handle = handle.clone();
        move || DivArea(handle)
    });
    let line = line_height(vcx);
    notch(vcx, -3.);
    frame(vcx, FRAME);
    frame(vcx, FRAME);
    notch(vcx, -3.);
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_near(handle.offset().y, line * -6.);

    notch(vcx, -6.);
    frame(vcx, FRAME);
    let turned_at = handle.offset().y;
    notch(vcx, 1.);
    frame(vcx, FRAME);
    assert!(
        handle.offset().y > turned_at,
        "the reverse moves back at once"
    );
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_near(handle.offset().y, turned_at + line);
}

#[gpui_kit::test]
fn setting_the_offset_or_a_trackpad_stops_a_glide(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let vcx = open(cx, {
        let handle = handle.clone();
        move || DivArea(handle)
    });
    notch(vcx, -20.);
    frame(vcx, FRAME);
    handle.set_offset(point(px(0.), px(-50.)));
    draw(vcx);
    frame(vcx, FRAME);
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_eq!(handle.offset().y, px(-50.));

    notch(vcx, -20.);
    frame(vcx, FRAME);
    let at = handle.offset().y;
    wheel(vcx, ScrollDelta::Pixels(point(px(0.), px(-5.))));
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_eq!(handle.offset().y, at - px(5.));
}

#[gpui_kit::test]
fn a_glide_ends_at_the_edge(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let vcx = open(cx, {
        let handle = handle.clone();
        move || DivArea(handle)
    });
    notch(vcx, -10_000.);
    for _ in 0..3 {
        frame(vcx, FRAME);
    }
    assert_eq!(handle.offset().y, px(-1_800.));
    frame(vcx, FRAME);
    assert_eq!(
        frame(vcx, FRAME),
        0,
        "pinned at the end, the glide stops well before its time is up"
    );
}

#[gpui_kit::test]
fn reduced_motion_jumps_at_once(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let vcx = open(cx, {
        let handle = handle.clone();
        move || DivArea(handle)
    });
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    draw(vcx);
    let line = line_height(vcx);
    notch(vcx, -3.);
    assert_eq!(handle.offset().y, line * -3.);
}

#[gpui_kit::test]
fn the_bounce_glides_a_notch_and_a_jump_stops_it(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let vcx = open(cx, {
        let handle = handle.clone();
        move || BounceArea(handle)
    });
    let target = line_height(vcx) * -3.;
    notch(vcx, -3.);
    assert_eq!(handle.offset().y, px(0.));
    frame(vcx, FRAME);
    let partway = handle.offset().y;
    assert!(partway < px(0.) && partway > target, "partway: {partway:?}");
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_near(handle.offset().y, target);

    notch(vcx, -10.);
    frame(vcx, FRAME);
    handle.set_offset(point(px(0.), px(0.)));
    draw(vcx);
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_eq!(handle.offset().y, px(0.));
}

#[gpui_kit::test]
fn a_list_glides_a_notch_and_a_jump_stops_it(cx: &mut TestAppContext) {
    let state = ListState::new(50, ListAlignment::Top, px(0.)).measure_all();
    let vcx = open(cx, {
        let state = state.clone();
        move || ListArea(state)
    });
    let top = |state: &ListState| {
        let top = state.logical_scroll_top();
        px(40.) * top.item_ix as f32 + top.offset_in_item
    };
    // `gpui::list` counts a line as 20 px.
    notch(vcx, -3.);
    frame(vcx, FRAME);
    let partway = top(&state);
    assert!(
        partway > px(0.) && partway < px(60.),
        "partway: {partway:?}"
    );
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_near(top(&state), px(60.));

    notch(vcx, -10.);
    frame(vcx, FRAME);
    state.scroll_to(ListOffset {
        item_ix: 0,
        offset_in_item: px(0.),
    });
    draw(vcx);
    frame(vcx, WHEEL_GLIDE_DURATION);
    assert_eq!(top(&state), px(0.));
}
