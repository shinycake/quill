//! Opening a story the server no longer has: the open is dropped and the
//! toast says the link is broken (tdesktop `lng_stories_link_invalid`).
//! Nothing reaches Telegram: the demo app has no connection.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::lang::{keys, tr};

const POSTER: i64 = 13;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-channels")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

#[gpui_kit::test]
fn a_story_the_server_no_longer_has_shows_the_broken_link_toast(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.open_story_viewer(ChatId(POSTER), 99, cx));
    frames(vcx);
    let waiting = app.read_with(vcx, |app, _| app.chat_list.pending_story_open);
    assert_eq!(waiting, Some((POSTER, 99)), "the open waits for getStory");
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stories.missing.insert((POSTER, 99));
        }
        cx.notify();
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.chat_list.pending_story_open, None,
            "the open is dropped"
        );
        assert!(!app.stories.viewer.is_open());
        let note = app.connection.note();
        assert_eq!(note.text(), tr(keys::STORIES_LINK_INVALID));
        assert!(note.is_toast(), "the note floats as a toast");
    });
}

#[gpui_kit::test]
fn a_story_known_to_be_gone_toasts_on_the_next_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stories.missing.insert((POSTER, 98));
        }
        app.open_story_viewer(ChatId(POSTER), 98, cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.chat_list.pending_story_open, None);
        assert_eq!(app.connection.note().text(), tr(keys::STORIES_LINK_INVALID));
    });
}
