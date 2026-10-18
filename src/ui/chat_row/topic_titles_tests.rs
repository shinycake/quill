#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! A forum row's topic titles through GPUI: they list the latest topics,
//! and a click opens that topic without the row's own click firing.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    // Another test on this thread may have left a zoom behind.
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-forum-row-topics");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    // Saved settings of another test may ask for three-line rows, whose
    // topic line is plain text.
    app.update(&mut vcx, |app, cx| {
        app.appearance.preview_lines = 2;
        cx.notify();
    });
    frames(&mut vcx);
    (app, vcx)
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| window.click(id, cx));
    frames(vcx);
}

fn opened(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (Option<ChatId>, Option<i32>) {
    app.update(vcx, |app, _| {
        app.session()
            .map(|s| (s.open_chat, s.open_topic))
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn the_row_lists_the_latest_topics_with_unread_ones_marked(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let topics = app.update(&mut vcx, |app, _| {
        app.session()
            .map(|s| s.chat_row_topics(ChatId(71)))
            .unwrap_or_default()
    });
    let shown: Vec<(&str, bool)> = topics.iter().map(|t| (t.name.as_str(), t.unread)).collect();
    assert_eq!(
        shown,
        [
            ("Off topic", true),
            ("Release planning", false),
            ("Backend", true),
            ("Design", false),
            ("Announcements", false),
        ]
    );
    assert!(vcx.update(|window, _| window.try_find("row-topic-71-6").is_some()));
}

#[gpui_kit::test]
fn a_title_opens_its_topic_and_not_just_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    // The newest titles are the ones in view; later ones clip at the edge.
    click(vcx, "row-topic-71-6");
    assert_eq!(opened(&app, vcx), (Some(ChatId(71)), Some(6)));
    click(vcx, "row-topic-72-4");
    assert_eq!(opened(&app, vcx), (Some(ChatId(72)), Some(4)));
}
