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
        rating_sent: false,
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
