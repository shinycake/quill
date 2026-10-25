//! A user who accepts no voice or video messages (tdesktop
//! `VoiceMessagesForbidden`): the record button is drawn forbidden and a
//! click names the refusal instead of starting a recording. On a demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::send_button::SendButtonKind;
use quill::send_rights::SendKind;

/// "Demo chat A", the private chat with user 11.
const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext, kind: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(kind);
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
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    (app, vcx)
}

fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(CHAT, window, cx);
    });
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn a_click_on_the_forbidden_record_button_names_the_user(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-voice-restricted");
    open(&app, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(app.record_forbidden(SendButtonKind::Record { video: false }));
        assert!(app.record_forbidden(SendButtonKind::Record { video: true }));
        assert!(!app.record_forbidden(SendButtonKind::Send));
        assert_eq!(app.send_denial(SendKind::Message), None, "text still goes");
        assert_eq!(app.send_denial(SendKind::Photos), None, "photos still go");
    });
    vcx.update(|window, cx| window.click("composer-send", cx));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(!app.recording_busy(), "no recording started");
        assert_eq!(
            app.connection.status_note(),
            "Demo chat A doesn't accept voice messages."
        );
    });
    app.update_in(&mut vcx, |app, _, cx| app.start_video_note_recording(cx));
    app.read_with(&vcx, |app, _| {
        assert!(!app.recording_busy());
        assert_eq!(
            app.connection.status_note(),
            "Demo chat A doesn't accept video messages."
        );
    });
}

#[gpui_kit::test]
fn the_record_button_is_live_for_a_user_who_accepts_voice_messages(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    open(&app, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(!app.record_forbidden(SendButtonKind::Record { video: false }));
        assert_eq!(app.send_denial(SendKind::VoiceMessages), None);
        assert_eq!(app.send_denial(SendKind::VideoMessages), None);
    });
}
