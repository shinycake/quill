//! The send box on a demo `QuillApp`: when it shows, what its caption,
//! buttons and tiles do, that more files join it, and that nothing under
//! it takes a click. Nothing reaches Telegram: the demo session applies a
//! send locally.

use super::options::Quality;
use super::tiles::{EditKind, TileAction, TileControls};
use crate::ui::app::QuillApp;
use crate::ui::send_box_tests::{fixture, new_app_sized, own_paid_channel, tray};
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Bounds, Entity, ExternalPaths, FileDropEvent, Focusable, Modifiers, Pixels, PlatformInput,
    TestAppContext, VisualTestContext, point, px, size,
};
use quill::composer::{AttachmentKind, ComposerAttachment};
use quill::ids::ChatId;

const PHOTO: (&str, AttachmentKind) = ("demo-thumb.png", AttachmentKind::Photo);
const VIDEO: (&str, AttachmentKind) = ("demo-clip-12s.mp4", AttachmentKind::Video);

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, mut vcx) = new_app_sized(cx, 1100.);
    frames(&mut vcx);
    (app, vcx)
}

fn show_tray(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    files: &[(&str, AttachmentKind)],
) {
    app.update_in(vcx, |app, _, cx| {
        tray(app, files);
        // The reads a pick, a paste or a drop starts; a Send waits for them.
        app.check_tray_videos(cx);
        cx.notify();
    });
    frames(vcx);
}

fn part(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let at = part(vcx, selector).center();
    vcx.simulate_click(at, Modifiers::none());
    frames(vcx);
}

fn boxed(vcx: &mut VisualTestContext) -> bool {
    vcx.debug_bounds("send-box-previews").is_some()
}

fn files(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    app.read_with(vcx, |app, _| app.pending_attachments().len())
}

fn text(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string())
}

fn field_focused(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update_in(vcx, |app, window, cx| {
        app.composer.read(cx).focus_handle(cx).is_focused(window)
    })
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.read_with(vcx, |app, _| app.session().and_then(|s| s.open_chat_id()))
}

fn type_draft(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, draft: &str) {
    app.update_in(vcx, |app, window, cx| {
        app.composer.update(cx, |input, cx| input.focus(window, cx));
    });
    vcx.simulate_input(draft);
    frames(vcx);
}

#[gpui_kit::test]
fn the_box_shows_while_the_open_chat_holds_files(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    assert!(!boxed(vcx));
    show_tray(&app, vcx, &[PHOTO, PHOTO]);
    assert!(boxed(vcx));
    assert!(vcx.update(|window, cx| window.has_active_dialog(cx)));

    // The tray is the chat's: another chat shows no box, and coming back
    // shows it again with its files.
    let home = open_chat(&app, vcx).expect("a chat is open");
    let other = ChatId(13);
    assert_ne!(home, other);
    app.update_in(vcx, |app, _, cx| {
        app.demo_session.as_mut().expect("demo").open_chat(other);
        app.sync_chat_ui(cx);
        cx.notify();
    });
    frames(vcx);
    assert!(!boxed(vcx), "the other chat holds no files");
    app.update_in(vcx, |app, _, cx| {
        app.demo_session.as_mut().expect("demo").open_chat(home);
        app.sync_chat_ui(cx);
        cx.notify();
    });
    frames(vcx);
    assert!(boxed(vcx));
    assert_eq!(files(&app, vcx), 2);

    // The last file removed closes it.
    app.update_in(vcx, |app, _, cx| {
        app.remove_attachment(0, cx);
        app.remove_attachment(0, cx);
    });
    frames(vcx);
    assert!(!boxed(vcx));
}

#[gpui_kit::test]
fn the_draft_is_the_caption_and_cancel_gives_it_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    type_draft(&app, vcx, "look at this");
    show_tray(&app, vcx, &[PHOTO]);
    // The field is in the box, focused, with what was typed.
    let (caption, dialog) = (part(vcx, "send-box-caption"), part(vcx, "dialog-0"));
    assert!(dialog.contains(&caption.center()), "{caption:?} {dialog:?}");
    assert!(field_focused(&app, vcx), "the caption takes the focus");
    vcx.simulate_input(" now");
    frames(vcx);
    assert_eq!(text(&app, vcx), "look at this now");

    click(vcx, "send-box-cancel");
    assert!(!boxed(vcx));
    assert_eq!(files(&app, vcx), 0);
    assert_eq!(text(&app, vcx), "look at this now", "back in the composer");
    assert!(field_focused(&app, vcx), "the composer takes the focus");
}

