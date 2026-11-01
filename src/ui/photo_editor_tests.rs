//! "Done" renders and encodes off the UI thread, and Rotate / Flip turn
//! only the preview until then.
use super::{EditorPurpose, PhotoEditor};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::photo_edit::{CropRect, Placed, Stroke, flip, render, rotate_ccw};
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use image::{Rgba, RgbaImage};
use quill::composer::{AttachmentExtras, AttachmentKind, ComposerAttachment};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(900.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| crate::ui::shell::QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app built");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

/// A picture with a size no other test uses, so its PNG can be told apart
/// in the shared edits folder.
fn picture(name: &str, width: u32, height: u32) -> PathBuf {
    let dir = quill::scratch::process_dir("quill-photo-editor");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    RgbaImage::from_fn(width, height, |x, y| Rgba([x as u8, y as u8, 9, 255]))
        .save(&path)
        .expect("png");
    path
}

/// Edited PNGs of `size` in the edits folder.
fn edited_of_size(size: (u32, u32)) -> HashSet<PathBuf> {
    let dir = quill::scratch::temp_work_dir("quill-edits");
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| image::image_dimensions(path).ok() == Some(size))
                .collect()
        })
        .unwrap_or_default()
}

fn open_on(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, path: &Path) {
    let path = path.to_path_buf();
    app.update_in(vcx, |app, _, cx| {
        let picked = vec![ComposerAttachment {
            file_name: "pic.png".into(),
            path,
            kind: AttachmentKind::Photo,
            spoiler: false,
            extras: AttachmentExtras::default(),
        }];
        app.update_pending_attachments(|tray| *tray = picked)
            .expect("an open chat");
        app.open_photo_editor(0, cx);
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, _| assert!(app.viewer.photo_editor.is_some()));
}

fn tray_path(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<PathBuf> {
    app.update_in(vcx, |app, _, _| {
        app.pending_attachments().first().map(|a| a.path.clone())
    })
}

#[gpui_kit::test]
fn done_returns_before_the_render_and_the_tray_gets_the_png(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let original = picture("done.png", 37, 21);
    open_on(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, _, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(cx);
        assert!(app.viewer.photo_editor.is_none());
    });
    // Nothing rendered on the calling stack.
    assert_eq!(
        tray_path(&app, &mut vcx).as_deref(),
        Some(original.as_path())
    );
    assert!(edited_of_size((21, 37)).is_empty());
    vcx.run_until_parked();
    let edited = tray_path(&app, &mut vcx).expect("still in the tray");
    assert_ne!(edited, original);
    assert_eq!(image::image_dimensions(&edited).ok(), Some((21, 37)));
    let _ = std::fs::remove_file(edited);
}

#[gpui_kit::test]
fn a_photo_gone_from_the_tray_leaves_no_png(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let original = picture("gone.png", 41, 23);
    open_on(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, _, cx| {
        app.save_photo_editor(cx);
        app.update_pending_attachments(|tray| tray.clear())
            .expect("an open chat");
    });
    vcx.run_until_parked();
    assert!(edited_of_size((41, 23)).is_empty());
}

/// The picture turned step by step, as Rotate and Flip did before.
fn stepped(picture: &RgbaImage, steps: &[bool]) -> RgbaImage {
    let mut turned = picture.clone();
    for &rotate in steps {
        turned = if rotate {
            rotate_ccw(&turned)
        } else {
            flip(&turned)
        };
    }
    turned
}

#[test]
fn pending_turns_render_the_same_pixels_and_preview() {
    let picture = RgbaImage::from_fn(30, 18, |x, y| Rgba([x as u8 * 8, y as u8 * 13, 50, 255]));
    let steps = [true, false, true, true, false];
    let mut editor = PhotoEditor::new(EditorPurpose::MessageEdit, picture.clone());
    for &rotate in &steps {
        if rotate {
            editor.rotate();
        } else {
            editor.flip();
        }
    }
    editor.crop = CropRect {
        x: 0.1,
        y: 0.2,
        w: 0.7,
        h: 0.6,
    };
    editor.strokes.push(Stroke {
        color: [255, 0, 0, 255],
        width: 0.1,
        points: vec![(0.2, 0.3), (0.8, 0.7)],
    });
    let turned = stepped(&picture, &steps);
    let placed: Vec<Placed> = editor.placed.iter().map(|(item, _)| item.clone()).collect();
    let expected = render(&turned, editor.crop, &editor.strokes, &placed);
    // A picture under 1600 px previews at full size, so the turned
    // preview is the turned picture in GPUI's BGRA order.
    let mut shown = turned;
    for pixel in shown.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    assert_eq!(editor.preview.as_bytes(0), Some(shown.as_raw().as_slice()));
    assert_eq!(editor.dimensions(), (18, 30));
    assert_eq!(editor.into_render_job().render(), expected);
}
