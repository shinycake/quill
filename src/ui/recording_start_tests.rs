use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::voice::VoiceCapture;

use crate::ui::app::QuillApp;
use crate::ui::recording_state::CaptureFor;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;

const CHAT: ChatId = ChatId(11);

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
    });
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    (app, vcx)
}

fn fake_microphone(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Arc<AtomicUsize> {
    let opens = Arc::new(AtomicUsize::new(0));
    let counted = opens.clone();
    app.update(vcx, |app, _| {
        app.recording.open_voice = Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join("quill-voice-start-test.ogg");
            Ok(VoiceCapture::preview(path, 1, vec![4]))
        });
    });
    opens
}

fn start(app: &Entity<QuillApp>, purpose: CaptureFor, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.start_voice_capture(purpose, cx));
}

#[gpui_kit::test]
fn pressing_record_returns_before_the_microphone_is_opened(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    let opens = fake_microphone(&app, &mut vcx);
    start(&app, CaptureFor::Message, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert_eq!(opens.load(Ordering::SeqCst), 0, "nothing opened yet");
        assert!(app.recording_busy(), "no second recording may start");
        assert!(!app.recording_active(), "no bar before the device is up");
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert!(app.recording_active(), "the capture is held");
        assert!(app.recording_here());
    });
}

#[gpui_kit::test]
fn a_start_cancelled_while_the_microphone_opens_leaves_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    let opens = fake_microphone(&app, &mut vcx);
    start(&app, CaptureFor::Message, &mut vcx);
    app.update(&mut vcx, |app, cx| app.cancel_recording_here(cx));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert!(!app.recording_active(), "what arrived was dropped");
        assert!(!app.recording_busy(), "the next recording may start");
    });
}

#[gpui_kit::test]
fn record_works_again_while_a_cancelled_microphone_has_not_answered(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    let opens = fake_microphone(&app, &mut vcx);
    start(&app, CaptureFor::Message, &mut vcx);
    app.update(&mut vcx, |app, cx| app.cancel_recording_here(cx));
    app.read_with(&vcx, |app, _| {
        assert!(!app.recording_busy(), "the record button answers again");
    });
    start(&app, CaptureFor::Message, &mut vcx);
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(
            opens.load(Ordering::SeqCst),
            2,
            "the second press opened too"
        );
        assert!(app.recording_active(), "the second press records");
    });
}

#[gpui_kit::test]
fn closing_the_story_reply_cancels_a_start_on_its_way_up(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    let _opens = fake_microphone(&app, &mut vcx);
    start(&app, CaptureFor::StoryReply, &mut vcx);
    app.update(&mut vcx, |app, cx| app.end_story_reply(cx));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(!app.recording_active());
        assert!(!app.recording_busy());
    });
}

#[gpui_kit::test]
fn a_microphone_that_will_not_open_says_why(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    app.update(&mut vcx, |app, _| {
        app.recording.open_voice = Arc::new(|| Err("No microphone found.".to_string()));
    });
    start(&app, CaptureFor::Message, &mut vcx);
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(!app.recording_busy());
        assert_eq!(app.connection.status_note(), "No microphone found.");
    });
}

#[gpui_kit::test]
fn a_cancelled_start_that_failed_says_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    app.update(&mut vcx, |app, _| {
        app.recording.open_voice = Arc::new(|| Err("No microphone found.".to_string()));
    });
    start(&app, CaptureFor::Message, &mut vcx);
    app.update(&mut vcx, |app, cx| app.cancel_recording_here(cx));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(!app.recording_busy());
        assert_ne!(app.connection.status_note(), "No microphone found.");
    });
}
