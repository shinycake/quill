//! The video avatar runs through the full video editor: a square crop,
//! 1 to 9.6 s, no quality or GIF choice, and Cancel back to the dialog.

use crate::ui::app::QuillApp;
use crate::ui::dialogs::edit_profile::EditProfileDialog;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::video_editor::EditorTarget;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::video_edit::VideoSource;

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
    let demo = ScreenshotDemo::named("ready-chats");
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

/// Open the editor on a clip and answer its probe with `info`.
fn edit_avatar(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, info: VideoSource) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_video_editor(
                EditorTarget::Avatar,
                std::env::temp_dir().join("quill-test-avatar.mp4"),
                None,
                window,
                cx,
            );
            let serial = app
                .composer_ui
                .send_box
                .video_editor
                .as_ref()
                .map_or(0, |editor| editor.serial);
            app.video_editor_probed(serial, Some(info), None, window, cx);
        });
    });
    frames(vcx);
}

#[gpui_kit::test]
fn a_video_avatar_opens_the_full_editor(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.dialogs.edit_profile_dialog =
                Some(EditProfileDialog::new(window, cx, "Ada", "", "", ""));
            cx.notify();
        });
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::EditProfile)));
    let clip = VideoSource {
        duration_ms: 20_000,
        width: 1280,
        height: 720,
        has_audio: true,
        ..VideoSource::default()
    };
    edit_avatar(&app, vcx, clip);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::VideoEditor)));
    let edits = app.read_with(vcx, |app, _| {
        app.composer_ui
            .send_box
            .video_editor
            .as_ref()
            .map(|editor| editor.edits)
    });
    let edits = edits.expect("editor");
    assert_eq!((edits.from_ms, edits.till_ms), (0, 9600));
    let crop = edits.crop.expect("a square crop");
    // Square in pixels, to the crop's thousandths.
    let (_, _, w, h) = crop.pixels(1280, 720);
    assert!(w.abs_diff(h) <= 2, "{w} x {h}");
    assert!(vcx.update(|window, _| window.try_find("video-editor-done").is_some()));
    assert!(vcx.update(|window, _| window.try_find("video-editor-gif").is_none()));
    // Cancel returns to the edit-profile dialog.
    vcx.update(|window, cx| window.click("video-editor-cancel", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| {
        app.composer_ui.send_box.video_editor.is_none()
    }));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::EditProfile)));
    // A clip under a second says so and the editor goes.
    edit_avatar(
        &app,
        vcx,
        VideoSource {
            duration_ms: 400,
            ..clip
        },
    );
    assert!(app.read_with(vcx, |app, _| {
        app.composer_ui.send_box.video_editor.is_none()
    }));
    assert!(app.read_with(vcx, |app, _| {
        app.connection.status_note.contains("at least 1 second")
    }));
}
