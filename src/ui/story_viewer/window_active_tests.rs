//! The story viewer plays only while its window is active (tdesktop
//! `Controller::updatePlayingAllowed` needs `_windowActive`): behind
//! another app, minimized, or with a chat window in front, the story
//! holds still and does not advance. Over the `ready-story-viewer` demo.
use super::open_tests::{frames, open_app};
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::time::{Duration, Instant};

/// What the viewer's tick does with the pause, then whether the story
/// would be over an hour from now.
fn finishes_later(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, _| {
        let now = Instant::now();
        let paused = app.story_playback_paused();
        app.stories.playback.set_paused(paused, now);
        app.story_segment_finished(now + Duration::from_secs(3600))
    })
}

#[gpui_kit::test]
fn an_inactive_window_holds_the_story(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let open = app.read_with(vcx, |app, _| app.stories.viewer.is_open());
    assert!(open, "the demo opens the viewer");
    assert!(
        app.read_with(vcx, |app, _| !app.story_playback_paused()),
        "the story plays in the active window"
    );

    vcx.deactivate_window();
    assert!(app.read_with(vcx, |app, _| app.story_playback_paused()));
    assert!(
        !finishes_later(&app, vcx),
        "a held story never runs out, so the viewer neither advances nor closes"
    );

    vcx.update(|window, _| window.activate_window());
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| !app.story_playback_paused()),
        "it plays on once the window is active again"
    );
}
