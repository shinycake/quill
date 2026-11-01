//! "Done" renders and encodes off the UI thread, and Rotate / Flip turn
//! only the preview until then.
use super::{EditorPurpose, PhotoEditor};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::photo_edit::{CropRect, Placed, Stroke, flip, render, rotate_ccw};
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use image::{Rgba, RgbaImage};
use quill::composer::{AttachmentExtras, AttachmentKind, ComposerAttachment};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    new_app_in(cx, "ready-privacy")
}

fn new_app_in(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    let demo = demo.to_string();
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
                Some(ScreenshotDemo::named(&demo)),
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
    app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
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
    app.update_in(&mut vcx, |app, window, cx| {
        app.save_photo_editor(window, cx);
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

/// The demo's open chat, and the path of the photo its newest message sent.
fn newest_sent_photo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<String> {
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_ref()?;
        let chat = session.open_chat?;
        let newest = session.histories.get(&chat.0)?.ordered().last().copied()?;
        let quill::telegram::envelope::MessageContent::Photo(photo) = &newest.content else {
            return None;
        };
        let size = photo.sizes.first()?;
        Some(session.file(size.file_id)?.usable_path()?.to_string())
    })
}

/// The editor opened again on tray item 0 as it is now, without waiting
/// for a decode, so the next Done starts while the last one renders.
fn reopen_now(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, original: &Path) {
    let pixels = image::open(original).expect("decodes").to_rgba8();
    app.update_in(vcx, |app, _, _| {
        let item = app.tray_item(0).expect("a tray item");
        app.viewer.photo_editor = Some(PhotoEditor::new(EditorPurpose::Attachment(item), pixels));
    });
}

#[gpui_kit::test]
fn a_send_during_the_render_waits_and_sends_the_edited_photo(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let original = picture("held.png", 43, 19);
    open_on(&app, &mut vcx, &original);
    let sent_before = newest_sent_photo(&app, &mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
        app.submit_composer(String::new(), window, cx);
        // Held: nothing went, and the tray still has the photo.
        assert_eq!(app.pending_attachments().len(), 1);
    });
    assert_eq!(newest_sent_photo(&app, &mut vcx), sent_before);
    vcx.run_until_parked();
    let sent = newest_sent_photo(&app, &mut vcx).expect("the photo went");
    assert_ne!(Path::new(&sent), original.as_path());
    assert_eq!(image::image_dimensions(&sent).ok(), Some((19, 43)));
    assert_eq!(tray_path(&app, &mut vcx), None, "the send cleared the tray");
    let _ = std::fs::remove_file(sent);
}

#[gpui_kit::test]
fn the_latest_done_wins_and_the_other_png_is_deleted(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let original = picture("twice.png", 47, 17);
    open_on(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
    });
    reopen_now(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, window, cx| {
        app.save_photo_editor(window, cx)
    });
    vcx.run_until_parked();
    let edited = tray_path(&app, &mut vcx).expect("still in the tray");
    assert_eq!(image::image_dimensions(&edited).ok(), Some((47, 17)));
    assert!(
        edited_of_size((17, 47)).is_empty(),
        "the first Done's PNG went"
    );
    let _ = std::fs::remove_file(edited);
}

fn attachment(path: &Path) -> ComposerAttachment {
    ComposerAttachment {
        file_name: "pic.png".into(),
        path: path.to_path_buf(),
        kind: AttachmentKind::Photo,
        spoiler: false,
        extras: AttachmentExtras::default(),
    }
}

#[gpui_kit::test]
fn an_edit_lands_on_its_photo_after_an_earlier_card_is_removed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let first = picture("shift-a.png", 59, 11);
    let second = picture("shift-b.png", 61, 13);
    let sent_before = newest_sent_photo(&app, &mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        let tray = vec![attachment(&first), attachment(&second)];
        app.update_pending_attachments(|items| *items = tray)
            .expect("an open chat");
        let item = app.tray_item(1).expect("the second card");
        let pixels = image::open(&second).expect("decodes").to_rgba8();
        app.viewer.photo_editor = Some(PhotoEditor::new(EditorPurpose::Attachment(item), pixels));
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
        app.remove_attachment(0, cx);
        app.submit_composer(String::new(), window, cx);
    });
    assert_eq!(newest_sent_photo(&app, &mut vcx), sent_before, "held");
    vcx.run_until_parked();
    let sent = newest_sent_photo(&app, &mut vcx).expect("the held Send went");
    assert_eq!(image::image_dimensions(&sent).ok(), Some((13, 61)));
    assert_eq!(tray_path(&app, &mut vcx), None, "nothing left held");
    let _ = std::fs::remove_file(sent);
}

#[gpui_kit::test]
fn reopening_the_editor_during_the_render_opens_the_pending_edit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let original = picture("reopen.png", 67, 15);
    open_on(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
        app.open_photo_editor(0, cx);
        assert!(app.viewer.photo_editor.is_none(), "not on the original");
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        let editor = app.viewer.photo_editor.as_ref().expect("opened");
        assert_eq!(editor.dimensions(), (15, 67), "on the rotated photo");
    });
    if let Some(edited) = tray_path(&app, &mut vcx) {
        let _ = std::fs::remove_file(edited);
    }
}

