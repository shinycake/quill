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
