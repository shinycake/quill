//! The floating date through the real app, list and render loop: it draws
//! while it fades and nothing while it rests, and it shows the day of the
//! row at the top from the moment that row is painted. Over the demo;
//! nothing reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::take_slice_renders;
use crate::ui::history::demo_chat::{A, frames, open, select};
use gpui_kit::{
    Entity, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase, VisualTestContext,
    point, px,
};
use std::time::Duration;

const ROWS: usize = 300;
const CLEAR_OF_SEPARATOR: usize = 30;
const PAST_DAY_CHANGE_HOLD_MS: u64 = 3200;
const BEFORE_SCROLL_BAR_HIDES_MS: u64 = 900;

fn opened(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, mut vcx) = open(cx, ROWS);
    select(&app, &mut vcx, A);
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (app, vcx)
}

fn scroll_to(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, row: usize) {
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item_offset(row, 12., cx));
    });
    vcx.run_until_parked();
}

fn day_start(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> usize {
    app.read_with(vcx, |app, _| {
        let rows = &app.history.rows;
        (2 * CLEAR_OF_SEPARATOR..rows.len().saturating_sub(2 * CLEAR_OF_SEPARATOR))
            .find(|ix| rows.get(*ix).is_some_and(|row| row.day_label().is_some()))
            .expect("a day starts here")
    })
}

fn day_of(app: &Entity<QuillApp>, vcx: &VisualTestContext, row: usize) -> String {
    app.read_with(vcx, |app, _| {
        let rows = app.history.rows.get(..=row).expect("the row");
        rows.iter()
            .rev()
            .find_map(|row| row.day_label().map(str::to_string))
            .expect("a day")
    })
}

fn date_up(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, day: &str) -> bool {
    let label = app.read_with(vcx, |app, _| app.history.scroll_date.top_label.clone());
    vcx.debug_bounds("floating-date").is_some() && label.as_deref() == Some(day)
}

fn advance(vcx: &mut VisualTestContext, ms: u64) {
    for _ in 0..ms / 100 {
        vcx.executor().advance_clock(Duration::from_millis(100));
        vcx.run_until_parked();
    }
}

fn renders_over(vcx: &mut VisualTestContext, ms: u64) -> u32 {
    let _ = take_slice_renders();
    advance(vcx, ms);
    let drawn = take_slice_renders();
    drawn.get("conversation").copied().unwrap_or(0)
}

#[gpui_kit::test]
fn the_date_draws_nothing_while_it_rests(cx: &mut TestAppContext) {
    let (app, mut vcx) = opened(cx);
    let vcx = &mut vcx;
    let start = day_start(&app, vcx);
    let row = start + CLEAR_OF_SEPARATOR;
    let day = day_of(&app, vcx, row);
    scroll_to(&app, vcx, start - CLEAR_OF_SEPARATOR);
    scroll_to(&app, vcx, row);
    frames(vcx);
    assert!(renders_over(vcx, 1000) > 0, "it fades in");
    assert!(date_up(&app, vcx, &day));

    assert_eq!(renders_over(vcx, BEFORE_SCROLL_BAR_HIDES_MS), 0, "it rests");
    assert!(date_up(&app, vcx, &day));

    assert!(renders_over(vcx, 1200) > 1, "it fades out");
    assert!(!date_up(&app, vcx, &day));
    let clock = app.read_with(vcx, |app, _| app.frame.frame_clock_running.get());
    assert!(!clock, "nothing asks for ticks");
}

#[gpui_kit::test]
fn a_later_scroll_moves_the_fade(cx: &mut TestAppContext) {
    let (app, mut vcx) = opened(cx);
    let vcx = &mut vcx;
    let row = day_start(&app, vcx) + CLEAR_OF_SEPARATOR;
    let day = day_of(&app, vcx, row);
    scroll_to(&app, vcx, row);
    frames(vcx);
    advance(vcx, PAST_DAY_CHANGE_HOLD_MS);
    scroll_to(&app, vcx, row + 1);
    frames(vcx);
    advance(vcx, 500);
    scroll_to(&app, vcx, row + 2);
    advance(vcx, 700);
    assert!(
        date_up(&app, vcx, &day),
        "the first timer only looked again"
    );
    advance(vcx, 500);
    assert!(!date_up(&app, vcx, &day), "a second after the last scroll");
}

#[gpui_kit::test]
fn the_date_shows_as_soon_as_a_separator_leaves_the_top(cx: &mut TestAppContext) {
    let (app, mut vcx) = opened(cx);
    let vcx = &mut vcx;
    let start = day_start(&app, vcx);
    let day = day_of(&app, vcx, start);
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item_offset(start, 0., cx));
    });
    frames(vcx);
    assert!(!date_up(&app, vcx, &day), "the separator says it");
    advance(vcx, 300);

    let center = vcx.update(|window, _| window.bounds().center());
    vcx.simulate_event(ScrollEvent {
        position: center,
        delta: ScrollDelta::Pixels(point(px(0.), px(-10.))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: gpui_kit::ScrollSource::Pointer,
    });
    vcx.run_until_parked();
    assert!(date_up(&app, vcx, &day));
}
