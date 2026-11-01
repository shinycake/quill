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
use quill::telegram::envelope::StickerItem;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    open_demo_sized(cx, 1200., 1000.)
}

fn open_demo_sized(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-sticker-set-owned");
    let handle = cx.open_window(size(px(width), px(height)), move |window, cx| {
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
fn a_set_image_is_padded_to_a_square_and_the_corners_cut(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        let image = image::RgbaImage::from_pixel(400, 200, image::Rgba([10, 20, 30, 255]));
        app.edit_pixels_for_set(
            78,
            quill::sticker_set_box::SetKind::Stickers,
            Some(image),
            cx,
        );
        let editor = app.viewer.photo_editor.as_ref().expect("editor");
        // tdesktop's canvas: a transparent square with the image fitted on
        // it; the crop is the whole canvas.
        assert_eq!(editor.working.dimensions(), (512, 512));
        assert_eq!(editor.crop, crate::ui::photo_edit::CropRect::FULL);
        assert!(editor.fixed_crop());
        let edited = editor.render_set_image();
        assert_eq!(edited.dimensions(), (512, 512));
        assert_eq!(edited.get_pixel(256, 20)[3], 0);
        assert_eq!(edited.get_pixel(256, 256)[3], 255);
    });
    frames(vcx);
    // Crop mode shows the set tools, with the corners menu.
    assert!(vcx.update(|window, _| window.try_find("photo-editor-corners").is_some()));
    assert!(vcx.update(|window, _| window.try_find("photo-editor-reset-crop").is_none()));
    // Done: the result is fitted, the corners are cut (Large by default).
    vcx.update(|window, cx| window.click("photo-editor-done", cx));
    frames(vcx);
    let create = app
        .read_with(vcx, |app, _| app.sticker_box.create.clone())
        .expect("create");
    assert_eq!(
        create.format,
        quill::telegram::envelope::StickerFormat::Webp
    );
    let written = image::open(&create.path).expect("png").to_rgba8();
    assert_eq!(written.dimensions(), (512, 512));
    assert_eq!(written.get_pixel(0, 0)[3], 0);
    assert_eq!(written.get_pixel(256, 256)[3], 255);
    let _ = std::fs::remove_file(&create.path);
}

#[gpui_kit::test]
fn an_image_the_editor_refuses_toasts_could_not_load(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        let long = image::RgbaImage::from_pixel(20, 400, image::Rgba([1, 1, 1, 255]));
        app.edit_pixels_for_set(
            78,
            quill::sticker_set_box::SetKind::Stickers,
            Some(long),
            cx,
        );
        assert!(app.viewer.photo_editor.is_none());
        assert_eq!(
            app.connection.status_note(),
            quill::sticker_set_box::OPEN_FAILED_NOTE
        );
    });
}

#[gpui_kit::test]
fn the_photo_editor_takes_the_box_aside_until_it_closes(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    // "Create a New Sticker" after "Choose Image": the editor paints inside
    // the app, so the kit box (and its hit barrier) must step aside.
    app.update(vcx, |app, cx| {
        let image = image::RgbaImage::from_pixel(300, 300, image::Rgba([10, 20, 30, 255]));
        app.edit_pixels_for_set(
            78,
            quill::sticker_set_box::SetKind::Stickers,
            Some(image),
            cx,
        );
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog.is_none()));
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu_ui.sticker_set_open));
    // Done in the editor hands the image to the "New Sticker" box.
    app.update(vcx, |app, cx| {
        let job = app
            .viewer
            .photo_editor
            .take()
            .expect("editor")
            .into_set_render_job();
        app.finish_set_image(
            78,
            quill::sticker_set_box::SetKind::Stickers,
            job,
            quill::sticker_image::Corners::None,
            cx,
        );
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerCreate)));
    let path = app
        .read_with(vcx, |app, _| app.sticker_box.create.clone())
        .map(|c| c.path);
    if let Some(path) = path {
        let _ = std::fs::remove_file(path);
    }
}

