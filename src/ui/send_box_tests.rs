//! The send box options on a demo `QuillApp`: paid media, High quality and
//! the price box. Nothing is sent: the demo session has no sender.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::composer::{AttachmentKind, ComposerAttachment};
use quill::ids::ChatId;
use quill::telegram::envelope::{ChannelMemberStatus, SupergroupFullAdmin};
use std::path::PathBuf;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures")
        .join(name)
}

/// Chat 13 is the demo channel; make it one the account owns that allows
/// paid media.
fn own_paid_channel(app: &mut QuillApp, allowed: bool) {
    let session = app.demo_session.as_mut().expect("demo session");
    session.open_chat(ChatId(13));
    let supergroup = session.chat_supergroup(ChatId(13)).expect("channel");
    session.groups.supergroup_full_infos.insert(
        supergroup,
        quill::state::SupergroupFullInfoData {
            admin: SupergroupFullAdmin {
                has_paid_media_allowed: allowed,
                ..SupergroupFullAdmin::default()
            },
            ..quill::state::SupergroupFullInfoData::default()
        },
    );
    session.chats.get_mut(&13).unwrap().my_member_status = Some(ChannelMemberStatus::Creator);
}

fn tray(app: &mut QuillApp, files: &[(&str, AttachmentKind)]) {
    let files: Vec<_> = files
        .iter()
        .map(|(name, kind)| ComposerAttachment::pick(&fixture(name), *kind).unwrap())
        .collect();
    app.update_pending_attachments(|tray| *tray = files)
        .expect("an open chat");
}

#[gpui_kit::test]
fn paid_media_is_for_channels_that_allow_it_and_the_price_box_sets_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        own_paid_channel(app, true);
        tray(
            app,
            &[
                ("demo-thumb.png", AttachmentKind::Photo),
                ("demo-clip.mp4", AttachmentKind::Video),
            ],
        );
        app.composer_ui.group_media = Some(true);
        assert!(app.paid_media_available());
        assert_eq!(app.composer_media_options().paid_stars, 0);

        app.open_price_dialog(window, cx);
        let input = app
            .composer_ui
            .send_box
            .price
            .as_ref()
            .unwrap()
            .input
            .clone();
        input.update(cx, |input, cx| input.set_value("10001", window, cx));
        assert!(!app.submit_price_dialog(cx), "over the limit stays open");
        assert!(app.composer_ui.send_box.price.as_ref().unwrap().too_high);
        input.update(cx, |input, cx| input.set_value("250", window, cx));
        assert!(app.submit_price_dialog(cx));
        assert!(app.composer_ui.send_box.price.is_none());
        assert_eq!(app.composer_media_options().paid_stars, 250);

        // Files can't be paid media, and neither can ungrouped media.
        app.composer_ui.group_media = Some(false);
        assert_eq!(app.composer_media_options().paid_stars, 0);
        app.composer_ui.group_media = Some(true);
        tray(app, &[("demo-notes.txt", AttachmentKind::Document)]);
        assert_eq!(app.composer_media_options().paid_stars, 0);

        // A channel that doesn't allow it, and a private chat.
        tray(app, &[("demo-thumb.png", AttachmentKind::Photo)]);
        assert_eq!(app.composer_media_options().paid_stars, 250);
        own_paid_channel(app, false);
        assert!(!app.paid_media_available());
        app.demo_session.as_mut().unwrap().open_chat(ChatId(11));
        assert_eq!(app.composer_media_options().paid_stars, 0);
    });
}

