//! The story header over the demo's photo story and live story: the
//! position follows the name as its own label, the age and "edited" sit
//! under the name, and a watched stream puts the LIVE badge there instead.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, Entity, Pixels, TestAppContext, VisualTestContext, px, size};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext, kind: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(kind))));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds(selector)
}

#[gpui_kit::test]
fn the_position_follows_the_name_and_the_age_sits_under_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-story-photo");
    let vcx = &mut vcx;
    assert!(app.read_with(vcx, |app, _| app.stories.viewer.is_open()));
    let identity = find(vcx, "story-header-identity").expect("the identity");
    let name = find(vcx, "story-header-name").expect("the name");
    let counter = find(vcx, "story-header-counter").expect("1/3 after the name");
    let age = find(vcx, "story-header-age").expect("the age line");
    assert!(
        counter.left() >= name.right(),
        "the counter follows the name"
    );
    assert!(
        counter.right() <= identity.right(),
        "the counter is not clipped"
    );
    assert!(age.top() >= name.bottom(), "the age is the second line");
    assert!(
        find(vcx, "live-badge").is_none(),
        "no LIVE badge on a photo"
    );
}

/// tdesktop `setVideoStreamViewers`: a watched stream's second line is the
/// LIVE badge and its viewers, with no age, and the live controls and
/// Close keep their size beside the identity.
#[gpui_kit::test]
fn a_live_story_shows_the_badge_under_the_name_and_no_age(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx, "ready-live-story");
    let vcx = &mut vcx;
    let identity = find(vcx, "story-header-identity").expect("the identity");
    let name = find(vcx, "story-header-name").expect("the name");
    let badge = find(vcx, "live-badge").expect("the LIVE badge");
    assert!(badge.top() >= name.bottom(), "the badge is the second line");
    assert!(badge.left() >= identity.left() && badge.right() <= identity.right());
    assert!(find(vcx, "story-header-age").is_none(), "no age while live");
}