#[gpui_kit::test]
fn a_video_sticker_adapts_through_the_video_editor(cx: &mut TestAppContext) {
    use crate::ui::sticker_set_box::state::PickExisting;
    use quill::sticker_set_box::SetKind;
    use quill::telegram::envelope::StickerFormat;
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let pick = PickExisting {
        set_id: 78,
        kind: SetKind::Emoji,
        adapt: true,
    };
    app.update(vcx, |app, cx| {
        app.sticker_box.pick = Some(pick);
        cx.notify();
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerPick)));
    // A video sticker with its file on disk (the demo's stickers are
    // backed by a local PNG stub; the probe runs in the background).
    let mut item = app
        .read_with(vcx, |app, _| {
            app.demo_session
                .as_ref()
                .and_then(|s| s.media.media_library.set_stickers.get(&78))
                .and_then(|stickers| stickers.first().cloned())
        })
        .expect("a demo sticker");
    item.set_id = 79;
    item.format = StickerFormat::Webm;
    item.emoji = "🔥".into();
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.pick_existing_sticker(&item, window, cx);
            // The stub isn't a video: the real probe's answer is stale
            // (the test answers the probe itself below).
            if let Some(editor) = app.composer_ui.send_box.video_editor.as_mut() {
                editor.serial += 1;
            }
        });
    });
    frames(vcx);
    // The video editor (tdesktop `RunVideoEditorAndCreate`) replaces the
    // chooser and sits over the set box.
    assert!(app.read_with(vcx, |app, _| app.sticker_box.pick.is_none()));
    assert!(app.read_with(vcx, |app, _| app.video_editor_for_set()));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::VideoEditor)));
    // The probe answers: three seconds at most, a square crop, no quality
    // or GIF choice.
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let serial = app
                .composer_ui
                .send_box
                .video_editor
                .as_ref()
                .map_or(0, |editor| editor.serial);
            let info = quill::video_edit::VideoSource {
                duration_ms: 10_000,
                width: 640,
                height: 480,
                has_audio: true,
                ..Default::default()
            };
            app.video_editor_probed(serial, Some(info), None, window, cx);
        });
    });
    frames(vcx);
    let (edits, emoji) = app.read_with(vcx, |app, _| {
        let editor = app
            .composer_ui
            .send_box
            .video_editor
            .as_ref()
            .expect("editor");
        (editor.edits, editor.set_emoji.clone())
    });
    assert_eq!((edits.from_ms, edits.till_ms), (0, 3000));
    assert_eq!(edits.crop.map(|crop| (crop.w, crop.h)), Some((750, 1000)));
    assert_eq!(emoji.as_deref(), Some("🔥"));
    assert!(vcx.update(|window, _| window.try_find("video-editor-gif").is_none()));
    // Done: the "New Emoji" box opens with the edits waiting to encode and
    // the sticker's emoji chosen (tdesktop's `CreateMediaBox`).
    vcx.update(|window, cx| window.click("video-editor-done", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| {
        app.composer_ui.send_box.video_editor.is_none()
    }));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerCreate)));
    let create = app
        .read_with(vcx, |app, _| app.sticker_box.create.clone())
        .expect("create");
    assert_eq!(create.format, StickerFormat::Webm);
    assert_eq!(create.emojis, vec!["🔥".to_string()]);
    let job = create.video.expect("the encode waits for Done");
    assert_eq!((job.edits.from_ms, job.edits.till_ms), (0, 3000));
    assert_eq!(job.info.width, 640);
    // Cancel in the box drops it and returns to the set box.
    vcx.update(|window, cx| window.click("sticker-create-cancel", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.sticker_box.create.is_none()
        && app.sticker_box.encoding.is_none()));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
}

/// "Adapt from a Sticker" on a still sticker: the pick returns before the
/// sticker is decoded, and the photo editor opens on it once the worker
/// is done.
#[gpui_kit::test]
fn a_still_sticker_decodes_off_the_ui_thread_before_the_editor_opens(cx: &mut TestAppContext) {
    use crate::ui::sticker_set_box::state::PickExisting;
    use quill::sticker_set_box::SetKind;
    use quill::telegram::envelope::StickerFormat;
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.sticker_box.pick = Some(PickExisting {
            set_id: 78,
            kind: SetKind::Emoji,
            adapt: true,
        });
        cx.notify();
    });
    frames(vcx);
    let mut item = app
        .read_with(vcx, |app, _| {
            app.demo_session
                .as_ref()
                .and_then(|s| s.media.media_library.set_stickers.get(&78))
                .and_then(|stickers| stickers.first().cloned())
        })
        .expect("a demo sticker");
    item.format = StickerFormat::Webp;
    let opened_at_pick = vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.pick_existing_sticker(&item, window, cx);
            app.viewer.photo_editor.is_some()
        })
    });
    assert!(!opened_at_pick, "the decode waits for the worker");
    assert!(app.read_with(vcx, |app, _| app.sticker_box.pick.is_none()));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.viewer.photo_editor.is_some()));
}