#[gpui_kit::test]
fn high_quality_is_offered_for_photos_over_1280_only(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("quill-send-box-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let big = dir.join("big.png");
    image::RgbImage::from_pixel(1400, 300, image::Rgb([9, 9, 9]))
        .save(&big)
        .unwrap();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, _| {
        tray(app, &[("demo-thumb.png", AttachmentKind::Photo)]);
        assert!(!app.tray_has_large_photo());
        let photo = ComposerAttachment::pick(&big, AttachmentKind::Photo).unwrap();
        app.update_pending_attachments(|tray| tray.push(photo))
            .expect("an open chat");
        assert!(app.tray_has_large_photo());
        assert!(!app.composer_media_options().hd_photos);
        app.update_pending_send(|send| send.hd_photos = true);
        assert!(app.composer_media_options().hd_photos);
        // Sent as files, the quality choice means nothing.
        app.update_pending_attachments(|tray| {
            quill::composer::ComposerAttachment::set_send_as_files(tray, true)
        })
        .expect("an open chat");
        assert!(!app.composer_media_options().hd_photos);
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_soundless_mp4_becomes_a_gif_after_the_background_check(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("quill-send-box-gif-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clip = dir.join("silent.mp4");
    let made = quill::media_tools::command("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=64x64:rate=5",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&clip)
        .output()
        .is_ok_and(|out| out.status.success());
    if !made {
        // Skipped where ffmpeg isn't installed.
        return;
    }
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.attach_dropped_files(std::slice::from_ref(&clip), cx);
        // The probe runs in the background: the tray waits for it.
        assert!(app.tray_videos_checking());
        assert!(!app.pending_attachments()[0].extras.gif);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(!app.tray_videos_checking());
        assert!(app.pending_attachments()[0].extras.gif);
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_slot_whose_files_are_being_read_says_so_and_other_chats_are_free(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        tray(app, &[("demo-thumb.png", AttachmentKind::Photo)]);
        let open = app.open_chat_id().expect("a chat is open");
        assert!(!app.tray_preparing());
        assert!(app.send_box_option_controls(cx).is_empty());

        let slot = app.send_slot().expect("an open slot");
        let job = app.composer_ui.send_box.prep.begin(slot);
        assert!(job.is_some());
        assert!(app.tray_preparing());
        assert_eq!(
            app.send_box_option_controls(cx).len(),
            1,
            "the tray says the files are being prepared"
        );

        // Another chat's composer is not held by it.
        let other = if open == ChatId(12) {
            ChatId(11)
        } else {
            ChatId(12)
        };
        app.demo_session.as_mut().unwrap().open_chat(other);
        assert!(!app.tray_preparing());
        app.demo_session.as_mut().unwrap().open_chat(open);
        assert!(app.tray_preparing(), "back in the chat it is still held");

        // The job ends: the label goes with it, nothing else stored.
        app.composer_ui.send_box.prep.finish(job.unwrap());
        assert!(!app.tray_preparing());
        assert!(app.send_box_option_controls(cx).is_empty());
    });
}

/// Paste the PNG fixture into the focused composer.
fn paste_picture(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let bytes = std::fs::read(fixture("demo-thumb.png")).expect("fixture");
    vcx.write_to_clipboard(gpui_kit::ClipboardItem::new_image(
        &gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, bytes),
    ));
    app.update_in(vcx, |app, window, cx| {
        let focus = app.composer.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        assert!(app.paste_image_from_clipboard(window, cx));
    });
}

#[gpui_kit::test]
fn a_picture_pasted_over_a_file_joins_it_as_a_file(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        tray(app, &[("demo-notes.txt", AttachmentKind::Document)]);
    });
    paste_picture(&app, &mut vcx);
    app.update_in(&mut vcx, |app, _, _| {
        let kinds: Vec<AttachmentKind> = app.pending_attachments().iter().map(|a| a.kind).collect();
        assert_eq!(kinds, [AttachmentKind::Document, AttachmentKind::Document]);
        assert_eq!(app.pending_attachments()[0].file_name, "demo-notes.txt");
        assert!(app.connection.note().text().contains("quill-paste-"));
    });
    // Over a photo it is a photo.
    app.update_in(&mut vcx, |app, _, _| {
        tray(app, &[("demo-thumb.png", AttachmentKind::Photo)]);
    });
    paste_picture(&app, &mut vcx);
    app.update_in(&mut vcx, |app, _, _| {
        let kinds: Vec<AttachmentKind> = app.pending_attachments().iter().map(|a| a.kind).collect();
        assert_eq!(kinds, [AttachmentKind::Photo, AttachmentKind::Photo]);
    });
}
