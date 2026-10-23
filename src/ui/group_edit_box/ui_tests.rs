//! The Edit box: it opens filled from the chat, an empty name blocks
//! Save, Save closes it, and Escape discards.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(
    cx: &mut TestAppContext,
    name: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(name);
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, shell, vcx)
}

fn title_text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| {
        app.group_edit_box()
            .map(|edit| edit.title_input.read(cx).value().to_string())
            .unwrap_or_default()
    })
}

fn type_title(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            if let Some(edit) = app.group_edit_box() {
                edit.title_input
                    .update(cx, |input, cx| input.set_value(text, window, cx));
            }
        });
    });
    frames(vcx);
}

#[gpui_kit::test]
fn the_box_opens_filled_with_its_rows(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupEditBox)));
    assert_eq!(title_text(&app, vcx), "Demo supergroup");
    let description = app.read_with(vcx, |app, cx| {
        app.group_edit_box()
            .map(|edit| edit.description_input.read(cx).value().to_string())
            .unwrap_or_default()
    });
    assert!(description.starts_with("Weekend hikes"));
    // Kit components register for lookup; the manage rows are plain
    // rows and show in the screenshots instead.
    for id in ["group-edit-photo", "group-edit-save", "group-edit-cancel"] {
        assert!(
            vcx.update(|window, _| window.try_find(id).is_some()),
            "{id}"
        );
    }
}

#[gpui_kit::test]
fn an_empty_name_keeps_the_box_open(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-group-edit-box");
    let vcx = &mut vcx;
    type_title(&app, vcx, "   ");
    vcx.update(|window, cx| window.click("group-edit-save", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_some()));
    assert!(!app.read_with(vcx, |app, _| {
        app.connection.status_note().contains("Demo mode")
    }));
}

#[gpui_kit::test]
fn save_sends_and_closes(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-channel-edit-box");
    let vcx = &mut vcx;
    type_title(&app, vcx, "Quill Builds");
    vcx.update(|window, cx| window.click("group-edit-save", cx));
    frames(vcx);
    // The demo has no live session: the save reaches the send step.
    assert!(app.read_with(vcx, |app, _| {
        app.connection.status_note().contains("Demo mode")
    }));
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_none()));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::GroupEditBox)));
}

#[gpui_kit::test]
fn escape_discards_the_edits(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-basic-group-edit-box");
    let vcx = &mut vcx;
    type_title(&app, vcx, "Unsaved");
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.group_edit_box().is_none()));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::GroupEditBox)));
}

#[gpui_kit::test]
fn the_crop_shows_over_the_box_and_hands_it_back(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-group-photo-crop");
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |s, _| s.open_dialog
        == Some(DialogKind::ProfilePhotoCrop)));
    vcx.update(|window, cx| window.click("photo-editor-done", cx));
    frames(vcx);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::GroupEditBox)));
    assert!(app.read_with(vcx, |app, _| {
        app.group_edit_box()
            .is_some_and(|edit| edit.photo.is_some())
    }));
}
