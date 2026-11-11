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
        need_rating: true,
        busy: false,
        need_debug_information: false,
        need_log: false,
        rating: quill::state::CallRating::Offered,
        debug_information_sent: false,
        debug_information_error: None,
        log_sent: false,
        log_error: None,
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

#[gpui_kit::test]
fn a_draft_for_an_earlier_call_is_not_submitted_for_the_next(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        show(app, 7);
        app.open_rating_detail(2, window, cx);
        assert!(app.dialogs.rating_detail.is_some());
        // Another call ended and its summary replaced the first one.
        show(app, 8);
        app.submit_call_rating(cx);
        assert_eq!(rating_sent(app), Some(false), "call 8 was never rated");
        assert!(app.dialogs.rating_detail.is_none());
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
        assert!(
            app.dialogs.rating_detail.is_none(),
            "the draft went with its summary"
        );
    });
}
