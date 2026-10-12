//! The send box options on a demo `QuillApp`: paid media, High quality and
//! the price box. Nothing is sent: the demo session has no sender.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
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
    app.composer_ui.pending_attachments = files
        .iter()
        .map(|(name, kind)| ComposerAttachment::pick(&fixture(name), *kind).unwrap())
        .collect();
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
        app.composer_ui
            .pending_attachments
            .push(ComposerAttachment::pick(&big, AttachmentKind::Photo).unwrap());
        assert!(app.tray_has_large_photo());
        assert!(!app.composer_media_options().hd_photos);
        app.composer_ui.send_box.hd_photos = true;
        assert!(app.composer_media_options().hd_photos);
        // Sent as files, the quality choice means nothing.
        quill::composer::ComposerAttachment::set_send_as_files(
            &mut app.composer_ui.pending_attachments,
            true,
        );
        assert!(!app.composer_media_options().hd_photos);
    });
    let _ = std::fs::remove_dir_all(&dir);
}