#[gpui_kit::test]
fn escape_drops_the_files_and_keeps_the_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    type_draft(&app, vcx, "kept");
    show_tray(&app, vcx, &[PHOTO, VIDEO]);
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(!boxed(vcx));
    assert_eq!(files(&app, vcx), 0);
    assert_eq!(text(&app, vcx), "kept");
    assert!(field_focused(&app, vcx));
    assert!(open_chat(&app, vcx).is_some(), "the chat stays open");
}

#[gpui_kit::test]
fn enter_in_the_caption_sends_and_leaves_the_composer_clean(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO, PHOTO]);
    vcx.simulate_input("two of them");
    frames(vcx);
    let sent = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| -> usize {
        app.read_with(vcx, |app, _| {
            app.session().map_or(0, |session| {
                let chat = session.open_chat_id().map_or(0, |chat| chat.0);
                session
                    .histories
                    .get(&chat)
                    .map_or(0, |history| history.messages.len())
            })
        })
    };
    let before = sent(&app, vcx);
    vcx.simulate_keystrokes("enter");
    frames(vcx);
    assert!(!boxed(vcx));
    assert_eq!(files(&app, vcx), 0);
    assert_eq!(text(&app, vcx), "");
    assert!(field_focused(&app, vcx));
    assert!(sent(&app, vcx) > before, "the demo session got the message");
}

#[gpui_kit::test]
fn the_send_button_sends_and_its_arrow_opens_the_send_menu(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO]);
    let menu_open = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| -> bool {
        app.read_with(vcx, |app, _| app.composer_ui.send_menu.open.is_some())
    };
    click(vcx, "send-box-send-options");
    assert!(menu_open(&app, vcx), "send without sound and Schedule");
    assert!(boxed(vcx));
    // Escape closes the menu, not the box.
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(!menu_open(&app, vcx));
    assert!(boxed(vcx));
    assert_eq!(files(&app, vcx), 1);

    click(vcx, "send-box-send");
    assert!(!boxed(vcx));
    assert_eq!(files(&app, vcx), 0);
    assert!(field_focused(&app, vcx));
}

#[gpui_kit::test]
fn a_paste_a_drop_and_add_put_more_files_in_the_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO]);

    // Paste, as the keymap dispatches it to the focused caption.
    let bytes = std::fs::read(fixture("demo-thumb.png")).expect("fixture");
    vcx.write_to_clipboard(gpui_kit::ClipboardItem::new_image(
        &gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, bytes),
    ));
    assert!(field_focused(&app, vcx));
    vcx.dispatch_action(gpui_kit::component::input::Paste);
    frames(vcx);
    assert_eq!(files(&app, vcx), 2, "the pasted picture joined");
    assert_eq!(text(&app, vcx), "", "and was not pasted as text");

    // A drop on the box.
    let at = part(vcx, "send-box-previews").center();
    for event in [
        FileDropEvent::Entered {
            position: at,
            paths: ExternalPaths([fixture("demo-notes.txt")].into_iter().collect()),
        },
        FileDropEvent::Pending { position: at },
        FileDropEvent::Submit { position: at },
    ] {
        vcx.update(|window, cx| window.dispatch_event(PlatformInput::FileDrop(event), cx));
        frames(vcx);
    }
    assert_eq!(files(&app, vcx), 3, "the dropped file joined");
    // A drop beside the box, over the chat list, joins it too and is not
    // the chat row's.
    let home = open_chat(&app, vcx);
    let beside = point(px(120.), px(300.));
    assert!(!part(vcx, "dialog-0").contains(&beside));
    for event in [
        FileDropEvent::Entered {
            position: beside,
            paths: ExternalPaths([fixture("demo-voice.ogg")].into_iter().collect()),
        },
        FileDropEvent::Pending { position: beside },
        FileDropEvent::Submit { position: beside },
    ] {
        vcx.update(|window, cx| window.dispatch_event(PlatformInput::FileDrop(event), cx));
        frames(vcx);
    }
    assert_eq!(files(&app, vcx), 4, "dropped beside the box");
    assert_eq!(open_chat(&app, vcx), home);

    // Add: the demo's picker answers with its fixture.
    click(vcx, "send-box-add");
    assert_eq!(files(&app, vcx), 5);
    assert!(boxed(vcx));
    let paths: Vec<_> = app.read_with(vcx, |app, _| {
        app.pending_attachments()
            .iter()
            .filter(|file| file.path.to_string_lossy().contains("quill-paste-"))
            .map(|file| file.path.clone())
            .collect()
    });
    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}

