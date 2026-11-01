//! Picture files open off the UI thread: the call returns at once and the
//! editor / status note appears when the decode task resolves.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::composer::{AttachmentExtras, AttachmentKind, ComposerAttachment};
use quill::ids::FileId;
use quill::sticker_set_box::SetKind;
use quill::telegram::envelope::{LocalFileState, ParsedFile, StickerFormat};
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

#[cfg(any(target_os = "macos", windows))]
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/images")
        .join(name)
}

fn attach(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, path: PathBuf) {
    app.update_in(vcx, |app, _, _| {
        let picked = vec![ComposerAttachment {
            file_name: "pic".into(),
            path,
            kind: AttachmentKind::Photo,
            spoiler: false,
            extras: AttachmentExtras::default(),
        }];
        app.update_pending_attachments(|tray| *tray = picked)
            .expect("an open chat");
    });
}

fn scratch(name: &str) -> PathBuf {
    let dir = quill::scratch::process_dir("quill-image-open");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join(Path::new(name))
}

#[gpui_kit::test]
fn photo_editor_opens_when_the_decode_resolves(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let png = scratch("opens.png");
    image::RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]))
        .save(&png)
        .expect("png");
    attach(&app, &mut vcx, png);
    app.update_in(&mut vcx, |app, _, cx| app.open_photo_editor(0, cx));
    // Nothing decoded on the calling stack.
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.viewer.photo_editor.is_none());
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.viewer.photo_editor.is_some());
    });
}

#[gpui_kit::test]
fn photo_editor_failure_shows_the_status_note(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let bad = scratch("broken.png");
    std::fs::write(&bad, b"not an image").expect("write");
    attach(&app, &mut vcx, bad);
    app.update_in(&mut vcx, |app, _, cx| app.open_photo_editor(0, cx));
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.viewer.photo_editor.is_none());
        assert_eq!(
            app.connection.status_note().to_string(),
            quill::lang::tr(quill::lang::keys::Q_MISC_COULDNT_OPEN_THIS_IMAGE_FOR).to_string()
        );
    });
}

fn editor_with_sticker_file(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    path: &str,
) -> FileId {
    let png = scratch("sticker-base.png");
    image::RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]))
        .save(&png)
        .expect("png");
    attach(app, vcx, png);
    app.update_in(vcx, |app, _, cx| app.open_photo_editor(0, cx));
    vcx.run_until_parked();
    let file = FileId(9_001);
    let path = crate::ui::demo::demo_media_allowlist()
        .join(path)
        .to_string_lossy()
        .into_owned();
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.media.files.insert(ParsedFile {
            id: file,
            size: 1,
            expected_size: 1,
            uploading: false,
            local: LocalFileState {
                path,
                can_be_downloaded: false,
                is_downloading_active: false,
                is_downloading_completed: true,
                downloaded_size: 1,
            },
        });
    });
    file
}

#[gpui_kit::test]
fn a_sticker_lands_on_the_photo_when_its_decode_resolves(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let file = editor_with_sticker_file(&app, &mut vcx, "demo-sticker.tgs");
    app.update_in(&mut vcx, |app, _, cx| {
        app.place_editor_sticker(file, StickerFormat::Tgs, cx);
        let editor = app.viewer.photo_editor.as_ref().expect("editor open");
        assert!(editor.placed.is_empty());
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        let editor = app.viewer.photo_editor.as_ref().expect("editor open");
        assert_eq!(editor.placed.len(), 1);
    });
}

#[gpui_kit::test]
fn a_sticker_that_will_not_decode_shows_the_status_note(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let file = editor_with_sticker_file(&app, &mut vcx, "demo-notes.txt");
    app.update_in(&mut vcx, |app, _, cx| {
        app.place_editor_sticker(file, StickerFormat::Tgs, cx);
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        let editor = app.viewer.photo_editor.as_ref().expect("editor open");
        assert!(editor.placed.is_empty());
        assert_eq!(
            app.connection.status_note().to_string(),
            quill::lang::tr(quill::lang::keys::Q_MISC_COULDNT_USE_THAT_STICKER).to_string()
        );
    });
}

/// HEIC opens in the editor (the old `image::open` could not read it).
#[cfg(any(target_os = "macos", windows))]
#[gpui_kit::test]
fn photo_editor_opens_a_heic(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    attach(&app, &mut vcx, fixture("halves.heic"));
    app.update_in(&mut vcx, |app, _, cx| app.open_photo_editor(0, cx));
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        let editor = app.viewer.photo_editor.as_ref().expect("editor opened");
        assert_eq!(editor.working.dimensions(), (32, 16));
    });
}

#[gpui_kit::test]
fn sticker_set_picker_reports_an_unreadable_file(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let bad = scratch("bad-sticker.png");
    std::fs::write(&bad, b"nope").expect("write");
    app.update_in(&mut vcx, |app, _, cx| {
        app.edit_image_for_set(1, SetKind::Stickers, &bad, cx);
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert!(app.viewer.photo_editor.is_none());
        assert!(!app.connection.status_note().is_empty());
    });
}
