//! The story header over the demo's photo story and live story: the
//! position follows the name as its own label, the age and "edited" sit
//! under the name, and a watched stream puts the LIVE badge there instead;
//! and the "Reposted from" link opens the original story alone, or its
//! poster's profile once the original turns out gone.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, px, size,
};
use quill::story_repost::RepostGone;
use quill::story_viewer::StoryOpenScope;

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

#[gpui_kit::test]
fn the_repost_link_opens_the_original_story(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-story-photo");
    let vcx = &mut vcx;
    let current = |vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.stories.viewer.current().map(|item| item.story_id)
        })
    };
    assert!(find(vcx, "story-viewer-repost").is_none(), "not a repost");
    app.update(vcx, |app, cx| app.step_story_viewer(1, cx));
    frames(vcx);
    assert_eq!(current(vcx), super::story_demo::STORIES.get(1).copied());
    let link = find(vcx, "story-viewer-repost").expect("the repost link");
    vcx.simulate_click(link.center(), Modifiers::default());
    frames(vcx);
    assert_eq!(current(vcx), super::story_demo::STORIES.first().copied());
    app.read_with(vcx, |app, _| {
        assert_eq!(app.stories.viewer.len(), 1, "the origin opens alone");
        assert!(app.stories.viewer.tray_walk().is_none(), "a single open");
    });
}

#[gpui_kit::test]
fn a_repost_whose_origin_turns_out_gone_opens_the_poster_on_the_first_press(
    cx: &mut TestAppContext,
) {
    use super::story_demo::{CHANNEL, GONE_ORIGIN};
    let (app, mut vcx) = open(cx, "ready-story-photo");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.step_story_viewer(1, cx);
        app.step_story_viewer(1, cx);
    });
    frames(vcx);
    let link = find(vcx, "story-viewer-repost").expect("the repost link");
    vcx.simulate_click(link.center(), Modifiers::default());
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.chat_list.pending_story_open,
            Some(super::PendingStoryOpen {
                chat_id: CHANNEL,
                story_id: GONE_ORIGIN,
                scope: StoryOpenScope::Single,
                if_gone: Some(RepostGone::OpenPeer { chat_id: CHANNEL }),
            }),
            "the press waits for the origin"
        );
        assert!(app.story_playback_paused(), "the repost holds meanwhile");
    });
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stories.missing.insert((CHANNEL, GONE_ORIGIN));
        }
        cx.notify();
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.chat_list.pending_story_open, None);
        assert!(!app.stories.viewer.is_open(), "the viewer closed");
        assert!(app.dialogs.profile_modal.is_some(), "the poster's profile");
    });
}
