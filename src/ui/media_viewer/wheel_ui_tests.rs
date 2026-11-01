//! A wheel notch over the viewer moves to the next item, over the
//! picture and over the black around it; with Cmd (Ctrl) it zooms
//! instead, and the last item stays put. A trackpad flick moves one
//! item, and the bottom controls keep the wheel. A horizontal swipe
//! pulls an arrow in and moves one item once it is fully in (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext, Entity, Modifiers, Pixels, Point, ScrollDelta, ScrollEvent, TouchPhase,
};
use gpui_kit::{TestAppContext, VisualTestContext, point, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};
use std::time::Duration;

pub(in crate::ui::media_viewer) fn new_app(
    cx: &mut TestAppContext,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

pub(in crate::ui::media_viewer) fn add_photo(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    id: i64,
) {
    add_captioned_photo(app, vcx, id, "");
}

fn add_captioned_photo(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    id: i64,
    caption: &str,
) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&11).expect("history");
        let first = history.ordered().into_iter().next();
        let mut message = first.expect("a message").clone();
        message.id = MessageId(id);
        message.content = MessageContent::Photo(PhotoContent {
            caption: caption.into(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes: vec![PhotoSizeView {
                type_name: "m".into(),
                width: 100,
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

pub(in crate::ui::media_viewer) fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

pub(in crate::ui::media_viewer) fn open(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    id: i64,
) {
    app.update_in(vcx, |app, _, cx| {
        app.close_settings();
        app.open_media_viewer(ChatId(11), MessageId(id), cx);
    });
    settle(vcx);
}

pub(in crate::ui::media_viewer) fn notch(
    vcx: &mut VisualTestContext,
    at: Point<Pixels>,
    lines: f32,
    modifiers: Modifiers,
) {
    vcx.simulate_mouse_move(at, None, Modifiers::default());
    settle(vcx);
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., lines)),
        modifiers,
        ..Default::default()
    });
    settle(vcx);
}

pub(in crate::ui::media_viewer) fn current(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> i64 {
    app.read_with(vcx, |app, _| {
        app.viewer.state.current().expect("an item").message_id.0
    })
}

/// The picture's center and a point on the black left of it.
fn targets(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (Point<Pixels>, Point<Pixels>) {
    let (bounds, _) = app
        .update_in(vcx, |app, window, cx| app.viewer_media_bounds(window, cx))
        .expect("the media box");
    let center = bounds.center();
    assert!(f32::from(bounds.left()) > 200., "room left of the picture");
    (center, point(bounds.left() - px(60.), center.y))
}

#[gpui_kit::test]
fn a_wheel_notch_moves_to_the_next_item_and_cmd_wheel_zooms(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    for id in [8901, 8902, 8903] {
        add_photo(&app, vcx, id);
    }
    open(&app, vcx, 8901);
    let (picture, backdrop) = targets(&app, vcx);

    notch(vcx, picture, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8902);
    notch(vcx, backdrop, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8903);
    notch(vcx, picture, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8903, "the last item stays put");
    notch(vcx, picture, 1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8902);

    notch(vcx, picture, 1., crate::ui::host_os::secondary_key());
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.viewer.state.current().map(|i| i.message_id.0),
            Some(8902)
        );
        assert!(app.viewer.zoom.is_zoomed());
    });
    notch(vcx, picture, -2., Modifiers::default());
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.viewer.state.current().map(|i| i.message_id.0),
            Some(8902),
            "zoomed in, a plain notch zooms instead of moving"
        );
        assert!(!app.viewer.zoom.is_zoomed(), "back at fit");
    });
}

#[gpui_kit::test]
fn a_trackpad_flick_moves_exactly_one_item(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    for id in [8901, 8902, 8903] {
        add_photo(&app, vcx, id);
    }
    open(&app, vcx, 8901);
    let (picture, _) = targets(&app, vcx);
    vcx.simulate_mouse_move(picture, None, Modifiers::default());
    settle(vcx);

    for frame in 0..5 {
        vcx.simulate_event(ScrollEvent {
            position: picture,
            delta: ScrollDelta::Pixels(point(px(0.), px(-70.))),
            modifiers: Modifiers::default(),
            touch_phase: if frame == 0 {
                TouchPhase::Started
            } else {
                TouchPhase::Moved
            },
            source: gpui_kit::ScrollSource::Pointer,
        });
    }
    settle(vcx);
    assert_eq!(current(&app, vcx), 8902, "350 px in one gesture moves once");
}

