//! The cover picker opened on a clip that is still downloading waits for
//! it instead of calling it unreadable for good. Over the
//! `ready-story-viewer` demo; nothing reaches Telegram.
use super::demo::{LEO, apply_video_story};
use super::open_tests::{frames, open_app};
use gpui_kit::TestAppContext;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::story_viewer::StoryOpenScope;
use std::sync::Arc;

#[gpui_kit::test]
fn a_cover_picker_waits_for_a_clip_that_is_not_local_yet(cx: &mut TestAppContext) {
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
    app.update_in(vcx, |app, _, cx| app.toggle_story_cover_edit(LEO, 3, cx));
    app.read_with(vcx, |app, _| {
        assert!(app.stories.viewer_ui().cover.is_some());
        assert!(app.cover_waits_for_clip(), "the clip is still downloading");
        assert!(!app.cover_clip_unreadable(), "not unreadable for good");
    });
    // The frames pass and the clip is still not local: it keeps waiting.
    frames(vcx);
    app.update(vcx, |app, cx| app.cover_upkeep(cx));
    assert!(app.read_with(vcx, |app, _| app.cover_waits_for_clip()));
    // The clip finishes downloading.
    app.update(vcx, |app, cx| {
        let sink: Arc<dyn DiagnosticSink> = app.demo_ui.sink.clone();
        if let Some(session) = app.demo_session.as_mut() {
            apply_video_story(session, &sink, &app.demo_ui.seq, true);
        }
        app.cover_upkeep(cx);
    });
    app.read_with(vcx, |app, _| {
        assert!(!app.cover_waits_for_clip(), "its length is being read");
        assert!(app.stories.viewer_ui().cover.is_some());
    });
}
