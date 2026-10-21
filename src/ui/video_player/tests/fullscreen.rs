//! Video full screen in the viewer on a scripted player. The test window
//! toggles full screen at once, so the mode reaches `Active` on the next
//! frame.

use super::Script;
use super::viewer::{open_scripted, settle, wake_controls};
use crate::ui::app::QuillApp;
use crate::ui::media_viewer::VideoFullscreen;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Bounds, Entity, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};
use std::cell::RefCell;
use std::rc::Rc;

const CLIP: u64 = 204;

fn ready_viewer(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let script = Rc::new(RefCell::new(Script {
        ready: true,
        ..Script::default()
    }));
    let (app, mut vcx) = open_scripted(cx, &script);
    // The demo opens the privacy page over the window; clicks go there.
    app.update_in(&mut vcx, |app, _, cx| {
        app.close_settings();
        cx.notify();
    });
    settle(&mut vcx);
    (app, vcx)
}

fn enter_fullscreen(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.viewer_toggle_fullscreen(window, cx)
    });
    settle(vcx);
}

fn mode(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> VideoFullscreen {
    app.read_with(vcx, |app, _| app.viewer.extra.video_fullscreen)
}

fn is_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.viewer.state.is_open())
}

/// The media's box, and whether a caption takes room under it.
fn media(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (Bounds<Pixels>, bool) {
    app.update_in(vcx, |app, window, cx| app.viewer_media_bounds(window, cx))
        .expect("an item in the viewer")
}

/// The top bar's close button is drawn, with the controls awake.
fn top_bar_shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update(vcx, |app, _| wake_controls(app));
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(("media-viewer-close", CLIP)).is_some()
    })
}

/// A click at `at`, each event on a freshly drawn frame (hit testing
/// reads the last frame drawn), as the kit's own `click` does.
fn click(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.update(|window, cx| {
        let events = [
            MouseMoveEvent {
                position: at,
                pressed_button: None,
                modifiers: Modifiers::none(),
            }
            .to_platform_input(),
            MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            MouseUpEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::none(),
                click_count: 1,
            }
            .to_platform_input(),
        ];
        for event in events {
            window.render_frame(cx);
            window.dispatch_event(event, cx);
        }
        window.render_frame(cx);
    });
    settle(vcx);
}

#[gpui_kit::test]
fn video_full_screen_fills_the_window_and_escape_leaves_it_first(cx: &mut TestAppContext) {
    let (app, mut vcx) = ready_viewer(cx);
    let vcx = &mut vcx;
    assert!(media(&app, vcx).1, "the clip's caption takes room");
    assert!(top_bar_shown(&app, vcx));
    enter_fullscreen(&app, vcx);
    assert_eq!(mode(&app, vcx), VideoFullscreen::Active);
    assert!(vcx.update(|window, _| window.is_fullscreen()));

    let viewport = vcx.update(|window, _| window.viewport_size());
    let (media_box, caption) = media(&app, vcx);
    let near = |a: Pixels, b: Pixels| (a - b).abs() < px(1.);
    // The largest aspect fit: centered, touching two opposite edges.
    assert!(
        near(media_box.left(), viewport.width - media_box.right()),
        "{media_box:?}"
    );
    assert!(
        near(media_box.top(), viewport.height - media_box.bottom()),
        "{media_box:?}"
    );
    assert!(
        (near(media_box.left(), px(0.)) && near(media_box.right(), viewport.width))
            || (near(media_box.top(), px(0.)) && near(media_box.bottom(), viewport.height)),
        "{media_box:?} in {viewport:?}"
    );
    assert!(!caption, "no caption in full screen");
    assert!(!top_bar_shown(&app, vcx));

    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert!(is_open(&app, vcx));
    assert_eq!(mode(&app, vcx), VideoFullscreen::Off);
    assert!(!vcx.update(|window, _| window.is_fullscreen()));
    assert!(top_bar_shown(&app, vcx));
    assert!(media(&app, vcx).1);

    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert!(!is_open(&app, vcx));
}

/// tdesktop: in full screen the whole window is the picture, so a click
/// on the black bars plays or pauses rather than closing the viewer.
#[gpui_kit::test]
fn a_click_on_the_bars_in_full_screen_plays_or_pauses(cx: &mut TestAppContext) {
    let (app, mut vcx) = ready_viewer(cx);
    let vcx = &mut vcx;
    enter_fullscreen(&app, vcx);
    let playing =
        |vcx: &mut VisualTestContext| app.read_with(vcx, |app, _| app.viewer_video_playing());
    let before = playing(vcx);
    let (media_box, _) = media(&app, vcx);
    let bar = point(px(40.), media_box.top() / 2.);
    assert!(
        media_box.top() > px(20.),
        "a letterboxed clip: {media_box:?}"
    );

    click(vcx, bar);
    assert!(is_open(&app, vcx));
    assert_ne!(playing(vcx), before);

    click(vcx, media_box.center());
    assert!(is_open(&app, vcx));
    assert_eq!(playing(vcx), before);
}

/// Out of full screen the black around the media is the backdrop: a
/// click there closes the viewer.
#[gpui_kit::test]
fn a_click_on_the_backdrop_closes_the_windowed_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = ready_viewer(cx);
    let vcx = &mut vcx;
    let (media_box, _) = media(&app, vcx);
    click(vcx, point(media_box.left() / 2., media_box.top() + px(20.)));
    assert!(!is_open(&app, vcx));
}
