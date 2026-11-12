//! The story viewer's tick rests while the story is held and nothing is
//! waited on, redraws what its wakes changed, and starts again from what
//! resumes the story. Over the `ready-story-viewer` demo.
use super::open_tests::{frames, open_app};
use crate::ui::app::QuillApp;
use crate::ui::stories_state::ViewerPanel;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::story_extras::CloseFriendsStage;
use std::time::Duration;

/// One wake of the tick.
fn wake(vcx: &mut VisualTestContext) {
    vcx.executor().advance_clock(Duration::from_millis(100));
    vcx.run_until_parked();
}

fn running(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.stories.tick.is_running())
}

/// Held with Space.
fn hold(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.toggle_story_pause(cx));
}

#[gpui_kit::test]
fn a_close_friends_list_arriving_under_the_open_panel_shows_without_other_input(
    cx: &mut TestAppContext,
) {
    use quill::state::CloseFriendsList;
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.stories.with_viewer_ui(|open| {
            open.show(ViewerPanel::CloseFriends(CloseFriendsStage::Awaiting))
        });
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.close_friends = CloseFriendsList::NotLoaded;
        }
        app.ensure_story_tick(cx);
    });
    frames(vcx);
    wake(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app
            .stories
            .viewer_ui()
            .close_friends()
            .clone()),
        CloseFriendsStage::Awaiting
    );
    assert!(
        running(&app, vcx),
        "the panel waits on the list: the tick stays"
    );
    // The list arrives; nothing else happens.
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.close_friends = CloseFriendsList::Ready(vec![4]);
        }
    });
    let _ = crate::ui::frame_clock::take_slice_renders();
    wake(vcx);
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let drawn = crate::ui::frame_clock::take_slice_renders();
    let stage = app.read_with(vcx, |app, _| {
        app.stories.viewer_ui().close_friends().clone()
    });
    assert!(
        stage.edit().is_some_and(|edit| edit.contains(4)),
        "the next wake moved the stage"
    );
    assert!(
        drawn.contains_key("story"),
        "and redrew the panel: {drawn:?}"
    );
    wake(vcx);
    assert!(
        !running(&app, vcx),
        "the editor is open and the story is held: the tick ends"
    );
}

#[gpui_kit::test]
fn a_held_story_with_nothing_waited_on_ends_its_tick_and_resuming_starts_it(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    assert!(running(&app, vcx), "a playing story ticks");
    hold(&app, vcx);
    wake(vcx);
    wake(vcx);
    assert!(!running(&app, vcx), "held: the tick has ended");
    app.update(vcx, |app, cx| app.toggle_story_pause(cx));
    assert!(running(&app, vcx), "resumed: it starts again");
}

#[gpui_kit::test]
fn the_window_becoming_active_starts_the_tick_of_a_story_held_behind_another_app(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    vcx.deactivate_window();
    app.update(vcx, |_, cx| cx.notify());
    frames(vcx);
    wake(vcx);
    wake(vcx);
    assert!(
        !running(&app, vcx),
        "behind another app: the tick has ended"
    );
    vcx.update(|window, _| window.activate_window());
    frames(vcx);
    assert!(
        running(&app, vcx),
        "the redraw of the active window starts it"
    );
}

#[gpui_kit::test]
fn a_showing_stealth_toast_keeps_the_tick_until_it_is_cleared(cx: &mut TestAppContext) {
    use crate::ui::story_stealth_ui::ShownStealthToast;
    use quill::story_stealth::StealthToast;
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    hold(&app, vcx);
    let toast = |until| ShownStealthToast {
        toast: StealthToast::cooldown(),
        until,
    };
    app.update(vcx, |app, cx| {
        let until = std::time::Instant::now() + Duration::from_secs(3600);
        app.stories
            .with_viewer_ui(|open| open.stealth_toast = Some(toast(until)));
        app.ensure_story_tick(cx);
    });
    wake(vcx);
    assert!(running(&app, vcx), "a toast on screen is waited on");
    app.update(vcx, |app, _| {
        let past = std::time::Instant::now();
        app.stories
            .with_viewer_ui(|open| open.stealth_toast = Some(toast(past)));
    });
    wake(vcx);
    assert!(
        app.read_with(vcx, |app, _| app
            .stories
            .viewer_ui()
            .stealth_toast
            .is_none()),
        "its time over, it is cleared"
    );
    wake(vcx);
    assert!(!running(&app, vcx), "and the held story rests");
}
