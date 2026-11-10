//! The story composer's stage through the real app: Post and Save are
//! answered by their own request, never by another composer's or call's.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, RequestId};
use quill::state::{StoryCheck, StoryPostOutcome};
use quill::story_composer::ComposerStage;
use quill::telegram::envelope::CanPostStoryResult;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the window made the app");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn answer_check(
    app: &mut QuillApp,
    request: RequestId,
    result: Result<CanPostStoryResult, String>,
) {
    if let Some(session) = app.demo_session.as_deref_mut() {
        session.stories.post.check = StoryCheck::Answered { request, result };
    }
}

/// A `canPostStory` answer from a composer that was closed does not reach
/// the composer opened after it, and the right answer does.
#[gpui_kit::test]
fn a_check_answer_reaches_only_the_composer_that_asked(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_story_composer(window, cx);
        app.stories.composer.stage = ComposerStage::Checking {
            request: RequestId(2),
            target: ChatId(5),
        };
        // The first composer's check (request 1) answers late.
        answer_check(app, RequestId(1), Err("Eligibility check failed".into()));
        app.story_composer_upkeep(cx);
        assert_eq!(
            app.stories.composer.stage,
            ComposerStage::Checking {
                request: RequestId(2),
                target: ChatId(5)
            },
            "still waiting for its own answer"
        );
        assert_eq!(app.stories.composer.local_error, None);
        answer_check(app, RequestId(2), Err("Eligibility check failed".into()));
        app.story_composer_upkeep(cx);
        assert_eq!(app.stories.composer.stage, ComposerStage::Editing);
        assert_eq!(
            app.stories.composer.local_error.as_deref(),
            Some("Eligibility check failed")
        );
    });
}

/// A post that failed or finished for an earlier composer does not show in
/// this one; its own outcome does, and a posted story keeps Post off.
#[gpui_kit::test]
fn a_post_outcome_reaches_only_its_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_story_composer(window, cx);
        app.stories.composer.stage = ComposerStage::Posting {
            request: RequestId(8),
        };
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.post.outcome = StoryPostOutcome::Failed {
                request: RequestId(7),
                message: "Posting failed: old".into(),
            };
        }
        app.story_composer_upkeep(cx);
        assert_eq!(
            app.stories.composer.stage,
            ComposerStage::Posting {
                request: RequestId(8)
            }
        );
        assert_eq!(app.stories.composer.local_error, None);
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.post.outcome = StoryPostOutcome::Succeeded {
                request: RequestId(8),
            };
        }
        app.story_composer_upkeep(cx);
        assert_eq!(app.stories.composer.stage, ComposerStage::Posted);
        // A second press of Post does nothing: no check, no second post.
        app.story_composer_post(cx);
        assert_eq!(app.stories.composer.stage, ComposerStage::Posted);
        assert_eq!(app.stories.composer.local_error, None);
    });
}

#[gpui_kit::test]
fn a_failed_post_shows_why_and_frees_the_button(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_story_composer(window, cx);
        app.stories.composer.stage = ComposerStage::Posting {
            request: RequestId(8),
        };
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.post.outcome = StoryPostOutcome::Failed {
                request: RequestId(8),
                message: "Posting failed: FLOOD".into(),
            };
        }
        app.story_composer_upkeep(cx);
        assert_eq!(app.stories.composer.stage, ComposerStage::Editing);
        assert_eq!(
            app.stories.composer.local_error.as_deref(),
            Some("Posting failed: FLOOD")
        );
    });
}

/// The edit's Save settles by its own request: another management call
/// taking the shared slot neither closes the composer nor fails it.
#[gpui_kit::test]
fn a_save_settles_by_its_own_request(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_story_edit(5, 3, window, cx);
        app.stories.composer.stage = ComposerStage::Saving {
            request: RequestId(30),
        };
        if let Some(session) = app.demo_session.as_deref_mut() {
            let manage = &mut session.stories.manage;
            manage.start(RequestId(30), ChatId(5), 3);
            // The viewer's cover editor takes the slot, and it fails first.
            manage.start(RequestId(31), ChatId(5), 3);
            manage.settle(RequestId(31), Some("Story update failed: COVER".into()));
        }
        app.story_composer_upkeep(cx);
        assert_eq!(
            app.stories.composer.stage,
            ComposerStage::Saving {
                request: RequestId(30)
            },
            "the cover's failure is not the edit's"
        );
        assert_eq!(app.stories.composer.local_error, None);
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.stories.manage.settle(RequestId(30), None);
        }
        app.story_composer_upkeep(cx);
        assert!(!app.stories.composer.open, "its own answer closed it");
    });
}

#[gpui_kit::test]
fn a_failed_save_shows_its_reason(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_story_edit(5, 3, window, cx);
        app.stories.composer.stage = ComposerStage::Saving {
            request: RequestId(40),
        };
        if let Some(session) = app.demo_session.as_deref_mut() {
            let manage = &mut session.stories.manage;
            manage.start(RequestId(40), ChatId(5), 3);
            manage.start(RequestId(41), ChatId(5), 3);
            manage.settle(RequestId(40), Some("Story update failed: GONE".into()));
        }
        app.story_composer_upkeep(cx);
        assert_eq!(app.stories.composer.stage, ComposerStage::Editing);
        assert_eq!(
            app.stories.composer.local_error.as_deref(),
            Some("Story update failed: GONE")
        );
        assert!(app.stories.composer.open);
    });
}

/// The cover picker's failed save shows under its story although the
/// privacy editor's call took the slot after it.
#[gpui_kit::test]
fn a_failed_cover_save_shows_under_its_story(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            let manage = &mut session.stories.manage;
            manage.start(RequestId(50), ChatId(5), 3);
            manage.start(RequestId(51), ChatId(5), 3);
            manage.settle(RequestId(50), Some("Story update failed: COVER".into()));
            assert_eq!(
                manage.error_for(5, 3),
                None,
                "the slot is the privacy call's"
            );
        }
        app.show_story_manage_failure(RequestId(50), Some((5, 3)));
        let shown = app
            .session()
            .and_then(|s| s.stories.manage.error_for(5, 3).map(str::to_string));
        assert_eq!(shown.as_deref(), Some("Story update failed: COVER"));
    });
}
