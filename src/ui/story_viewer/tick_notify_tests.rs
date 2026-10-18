//! The story viewer's 100 ms tick redraws the viewer overlay alone, and the
//! custom-emoji warm-up behind it repeats only when the story or what it
//! needs changed. Over the `ready-story-viewer` demo; nothing reaches
//! Telegram.
use super::open_tests::{frames, open_app};
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

fn renders(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    tick: impl Fn(&mut QuillApp, &mut gpui_kit::Context<QuillApp>),
) -> std::collections::BTreeMap<&'static str, u32> {
    let _ = crate::ui::frame_clock::take_slice_renders();
    app.update(vcx, |app, cx| tick(app, cx));
    vcx.run_until_parked();
    // A display frame: dirty views only.
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    crate::ui::frame_clock::take_slice_renders()
}

#[gpui_kit::test]
fn the_tick_redraws_only_the_story_overlay(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let open = app.read_with(vcx, |app, _| app.stories.viewer.is_open());
    assert!(open, "the demo opens the viewer");
    let tick = renders(&app, vcx, |app, cx| app.notify_story(cx));
    assert!(tick.contains_key("story"), "{tick:?}");
    for other in ["sidebar", "conversation", "composer", "(app notified)"] {
        assert!(!tick.contains_key(other), "{other} rendered: {tick:?}");
    }
}

#[gpui_kit::test]
fn an_app_notify_still_redraws_the_story_overlay(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let all = renders(&app, vcx, |_, cx| cx.notify());
    assert!(all.contains_key("story"), "{all:?}");
    assert!(all.contains_key("(app notified)"), "{all:?}");
}

#[gpui_kit::test]
fn a_step_shows_the_next_story_in_the_overlay(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let key = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.stories.viewer.current().map(|item| item.key())
        })
    };
    let before = key(&app, vcx);
    let stepped = renders(&app, vcx, |app, cx| app.step_story_viewer(1, cx));
    assert_ne!(key(&app, vcx), before);
    assert!(stepped.contains_key("story"), "{stepped:?}");
}

#[gpui_kit::test]
fn the_emoji_warm_up_fetches_once_per_story_and_need(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let warm = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, _| {
            app.warm_story_custom_emoji_with(std::time::Instant::now(), |_, _| true)
        })
    };
    assert!(warm(&app, vcx), "the first tick warms");
    assert!(!warm(&app, vcx), "the next tick finds nothing new");
    assert!(!warm(&app, vcx));
    app.update(vcx, |app, cx| app.step_story_viewer(1, cx));
    frames(vcx);
    assert!(warm(&app, vcx), "another story warms again");
    assert!(!warm(&app, vcx));
    // Closing the viewer forgets the record.
    app.update(vcx, |app, _| {
        app.stories.emoji_warm = None;
    });
    assert!(warm(&app, vcx), "a cleared record warms again");
}

#[gpui_kit::test]
fn a_failed_emoji_fetch_is_retried_on_the_next_tick(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let tick = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, ok: bool| {
        app.update(vcx, |app, _| {
            app.warm_story_custom_emoji_with(std::time::Instant::now(), |_, _| ok)
        })
    };
    assert!(tick(&app, vcx, false), "the first fetch runs and fails");
    assert!(
        app.read_with(vcx, |app, _| app.stories.emoji_warm.is_none()),
        "a failed fetch is not recorded"
    );
    assert!(tick(&app, vcx, false), "the next tick tries again");
    assert!(tick(&app, vcx, true), "and again; this time it succeeds");
    assert!(!tick(&app, vcx, true), "now it is recorded and gated");
}

#[gpui_kit::test]
fn a_tdlib_error_for_the_emoji_fetch_makes_the_next_tick_fetch_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let now = std::time::Instant::now();
    let warm = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, at| {
        app.update(vcx, |app, _| {
            app.warm_story_custom_emoji_with(at, |_, _| true)
        })
    };
    assert!(warm(&app, vcx, now), "the fetch was sent and recorded");
    assert!(!warm(&app, vcx, now), "nothing new while it is out");
    // TDLib answers the request with an error.
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stories.custom_emoji_fetch_failed(now);
        }
    });
    assert!(!warm(&app, vcx, now), "the retry waits out its backoff");
    let later = now + quill::state::STORIES_RETRY_FIRST + std::time::Duration::from_secs(1);
    assert!(
        warm(&app, vcx, later),
        "after the wait the fetch runs again"
    );
    assert!(!warm(&app, vcx, later), "and is recorded again");
}
