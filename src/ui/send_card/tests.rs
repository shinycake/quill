//! The send card on a demo `QuillApp`: where its two columns sit, what a
//! tile offers, and that each choice is one click or one call away.
//! Nothing is sent: the demo session has no sender.

use super::layout::{CardShape, OPTIONS_WIDTH};
use super::options::Quality;
use super::tiles::{EditKind, TileAction, TileControls};
use crate::ui::app::QuillApp;
use crate::ui::send_box_tests::{fixture, new_app_sized, own_paid_channel, tray};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext};
use quill::composer::{AttachmentKind, ComposerAttachment};

const PHOTO: (&str, AttachmentKind) = ("demo-thumb.png", AttachmentKind::Photo);
const VIDEO: (&str, AttachmentKind) = ("demo-clip-12s.mp4", AttachmentKind::Video);

/// Draw until the card has the chat's measured width.
fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn show_tray(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    files: &[(&str, AttachmentKind)],
) {
    app.update_in(vcx, |app, _, cx| {
        tray(app, files);
        cx.notify();
    });
    frames(vcx);
}

fn part(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

#[gpui_kit::test]
fn options_sit_beside_the_previews_and_the_card_hugs_them(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app_sized(cx, 1300.);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO, PHOTO]);
    let (card, previews, options) = (
        part(vcx, "send-card"),
        part(vcx, "send-card-previews"),
        part(vcx, "send-card-options"),
    );
    assert!(
        options.left() >= previews.right(),
        "{options:?} {previews:?}"
    );
    assert_eq!(options.top(), previews.top());
    assert_eq!(f32::from(options.size.width), OPTIONS_WIDTH);
    // No dead space: the card ends where its options do.
    assert!(
        card.right() - options.right() < gpui_kit::px(12.),
        "{card:?}"
    );
    let pane = f32::from(app.read_with(vcx, |app, _| app.history.measure.get().pane()));
    assert!(
        f32::from(card.size.width) < pane - 100.,
        "{card:?} in {pane}"
    );
}

#[gpui_kit::test]
fn pictures_sent_separately_stand_side_by_side(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app_sized(cx, 1300.);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        tray(app, &[PHOTO, PHOTO]);
        app.set_send_card_grouped(false, cx);
    });
    frames(vcx);
    let previews = part(vcx, "send-card-previews");
    let (first, second) = (
        part(vcx, "send-card-message-0"),
        part(vcx, "send-card-message-1"),
    );
    assert!(second.left() > first.right(), "{first:?} {second:?}");
    assert_eq!(second.top(), first.top());
    assert_eq!(second.size, first.size);
    // The column is as wide as the two, so the options follow them.
    assert_eq!(second.right(), previews.right());
    assert!(part(vcx, "send-card-options").left() >= previews.right());
}

#[gpui_kit::test]
fn a_narrow_chat_puts_the_options_above_the_previews(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app_sized(cx, 760.);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO, PHOTO]);
    let pane = f32::from(app.read_with(vcx, |app, _| app.history.measure.get().pane()));
    assert_eq!(
        super::layout::CardRoom::for_pane(pane).shape,
        CardShape::Stacked,
        "the test window leaves a chat {pane} wide"
    );
    let (previews, options) = (
        part(vcx, "send-card-previews"),
        part(vcx, "send-card-options"),
    );
    assert!(
        options.bottom() <= previews.top(),
        "{options:?} {previews:?}"
    );
    assert_eq!(options.left(), previews.left());
    assert!(previews.right() <= part(vcx, "send-card").right());
}

#[gpui_kit::test]
fn one_click_on_a_tile_hides_it_with_a_spoiler_and_one_more_shows_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app_sized(cx, 1300.);
    let vcx = &mut vcx;
    show_tray(&app, vcx, &[PHOTO]);
    let hidden = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.pending_attachments().first().map(|photo| photo.spoiler)
        })
    };
    assert_eq!(hidden(&app, vcx), Some(false));
    for expected in [true, false] {
        let button = part(vcx, "send-card-spoiler-0");
        vcx.simulate_click(button.center(), Modifiers::none());
        frames(vcx);
        assert_eq!(hidden(&app, vcx), Some(expected));
    }
}

#[gpui_kit::test]
fn a_tile_offers_what_its_file_and_chat_allow(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app_sized(cx, 1300.);
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
    let (app, mut vcx) = new_app_sized(cx, 1300.);
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
    let (app, mut vcx) = new_app_sized(cx, 1300.);
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