#[gpui_kit::test]
fn nothing_under_the_box_takes_a_click_and_a_click_outside_keeps_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    let home = open_chat(&app, vcx);
    // Where a click lands on another chat's row while nothing covers it.
    let dialog_left = {
        show_tray(&app, vcx, &[PHOTO, PHOTO]);
        part(vcx, "dialog-0").left()
    };
    let row = point(px(120.), px(300.));
    assert!(row.x < dialog_left, "the point is outside the box");
    vcx.simulate_click(row, Modifiers::none());
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), home, "the chat list took no click");
    assert!(boxed(vcx), "a click outside does not drop the files");
    assert_eq!(files(&app, vcx), 2);

    // The same click with no box does open that chat: the point is live.
    click(vcx, "send-box-cancel");
    vcx.simulate_click(row, Modifiers::none());
    frames(vcx);
    assert_ne!(open_chat(&app, vcx), home, "the row is under the point");
}

#[gpui_kit::test]
fn a_tile_button_does_its_own_thing_and_no_more(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO]);
    let hidden = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.pending_attachments().first().map(|photo| photo.spoiler)
        })
    };
    assert_eq!(hidden(&app, vcx), Some(false));
    for expected in [true, false] {
        click(vcx, "send-card-spoiler-0");
        assert_eq!(hidden(&app, vcx), Some(expected));
        assert!(boxed(vcx));
        assert_eq!(files(&app, vcx), 1);
    }
}

#[gpui_kit::test]
fn the_emoji_panel_opens_over_the_box_and_closes_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO]);
    let shown = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| -> bool {
        app.read_with(vcx, |app, _| app.send_box_emoji_shown())
    };
    click(vcx, "send-box-emoji");
    assert!(shown(&app, vcx));
    let panel = part(vcx, "send-box-emoji-panel");
    assert!(panel.size.height > px(100.), "{panel:?}");
    assert!(
        !app.read_with(vcx, |app, _| app.media_panel_over_composer()),
        "the box hosts it, not the composer"
    );

    // A click on the box beside the panel closes the panel and does
    // nothing else: Cancel is under the click.
    let cancel = part(vcx, "send-box-cancel").center();
    assert!(!panel.contains(&cancel));
    vcx.simulate_click(cancel, Modifiers::none());
    frames(vcx);
    assert!(!shown(&app, vcx));
    assert!(boxed(vcx), "Cancel under the cover was not pressed");
    assert!(field_focused(&app, vcx));

    // Escape closes the panel first, then the box.
    click(vcx, "send-box-emoji");
    assert!(shown(&app, vcx));
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(!shown(&app, vcx));
    assert!(boxed(vcx));

    // Sent with the panel open: it does not stay behind over the composer.
    click(vcx, "send-box-emoji");
    app.update_in(vcx, |app, window, cx| {
        app.submit_composer(String::new(), Default::default(), window, cx);
    });
    frames(vcx);
    assert!(!boxed(vcx));
    assert!(!app.read_with(vcx, |app, _| app.media_panel_open()));
}

#[gpui_kit::test]
fn the_photo_editor_takes_the_window_and_the_box_comes_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO, PHOTO]);
    app.update_in(vcx, |app, _, cx| app.open_photo_editor(0, cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.viewer.photo_editor.is_some(), "the editor opened");
        assert!(!app.send_box_shown(), "the box steps aside");
    });
    assert!(!boxed(vcx));
    app.update_in(vcx, |app, _, cx| app.close_photo_editor(cx));
    frames(vcx);
    assert!(boxed(vcx), "back with its files");
    assert_eq!(files(&app, vcx), 2);
    assert!(field_focused(&app, vcx));
}

