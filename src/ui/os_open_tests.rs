use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::status_note::NoteKind;
use futures_channel::oneshot;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::lang::{keys, tr};
use quill::platform::{OsOpen, script_next_os_open};

const URL: &str = "https://example.com/a";

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn note(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (String, NoteKind) {
    app.read_with(vcx, |app, _| {
        let note = app.connection.note();
        (note.text().to_string(), note.kind())
    })
}

fn open_link(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> oneshot::Sender<bool> {
    let (answer, answered) = oneshot::channel();
    script_next_os_open(OsOpen::Pending(answered));
    app.update_in(vcx, |app, _, cx| app.open_message_url(URL, cx));
    vcx.run_until_parked();
    answer
}

#[gpui_kit::test]
fn a_link_says_nothing_until_the_shell_answers(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let before = note(&app, vcx);
    let answer = open_link(&app, vcx);
    assert_eq!(note(&app, vcx), before);

    answer.send(true).unwrap();
    vcx.run_until_parked();
    assert_eq!(
        note(&app, vcx),
        (tr(keys::Q_CHAT_OPENED_LINK).to_string(), NoteKind::Info)
    );
}

#[gpui_kit::test]
fn a_late_failure_shows_the_error(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let answer = open_link(&app, vcx);
    answer.send(false).unwrap();
    vcx.run_until_parked();
    assert_eq!(
        note(&app, vcx),
        (
            tr(keys::Q_CHAT_COULD_NOT_OPEN_LINK).to_string(),
            NoteKind::Error
        )
    );
}

#[gpui_kit::test]
fn a_thread_that_died_counts_as_a_failure(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    drop(open_link(&app, vcx));
    vcx.run_until_parked();
    assert_eq!(note(&app, vcx).1, NoteKind::Error);
}

#[gpui_kit::test]
fn a_known_answer_shows_in_the_same_call(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    script_next_os_open(OsOpen::Answered(false));
    app.update_in(vcx, |app, _, cx| {
        app.open_message_url(URL, cx);
        assert_eq!(app.connection.note().kind(), NoteKind::Error);
    });
}

#[gpui_kit::test]
fn the_sound_settings_note_comes_with_a_late_failure(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (answer, answered) = oneshot::channel();
    app.update_in(vcx, |app, _, cx| {
        app.sound_prefs_answer(OsOpen::Pending(answered), cx);
    });
    answer.send(false).unwrap();
    vcx.run_until_parked();
    assert_eq!(
        note(&app, vcx),
        (
            tr(keys::Q_OPS_COULDNT_OPEN_THE_SYSTEM_SOUND).to_string(),
            NoteKind::Error
        )
    );
}

#[gpui_kit::test]
fn license_notices_that_fail_late_open_the_published_copy(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (answer, answered) = oneshot::channel();
    app.update_in(vcx, |app, _, cx| {
        app.published_notices_unless_opened(OsOpen::Pending(answered), cx);
    });
    vcx.run_until_parked();
    assert_eq!(vcx.opened_url(), None);

    answer.send(false).unwrap();
    vcx.run_until_parked();
    assert_eq!(vcx.opened_url().as_deref(), Some(quill::about::NOTICES_URL));
}

#[gpui_kit::test]
fn license_notices_that_open_stay_off_the_web(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let (answer, answered) = oneshot::channel();
    app.update_in(vcx, |app, _, cx| {
        app.published_notices_unless_opened(OsOpen::Pending(answered), cx);
    });
    answer.send(true).unwrap();
    vcx.run_until_parked();
    assert_eq!(vcx.opened_url(), None);
}
