//! A video story whose clip never becomes local falls back to its picture
//! and the duration clock once the wait runs out, instead of freezing on
//! the first frame. Over the `ready-story-viewer` demo; nothing reaches
//! Telegram.
use super::demo::{LEO, apply_video_story};
use super::open_tests::{frames, open_app};
use crate::ui::story_video_state::{StoryVideo, VIDEO_WAIT};
use gpui_kit::TestAppContext;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::story_viewer::StoryOpenScope;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[gpui_kit::test]
fn a_clip_that_never_arrives_falls_back_to_the_picture(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        let sink: Arc<dyn DiagnosticSink> = app.demo_ui.sink.clone();
        if let Some(session) = app.demo_session.as_mut() {
            apply_video_story(session, &sink, &app.demo_ui.seq, false);
        }
        app.open_story_viewer(ChatId(LEO), 3, StoryOpenScope::Single, cx);
    });
    frames(vcx);
    let driven = app.read_with(vcx, |app, _| app.story_video_driven());
    if !crate::ui::video_player::supported() {
        assert!(!driven, "no player: the clock drives the story");
        return;
    }
    assert!(driven, "the player is to drive the video story");
    app.update(vcx, |app, cx| {
        // The download ended without a local file (TDLib gave up on it).
        if let Some(session) = app.demo_session.as_mut() {
            session.media.downloads.clear();
        }
        // Ticks while the clip is missing must not restart the wait.
        app.sync_story_video(cx);
        if let (StoryVideo::Waiting { since, .. }, Some(long_ago)) = (
            &mut app.stories.video,
            Instant::now().checked_sub(VIDEO_WAIT + Duration::from_secs(1)),
        ) {
            *since = long_ago;
        }
        app.sync_story_video(cx);
        app.sync_story_video(cx);
    });
    let (failed, driven) = app.read_with(vcx, |app, _| {
        (
            app.stories.video.failed_for(Some((LEO, 3))),
            app.story_video_driven(),
        )
    });
    assert!(failed, "the wait ran out");
    assert!(!driven, "the duration clock takes over");
}
