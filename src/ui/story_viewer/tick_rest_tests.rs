//! The story viewer's frame clock rests while the story is held and nothing
//! is really out, redraws what its frames changed, and starts again from
//! what resumes the story. Over the `ready-story-viewer` demo.
use super::open_tests::{frames, open_app};
use crate::ui::app::QuillApp;
use crate::ui::stories_state::ViewerPanel;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::story_extras::CloseFriendsStage;
use std::time::Duration;

/// One frame of the clock: it wakes, redraws the app, and the upkeep asks
/// for the next frame while something moves.
fn wake(vcx: &mut VisualTestContext) {
    vcx.executor().advance_clock(Duration::from_millis(100));
    vcx.run_until_parked();
    frames(vcx);
}

fn running(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.frame.clock_running())
}

/// Frames until the clock has had its idle wakes.
fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        wake(vcx);
    }
}

/// Held with Space.
fn hold(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.toggle_story_pause(cx));
}

/// The close-friends panel open over a held story, with the session's list
/// in the state `list` says.
fn open_close_friends(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    list: quill::state::CloseFriendsList,
) {
    app.update(vcx, |app, cx| {
        app.stories.with_viewer_ui(|open| {
            open.show(ViewerPanel::CloseFriends(CloseFriendsStage::Awaiting))
        });
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.close_friends = list;
        }
        app.ensure_story_tick(cx);
    });
    frames(vcx);
}

#[gpui_kit::test]
fn a_close_friends_list_arriving_under_the_open_panel_shows_without_other_input(
    cx: &mut TestAppContext,
) {
    use quill::ids::RequestId;
    use quill::state::CloseFriendsList;
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    open_close_friends(
        &app,
        vcx,
        CloseFriendsList::Loading {
            request: RequestId(1),
            stale: None,
        },
    );
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
        "the load is out: the clock stays for its answer"
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
        "the next frame moved the stage"
    );
    assert!(
        drawn.contains_key("story"),
        "and redrew the panel: {drawn:?}"
    );
    settle(vcx);
    assert!(
        !running(&app, vcx),
        "the editor is open and the story is held: the clock stops"
    );
}

#[gpui_kit::test]
fn a_close_friends_panel_with_no_load_out_lets_the_clock_rest(cx: &mut TestAppContext) {
    use quill::state::CloseFriendsList;
    for list in [
        CloseFriendsList::NotLoaded,
        CloseFriendsList::Failed { stale: None },
    ] {
        let (app, mut vcx) = open_app(cx, "ready-story-viewer");
        let vcx = &mut vcx;
        open_close_friends(&app, vcx, list);
        settle(vcx);
        assert!(
            !running(&app, vcx),
            "nothing will arrive, so nothing is waited on"
        );
    }
}

#[gpui_kit::test]
fn a_held_story_with_nothing_waited_on_ends_its_tick_and_resuming_starts_it(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    assert!(running(&app, vcx), "a playing story ticks");
    hold(&app, vcx);
    settle(vcx);
    assert!(!running(&app, vcx), "held: the clock has stopped");
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
    settle(vcx);
    assert!(
        !running(&app, vcx),
        "behind another app: the clock has stopped"
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
    frames(vcx);
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
    settle(vcx);
    assert!(!running(&app, vcx), "and the held story rests");
}

#[gpui_kit::test]
fn a_held_story_with_a_custom_emoji_that_never_resolves_rests(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    hold(&app, vcx);
    // The reaction the story shows is a custom emoji no sticker is known for.
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.available_reactions = quill::state::fetch::Fetch::Ready(vec![
                quill::telegram::envelope::StoryAvailableReactionView {
                    kind: quill::telegram::envelope::StoryAvailableReactionKind::CustomEmoji(7),
                    needs_premium: false,
                },
            ]);
        }
    });
    frames(vcx);
    settle(vcx);
    assert!(
        !running(&app, vcx),
        "an emoji that is not being fetched or downloaded is not waited on"
    );
}