#[gpui_kit::test]
fn pictures_sent_separately_stand_side_by_side(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        tray(app, &[PHOTO, PHOTO]);
        app.set_send_card_grouped(false, cx);
    });
    frames(vcx);
    let previews = part(vcx, "send-box-previews");
    let (first, second) = (
        part(vcx, "send-card-message-0"),
        part(vcx, "send-card-message-1"),
    );
    assert!(second.left() > first.right(), "{first:?} {second:?}");
    assert_eq!(second.top(), first.top());
    assert_eq!(second.size, first.size);
    // Both fit: the box never scrolls sideways.
    assert!(first.left() >= previews.left() && second.right() <= previews.right());

    // A picture as wide as the mosaic's room fits the box too.
    app.update_in(vcx, |app, _, cx| {
        tray(app, &[("demo-gif-still-wide.jpg", AttachmentKind::Photo)]);
        cx.notify();
    });
    frames(vcx);
    let (previews, wide) = (
        part(vcx, "send-box-previews"),
        part(vcx, "send-card-message-0"),
    );
    assert_eq!(wide.size.width, px(400.));
    assert!(wide.left() >= previews.left() && wide.right() <= previews.right());
    // The options are under the previews, inside the box.
    let options = part(vcx, "send-box-options");
    assert!(options.top() >= previews.bottom(), "{options:?}");
}

/// The box in a window `height` tall: its surface, the part that scrolls,
/// the options, the first media message and the Send button.
fn box_at(
    vcx: &mut VisualTestContext,
    height: f32,
) -> (
    Bounds<Pixels>,
    Bounds<Pixels>,
    Bounds<Pixels>,
    Bounds<Pixels>,
    Bounds<Pixels>,
) {
    vcx.simulate_resize(size(px(1100.), px(height)));
    frames(vcx);
    (
        part(vcx, "dialog-0"),
        part(vcx, "send-box-scroll"),
        part(vcx, "send-box-options"),
        part(vcx, "send-card-message-0"),
        part(vcx, "send-box-send"),
    )
}

#[gpui_kit::test]
fn a_short_window_shrinks_the_previews_and_keeps_every_option_in_view(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    // A lone picture twice as tall as wide takes the whole height it may.
    let dir = quill::scratch::process_dir("quill-send-box-height");
    std::fs::create_dir_all(&dir).expect("scratch");
    let tall = dir.join("tall.png");
    image::RgbaImage::from_pixel(40, 200, image::Rgba([9, 9, 9, 255]))
        .save(&tall)
        .expect("picture");
    app.update_in(vcx, |app, _, cx| {
        let photo = ComposerAttachment::pick(&tall, AttachmentKind::Photo).expect("picture");
        app.update_pending_attachments(|tray| *tray = vec![photo])
            .expect("an open chat");
        cx.notify();
    });
    frames(vcx);

    // A window with room: the picture stands as tall as a preview gets.
    let mut full = None;
    for height in [900., 700., 600.] {
        let (surface, scroll, options, media, send) = box_at(vcx, height);
        assert_eq!(media.size.height, px(240.), "at {height}");
        assert!(options.bottom() <= scroll.bottom(), "at {height}");
        assert!(send.bottom() <= surface.bottom() && surface.bottom() <= px(height));
        full = Some(surface.size.height);
    }
    // A shorter one: the picture gives way, every option row is in view
    // and nothing scrolls.
    for height in [550., 510., 480.] {
        let (surface, scroll, options, media, send) = box_at(vcx, height);
        assert!(media.size.height < px(240.), "{media:?} at {height}");
        assert!(media.size.height >= px(120.), "{media:?} at {height}");
        assert!(
            options.bottom() <= scroll.bottom(),
            "{options:?} in {scroll:?} at {height}"
        );
        assert!(send.bottom() <= surface.bottom() && surface.bottom() <= px(height));
        assert!(Some(surface.size.height) < full);
    }
    // Too short for even the least preview: it stops shrinking and the
    // part over the caption scrolls, with the buttons still in the window.
    let (surface, scroll, options, media, send) = box_at(vcx, 380.);
    assert_eq!(media.size.height, px(120.));
    assert!(options.bottom() > scroll.bottom(), "the options scroll");
    assert!(send.bottom() <= surface.bottom() && surface.bottom() <= px(380.));
    let _ = std::fs::remove_file(tall);
}

