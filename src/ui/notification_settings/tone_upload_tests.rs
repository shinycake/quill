//! The sound picker's "Upload Sound" against a demo window (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::notification_settings::SoundPickerTarget;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::tone_upload::ToneUpload;

const BUTTON: &str = "notif-sound-list-upload";
const PENDING: &str = "notif-sound-list-pending-radio";

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-notify-upload-sound");
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("demo window did not build");
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn present(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

fn tone_file(name: &str, size: usize) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("quill-tone-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(name);
    std::fs::write(&path, vec![0u8; size]).expect("temp file");
    path
}

fn upload_state(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ToneUpload> {
    app.update(vcx, |app, _| {
        app.session().map(|s| s.settings.tone_upload.clone())
    })
}

fn idle(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.tone_upload = ToneUpload::Idle;
        }
        cx.notify();
    });
    frames(vcx);
}

#[gpui_kit::test]
fn the_picker_offers_upload_sound_and_shows_the_file_in_flight(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    assert!(present(&mut vcx, BUTTON), "Upload Sound button");
    assert!(present(&mut vcx, PENDING), "the demo file is uploading");
    idle(&app, &mut vcx);
    assert!(present(&mut vcx, BUTTON));
    assert!(!present(&mut vcx, PENDING));
}

#[gpui_kit::test]
fn a_picked_file_adds_a_pending_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    idle(&app, &mut vcx);
    let path = tone_file("Glass Chime.mp3", 2000);
    app.update(&mut vcx, |app, cx| {
        app.begin_tone_upload(SoundPickerTarget::Chat(ChatId(11)), &path, cx);
    });
    frames(&mut vcx);
    assert!(present(&mut vcx, PENDING));
    assert_eq!(
        upload_state(&app, &mut vcx),
        Some(ToneUpload::Uploading {
            name: "Glass Chime".into()
        })
    );
}

#[gpui_kit::test]
fn a_file_that_is_not_an_mp3_adds_no_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    idle(&app, &mut vcx);
    let path = tone_file("notes.txt", 20);
    app.update(&mut vcx, |app, cx| {
        app.begin_tone_upload(SoundPickerTarget::Chat(ChatId(11)), &path, cx);
    });
    frames(&mut vcx);
    assert!(!present(&mut vcx, PENDING));
    assert_eq!(upload_state(&app, &mut vcx), Some(ToneUpload::Idle));
}

#[gpui_kit::test]
fn the_new_sound_is_selected_where_the_upload_started(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, cx| {
        app.notify.tone_upload_target = Some(SoundPickerTarget::Chat(ChatId(11)));
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.tone_upload = ToneUpload::Added { sound_id: 901 };
        }
        app.settle_tone_upload(cx);
    });
    frames(&mut vcx);
    assert_eq!(upload_state(&app, &mut vcx), Some(ToneUpload::Idle));
    let sound = app.update(&mut vcx, |app, _| {
        app.session()
            .and_then(|s| s.chats.get(&11))
            .map(|chat| chat.notification_settings.sound_id)
    });
    assert_eq!(sound, Some(901));
}
