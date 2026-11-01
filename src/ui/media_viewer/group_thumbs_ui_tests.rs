//! The thumbnail strip in the real viewer: one thumb per album item with
//! the current one wider, a click on a thumb opens its item, the strip
//! keeps the wheel, a lone photo has no strip, and a group's photo gallery
//! has one too (needs the `demo-capture` feature for gpui-kit's test
//! support).
use super::ViewerExtra;
use super::navigation::wheel_ui_tests::{current, new_app, notch, open, settle};
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestQueryExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ids::{FileId, MessageId};
use quill::media_viewer::{MediaViewer, MediaViewerItem, thumb_file_ids};
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};
use quill::wake::{Signal, UPDATES};
use std::time::Duration;

/// A 160 x 100 photo, in album `album` (0: none).
fn add_photo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64, album: i64) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&11).expect("history");
        let mut message = history.messages.get(&101).expect("a message").clone();
        message.id = MessageId(id);
        message.media_album_id = album;
        message.content = MessageContent::Photo(PhotoContent {
            caption: String::new(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes: vec![PhotoSizeView {
                type_name: "m".into(),
                width: 160,
                height: 100,
                file_id: FileId(id as i32),
            }],
            is_secret: false,
            has_spoiler: false,
            has_stickers: false,
            minithumbnail: None,
        });
        history.messages.insert(id, message);
        cx.notify();
    });
}

fn album_of_three(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    for id in [8901, 8902, 8903] {
        add_photo(app, vcx, id, 4242);
    }
    open(app, vcx, 8901);
}

/// Let the slide to a new current thumb finish. GPUI times it on the
/// executor clock, so this advances the test one.
fn slide(vcx: &mut VisualTestContext) {
    vcx.executor().advance_clock(Duration::from_millis(200));
    settle(vcx);
}

const THUMBS: [&str; 3] = [
    "media-viewer-group-thumb-1",
    "media-viewer-group-thumb-2",
    "media-viewer-group-thumb-3",
];

#[gpui_kit::test]
fn an_album_of_three_shows_three_thumbs_with_the_current_one_wider(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    album_of_three(&app, vcx);
    let strip = vcx
        .debug_bounds("media-viewer-group-thumbs")
        .expect("the strip");
    let thumbs: Vec<_> = THUMBS
        .iter()
        .map(|id| vcx.debug_bounds(id).expect("a thumb"))
        .collect();
    assert!(vcx.debug_bounds("media-viewer-group-thumb-4").is_none());
    assert_eq!(
        f32::from(thumbs[0].size.width),
        128.0,
        "160 x 100 at 80 high"
    );
    assert_eq!(f32::from(thumbs[1].size.width), 56.0);
    assert_eq!(f32::from(thumbs[2].size.width), 56.0);
    assert!(thumbs.iter().all(|t| f32::from(t.size.height) == 80.0));
    let band = vcx
        .debug_bounds("media-viewer-bottom-band")
        .expect("the band");
    let window_center = f32::from(band.center().x);
    assert!(
        (f32::from(thumbs[0].center().x) - window_center).abs() < 1.0,
        "the current thumb is centred"
    );
    assert!(strip.bottom() <= band.bottom(), "inside the bottom band");
    let (media, _) = app
        .update_in(vcx, |app, window, cx| app.viewer_media_bounds(window, cx))
        .expect("the media box");
    assert!(media.bottom() <= strip.top(), "the picture stays above it");
}

#[gpui_kit::test]
fn clicking_the_third_thumb_opens_the_third_item(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    album_of_three(&app, vcx);
    let third = vcx.debug_bounds(THUMBS[2]).expect("thumb 3");
    vcx.simulate_click(third.center(), Modifiers::default());
    slide(vcx);
    assert_eq!(current(&app, vcx), 8903);
    assert!(app.read_with(vcx, |app, _| app.viewer.state.is_open()));
    let third = vcx.debug_bounds(THUMBS[2]).expect("thumb 3");
    assert_eq!(f32::from(third.size.width), 128.0, "now the current one");
    let first = vcx.debug_bounds(THUMBS[0]).expect("thumb 1");
    vcx.simulate_click(first.center(), Modifiers::default());
    slide(vcx);
    assert_eq!(current(&app, vcx), 8901, "and back to the first");
}

