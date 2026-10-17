//! Opening stories: one the server no longer has drops the open and toasts
//! that the link is broken (tdesktop `lng_stories_link_invalid`), and a
//! poster's stories that arrive after the viewer opened join its list.
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
use quill::story_viewer::StoryViewerItem;

const POSTER: i64 = 13;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_app(cx: &mut TestAppContext, kind: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
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

#[gpui_kit::test]
fn a_story_the_server_no_longer_has_shows_the_broken_link_toast(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-channels");
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
    let (app, mut vcx) = open_app(cx, "ready-channels");
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

/// Maya's stories in the `ready-story-viewer` demo.
const MAYA: i64 = 52;

#[gpui_kit::test]
fn a_story_that_arrives_after_the_viewer_opened_joins_the_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let late = app.update(vcx, |app, cx| {
        app.close_story_viewer(cx);
        let late = app
            .demo_session
            .as_mut()
            .and_then(|session| session.stories.stories.remove(&(MAYA, 6)));
        app.open_story_viewer(ChatId(MAYA), 5, cx);
        late
    });
    frames(vcx);
    let shown = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            let viewer = &app.stories.viewer;
            (
                viewer.current().map(StoryViewerItem::key),
                viewer.position(),
            )
        })
    };
    assert_eq!(shown(&app, vcx), (Some((ChatId(MAYA), 5)), Some((1, 1))));
    app.update(vcx, |app, cx| {
        if let (Some(session), Some(late)) = (app.demo_session.as_mut(), late) {
            session.stories.stories.insert((MAYA, 6), late);
        }
        cx.notify();
    });
    frames(vcx);
    assert_eq!(
        shown(&app, vcx),
        (Some((ChatId(MAYA), 5)), Some((1, 2))),
        "the late story joins behind the current one"
    );
    app.update(vcx, |app, cx| app.step_story_viewer(1, cx));
    frames(vcx);
    assert_eq!(shown(&app, vcx), (Some((ChatId(MAYA), 6)), Some((2, 2))));
}
