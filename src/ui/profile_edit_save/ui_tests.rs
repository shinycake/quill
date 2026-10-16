//! The edit-profile form: one Save, disabled until a change, Escape
//! discards, and the labelled fields are there.

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

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-profile-edit");
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

fn type_bio(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, text: &str) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            if let Some(dialog) = &app.dialogs.edit_profile_dialog {
                dialog
                    .bio_input
                    .update(cx, |input, cx| input.set_value(text, window, cx));
            }
        });
    });
    frames(vcx);
}

#[gpui_kit::test]
fn save_is_disabled_until_a_field_changes(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert!(!app.read_with(vcx, |app, cx| app.profile_form_can_save(cx)));
    // A click on the disabled Save sends nothing: the demo-mode note a
    // real attempt would leave stays unset.
    vcx.update(|window, cx| window.click("edit-profile-save", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| {
        !app.connection.status_note.contains("Demo mode")
    }));
    type_bio(&app, vcx, "A new bio");
    assert!(app.read_with(vcx, |app, cx| app.profile_form_can_save(cx)));
    vcx.update(|window, cx| window.click("edit-profile-save", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| {
        app.connection.status_note.contains("Demo mode")
    }));
    // Typing the opening text back disables it again.
    let original = app.read_with(vcx, |app, _| {
        app.dialogs
            .edit_profile_dialog
            .as_ref()
            .map(|d| d.initial.bio.clone())
            .unwrap_or_default()
    });
    type_bio(&app, vcx, &original);
    assert!(!app.read_with(vcx, |app, cx| app.profile_form_can_save(cx)));
    // Over the bio limit it stays disabled.
    type_bio(&app, vcx, &"x".repeat(200));
    assert!(!app.read_with(vcx, |app, cx| app.profile_form_can_save(cx)));
}

#[gpui_kit::test]
fn the_form_is_one_form_with_one_save(cx: &mut TestAppContext) {
    let (_app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    for id in [
        "edit-profile-save",
        "edit-profile-cancel",
        "edit-profile-set-photo",
    ] {
        assert!(
            vcx.update(|window, _| window.try_find(id).is_some()),
            "{id}"
        );
    }
    // The per-field saves and the path box are gone.
    for id in [
        "edit-profile-save-name",
        "edit-profile-save-bio",
        "edit-profile-close",
    ] {
        assert!(
            vcx.update(|window, _| window.try_find(id).is_none()),
            "{id}"
        );
    }
}

#[gpui_kit::test]
fn escape_discards_the_edits(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::EditProfile)));
    type_bio(&app, vcx, "Unsaved");
    vcx.update(|window, cx| window.press("escape", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.dialogs.edit_profile_dialog.is_none()));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::EditProfile)));
}

#[gpui_kit::test]
fn cancel_closes_without_saving(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    type_bio(&app, vcx, "Unsaved");
    vcx.update(|window, cx| window.click("edit-profile-cancel", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.dialogs.edit_profile_dialog.is_none()));
}
