#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The chat list's elastic overscroll and the pull that expands the stories
//! strip, through GPUI's real scroll dispatch and the kit's bounce
//! (`docs/decisions/elastic-chat-list.md`). Time moves with the test
//! executor's clock, never a sleep.

use crate::ui::app::QuillApp;
use crate::ui::chat_row::ChatListItem;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase, VisualTestContext, point, px, size,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The ready-chats demo with a list long enough to scroll, and (when
/// `stories`) two posters with active stories.
fn open(cx: &mut TestAppContext, stories: bool) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            crate::ui::demo::add_stress_chats(session, 40);
            if stories {
                let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
                let seq = AtomicU64::new(session.last_seq);
                for chat in [11, 12] {
                    let json = format!(
                        r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat},"list":{{"@type":"storyListMain"}},"order":"{chat}","can_be_archived":false,"max_read_story_id":0,"stories":[{{"@type":"storyInfo","story_id":1,"date":1790000000,"is_for_close_friends":false,"is_live":false}}]}}}}"#
                    );
                    if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                        session.apply(owned);
                    }
                }
            }
        }
        app.notify_sidebar(cx);
        cx.notify();
    });
    frames(&mut vcx);
    (app, vcx)
}

fn centre(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Point<Pixels> {
    app.update(vcx, |app, _| app.chat_list.scroll.bounds().center())
}

fn wheel(vcx: &mut VisualTestContext, at: Point<Pixels>, phase: TouchPhase, delta: ScrollDelta) {
    vcx.simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        modifiers: Modifiers::default(),
        touch_phase: phase,
    });
}

fn pixels(dy: f32) -> ScrollDelta {
    ScrollDelta::Pixels(point(px(0.), px(dy)))
}

/// A finger on the trackpad: down, then `steps` moves of `dy` px each (a
/// positive `dy` pulls the list down), with a frame between them.
fn drag(vcx: &mut VisualTestContext, at: Point<Pixels>, steps: usize, dy: f32) {
    wheel(vcx, at, TouchPhase::Started, pixels(0.));
    for _ in 0..steps {
        wheel(vcx, at, TouchPhase::Moved, pixels(dy));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn displaced(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> f32 {
    app.update(vcx, |app, _| app.stories.strip.bounce.get().offset)
}

fn expanded(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, _| app.stories.strip.pull.expanded())
}

fn strip_listed(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, _| {
        app.chat_list
            .items
            .iter()
            .any(|item| matches!(item, ChatListItem::StoryStrip))
    })
}

/// Let the edge return: the test clock moves in frames, as the real one does.
fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..90 {
        vcx.executor().advance_clock(Duration::from_millis(16));
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn the_strip_rests_collapsed(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    assert!(!expanded(&app, vcx));
    assert!(!strip_listed(&app, vcx));
    assert!(app.update(vcx, |app, _| app.story_stack_opacity()) > 0.99);
}

#[gpui_kit::test]
fn pulling_the_top_stretches_the_list_and_it_springs_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, false);
    let vcx = &mut vcx;
    let at = centre(&app, vcx);
    drag(vcx, at, 6, 8.);
    let held = displaced(&app, vcx);
    assert!(held > 1., "the list follows the finger: {held}");
    assert!(app.update(vcx, |app, _| app.stories.strip.bounce.get().held));

    wheel(vcx, at, TouchPhase::Ended, pixels(0.));
    settle(vcx);
    assert_eq!(displaced(&app, vcx), 0., "the edge returned");
    assert!(!expanded(&app, vcx), "no strip to pull open");
}

#[gpui_kit::test]
fn pulling_the_bottom_stretches_it_the_other_way(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, false);
    let vcx = &mut vcx;
    let at = centre(&app, vcx);
    // Scroll to the end in one go, then keep pushing.
    app.update(vcx, |app, _| {
        let mut offset = app.chat_list.scroll.offset();
        offset.y = -app.chat_list.scroll.max_offset().y;
        app.chat_list.scroll.set_offset(offset);
    });
    frames(vcx);
    drag(vcx, at, 6, -8.);
    assert!(displaced(&app, vcx) < -1.);
    wheel(vcx, at, TouchPhase::Ended, pixels(0.));
    settle(vcx);
    assert_eq!(displaced(&app, vcx), 0.);
}