#[gpui_kit::test]
fn a_wheel_over_the_strip_keeps_the_item(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    album_of_three(&app, vcx);
    let strip = vcx
        .debug_bounds("media-viewer-group-thumbs")
        .expect("the strip");
    notch(vcx, strip.center(), -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8901);
    let second = vcx.debug_bounds(THUMBS[1]).expect("thumb 2");
    notch(vcx, second.center(), -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8901, "over a thumb too");
}

#[gpui_kit::test]
fn the_strip_follows_the_arrows_and_a_lone_photo_has_none(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    album_of_three(&app, vcx);
    app.update_in(vcx, |app, _, cx| app.step_media_viewer(1, cx));
    slide(vcx);
    let second = vcx.debug_bounds(THUMBS[1]).expect("thumb 2");
    assert_eq!(f32::from(second.size.width), 128.0);

    add_photo(&app, vcx, 8950, 0);
    open(&app, vcx, 8950);
    assert_eq!(current(&app, vcx), 8950);
    assert!(vcx.debug_bounds("media-viewer-group-thumbs").is_none());
}

/// The `ready-info-group-photos` demo, its info panel open.
fn group_info(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-info-group-photos");
    let handle = cx.open_window(size(px(1200.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    settle(&mut vcx);
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    (app, vcx)
}

fn click_the_avatar(vcx: &mut VisualTestContext) {
    let avatar = vcx.update(|window, _| window.find_by_label("View photos").bounds());
    vcx.simulate_click(avatar.center(), Modifiers::default());
    settle(vcx);
}

/// The `ready-info-group-photos` demo with its group's photos open in the
/// viewer from the info panel's avatar.
fn group_photos(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, mut vcx) = group_info(cx);
    click_the_avatar(&mut vcx);
    (app, vcx)
}

fn header(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<String> {
    app.read_with(vcx, |app, _| app.profile_photo_facts().map(|f| f.header))
}

/// The pointer rests on the viewer's controls, as it does on the way to a
/// thumb. The strip hides with the other controls after 1.1 s of idle
/// time measured on the wall clock, which a slow runner spends on the
/// first frames' image decoding alone.
fn hold_controls(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.viewer_note_pointer(true, cx));
    settle(vcx);
}

/// 1100 x 739 photos at 80 high.
const PHOTO_WIDTH: f32 = 80.0 * 1100.0 / 739.0;

#[gpui_kit::test]
fn a_group_photo_gallery_shows_a_strip_and_a_thumb_opens_its_photo(cx: &mut TestAppContext) {
    let (app, mut vcx) = group_photos(cx);
    let vcx = &mut vcx;
    assert_eq!(current(&app, vcx), 9303);
    hold_controls(&app, vcx);
    let thumbs: Vec<_> = THUMBS
        .iter()
        .map(|id| vcx.debug_bounds(id).expect("a thumb per photo"))
        .collect();
    assert!(vcx.debug_bounds("media-viewer-group-thumb-4").is_none());
    assert!((f32::from(thumbs[0].size.width) - PHOTO_WIDTH).abs() < 1.0);
    assert_eq!(f32::from(thumbs[2].size.width), 56.0);

    let third = vcx.debug_bounds(THUMBS[2]).expect("thumb 3");
    vcx.simulate_click(third.center(), Modifiers::default());
    slide(vcx);
    hold_controls(&app, vcx);
    assert_eq!(current(&app, vcx), 9301, "the oldest photo");
    assert_eq!(header(&app, vcx).as_deref(), Some("Photo 3 of 3"));
    let third = vcx.debug_bounds(THUMBS[2]).expect("thumb 3");
    assert!((f32::from(third.size.width) - PHOTO_WIDTH).abs() < 1.0);
    let band = vcx
        .debug_bounds("media-viewer-bottom-band")
        .expect("the band");
    assert!(
        (f32::from(third.center().x) - f32::from(band.center().x)).abs() < 1.0,
        "the strip slid the new current thumb to the centre"
    );

    app.update_in(vcx, |app, _, cx| app.step_media_viewer(-1, cx));
    slide(vcx);
    hold_controls(&app, vcx);
    assert_eq!(header(&app, vcx).as_deref(), Some("Photo 2 of 3"));
    let second = vcx.debug_bounds(THUMBS[1]).expect("thumb 2");
    assert!((f32::from(second.size.width) - PHOTO_WIDTH).abs() < 1.0);
}

/// The first file of each of `items`, the one its thumb shows.
fn thumb_files(items: &[MediaViewerItem]) -> Vec<FileId> {
    items
        .iter()
        .filter_map(|item| thumb_file_ids(item).first().copied())
        .collect()
}

#[gpui_kit::test]
fn a_photo_that_lands_has_its_thumb_asked_for_by_the_same_pass(cx: &mut TestAppContext) {
    let (app, mut vcx) = group_photos(cx);
    let vcx = &mut vcx;
    let (asked, gallery) = app.update(vcx, |app, cx| {
        let gallery = app.viewer.state.items().to_vec();
        app.viewer.state = MediaViewer::open_profile(gallery[..2].to_vec(), 0, 2);
        app.viewer.extra.strip_asked.clear();
        app.pump_profile_gallery(cx);
        (app.viewer.extra.strip_asked.clone(), gallery)
    });
    assert_eq!(gallery.len(), 3);
    assert_eq!(asked, thumb_files(&gallery), "every tile the pass drew");
}

#[gpui_kit::test]
fn the_first_frame_of_a_viewer_wakes_the_pass_that_asks_for_its_thumbs(cx: &mut TestAppContext) {
    let (app, mut vcx) = group_info(cx);
    let vcx = &mut vcx;
    let woke = UPDATES.epoch();
    click_the_avatar(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.viewer.extra.strip_asked.is_empty()),
        "the open had no width to lay the strip out in"
    );
    assert_ne!(UPDATES.epoch(), woke, "a resting poll loop runs a pass");
    let (asked, gallery) = app.update(vcx, |app, cx| {
        app.pump_profile_gallery(cx);
        (
            app.viewer.extra.strip_asked.clone(),
            app.viewer.state.items().to_vec(),
        )
    });
    assert_eq!(asked, thumb_files(&gallery));
}

#[test]
fn a_strip_wakes_a_pass_once_per_width() {
    static WAKE: Signal = Signal::new();
    let mut extra = ViewerExtra::default();
    let frames_at = |extra: &mut ViewerExtra, width: f32| {
        for _ in 0..3 {
            extra.set_strip_lane(width, &WAKE);
        }
        WAKE.epoch()
    };
    assert_eq!(frames_at(&mut extra, 800.0), 1, "the first width");
    assert_eq!(frames_at(&mut extra, 800.0), 1, "the same width");
    assert_eq!(frames_at(&mut extra, 640.0), 2, "a new width");
}

/// Past the idle wait on both clocks: the wall clock the viewer measures
/// idle time on, and the executor clock its hide timer sleeps on.
fn idle(vcx: &mut VisualTestContext) {
    std::thread::sleep(Duration::from_millis(1200));
    vcx.executor().advance_clock(Duration::from_millis(1200));
    settle(vcx);
}

#[gpui_kit::test]
fn the_strip_stays_up_while_the_pointer_rests_on_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    album_of_three(&app, vcx);
    let second = vcx.debug_bounds(THUMBS[1]).expect("thumb 2");
    vcx.simulate_mouse_move(second.center(), None, Modifiers::default());
    settle(vcx);
    vcx.simulate_click(second.center(), Modifiers::default());
    slide(vcx);
    idle(vcx);
    assert_eq!(current(&app, vcx), 8902);
    assert!(
        app.read_with(vcx, |app, _| !app.viewer.controls_hidden),
        "a click on a thumb under the resting pointer keeps the strip up"
    );

    let band = vcx
        .debug_bounds("media-viewer-bottom-band")
        .expect("the band");
    let above = gpui_kit::point(band.center().x, band.top() - px(80.0));
    vcx.simulate_mouse_move(above, None, Modifiers::default());
    settle(vcx);
    idle(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.viewer.controls_hidden),
        "off the strip, the controls fade after the idle wait"
    );
}
