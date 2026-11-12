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
            app.video_editor_probed(Some(info), None, window, cx);
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
            .map(|editor| editor.edits())
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
        app.connection.status_note().contains("at least 1 second")
    }));
}

/// A failed encode of any caller goes through one transition: the editor is
/// ready again with the line (and its preview player comes back where video
/// playback is supported).
#[gpui_kit::test]
fn a_failed_avatar_encode_returns_the_editor_with_its_line(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let clip = VideoSource {
        duration_ms: 20_000,
        width: 1280,
        height: 720,
        has_audio: true,
        ..VideoSource::default()
    };
    edit_avatar(&app, vcx, clip);
    let note = app.update(vcx, |app, cx| {
        let started = app
            .composer_ui
            .send_box
            .video_editor
            .as_mut()
            .map(|editor| {
                let ready = editor.can_encode();
                if ready {
                    editor.start_encode(VideoSource::default(), gpui_kit::Task::ready(()));
                }
                ready
            });
        assert!(
            started == Some(true)
                && app
                    .composer_ui
                    .send_box
                    .video_editor
                    .as_ref()
                    .is_some_and(|editor| editor.busy()),
            "a ready editor starts encoding"
        );
        app.video_encode_failed("ffmpeg said no".into(), cx);
        app.composer_ui
            .send_box
            .video_editor
            .as_ref()
            .and_then(|editor| editor.note())
    });
    assert_eq!(note.as_deref(), Some("ffmpeg said no"));
}

/// The encode's task belongs to the editor's stage: closing the editor
/// while it runs stops the wait for its answer, instead of leaving a
/// detached task that lands on whatever is open later.
#[gpui_kit::test]
fn closing_the_editor_stops_the_encode_it_waits_for(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let clip = VideoSource {
        duration_ms: 20_000,
        width: 1280,
        height: 720,
        has_audio: true,
        ..VideoSource::default()
    };
    for close in [false, true] {
        edit_avatar(&app, vcx, clip);
        let answered = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = answered.clone();
        app.update(vcx, |app, cx| {
            let job = cx.spawn(async move |_, _| flag.set(true));
            if let Some(editor) = app.composer_ui.send_box.video_editor.as_mut()
                && editor.can_encode()
            {
                editor.start_encode(clip, job);
            }
            if close {
                app.close_video_editor(cx);
            }
        });
        vcx.run_until_parked();
        assert_eq!(
            answered.get(),
            !close,
            "the job runs while the editor holds it and goes with it when it closes"
        );
        app.update(vcx, |app, cx| app.close_video_editor(cx));
    }
}

#[gpui_kit::test]
fn opening_an_editor_over_another_deletes_the_first_ones_cover_file(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let clip = VideoSource {
        duration_ms: 5_000,
        width: 1280,
        height: 720,
        has_audio: true,
        ..VideoSource::default()
    };
    edit_avatar(&app, vcx, clip);
    let cover = quill::scratch::unique_dir("quill-cover-test").join("cover.png");
    std::fs::write(&cover, b"png").expect("write the cover");
    app.update(vcx, |app, _| {
        if let Some(editor) = app.composer_ui.send_box.video_editor.as_mut() {
            editor.cover = Some(crate::ui::video_editor::CoverFrame {
                at_ms: 0,
                path: cover.clone(),
                still: None,
            });
        }
    });
    edit_avatar(&app, vcx, clip);
    assert!(
        !cover.exists(),
        "the replaced editor's cover frame is deleted"
    );
}