fn adapt_pick(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> (Vec<StickerItem>, crate::ui::sticker_set_box::state::PickExisting) {
    use crate::ui::sticker_set_box::state::PickExisting;
    use quill::sticker_set_box::SetKind;
    use quill::telegram::envelope::StickerFormat;
    let pick = PickExisting {
        set_id: 78,
        kind: SetKind::Stickers,
        adapt: true,
    };
    let items: Vec<StickerItem> = app
        .read_with(vcx, |app, _| {
            app.demo_session
                .as_ref()
                .and_then(|s| s.media.media_library.set_stickers.get(&78).cloned())
        })
        .expect("demo stickers")
        .into_iter()
        .take(2)
        .map(|mut item| {
            item.format = StickerFormat::Webp;
            item
        })
        .collect();
    assert_eq!(items.len(), 2, "two demo stickers");
    (items, pick)
}

fn choose(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    pick: crate::ui::sticker_set_box::state::PickExisting,
    item: &StickerItem,
) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.sticker_box.pick = Some(pick);
            app.pick_existing_sticker(item, window, cx);
        })
    });
}

/// Closing the set box while a picked sticker decodes: the editor does not
/// pop up for the box the user dismissed.
#[gpui_kit::test]
fn a_sticker_picked_then_closed_opens_no_editor(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let (items, pick) = adapt_pick(&app, vcx);
    let item = &items[0];
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.sticker_box.pick = Some(pick);
            app.pick_existing_sticker(item, window, cx);
            app.close_sticker_set_dialog(cx);
        })
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.viewer.photo_editor.is_none()));
}

/// Two picks in a row: the editor shows the later one, even when the
/// earlier decode lands last.
#[gpui_kit::test]
fn the_later_pick_wins_when_the_earlier_decode_lands_last(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let (items, pick) = adapt_pick(&app, vcx);
    let (first, second) = (&items[0], &items[1]);
    choose(&app, vcx, pick, first);
    choose(&app, vcx, pick, second);
    let picture = |width, height| Some(image::RgbaImage::new(width, height));
    app.update(vcx, |app, cx| {
        app.land_still_pick(pick, second.file_id, picture(100, 100), cx);
        app.land_still_pick(pick, first.file_id, picture(300, 200), cx);
    });
    frames(vcx);
    let shown = app.read_with(vcx, |app, _| {
        app.viewer.photo_editor.as_ref().and_then(|editor| {
            editor
                .placed
                .first()
                .map(|(item, _)| item.image.dimensions())
        })
    });
    assert_eq!(shown, Some((100, 100)));
}

#[gpui_kit::test]
fn a_video_the_editor_cannot_take_closes_it_with_the_bad_video_toast(cx: &mut TestAppContext) {
    use quill::sticker_set_box::SetKind;
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let clip = std::path::PathBuf::from("not-a-video.webm");
            app.open_set_video_editor(78, SetKind::Stickers, clip, None, window, cx);
        });
    });
    // The editor opens, then the probe of the missing file fails.
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::VideoEditor)));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| {
        app.composer_ui.send_box.video_editor.is_none()
            && app.connection.status_note() == quill::sticker_video::BAD_VIDEO_NOTE
    }));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
}

#[gpui_kit::test]
fn a_chosen_video_still_downloading_opens_the_editor_when_it_arrives(cx: &mut TestAppContext) {
    use crate::ui::sticker_set_box::state::{PendingVideo, PickExisting};
    use quill::sticker_set_box::SetKind;
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    let pick = PickExisting {
        set_id: 78,
        kind: SetKind::Stickers,
        adapt: false,
    };
    // A demo sticker whose file is on disk stands in for the download
    // that just finished.
    let file_id = app
        .read_with(vcx, |app, _| {
            app.demo_session
                .as_ref()
                .and_then(|s| s.media.media_library.set_stickers.get(&78))
                .and_then(|stickers| stickers.first().map(|item| item.file_id))
        })
        .expect("a demo sticker");
    app.update(vcx, |app, cx| {
        app.sticker_box.pending_video = Some(PendingVideo {
            pick,
            file_id,
            emoji: Some("🎉".into()),
        });
        cx.notify();
    });
    // The next frame opens the editor; the stub isn't a video, so the
    // real probe's answer is made stale.
    vcx.update(|window, cx| window.render_frame(cx));
    app.update(vcx, |app, _| {
        if let Some(editor) = app.composer_ui.send_box.video_editor.as_mut() {
            editor.serial += 1;
        }
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.sticker_box.pending_video.is_none()));
    assert!(app.read_with(vcx, |app, _| app.video_editor_for_set()));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::VideoEditor)));
    assert_eq!(
        app.read_with(vcx, |app, _| app
            .composer_ui
            .send_box
            .video_editor
            .as_ref()
            .and_then(|editor| editor.set_emoji.clone())),
        Some("🎉".to_string())
    );
    vcx.update(|window, cx| window.click("video-editor-cancel", cx));
    frames(vcx);
}

