//! An opened chat lands with the "Unread messages" bar at the top of the
//! view, or at the bottom without a bar when the unread run fits on the
//! last screen, through the real app, scroller and paint.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const A: ChatId = ChatId(11);
const B: ChatId = ChatId(12);

fn frames(vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The ready-chats demo with incoming messages 1000..`last` in chat A, read
/// up to `read`.
fn open(cx: &mut TestAppContext, last: i64, read: i64) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let seq = AtomicU64::new(session.last_seq);
            for id in 1_000..last {
                let json = format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Message {id} with enough words to make a row","entities":[]}}}}}}}}"#,
                    1_790_000_000 + id * 60
                );
                if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                    session.apply(owned);
                }
            }
            let newest = last - 1;
            let json = format!(
                r#"{{"@type":"updateChatLastMessage","chat_id":11,"last_message":{{"id":{newest},"chat_id":11,"is_outgoing":false,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Message {newest}","entities":[]}}}}}},"positions":[]}}"#,
                1_790_000_000 + newest * 60
            );
            if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                session.apply(owned);
            }
            let json = format!(
                r#"{{"@type":"updateChatReadInbox","chat_id":11,"last_read_inbox_message_id":{read},"unread_count":{}}}"#,
                last - 1 - read
            );
            if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                session.apply(owned);
            }
        }
        cx.notify();
    });
    frames(&mut vcx);
    // Open chat A afresh, so it anchors with these messages.
    for chat in [B, A] {
        app.update_in(&mut vcx, |app, window, cx| {
            app.select_listed_chat(chat, window, cx)
        });
        frames(&mut vcx);
    }
    (app, vcx)
}

fn bar_anchor(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<i64> {
    app.update(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .and_then(|s| s.histories.get(&A.0))
            .and_then(|h| h.unread_anchor)
            .map(|id| id.0)
    })
}

#[gpui_kit::test]
fn a_long_unread_run_opens_with_the_bar_at_the_top(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 1_080, 1_040);
    let vcx = &mut vcx;
    assert_eq!(bar_anchor(&app, vcx), Some(1_040));
    let bar_top = vcx.debug_bounds("unread-bar").expect("bar painted").top();
    let viewport = app.update(vcx, |app, _| app.history.viewport.get());
    let viewport = viewport.expect("history painted");
    assert!(
        (bar_top - viewport.top()).abs() < px(0.5),
        "bar at {bar_top:?}, view at {:?}",
        viewport.top()
    );
    assert_eq!(app.update(vcx, |app, _| app.history.bar_anchor.get()), None);
}

#[gpui_kit::test]
fn an_unread_run_that_fits_opens_at_the_bottom_without_a_bar(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 1_060, 1_057);
    let vcx = &mut vcx;
    assert_eq!(bar_anchor(&app, vcx), None);
    let (divider, following, held) = app.update(vcx, |app, cx| {
        (
            app.history.rows.iter().any(|row| row.unread_divider()),
            app.history.scroller.read(cx).is_following_tail(),
            app.demo_session
                .as_ref()
                .and_then(|s| s.histories.get(&A.0))
                .is_some_and(|h| h.opened_with_unread),
        )
    });
    assert!(!divider);
    assert!(following);
    assert!(held, "reads are still held back until the rows are seen");
}
