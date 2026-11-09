//! What the viewer holds for the story on screen goes with that story: a
//! menu answer for a story that moved on does nothing, what is open over
//! a story pauses it, and a step or a new open starts clean. Over the
//! `ready-story-viewer` demo, which opens on Leo's story 3.
use super::icon_chrome::MenuAct;
use super::open_tests::{frames, open_app};
use crate::ui::story_albums::StoryPrivacyEdit;
use gpui_kit::TestAppContext;
use quill::ids::ChatId;
use quill::story_composer::StoryPrivacy;
use quill::story_page::{StoryPageOp, StoryPageOpState};
use quill::story_viewer::StoryOpenScope;

const LEO: ChatId = ChatId(105);
const MAYA: i64 = 52;

fn privacy_edit() -> StoryPrivacyEdit {
    StoryPrivacyEdit {
        chat_id: LEO.0,
        story_id: 3,
        privacy: StoryPrivacy::Everyone,
        selected_user_ids: Vec::new(),
    }
}

#[gpui_kit::test]
fn a_menu_answer_for_another_story_does_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    app.update_in(&mut vcx, |app, window, cx| {
        app.stories.with_viewer_ui(|open| open.menu_open = true);
        app.run_story_menu(MenuAct::React, (LEO, 2), window, cx);
        let open = app.stories.viewer_ui();
        assert!(!open.reaction_picker_open, "the menu was for story 2");
        assert!(!open.menu_open, "the menu closed");
        app.run_story_menu(MenuAct::React, (LEO, 3), window, cx);
        assert!(app.stories.viewer_ui().reaction_picker_open);
    });
}

#[gpui_kit::test]
fn the_open_menu_and_the_composer_pause_the_story(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    app.update(&mut vcx, |app, _| {
        assert!(!app.story_playback_paused());
        app.stories.with_viewer_ui(|open| open.menu_open = true);
        assert!(app.story_playback_paused(), "the menu is open");
        app.stories.with_viewer_ui(|open| open.menu_open = false);
        app.stories.composer.open = true;
        assert!(
            app.story_playback_paused(),
            "Edit or Repost is open over it"
        );
    });
}

#[gpui_kit::test]
fn a_step_drops_the_last_storys_editors(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    app.update(&mut vcx, |app, cx| {
        app.stories.with_viewer_ui(|open| {
            open.privacy_edit = Some(privacy_edit());
            open.menu_open = true;
            open.share_open = true;
        });
        app.step_story_viewer(1, cx);
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        let open = app.stories.viewer_ui();
        assert_ne!(
            app.stories.viewer.current().map(|item| item.chat_id),
            Some(LEO)
        );
        assert!(open.privacy_edit.is_none(), "Leo's privacy editor");
        assert!(!open.menu_open);
        assert!(!open.share_open);
    });
}

#[gpui_kit::test]
fn opening_over_the_viewer_starts_clean(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    app.update(&mut vcx, |app, cx| {
        app.stories.with_viewer_ui(|open| {
            open.privacy_edit = Some(privacy_edit());
            open.reply_open = true;
        });
        app.open_story_viewer(ChatId(MAYA), 5, StoryOpenScope::Tray, cx);
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        let open = app.stories.viewer_ui();
        assert!(app.stories.viewer.is_open());
        assert!(open.privacy_edit.is_none(), "the old viewer's editor");
        assert!(!open.reply_open, "the old viewer's reply row");
    });
}

/// The bare release (a deleted story, the passcode lock) ends a story
/// reply being recorded too: it is cancelled, never sent to the chat
/// under the viewer.
#[gpui_kit::test]
fn a_release_cancels_the_story_reply_being_recorded(cx: &mut TestAppContext) {
    use crate::ui::recording_state::CaptureFor;
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    app.update(&mut vcx, |app, cx| {
        app.recording.open_voice = std::sync::Arc::new(|| {
            let path = std::env::temp_dir().join("quill-story-reply-release-test.ogg");
            Ok(quill::voice::VoiceCapture::preview(path, 1, vec![4]))
        });
        app.stories.with_viewer_ui(|open| open.reply_open = true);
        app.start_voice_capture(CaptureFor::StoryReply, cx);
    });
    vcx.run_until_parked();
    app.update(&mut vcx, |app, cx| {
        assert!(app.recording_active(), "the reply is recording");
        app.release_story_viewer(cx);
        assert!(!app.recording_busy(), "the reply was cancelled");
    });
}

#[gpui_kit::test]
fn a_finished_page_op_goes_with_the_story(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let op = |state| {
        Some(StoryPageOp {
            label: "Saving".into(),
            state,
            request: None,
        })
    };
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.page_op = op(StoryPageOpState::Sending);
        }
        app.close_story_viewer(cx);
        let kept = app
            .demo_session
            .as_deref()
            .map(|s| s.stories.page_op.is_some());
        assert_eq!(kept, Some(true), "an op in flight keeps its line");
    });
    app.update(&mut vcx, |app, cx| {
        app.open_story_viewer(LEO, 3, StoryOpenScope::Tray, cx);
    });
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.page_op = op(StoryPageOpState::Failed("no".into()));
        }
        app.close_story_viewer(cx);
        let kept = app
            .demo_session
            .as_deref()
            .map(|s| s.stories.page_op.is_some());
        assert_eq!(kept, Some(false), "a failed op's line was about this story");
    });
}

/// A story edit, or a Hide, answered after its story was left never shows
/// under the next story, and does not hold the next story's buttons.
#[gpui_kit::test]
fn a_late_story_answer_stays_with_its_story(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let request = quill::ids::RequestId(900);
    app.update(&mut vcx, |app, cx| {
        app.stories
            .with_viewer_ui(|open| open.page_op = Some(request));
        if let Some(session) = app.demo_session.as_deref_mut() {
            let manage = &mut session.stories.manage;
            manage.pending = true;
            manage.story = Some((LEO.0, 3));
            manage.request = Some(request);
            session.stories.page_op = Some(StoryPageOp {
                label: "hiding stories".into(),
                state: StoryPageOpState::Sending,
                request: Some(request),
            });
        }
        assert!(app.story_manage_status().is_some(), "Saving… under story 3");
        assert!(app.viewer_story_page_op().is_some());
        app.step_story_viewer(1, cx);
    });
    frames(&mut vcx);
    app.update(&mut vcx, |app, _| {
        let Some((chat_id, story_id)) = app.stories.viewer.current().map(|item| item.key()) else {
            panic!("the viewer moved on");
        };
        assert_ne!(chat_id, LEO);
        let busy = app
            .session()
            .is_some_and(|s| s.stories.manage.pending_for(chat_id.0, story_id));
        assert!(
            !busy,
            "the next story's Edit, Cover and Privacy stay enabled"
        );
        // Both answers land now, as errors.
        if let Some(session) = app.demo_session.as_deref_mut() {
            let manage = &mut session.stories.manage;
            manage.pending = false;
            manage.request = None;
            manage.error = Some("Story update failed: STORY_ID_INVALID".into());
            session.stories.page_op = Some(StoryPageOp {
                label: "hiding stories".into(),
                state: StoryPageOpState::Failed("PEER_ID_INVALID".into()),
                request: Some(request),
            });
        }
        assert_eq!(app.story_manage_status(), None);
        assert!(app.viewer_story_page_op().is_none());
    });
}
