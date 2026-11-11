//! Panels that belong to one story (Report, viewers) over the
//! `ready-story-viewer` demo. Nothing reaches Telegram.
use super::open_tests::{frames, open_app};
use gpui_kit::TestAppContext;
use quill::state::{StoryReportFlow, StoryReportStage};

#[gpui_kit::test]
fn moving_to_another_story_closes_its_report_and_viewers_panels(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(open) = app.stories.viewer_ui_mut() {
            open.report_open = true;
        }
        if let Some(session) = app.demo_session.as_mut() {
            session.begin_story_report(105, 3, quill::ids::RequestId(1));
            session.stories.manage.error = Some("Pick at least one user".into());
        }
    });
    let paused = app.read_with(vcx, |app, _| app.story_playback_paused());
    assert!(paused, "an open Report panel holds the story");
    app.update(vcx, |app, cx| app.step_story_viewer(1, cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.stories.viewer_ui().report_open);
        assert!(!app.stories.viewer_ui().viewers_open);
        let flow = app
            .demo_session
            .as_ref()
            .and_then(|session| session.stories.report.as_ref());
        assert!(flow.is_none(), "the old story's flow is gone");
        let error = app
            .demo_session
            .as_ref()
            .and_then(|session| session.stories.manage.error.clone());
        assert_eq!(error, None, "an edit's failure stays with its story");
    });
}

#[test]
fn a_report_flow_is_matched_by_poster_and_story() {
    let flow = StoryReportFlow {
        chat_id: 105,
        story_id: 3,
        stage: StoryReportStage::Checking {
            request: quill::ids::RequestId(1),
        },
    };
    assert!(flow.is_for(105, 3));
    assert!(!flow.is_for(52, 3), "the same story id from another poster");
}

/// Opening a panel closes the others: statistics, share, the privacy
/// editor and the viewers list never stack under the story.
#[gpui_kit::test]
fn opening_one_panel_closes_the_others(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let open_panels = |app: &gpui_kit::Entity<crate::ui::app::QuillApp>,
                       vcx: &mut gpui_kit::VisualTestContext| {
        app.read_with(vcx, |app, _| {
            let ui = app.stories.viewer_ui();
            (
                ui.stats_open,
                ui.share_open,
                ui.privacy_edit.is_some(),
                ui.viewers_open,
            )
        })
    };
    app.update(vcx, |app, cx| app.toggle_story_stats(cx));
    assert_eq!(open_panels(&app, vcx), (true, false, false, false));
    app.update_in(vcx, |app, window, cx| app.toggle_story_share(window, cx));
    assert_eq!(
        open_panels(&app, vcx),
        (false, true, false, false),
        "Share replaced the statistics"
    );
    app.update_in(vcx, |app, window, cx| {
        app.toggle_story_privacy_edit(window, cx)
    });
    assert_eq!(
        open_panels(&app, vcx),
        (false, false, true, false),
        "the privacy editor replaced Share"
    );
    app.update(vcx, |app, cx| app.toggle_story_stats(cx));
    assert_eq!(
        open_panels(&app, vcx),
        (true, false, false, false),
        "Statistics replaced the privacy editor"
    );
    app.update(vcx, |app, cx| app.toggle_story_viewers(cx));
    assert_eq!(open_panels(&app, vcx), (false, false, false, true));
}

/// Save on the close-friends editor leaves `Saving` with the answer to its
/// own call, also when the list was never loaded: a failure puts the edit
/// back to try again, a success closes the editor.
#[gpui_kit::test]
fn the_close_friends_save_leaves_saving_with_its_answer(cx: &mut TestAppContext) {
    use quill::ids::RequestId;
    use quill::story_extras::{CloseFriendsEdit, CloseFriendsStage};
    use quill::story_page::{StoryPageOp, StoryPageOpState};
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let set_op = |app: &mut crate::ui::app::QuillApp, state| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.close_friends = quill::state::CloseFriendsList::NotLoaded;
            session.stories.page_ops.begin(StoryPageOp {
                label: "saving close friends".into(),
                state,
                request: RequestId(60),
                chat_id: None,
            });
        }
    };
    app.update(vcx, |app, cx| {
        let mut edit = CloseFriendsEdit::new(&[]);
        edit.toggle(7);
        app.stories.with_viewer_ui(|open| {
            open.close_friends = CloseFriendsStage::Saving {
                edit,
                request: RequestId(60),
            }
        });
        set_op(app, StoryPageOpState::Sending);
        app.tick_close_friends(cx);
        assert!(app.stories.viewer_ui().close_friends.is_saving());
        set_op(app, StoryPageOpState::Failed("no".into()));
        app.tick_close_friends(cx);
        let stage = &app.stories.viewer_ui().close_friends;
        assert!(!stage.is_saving(), "the failure ended Saving");
        assert!(
            stage.edit().is_some_and(|edit| edit.contains(7)),
            "the edit is back, with its tick"
        );
        // Saved again: the answer closes the editor.
        let edit = stage.edit().cloned().unwrap_or_default();
        app.stories.with_viewer_ui(|open| {
            open.close_friends = CloseFriendsStage::Saving {
                edit,
                request: RequestId(60),
            }
        });
        set_op(app, StoryPageOpState::Succeeded);
        app.tick_close_friends(cx);
        assert!(!app.stories.viewer_ui().close_friends.is_open());
    });
}

/// The copy of a story's file to Downloads runs in the background: a second
/// press while it runs starts nothing, also after stepping away and back,
/// and the result waits for its story.
#[gpui_kit::test]
fn a_story_media_save_belongs_to_its_story(cx: &mut TestAppContext) {
    use crate::ui::stories_state::MediaSave;
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let key = app.update(vcx, |app, cx| {
        let key = app.stories.viewer.current().map(|item| item.key());
        let key = key.expect("the demo viewer shows a story");
        app.stories.media_saves.insert(key, MediaSave::Saving);
        app.stories
            .with_viewer_ui(|open| open.notice = Some("Saving…".into()));
        app.save_story_media(cx);
        assert_eq!(
            app.stories.viewer_ui().notice.as_deref(),
            Some("Saving…"),
            "the second press changed nothing"
        );
        app.step_story_viewer(1, cx);
        key
    });
    frames(vcx);
    app.update(vcx, |app, cx| {
        assert_eq!(app.stories.viewer_ui().notice, None);
        assert_eq!(
            app.stories.media_saves.get(&key),
            Some(&MediaSave::Saving),
            "the copy is still the first story's"
        );
        // It finishes while the other story shows; back on the first, the
        // result is there and the save is free to run again.
        app.stories
            .media_saves
            .insert(key, MediaSave::Done("Saved to Downloads".into()));
        app.step_story_viewer(-1, cx);
        assert_eq!(
            app.stories.viewer_ui().notice.as_deref(),
            Some("Saved to Downloads")
        );
    });
}
