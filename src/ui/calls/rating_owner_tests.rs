//! A rating drafted on one call's end screen belongs to that call: the
//! next call's end screen neither shows the draft nor submits it.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::CallSummary;

fn open_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-chats");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn ended(call_id: i32) -> CallSummary {
    CallSummary {
        call_id,
        user_id: 12,
        is_outgoing: true,
        is_video: false,
        duration_secs: 30,
        had_audio: true,
        end_line: String::new(),
        end: quill::state::CallEnd::Ended,
        rating: quill::state::CallRating::Offered,
        debug_information: quill::state::CallUpload::NotNeeded,
        log: quill::state::CallUpload::NotNeeded,
        final_transport: None,
        reconnect_attempts: 0,
        muted: false,
    }
}

fn show(app: &mut QuillApp, call_id: i32) {
    if let Some(session) = app.demo_session.as_deref_mut() {
        session.calls.summary = Some(ended(call_id));
    }
}

fn rating_sent(app: &QuillApp) -> Option<bool> {
    let session = app.demo_session.as_deref()?;
    Some(session.calls.summary.as_ref()?.rating == quill::state::CallRating::Sent)
}

fn draft_of(app: &QuillApp) -> Option<quill::state::RatingDraft> {
    let session = app.demo_session.as_deref()?;
    session.calls.summary.as_ref()?.rating.draft().cloned()
}

#[gpui_kit::test]
fn a_draft_for_an_earlier_call_is_not_submitted_for_the_next(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        show(app, 7);
        app.open_rating_detail(2, window, cx);
        assert!(draft_of(app).is_some());
        // Another call ended and its summary replaced the first one: the
        // draft was the first summary's own stage and went with it.
        show(app, 8);
        assert!(draft_of(app).is_none());
        app.submit_call_rating(cx);
        assert_eq!(rating_sent(app), Some(false), "call 8 was never rated");
    });
}

#[gpui_kit::test]
fn a_draft_submits_for_its_own_call_and_goes_with_its_summary(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        show(app, 7);
        app.open_rating_detail(5, window, cx);
        app.submit_call_rating(cx);
        assert_eq!(rating_sent(app), Some(true));
        show(app, 9);
        app.open_rating_detail(1, window, cx);
        app.dismiss_call_summary(cx);
        assert!(draft_of(app).is_none(), "the draft went with its summary");
    });
}

#[gpui_kit::test]
fn the_editor_edits_the_summarys_own_draft(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        show(app, 7);
        app.open_rating_detail(2, window, cx);
        app.set_rating_stars(4, cx);
        app.toggle_rating_problem(1, cx);
        let draft = draft_of(app).expect("editing");
        assert_eq!(draft.stars, 4);
        assert!(draft.problems[1] && !draft.problems[0]);
        app.toggle_rating_problem(1, cx);
        assert!(!draft_of(app).expect("editing").problems[1]);
    });
}

fn summary_shown(app: &QuillApp) -> bool {
    app.demo_session
        .as_deref()
        .is_some_and(|session| session.calls.summary.is_some())
}

#[gpui_kit::test]
fn a_redial_dismisses_the_summary_when_the_dial_goes_out_and_a_plain_dial_keeps_it(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = open_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        show(app, 7);
        app.open_rating_detail(3, window, cx);
        // A call placed from somewhere else leaves the end card to be rated.
        app.dial_user(12, false, cx);
        assert!(summary_shown(app));
        assert!(draft_of(app).is_some());
        // Call again from the end card itself replaces it.
        app.redial(12, false, cx);
        assert!(!summary_shown(app));
        assert!(draft_of(app).is_none());
    });
}
