//! The viewer's `+` / `-` keys step through tdesktop's zoom levels with a
//! fit stop on the way, `0` and Ctrl+0 toggle natural size, and Cmd
//! (Ctrl) + wheel still zooms continuously (needs the `demo-capture`
//! feature for gpui-kit's test support).
use super::wheel_ui_tests::{new_app, notch, settle};
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::ids::{FileId, MessageId};
use quill::media_viewer::{ZoomLevels, wheel_zoom_factor};
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};

/// A photo larger than the window's media area: it fits between levels -1
/// and 0.
const WIDE: (i32, i32) = (1600, 1000);

fn add_photo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64, size: (i32, i32)) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&11).expect("history");
        let mut message = history.messages.get(&101).expect("a message").clone();
        message.id = MessageId(id);
        message.content = MessageContent::Photo(PhotoContent {
            caption: "".into(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes: vec![PhotoSizeView {
                type_name: "y".into(),
                width: size.0,
                height: size.1,
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

/// Open photo `id` with the chat list focused, as after a click on a
/// message, so the app's key bindings dispatch.
fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64) {
    super::wheel_ui_tests::open(app, vcx, id);
    app.update_in(vcx, |app, window, cx| window.focus(&app.focus_sidebar, cx));
    settle(vcx);
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    settle(vcx);
}

fn levels(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ZoomLevels {
    app.read_with(vcx, |app, _| app.viewer.zoom.levels.expect("a zoom ladder"))
}

/// The zoom against the photo's natural size.
fn scale(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> f32 {
    let fit = levels(app, vcx).fit();
    app.read_with(vcx, |app, _| app.viewer.zoom.zoom * fit)
}

fn label(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> i32 {
    app.read_with(vcx, |app, _| app.viewer.zoom.percent().round() as i32)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3 * b.abs().max(1.0)
}

#[gpui_kit::test]
fn plus_from_fit_reaches_the_next_level_and_minus_steps_back_past_fit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8911, WIDE);
    open(&app, vcx, 8911);
    let ladder = levels(&app, vcx);
    assert!(
        ladder.fit() > 0.5 && ladder.fit() < 1.0,
        "fit {} sits between levels -1 and 0",
        ladder.fit()
    );
    assert_eq!(label(&app, vcx), (ladder.fit() * 100.0).round() as i32);

    press(vcx, "=");
    assert!(
        close(scale(&app, vcx), 1.0),
        "+ goes to level 0, natural size"
    );
    assert_eq!(label(&app, vcx), 100);
    press(vcx, "=");
    assert!(close(scale(&app, vcx), 2.0), "then level 1");
    assert_eq!(label(&app, vcx), 200);

    press(vcx, "- -");
    assert_eq!(
        app.read_with(vcx, |app, _| app.viewer.zoom.zoom),
        1.0,
        "back at fit"
    );
    press(vcx, "-");
    assert!(close(scale(&app, vcx), 0.5), "below fit, level -1");
    assert_eq!(label(&app, vcx), 50);
}

#[gpui_kit::test]
fn cmd_wheel_still_zooms_continuously(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8911, WIDE);
    open(&app, vcx, 8911);
    let (bounds, _) = app
        .update_in(vcx, |app, window, cx| app.viewer_media_bounds(window, cx))
        .expect("the media box");
    notch(
        vcx,
        bounds.center(),
        1.,
        crate::ui::host_os::secondary_key(),
    );
    let zoom = app.read_with(vcx, |app, _| app.viewer.zoom.zoom);
    assert!(
        close(zoom, wheel_zoom_factor(40.0)),
        "one notch is one wheel step, not a ladder stop: {zoom}"
    );
    let between = scale(&app, vcx);
    press(vcx, "=");
    let next = levels(&app, vcx).step(between, true);
    assert!(next > between * 1.001);
    assert!(
        close(scale(&app, vcx), next),
        "+ from between stops goes to the next one"
    );
}

/// Cmd+0 on macOS, Ctrl+0 elsewhere.
fn primary_zero() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-0"
    } else {
        "ctrl-0"
    }
}

fn at_fit(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.viewer.zoom.zoom == 1.0)
}

#[gpui_kit::test]
fn zero_and_primary_zero_toggle_fit_and_natural_size(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8911, WIDE);
    open(&app, vcx, 8911);

    press(vcx, "0");
    assert!(close(scale(&app, vcx), 1.0), "fit to natural size");
    assert_eq!(label(&app, vcx), 100);
    press(vcx, "0");
    assert!(at_fit(&app, vcx), "natural size back to fit");

    press(vcx, primary_zero());
    assert!(close(scale(&app, vcx), 1.0), "Ctrl+0 too");
    press(vcx, primary_zero());
    assert!(at_fit(&app, vcx));
    assert!(
        app.read_with(vcx, |app, _| app.viewer.state.is_open()),
        "the viewer kept the chord from Saved Messages"
    );

    press(vcx, "= =");
    press(vcx, "0");
    assert!(close(scale(&app, vcx), 1.0), "from 2x to natural size");
}
