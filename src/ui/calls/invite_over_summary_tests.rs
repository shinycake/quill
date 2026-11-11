//! A conference invitation that rings after a 1:1 call ended (no rating
//! asked) takes the call window and the ring: an older call's end screen
//! does not hide it.
use crate::ui::app::QuillApp;
use crate::ui::call_panel::CallPhase;
use crate::ui::call_sounds::CallSound;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::state::{CallSummary, ConferenceInvite};

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

fn ended_without_rating(call_id: i32) -> CallSummary {
    CallSummary {
        call_id,
        user_id: 12,
        is_outgoing: true,
        is_video: false,
        duration_secs: 30,
        had_audio: true,
        end_line: String::new(),
        need_rating: false,
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

#[gpui_kit::test]
fn an_invitation_after_an_ended_call_rings_in_the_call_window(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.update(&mut vcx, |app, cx| {
        let Some(session) = app.demo_session.as_deref_mut() else {
            panic!("the demo has a session");
        };
        session.calls.active_call = None;
        // A call ended long ago with no rating asked: its summary stays,
        // its window has closed, and now an invitation rings.
        session.calls.summary = Some(ended_without_rating(7));
        session.calls.conference_invite = Some(ConferenceInvite {
            chat_id: ChatId(41),
            message_id: MessageId(90 << 20),
            unique_id: 7001,
            user_id: 41,
            is_video: false,
            others: Vec::new(),
            rings_until: i64::MAX,
        });
        app.sync_call_sounds();
        app.sync_call_windows(cx);
        assert_eq!(app.calls.sounds.looping(), Some(CallSound::Incoming));
        let phase = app.call_snapshot().map(|snap| snap.phase);
        assert!(
            phase == Some(CallPhase::ConferenceInvite),
            "the call window shows Join and Decline"
        );
    });
    vcx.run_until_parked();
    app.update(&mut vcx, |app, _| {
        assert!(app.calls.window.is_some(), "the call window is open");
    });
}

/// Round 15 C finding 3: an invitation with no call and no end screen
/// keeps the call tick, the one thing that loops its ringtone.
#[gpui_kit::test]
fn an_invitation_alone_keeps_the_call_tick(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.update(&mut vcx, |app, cx| {
        let Some(session) = app.demo_session.as_deref_mut() else {
            panic!("the demo has a session");
        };
        session.calls.active_call = None;
        session.calls.summary = None;
        session.calls.conference_invite = Some(ConferenceInvite {
            chat_id: ChatId(41),
            message_id: MessageId(90 << 20),
            unique_id: 7002,
            user_id: 41,
            is_video: false,
            others: Vec::new(),
            rings_until: i64::MAX,
        });
        app.ensure_call_tick(cx);
        assert!(app.calls.tick_active, "the tick starts");
    });
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(3));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.calls.tick_active, "and keeps beating");
        assert_eq!(app.calls.sounds.looping(), Some(CallSound::Incoming));
    });
}

/// Round 15 C finding 2: a second call ringing in during a call under the
/// lock rings, and the lock screen shows it with Decline and End & Answer
/// (its swap box is a layer of the locked window, not drawn).
#[gpui_kit::test]
fn a_second_call_under_the_lock_rings_and_can_be_declined(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    use quill::state::{ActiveCall, SecondCall};
    use quill::telegram::envelope::CallState;
    let (app, mut vcx) = open_app(cx);
    app.update(&mut vcx, |app, cx| {
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
        let Some(session) = app.demo_session.as_deref_mut() else {
            panic!("the demo has a session");
        };
        session.calls.summary = None;
        session.calls.active_call = Some(ActiveCall::fresh(
            5,
            12,
            true,
            false,
            CallState::Ready,
            None,
        ));
        session.calls.second_call = SecondCall::Prompt {
            call_id: 6,
            user_id: 41,
            is_video: false,
        };
        app.sync_call_sounds();
        assert_eq!(app.calls.sounds.looping(), Some(CallSound::Incoming));
        cx.notify();
    });
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    let drawn = |vcx: &mut VisualTestContext, id: &'static str| {
        vcx.update(|window, _| window.try_find(id).is_some())
    };
    assert!(drawn(&mut vcx, "lock-swap-call-answer"));
    vcx.update(|window, cx| window.click("lock-swap-call-decline", cx));
    vcx.run_until_parked();
    app.update(&mut vcx, |app, _| {
        let prompt = app
            .session()
            .and_then(|session| session.calls.second_call.prompt());
        assert!(prompt.is_none(), "declined");
        assert!(app.account.passcode.is_locked(), "still locked");
        app.sync_call_sounds();
        assert_ne!(app.calls.sounds.looping(), Some(CallSound::Incoming));
    });
}