#[gpui_kit::test]
fn a_held_send_whose_chat_closed_says_it_did_not_go(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let original = picture("left.png", 71, 17);
    open_on(&app, &mut vcx, &original);
    let first_chat = app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
        app.submit_composer(String::new(), window, cx);
        let session = app.demo_session.as_mut().expect("demo session");
        let open = session.open_chat.expect("an open chat");
        let other = session
            .chats
            .keys()
            .map(|&chat| quill::ids::ChatId(chat))
            .find(|&chat| chat != open)
            .expect("another chat");
        session.open_chat(other);
        cx.notify();
        open
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(
            app.connection.note().text(),
            quill::lang::tr(quill::lang::keys::Q_MISC_NOT_SENT_THE_CHAT_CLOSED_BEFORE_THE_PHOTO)
        );
    });
    app.update_in(&mut vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(first_chat);
        }
        cx.notify();
    });
    let edited = tray_path(&app, &mut vcx).expect("the photo waits in its tray");
    assert_eq!(image::image_dimensions(&edited).ok(), Some((17, 71)));
    let _ = std::fs::remove_file(edited);
}

/// Tap the rendering card and press Send, in `order`: the photo goes once
/// the render lands, and no editor opens over the empty tray.
fn send_with_a_deferred_open(cx: &mut TestAppContext, name: &str, send_first: bool) {
    let (app, mut vcx) = new_app(cx);
    let original = picture(name, 73, if send_first { 29 } else { 31 });
    open_on(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
        if send_first {
            app.submit_composer(String::new(), window, cx);
            app.open_photo_editor(0, cx);
        } else {
            app.open_photo_editor(0, cx);
            app.submit_composer(String::new(), window, cx);
        }
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.pending_attachments().is_empty(), "the photo went");
        assert!(
            app.viewer.photo_editor.is_none(),
            "no editor on a sent photo"
        );
    });
    if let Some(sent) = newest_sent_photo(&app, &mut vcx) {
        let _ = std::fs::remove_file(sent);
    }
}

#[gpui_kit::test]
fn a_tap_then_send_mid_render_sends_and_opens_no_editor(cx: &mut TestAppContext) {
    send_with_a_deferred_open(cx, "tap-send.png", false);
}

#[gpui_kit::test]
fn a_send_then_tap_mid_render_sends_and_opens_no_editor(cx: &mut TestAppContext) {
    send_with_a_deferred_open(cx, "send-tap.png", true);
}

/// Whether tray card `index` shows that its edit is still being saved.
fn card_shows_rendering(vcx: &mut VisualTestContext, index: usize) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    let selector: &'static str =
        Box::leak(format!("composer-attach-rendering-{index}").into_boxed_str());
    vcx.debug_bounds(selector).is_some()
}

#[gpui_kit::test]
fn a_rendering_card_shows_it_until_the_edit_lands(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app_in(cx, "ready-chats");
    let original = picture("spin.png", 79, 13);
    open_on(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
    });
    assert!(card_shows_rendering(&mut vcx, 0));
    vcx.run_until_parked();
    assert!(!card_shows_rendering(&mut vcx, 0));
    if let Some(edited) = tray_path(&app, &mut vcx) {
        let _ = std::fs::remove_file(edited);
    }
}

/// Tap the rendering card, do `then`, and let the render land: the editor
/// stays closed.
fn tap_mid_render_then(
    cx: &mut TestAppContext,
    name: &str,
    height: u32,
    then: impl FnOnce(&mut QuillApp, &mut gpui_kit::Context<QuillApp>),
) {
    let (app, mut vcx) = new_app(cx);
    let original = picture(name, 83, height);
    open_on(&app, &mut vcx, &original);
    app.update_in(&mut vcx, |app, window, cx| {
        app.photo_editor_mut(PhotoEditor::rotate, cx);
        app.save_photo_editor(window, cx);
        app.open_photo_editor(0, cx);
        then(app, cx);
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(
            app.viewer.photo_editor.is_none(),
            "no editor long after the tap"
        );
    });
    if let Some(edited) = tray_path(&app, &mut vcx) {
        let _ = std::fs::remove_file(edited);
    }
}

#[gpui_kit::test]
fn a_tap_mid_render_then_a_chat_change_opens_nothing(cx: &mut TestAppContext) {
    tap_mid_render_then(cx, "away.png", 11, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        let open = session.open_chat.expect("an open chat");
        let other = session
            .chats
            .keys()
            .map(|&chat| quill::ids::ChatId(chat))
            .find(|&chat| chat != open)
            .expect("another chat");
        session.open_chat(other);
        session.open_chat(open);
    });
}

#[gpui_kit::test]
fn a_tap_mid_render_then_an_editor_closing_opens_nothing(cx: &mut TestAppContext) {
    tap_mid_render_then(cx, "closed.png", 9, |app, cx| app.close_photo_editor(cx));
}
