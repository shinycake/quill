//! The album strip in the real viewer: one thumb per album item with the
//! current one wider, a click on a thumb opens its item, the strip keeps
//! the wheel, and a lone photo has no strip (needs the `demo-capture`
//! feature for gpui-kit's test support).
use super::navigation::wheel_ui_tests::{current, new_app, notch, open, settle};
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};
use quill::ids::{FileId, MessageId};
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};
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
/// wall clock, so this waits for real rather than advancing the test one.
fn slide(vcx: &mut VisualTestContext) {
    std::thread::sleep(Duration::from_millis(200));
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