#[gpui_kit::test]
fn a_short_pull_does_not_expand_the_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    let at = centre(&app, vcx);
    drag(vcx, at, 3, 8.);
    let held = displaced(&app, vcx);
    assert!(
        held > 1. && held < quill::stories_pull::EXPAND_PULL_PX,
        "{held}"
    );
    wheel(vcx, at, TouchPhase::Ended, pixels(0.));
    settle(vcx);
    assert!(!expanded(&app, vcx));
    assert!(!strip_listed(&app, vcx));
    assert_eq!(displaced(&app, vcx), 0.);
}

#[gpui_kit::test]
fn a_pull_past_the_threshold_expands_the_strip_and_it_stays(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    let at = centre(&app, vcx);
    drag(vcx, at, 40, 12.);
    assert!(expanded(&app, vcx), "the strip opened mid-pull");
    wheel(vcx, at, TouchPhase::Ended, pixels(0.));
    settle(vcx);
    assert!(expanded(&app, vcx));
    assert!(strip_listed(&app, vcx));
    // The strip slid in: the list rests at its very top.
    assert_eq!(app.update(vcx, |app, _| app.chat_list_scroll_top()), 0.);
    assert_eq!(displaced(&app, vcx), 0.);
    assert!(app.update(vcx, |app, _| app.story_stack_opacity()) < 0.01);
}

#[gpui_kit::test]
fn a_fling_into_the_top_never_expands_the_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    let at = centre(&app, vcx);
    // Momentum: pixel deltas with no finger down, hitting the top.
    for _ in 0..30 {
        wheel(vcx, at, TouchPhase::Moved, pixels(160.));
        vcx.update(|window, cx| window.render_frame(cx));
    }
    settle(vcx);
    assert!(!expanded(&app, vcx));
}

#[gpui_kit::test]
fn the_wheel_does_not_bounce(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    let at = centre(&app, vcx);
    for _ in 0..30 {
        wheel(
            vcx,
            at,
            TouchPhase::Moved,
            ScrollDelta::Lines(point(0., 3.)),
        );
        vcx.update(|window, cx| window.render_frame(cx));
    }
    assert_eq!(displaced(&app, vcx), 0.);
    assert!(!expanded(&app, vcx));
}

#[gpui_kit::test]
fn reduced_motion_turns_the_pull_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    let at = centre(&app, vcx);
    drag(vcx, at, 40, 12.);
    assert_eq!(displaced(&app, vcx), 0.);
    assert!(!expanded(&app, vcx));
}

#[gpui_kit::test]
fn tapping_the_stack_expands_the_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.expand_story_strip(cx));
    frames(vcx);
    settle(vcx);
    assert!(expanded(&app, vcx));
    assert!(strip_listed(&app, vcx));
    assert_eq!(app.update(vcx, |app, _| app.chat_list_scroll_top()), 0.);
}

#[gpui_kit::test]
fn scrolling_the_strip_out_of_sight_collapses_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.expand_story_strip(cx));
    settle(vcx);
    let rows_at_top = app.update(vcx, |app, _| app.chat_list.items.len());
    // Scroll past the strip's height (a plain wheel).
    app.update(vcx, |app, _| {
        let mut offset = app.chat_list.scroll.offset();
        offset.y = px(-120.);
        app.chat_list.scroll.set_offset(offset);
    });
    frames(vcx);
    frames(vcx);
    assert!(!expanded(&app, vcx));
    assert!(!strip_listed(&app, vcx));
    assert_eq!(
        app.update(vcx, |app, _| app.chat_list.items.len()),
        rows_at_top - 1
    );
    // What was on screen stays: 120 px down with the strip is 43 px down
    // without it.
    let top = app.update(vcx, |app, _| app.chat_list_scroll_top());
    assert!((top - 43.).abs() < 1., "{top}");
}
