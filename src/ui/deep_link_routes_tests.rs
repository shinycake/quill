//! A linked media timestamp waits for its message until a deadline, however
//! rarely the poll loop runs (needs the `demo-capture` feature for gpui-kit's
//! test support).
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use std::time::{Duration, Instant};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

#[gpui_kit::test]
fn a_seek_waits_for_its_message_until_its_deadline(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let now = Instant::now();
    let give_up_at = now + Duration::from_secs(12);
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.playback.pending_media_seek = Some((ChatId(1), MessageId(2), 5, give_up_at));
            app.tick_media_seek(now, cx);
            assert!(app.playback.pending_media_seek.is_some(), "still in time");
            app.tick_media_seek(give_up_at, cx);
            assert!(app.playback.pending_media_seek.is_none(), "given up");
        });
    });
}