#[gpui_kit::test]
fn the_chooser_lists_saved_gifs_except_when_adapting(cx: &mut TestAppContext) {
    use crate::ui::sticker_set_box::state::PickExisting;
    use quill::ids::FileId;
    use quill::sticker_set_box::SetKind;
    use quill::telegram::envelope::AnimationItem;
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stickers.gifs.animations.push(AnimationItem {
                duration: 2,
                width: 320,
                height: 240,
                file_name: "dance.mp4".into(),
                mime_type: "video/mp4".into(),
                file_id: FileId(9_001),
                thumb_file_id: None,
                thumb_width: 0,
                thumb_height: 0,
                minithumbnail: None,
                thumb_still: true,
            });
        }
        app.sticker_box.pick = Some(PickExisting {
            set_id: 78,
            kind: SetKind::Stickers,
            adapt: false,
        });
        cx.notify();
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerPick)));
    let offered = app.read_with(vcx, |app, _| {
        app.sticker_box
            .pick
            .map(|pick| app.pick_gif_candidates(pick).len())
    });
    assert_eq!(offered, Some(1));
    // "Adapt from a Sticker" is stickers only.
    let adapt = PickExisting {
        set_id: 78,
        kind: SetKind::Emoji,
        adapt: true,
    };
    assert!(app.read_with(vcx, |app, _| app.pick_gif_candidates(adapt).is_empty()));
    // Choosing the GIF (its file isn't here) closes the chooser and waits
    // for the download.
    let animation = app
        .read_with(vcx, |app, _| {
            app.sticker_box
                .pick
                .and_then(|pick| app.pick_gif_candidates(pick).into_iter().next())
        })
        .expect("the GIF");
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.pick_gif_for_set(&animation, window, cx));
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| {
        app.sticker_box.pick.is_none()
            && app
                .sticker_box
                .pending_video
                .as_ref()
                .is_some_and(|pending| pending.file_id == FileId(9_001))
    }));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
}

#[gpui_kit::test]
fn the_new_sticker_box_needs_an_emoji_and_takes_at_most_seven(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.sticker_box.create = Some(super::state::CreateSticker {
            video: None,
            set_id: 78,
            kind: quill::sticker_set_box::SetKind::Stickers,
            path: std::path::PathBuf::from(crate::ui::demo::demo_thumb_png_path()),
            format: quill::telegram::envelope::StickerFormat::Webp,
            preview: None,
            emojis: Vec::new(),
        });
        cx.notify();
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerCreate)));
    // Done without an emoji: tdesktop's "Please choose an emoji." and the
    // box stays.
    vcx.update(|window, cx| window.click("sticker-create-done", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.sticker_box.create.is_some()));
    assert_eq!(
        app.read_with(vcx, |app, _| app.connection.status_note().to_string()),
        quill::sticker_set_box::EMOJI_REQUIRED_NOTE
    );
    // The first grid cell toggles its emoji on and off.
    vcx.update(|window, cx| window.click(("sticker-create-emoji", 0u64), cx));
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app
            .sticker_box
            .create
            .as_ref()
            .map(|c| c.emojis.len())),
        Some(1)
    );
    vcx.update(|window, cx| window.click(("sticker-create-emoji", 0u64), cx));
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app
            .sticker_box
            .create
            .as_ref()
            .map(|c| c.emojis.len())),
        Some(0)
    );
    // At most seven emoji (`MAX_EMOJIS`).
    app.update(vcx, |app, cx| {
        for emoji in ["😀", "😃", "😄", "😁", "😆", "😅", "🤣", "😂", "🙂"] {
            app.toggle_create_emoji(emoji, cx);
        }
    });
    assert_eq!(
        app.read_with(vcx, |app, _| app
            .sticker_box
            .create
            .as_ref()
            .map(|c| c.emojis.len())),
        Some(quill::sticker_set_box::MAX_EMOJIS)
    );
    frames(vcx);
    vcx.update(|window, cx| window.click("sticker-create-done", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.sticker_box.create.is_none()));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
}

