//! What the passcode lock reaches: a recording stops, a new one is
//! refused, and a link that resolves while locked waits for the unlock.

use std::sync::Arc;

use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::state::DeepLinkAction;
use quill::voice::VoiceCapture;

use crate::ui::app::QuillApp;
use crate::ui::links_state::PendingOpen;
use crate::ui::recording_state::CaptureFor;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;

const CHAT: ChatId = ChatId(11);
const OTHER: ChatId = ChatId(12);

fn open_chat(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("demo app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(CHAT, window, cx);
        app.recording.open_voice = Arc::new(|| {
            let path = std::env::temp_dir().join("quill-lock-gate-test.ogg");
            Ok(VoiceCapture::preview(path, 1, vec![4]))
        });
    });
    frame(&mut vcx);
    (app, vcx)
}

fn frame(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
}

fn lock(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn locking_stops_a_voice_recording_without_sending_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    app.update(&mut vcx, |app, cx| {
        app.start_voice_capture(CaptureFor::Message, cx)
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| assert!(app.recording_active(), "recording"));
    lock(&app, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(app.account.passcode.locked);
        assert!(!app.recording_busy(), "the microphone is off");
    });
}

#[gpui_kit::test]
fn no_recording_starts_while_locked(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    lock(&app, &mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.start_voice_capture(CaptureFor::Message, cx);
        app.start_video_capture(CaptureFor::Message, false, cx);
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(!app.recording_busy(), "the start was refused");
    });
}

#[gpui_kit::test]
fn a_link_resolved_while_locked_opens_after_the_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    lock(&app, &mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.links.pending_deep_link_open = Some(PendingOpen::user(
            OTHER,
            DeepLinkAction::OpenUser { user_id: OTHER.0 },
        ));
        cx.notify();
    });
    frame(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(app.links.pending_deep_link_open.is_some(), "held");
        assert_eq!(
            app.open_chat_id(),
            Some(CHAT),
            "nothing opened under the lock"
        );
    });
    app.update(&mut vcx, |app, cx| {
        app.account.passcode.locked = false;
        cx.notify();
    });
    frame(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(
            app.links.pending_deep_link_open.is_none(),
            "the link ran after the unlock"
        );
    });
}
