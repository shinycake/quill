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
            let shown = app
                .session()
                .and_then(|session| session.open_chat_id())
                .unwrap_or(ChatId(1));
            app.playback.pending_media_seek = Some((shown, MessageId(2), 5, give_up_at));
            app.tick_media_seek(now, cx);
            assert!(app.playback.pending_media_seek.is_some(), "still in time");
            app.tick_media_seek(give_up_at, cx);
            assert!(app.playback.pending_media_seek.is_none(), "given up");
        });
    });
}

#[gpui_kit::test]
fn a_seek_is_dropped_when_its_chat_is_no_longer_shown(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let now = Instant::now();
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.playback.pending_media_seek = Some((
                ChatId(424_242),
                MessageId(2),
                5,
                now + Duration::from_secs(12),
            ));
            app.tick_media_seek(now, cx);
            assert!(app.playback.pending_media_seek.is_none());
        });
    });
}

/// Sweep r3-08 (15-001): the seek waits in the account's own screen state,
/// so another account that shows the same chat id never fires or drops it.
#[gpui_kit::test]
fn a_seek_does_not_follow_an_account_switch(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let shown = app
                .session()
                .and_then(|session| session.open_chat_id())
                .unwrap_or(ChatId(1));
            let give_up_at = Instant::now() + Duration::from_secs(12);
            app.playback.pending_media_seek = Some((shown, MessageId(2), 5, give_up_at));
            app.switch_account(&quill::ids::AccountKey("other".into()), "", window, cx);
            assert!(
                app.playback.pending_media_seek.is_none(),
                "the other account's screen holds no seek of this one"
            );
        });
    });
}
