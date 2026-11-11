#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! "Set Photo" in the profile crop renders off the UI thread and applies
//! only while the box that asked is still open (sweep 12-A002).

use crate::ui::app::QuillApp;
use crate::ui::dialogs::edit_profile::EditProfileDialog;
use crate::ui::keybindings::bind_keys;
use crate::ui::photo_editor::{EditorPurpose, PhotoEditor};
use crate::ui::profile_photo_sources::PhotoTarget;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

/// The note before and after pressing Set Photo.
fn set_photo(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    close_meanwhile: bool,
) -> (String, String) {
    let before = app.read_with(vcx, |app, _| app.connection.note().text().to_string());
    app.update_in(vcx, |app, window, cx| {
        app.dialogs.edit_profile_dialog =
            Some(EditProfileDialog::new(window, cx, "Ada", "", "", ""));
        let editor = PhotoEditor::new(
            EditorPurpose::Profile(PhotoTarget::Profile),
            image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255])),
        );
        app.finish_profile_crop(editor, PhotoTarget::Profile, cx);
        if close_meanwhile {
            app.dialogs.edit_profile_dialog = None;
        }
    });
    vcx.run_until_parked();
    let after = app.read_with(vcx, |app, _| app.connection.note().text().to_string());
    (before, after)
}

#[gpui_kit::test]
fn the_photo_is_applied_while_its_dialog_is_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let (before, after) = set_photo(&app, &mut vcx, false);
    assert_ne!(
        before, after,
        "the demo has no account: the open dialog is told so"
    );
}

#[gpui_kit::test]
fn a_dialog_closed_before_the_render_ends_gets_no_photo(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let (before, after) = set_photo(&app, &mut vcx, true);
    assert_eq!(before, after, "nothing is applied or said");
}