#[gpui_kit::test]
fn a_wheel_over_the_caption_or_the_bottom_band_keeps_the_item(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    for id in [8901, 8902] {
        add_captioned_photo(&app, vcx, id, "A caption under the photo");
    }
    open(&app, vcx, 8901);
    let caption = vcx
        .debug_bounds("media-viewer-caption-row")
        .expect("the caption row");
    let band = vcx
        .debug_bounds("media-viewer-bottom-band")
        .expect("the bottom band");
    let (_, backdrop) = targets(&app, vcx);

    notch(vcx, caption.center(), -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8901, "over the caption");
    let padding = point(band.left() + px(8.), band.bottom() - px(4.));
    notch(vcx, padding, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8901, "over the band around the rows");
    notch(vcx, backdrop, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8902, "over the backdrop it still moves");
}

fn trackpad(vcx: &mut VisualTestContext, at: Point<Pixels>, dx: f32, dy: f32, phase: TouchPhase) {
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        modifiers: Modifiers::default(),
        touch_phase: phase,
        source: gpui_kit::ScrollSource::Pointer,
    });
    settle(vcx);
}

/// Photo 1 of 3 open, the pointer over the picture.
fn three_photos(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Point<Pixels> {
    for id in [8901, 8902, 8903] {
        add_photo(app, vcx, id);
    }
    open(app, vcx, 8901);
    let (picture, _) = targets(app, vcx);
    vcx.simulate_mouse_move(picture, None, Modifiers::default());
    settle(vcx);
    picture
}

#[gpui_kit::test]
fn a_full_swipe_moves_to_the_next_item(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let picture = three_photos(&app, vcx);

    trackpad(vcx, picture, 0., 0., TouchPhase::Started);
    trackpad(vcx, picture, -60., 0., TouchPhase::Moved);
    assert!(vcx.debug_bounds("media-viewer-swipe-next").is_some());
    assert_eq!(current(&app, vcx), 8901, "nothing moves before the release");
    trackpad(vcx, picture, 0., 0., TouchPhase::Ended);
    assert_eq!(current(&app, vcx), 8902);
    assert!(vcx.debug_bounds("media-viewer-swipe-next").is_none());
    for _ in 0..5 {
        trackpad(vcx, picture, -40., 0., TouchPhase::Moved);
    }
    assert_eq!(current(&app, vcx), 8902, "momentum does not move again");
    assert!(vcx.debug_bounds("media-viewer-swipe-next").is_none());
}

#[gpui_kit::test]
fn a_short_swipe_cancels(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let picture = three_photos(&app, vcx);

    trackpad(vcx, picture, 0., 0., TouchPhase::Started);
    trackpad(vcx, picture, -40., 0., TouchPhase::Moved);
    let shown = vcx
        .debug_bounds("media-viewer-swipe-next")
        .expect("the next arrow pulled in");
    assert!(shown.left() > px(1000.), "at the right edge");
    trackpad(vcx, picture, 10., 0., TouchPhase::Moved);
    trackpad(vcx, picture, 0., 0., TouchPhase::Ended);
    assert_eq!(current(&app, vcx), 8901, "a 30 px swipe cancels");
    assert!(vcx.debug_bounds("media-viewer-swipe-next").is_none());
}

#[gpui_kit::test]
fn a_swipe_right_on_the_first_item_does_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let picture = three_photos(&app, vcx);

    trackpad(vcx, picture, 0., 0., TouchPhase::Started);
    trackpad(vcx, picture, 80., 0., TouchPhase::Moved);
    assert!(vcx.debug_bounds("media-viewer-swipe-prev").is_none());
    trackpad(vcx, picture, 0., 0., TouchPhase::Ended);
    assert_eq!(current(&app, vcx), 8901);
}

#[gpui_kit::test]
fn a_vertical_first_gesture_stays_with_the_wheel(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let picture = three_photos(&app, vcx);

    trackpad(vcx, picture, 0., -130., TouchPhase::Started);
    assert_eq!(current(&app, vcx), 8902, "the wheel rule moved it");
    trackpad(vcx, picture, -80., 0., TouchPhase::Moved);
    assert!(vcx.debug_bounds("media-viewer-swipe-next").is_none());
    trackpad(vcx, picture, 0., 0., TouchPhase::Ended);
    assert_eq!(current(&app, vcx), 8902, "the sideways part did not swipe");
}

#[gpui_kit::test]
fn a_swipe_without_phases_ends_after_a_pause(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let picture = three_photos(&app, vcx);

    trackpad(vcx, picture, -30., 0., TouchPhase::Moved);
    trackpad(vcx, picture, -30., 0., TouchPhase::Moved);
    assert!(vcx.debug_bounds("media-viewer-swipe-next").is_some());
    assert_eq!(current(&app, vcx), 8901);
    vcx.executor().advance_clock(Duration::from_millis(350));
    settle(vcx);
    assert_eq!(current(&app, vcx), 8902);
    assert!(vcx.debug_bounds("media-viewer-swipe-next").is_none());
}
