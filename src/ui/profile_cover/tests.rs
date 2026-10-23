//! The cover's story ring opens the stories, the rating pill and the
//! "when?" button open their boxes, and your own profile is Saved Messages.

use crate::ui::app::QuillApp;
use crate::ui::demos::profile_cover::{COVER_HIDDEN, COVER_STORIES};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::lang::{keys, tr};

fn new_app(
    cx: &mut TestAppContext,
    demo: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, shell, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.debug_bounds(selector).is_some()
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx.debug_bounds(selector).expect(selector);
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

#[gpui_kit::test]
fn the_ringed_avatar_opens_the_first_unread_story(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-info-cover-stories");
    assert!(find(&mut vcx, "profile-rating"));
    click(&mut vcx, "profile-cover-avatar");
    let opened = app.read_with(&vcx, |app, _| {
        app.chat_list
            .pending_story_open
            .map(|open| (open.chat_id, open.story_id))
            .or_else(|| app.stories.viewer.is_open().then_some((COVER_STORIES, 0)))
    });
    assert_eq!(opened.map(|(chat, _)| chat), Some(COVER_STORIES));
    if let Some((_, story)) = opened.filter(|(_, story)| *story != 0) {
        assert_eq!(story, 2, "the first story after max_read_story_id");
    }
}

#[gpui_kit::test]
fn the_rating_pill_opens_the_rating_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-info-cover-stories");
    click(&mut vcx, "profile-rating");
    assert_eq!(dialog(&shell, &mut vcx), Some(DialogKind::StarsRating));
    click(&mut vcx, "rating-box-understood");
    assert_eq!(
        app.read_with(&vcx, |app, _| app.profile_cover.rating_box),
        None
    );
    assert_ne!(dialog(&shell, &mut vcx), Some(DialogKind::StarsRating));
}

#[gpui_kit::test]
fn hidden_last_seen_offers_to_show_yours(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-info-cover-last-seen");
    assert!(!find(&mut vcx, "profile-rating"));
    click(&mut vcx, "profile-cover-when");
    assert_eq!(
        app.read_with(&vcx, |app, _| app.profile_cover.last_seen_box),
        Some(COVER_HIDDEN)
    );
    assert_eq!(dialog(&shell, &mut vcx), Some(DialogKind::ShowLastSeen));
    click(&mut vcx, "last-seen-show");
    let note = app.read_with(&vcx, |app, _| app.connection.note().text().to_string());
    assert_eq!(note, tr(keys::LASTSEEN_SHOWN_TOAST));
    assert_ne!(dialog(&shell, &mut vcx), Some(DialogKind::ShowLastSeen));
}

#[gpui_kit::test]
fn your_own_profile_is_saved_messages(cx: &mut TestAppContext) {
    let (_app, _shell, mut vcx) = new_app(cx, "ready-info-cover-saved");
    assert!(find(&mut vcx, "saved-messages-cover"));
    assert!(!find(&mut vcx, "profile-cover-avatar"));
    assert!(!find(&mut vcx, "profile-rating"));
    assert!(!find(&mut vcx, "profile-cover-when"));
}
