//! A linked media timestamp waits for its message until a deadline its stage
//! owns, however rarely the poll loop runs (needs the `demo-capture` feature for gpui-kit's
//! test support).
use super::{MediaSeek, SEEK_WAIT};
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};

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
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let shown = app
                .session()
                .and_then(|session| session.open_chat_id())
                .unwrap_or(ChatId(1));
            app.begin_media_seek(shown, MessageId(2), 5, cx);
            app.tick_media_seek(cx);
            assert!(
                matches!(app.playback.media_seek, MediaSeek::Seeking { .. }),
                "still in time"
            );
        });
    });
    vcx.executor().advance_clock(SEEK_WAIT);
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        app.update(cx, |app, _| {
            assert!(
                matches!(app.playback.media_seek, MediaSeek::Idle),
                "given up, with no poll running"
            );
        });
    });
}

#[gpui_kit::test]
fn a_seek_is_dropped_when_its_chat_is_no_longer_shown(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.begin_media_seek(ChatId(424_242), MessageId(2), 5, cx);
            app.tick_media_seek(cx);
            assert!(matches!(app.playback.media_seek, MediaSeek::Idle));
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
            app.begin_media_seek(shown, MessageId(2), 5, cx);
            app.switch_account(&quill::ids::AccountKey("other".into()), "", window, cx);
            assert!(
                matches!(app.playback.media_seek, MediaSeek::Idle),
                "the other account's screen holds no seek of this one"
            );
        });
    });
}
