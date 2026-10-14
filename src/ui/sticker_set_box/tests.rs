//! UI tests on the `ready-sticker-set-owned` demo: the creator's footer and
//! "+" cell, reorder by drag behind "Edit Stickers", the rename box, the
//! delete-sticker box and "Delete for Everyone". The demo has no sender, so
//! edits land in its own session.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::StickerSetViewStage;

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
    let demo = ScreenshotDemo::named("ready-sticker-set-owned");
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

fn viewed_ids(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<i32> {
    app.read_with(vcx, |app, _| {
        match &app
            .demo_session
            .as_ref()
            .and_then(|s| s.stickers.sticker_set_view.as_ref())
            .expect("view")
            .stage
        {
            StickerSetViewStage::Ready { stickers, .. } => {
                stickers.iter().map(|s| s.file_id.0).collect()
            }
            _ => Vec::new(),
        }
    })
}

fn viewed_title(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, _| {
        match &app
            .demo_session
            .as_ref()
            .and_then(|s| s.stickers.sticker_set_view.as_ref())
            .expect("view")
            .stage
        {
            StickerSetViewStage::Ready { title, .. } => title.clone(),
            _ => String::new(),
        }
    })
}

#[gpui_kit::test]
fn an_owned_set_shows_the_creator_footer_and_the_add_cell(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
    assert!(vcx.update(|window, _| window.try_find("sticker-set-edit").is_some()));
    assert!(vcx.update(|window, _| window.try_find("sticker-set-add").is_some()));
    assert!(vcx.update(|window, _| window.try_find("sticker-set-more").is_some()));
    assert!(vcx.update(|window, _| window.try_find(("sticker-set-cell", 5u64)).is_some()));
    assert!(vcx.update(|window, _| window.try_find("sticker-set-reorder-done").is_none()));
    // The panel's header of an owned set carries the "edit" badge.
    app.read_with(vcx, |app, _| {
        let session = app.demo_session.as_ref().expect("demo");
        assert!(session.is_owned_sticker_set(78));
        assert_eq!(session.owned_sticker_sets(false).len(), 1);
        assert!(session.owned_sticker_sets(true).is_empty());
    });
}

#[gpui_kit::test]
fn edit_stickers_reorders_by_drag_and_done_leaves_the_mode(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert_eq!(
        viewed_ids(&app, vcx),
        vec![9100, 9101, 9102, 9103, 9104, 9105]
    );
    vcx.update(|window, cx| window.click("sticker-set-edit", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.sticker_box.reordering));
    assert!(vcx.update(|window, _| window.try_find("sticker-set-reorder-done").is_some()));
    assert!(
        vcx.update(|window, _| window.try_find("sticker-set-add").is_none()),
        "no + cell while reordering"
    );
    vcx.update(|window, cx| {
        window.drag_to(("sticker-set-cell", 0u64), ("sticker-set-cell", 2u64), cx)
    });
    frames(vcx);
    assert_eq!(
        viewed_ids(&app, vcx),
        vec![9101, 9102, 9100, 9103, 9104, 9105]
    );
    vcx.update(|window, cx| window.click("sticker-set-reorder-done", cx));
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.sticker_box.reordering));
    assert!(vcx.update(|window, _| window.try_find("sticker-set-edit").is_some()));
}

#[gpui_kit::test]
fn the_rename_box_changes_the_title(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| app.open_rename_set(window, cx));
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSetRename)));
    app.update_in(vcx, |app, window, cx| {
        assert_eq!(app.sticker_box.rename_input.read(cx).value(), "My doodles");
        app.sticker_box
            .rename_input
            .update(cx, |input, cx| input.set_value("  Cats ", window, cx));
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("sticker-set-rename-done", cx));
    frames(vcx);
    assert_eq!(viewed_title(&app, vcx), "Cats");
    assert!(!app.read_with(vcx, |app, _| app.sticker_box.rename_open));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
}

#[gpui_kit::test]
fn the_delete_sticker_box_removes_the_sticker(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        let item = match &app
            .demo_session
            .as_ref()
            .and_then(|s| s.stickers.sticker_set_view.as_ref())
            .expect("view")
            .stage
        {
            StickerSetViewStage::Ready { stickers, .. } => stickers[1].clone(),
            _ => panic!("ready"),
        };
        app.ask_delete_sticker(item, cx);
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerDelete)));
    vcx.update(|window, cx| window.click("sticker-delete-confirm", cx));
    frames(vcx);
    assert_eq!(viewed_ids(&app, vcx), vec![9100, 9102, 9103, 9104, 9105]);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
}

#[gpui_kit::test]
fn delete_for_everyone_asks_then_closes_the_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.sticker_box.delete_set_open = true;
        cx.notify();
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSetDelete)));
    // Cancel keeps the set and returns to the box.
    vcx.update(|window, cx| window.click("sticker-set-delete-cancel", cx));
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
    app.update(vcx, |app, cx| {
        app.sticker_box.delete_set_open = true;
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("sticker-set-delete-confirm", cx));
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.message_ui.menu_ui.sticker_set_open));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog.is_none()));
}

#[gpui_kit::test]
fn the_square_crop_stays_square_for_a_set_image(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        let image = image::RgbaImage::from_pixel(400, 200, image::Rgba([10, 20, 30, 255]));
        app.viewer.photo_editor = Some(crate::ui::photo_editor::PhotoEditor::for_set(
            image,
            78,
            quill::sticker_set_box::SetKind::Stickers,
        ));
        let editor = app.viewer.photo_editor.as_ref().expect("editor");
        let crop = editor.crop;
        // 200 px square on a 400×200 image: half the width, all the height.
        assert!((crop.w - 0.5).abs() < 0.01 && (crop.h - 1.0).abs() < 0.01);
        assert!((crop.x - 0.25).abs() < 0.01);
        cx.notify();
    });
}