#[gpui_kit::test]
fn add_an_existing_sticker_opens_the_chooser_and_cancel_returns(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_pick_existing(78, quill::sticker_set_box::SetKind::Stickers, false, cx);
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerPick)));
    // The set's own six stickers are not offered.
    let candidates = app.read_with(vcx, |app, _| {
        app.pick_candidates(super::state::PickExisting {
            set_id: 78,
            kind: quill::sticker_set_box::SetKind::Stickers,
            adapt: false,
        })
    });
    assert!(candidates.iter().all(|item| item.set_id != 78));
    vcx.update(|window, cx| window.click("sticker-pick-cancel", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.sticker_box.pick.is_none()));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
}

#[gpui_kit::test]
fn delete_for_myself_asks_with_the_set_title(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.sticker_box.remove_set_open = true;
        cx.notify();
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSetRemove)));
    assert!(vcx.update(|window, _| window.try_find("sticker-set-remove-confirm").is_some()));
    vcx.update(|window, cx| window.click("sticker-set-remove-cancel", cx));
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.sticker_box.remove_set_open));
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
    app.update(vcx, |app, cx| {
        app.sticker_box.remove_set_open = true;
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("sticker-set-remove-confirm", cx));
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.sticker_box.remove_set_open));
}

#[gpui_kit::test]
fn a_set_less_sticker_message_lists_the_owned_sets_to_join(cx: &mut TestAppContext) {
    use quill::ids::{ChatId, FileId, MessageId};
    use quill::telegram::envelope::{MessageContent, StickerContent, StickerFormat};
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.close_sticker_set_dialog(cx);
        // Turn a demo message into a sticker that belongs to no set.
        let session = app.demo_session.as_mut().expect("demo");
        let message = session
            .histories
            .get_mut(&11)
            .and_then(|history| history.messages.get_mut(&101))
            .expect("demo message");
        message.content = MessageContent::Sticker(StickerContent {
            emoji: "🎨".into(),
            width: 512,
            height: 512,
            format: StickerFormat::Webp,
            file_id: FileId(9200),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            is_premium: false,
            requires_premium: false,
            premium_animation: None,
            set_id: 0,
        });
        app.message_ui.menu = Some(crate::ui::menu_states::MessageMenuState {
            chat_id: ChatId(11),
            message_id: MessageId(101),
            position: gpui_kit::point(px(420.), px(200.)),
        });
        cx.notify();
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog.is_none()));
    assert!(vcx.update(|window, _| window.try_find("menu-add-to-sticker-set").is_some()));
    assert!(
        vcx.update(|window, _| window.try_find("menu-add-to-emoji-set").is_none()),
        "no owned emoji set in the demo"
    );
    vcx.update(|window, cx| window.click("menu-add-to-sticker-set", cx));
    frames(vcx);
    assert!(vcx.update(|window, _| window.try_find(("menu-add-to-set", 78u64)).is_some()));
    vcx.update(|window, cx| window.click(("menu-add-to-set", 78u64), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu.is_none()));
}

#[gpui_kit::test]
fn the_set_box_fits_the_narrowest_window(cx: &mut TestAppContext) {
    // 380 px is the smallest window Quill allows; the box and its grid are
    // wider than that by design and must shrink to it.
    let (_app, shell, mut vcx) = open_demo_sized(cx, 380., 600.);
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |shell, _| shell.open_dialog
        == Some(DialogKind::StickerSet)));
    let window_width = vcx.update(|window, _| f32::from(window.viewport_size().width));
    for id in ["sticker-set-edit", "sticker-set-add"] {
        let bounds = vcx
            .update(|window, _| window.try_find(id))
            .expect("the footer button")
            .bounds();
        assert!(
            f32::from(bounds.right()) <= window_width && f32::from(bounds.left()) >= 0.,
            "{id} sits inside the {window_width} px window: {bounds:?}"
        );
    }
    let cell = vcx
        .update(|window, _| window.try_find(("sticker-set-cell", 5u64)))
        .expect("a sticker cell")
        .bounds();
    assert!(
        f32::from(cell.right()) <= window_width,
        "the grid wraps inside the window: {cell:?}"
    );
}