#[gpui_kit::test]
fn a_tile_offers_what_its_file_and_chat_allow(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    let actions = |app: &QuillApp, index: usize| {
        let file = app.pending_attachments().get(index).expect("a file");
        app.send_card_tile_actions(file)
    };
    app.update_in(vcx, |app, _, _| {
        tray(app, &[PHOTO, ("demo-notes.txt", AttachmentKind::Document)]);
        assert_eq!(
            actions(app, 0),
            [
                TileAction::Edit(EditKind::Photo),
                TileAction::Spoiler { hidden: false },
                TileAction::Remove,
            ]
        );
        assert_eq!(actions(app, 1), [TileAction::Remove]);

        // A channel post's video can have a cover.
        own_paid_channel(app, true);
        tray(app, &[PHOTO, VIDEO]);
        app.set_composer_group_media(Some(true));
        assert_eq!(
            actions(app, 1),
            [
                TileAction::Edit(EditKind::Video),
                TileAction::Spoiler { hidden: false },
                TileAction::Cover { chosen: false },
                TileAction::Remove,
            ]
        );
        app.update_pending_attachment(1, |video| {
            video.extras.cover = Some(fixture("demo-thumb.png"));
        });
        assert!(actions(app, 1).contains(&TileAction::Cover { chosen: true }));

        // Paid media is blurred by Telegram: no spoiler of its own.
        app.update_pending_send(|send| send.paid_stars = 50);
        assert!(app.tray_paid_price_set());
        assert!(
            !actions(app, 0)
                .iter()
                .any(|action| matches!(action, TileAction::Spoiler { .. }))
        );
    });
}

#[test]
fn a_tile_folds_its_controls_into_a_menu_only_when_they_do_not_fit() {
    // Three buttons need 32 + 3 × 22 points.
    assert_eq!(TileControls::for_tile(98, 3), TileControls::Buttons);
    assert_eq!(TileControls::for_tile(97, 3), TileControls::Menu);
    assert_eq!(TileControls::for_tile(120, 4), TileControls::Buttons);
    assert_eq!(TileControls::for_tile(119, 4), TileControls::Menu);
    // A menu of one would be that one button behind a click.
    assert_eq!(TileControls::for_tile(10, 1), TileControls::Buttons);
    assert_eq!(TileControls::for_tile(10, 0), TileControls::Buttons);
}

#[gpui_kit::test]
fn quality_is_one_choice_of_three_and_files_keep_the_way_back(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("quill-send-card-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let big = dir.join("big.png");
    image::RgbImage::from_pixel(1400, 300, image::Rgb([9, 9, 9]))
        .save(&big)
        .expect("a picture");
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    let all = [Quality::Compressed, Quality::Hd, Quality::File];
    app.update_in(vcx, |app, _, cx| {
        tray(app, &[("demo-notes.txt", AttachmentKind::Document)]);
        assert_eq!(app.send_card_quality(), None, "a file has no quality");

        // A small photo gains nothing from HD.
        tray(app, &[PHOTO]);
        assert_eq!(
            app.send_card_quality(),
            Some((
                Quality::Compressed,
                &[Quality::Compressed, Quality::File][..]
            ))
        );

        let photo = ComposerAttachment::pick(&big, AttachmentKind::Photo).expect("a photo");
        app.update_pending_attachments(|tray| tray.push(photo))
            .expect("an open chat");
        assert_eq!(
            app.send_card_quality(),
            Some((Quality::Compressed, &all[..]))
        );

        app.set_send_card_quality(Quality::Hd, cx);
        assert_eq!(app.send_card_quality(), Some((Quality::Hd, &all[..])));
        assert!(app.composer_media_options().hd_photos);

        // As files the same three stay on offer, so one click goes back.
        app.set_send_card_quality(Quality::File, cx);
        assert_eq!(app.send_card_quality(), Some((Quality::File, &all[..])));
        assert!(
            app.pending_attachments()
                .iter()
                .all(|file| file.kind == AttachmentKind::Document)
        );
        assert!(!app.composer_media_options().hd_photos);

        app.set_send_card_quality(Quality::Compressed, cx);
        assert_eq!(
            app.send_card_quality(),
            Some((Quality::Compressed, &all[..]))
        );
        assert!(
            app.pending_attachments()
                .iter()
                .all(|file| file.kind == AttachmentKind::Photo)
        );
        assert!(!app.composer_media_options().hd_photos);
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn album_or_separately_is_set_not_flipped(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        tray(app, &[PHOTO, PHOTO]);
        app.set_send_card_grouped(false, cx);
        assert!(!app.composer_group_media_effective());
        // Picking the segment that is already chosen changes nothing.
        app.set_send_card_grouped(false, cx);
        assert!(!app.composer_group_media_effective());
        app.set_send_card_grouped(true, cx);
        assert!(app.composer_group_media_effective());
    });
}
