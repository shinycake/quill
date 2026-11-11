//! "Send as archive": the zip answers for the chat it was dropped in, and
//! its note never outlives the answer (needs the `demo-capture` feature
//! for gpui-kit's test support).
use super::ArchiveJob;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::folder_archive::ArchiveError;
use quill::lang::{keys, tr};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-media")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("window built");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

/// Sweep 03-050, 09-055: an answer for a chat that is no longer open is
/// dropped without a note of its own, and the "Preparing" note goes.
#[gpui_kit::test]
fn an_archive_for_another_chat_leaves_neither_its_error_nor_its_note(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        let mut slot = app.send_slot().expect("a chat is open");
        slot.draft.chat_id += 1;
        app.composer_ui.archive = ArchiveJob::Building(slot.clone());
        app.set_note(StatusNote::silent(tr(keys::Q_MISC_PREPARING_THE_ARCHIVE)));
        app.archive_ready(slot, Err(ArchiveError::TooLarge), cx);
        assert_eq!(app.composer_ui.archive, ArchiveJob::Idle);
        assert!(app.connection.note().is_empty(), "no error for this chat");
    });
}

/// The answer for the open chat still shows its failure there.
#[gpui_kit::test]
fn an_archive_failing_in_the_open_chat_says_so(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        let slot = app.send_slot().expect("a chat is open");
        app.composer_ui.archive = ArchiveJob::Building(slot.clone());
        app.archive_ready(slot, Err(ArchiveError::TooLarge), cx);
        assert_eq!(app.composer_ui.archive, ArchiveJob::Idle);
        assert!(!app.connection.note().is_empty());
    });
}
