//! A click on the story panel stays on the panel: only the dimmed backdrop
//! around it closes the viewer (tdesktop closes on a click outside the
//! content). Over the `ready-story-viewer` demo; nothing reaches Telegram.
use super::open_tests::{frames, open_app};
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext, point, px};

fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.stories.viewer.is_open())
}

#[gpui_kit::test]
fn clicks_on_the_story_and_its_header_keep_the_viewer_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    assert!(open(&app, vcx), "the demo opens the viewer");
    let frame = vcx.debug_bounds("story-viewer-frame").expect("the story");
    vcx.simulate_click(frame.center(), Modifiers::default());
    frames(vcx);
    assert!(open(&app, vcx), "a click on the story itself");
    let pause = vcx.debug_bounds("story-pause").expect("the pause button");
    vcx.simulate_click(pause.center(), Modifiers::default());
    frames(vcx);
    assert!(open(&app, vcx), "a click on the pause button");
    let paused = app.read_with(vcx, |app, _| app.stories.pause.is_paused());
    assert!(paused, "the pause button pauses");
    let panel = vcx.debug_bounds("story-viewer-panel").expect("the panel");
    let edge = point(panel.origin.x + px(4.), panel.origin.y + px(4.));
    vcx.simulate_click(edge, Modifiers::default());
    frames(vcx);
    assert!(open(&app, vcx), "a click on the panel's padding");
}

#[gpui_kit::test]
fn a_click_on_the_backdrop_closes_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let panel = vcx.debug_bounds("story-viewer-panel").expect("the panel");
    let below = point(
        panel.center().x,
        panel.origin.y + panel.size.height + px(8.),
    );
    let outside = if below.y < px(895.) {
        below
    } else {
        point(px(8.), px(8.))
    };
    vcx.simulate_click(outside, Modifiers::default());
    frames(vcx);
    assert!(!open(&app, vcx), "the backdrop closes the viewer");
}
