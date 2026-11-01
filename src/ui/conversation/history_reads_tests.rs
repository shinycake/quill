//! Only rows whose bottom edge is on screen count as read, through the real
//! app, scroller and paint: the list's overdraw rows are not.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const A: ChatId = ChatId(11);

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The ready-chats demo with 120 more messages in Demo chat A, opened.
fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
    vcx.update(|window, _| window.activate_window());
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let seq = AtomicU64::new(session.last_seq);
            for i in 0..120 {
                let json = format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":11,"is_outgoing":false,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Message {i} with enough words to make a row","entities":[]}}}}}}}}"#,
                    2_000 + i,
                    1_790_000_000 + i * 600
                );
                if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                    session.apply(owned);
                }
            }
        }
        cx.notify();
    });
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx)
    });
    frames(&mut vcx);
    (app, vcx)
}

/// The seen ids, and the ids of every painted row split by whether its
/// bottom edge is inside the viewport.
fn snapshot(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> (Vec<MessageId>, Vec<MessageId>, Vec<MessageId>) {
    app.update(vcx, |app, _| {
        let seen = app
            .history
            .seen
            .borrow()
            .as_ref()
            .filter(|(chat, _)| *chat == A)
            .map(|(_, ids)| ids.clone())
            .unwrap_or_default();
        let viewport = app.history.viewport.get().expect("history painted");
        let (mut shown, mut off) = (Vec::new(), Vec::new());
        for (ix, bounds) in app.history.hit_rows.borrow().iter() {
            let ids = app
                .history
                .rows
                .get(*ix)
                .map(|row| row.message_ids())
                .unwrap_or_default();
            if bounds.bottom() >= viewport.top() && bounds.bottom() <= viewport.bottom() {
                shown.extend(ids);
            } else {
                off.extend(ids);
            }
        }
        (seen, shown, off)
    })
}

#[gpui_kit::test]
fn overdraw_rows_below_the_view_are_not_read(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    // Mid-history: the list paints overdraw rows above and below the view.
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item(40, cx));
    });
    frames(vcx);
    let (mut seen, mut shown, off) = snapshot(&app, vcx);
    assert!(!off.is_empty(), "the list paints rows outside the view");
    seen.sort_unstable_by_key(|id| id.0);
    shown.sort_unstable_by_key(|id| id.0);
    shown.dedup();
    assert!(!seen.is_empty());
    assert_eq!(seen, shown);
    assert!(off.iter().all(|id| !seen.contains(id)));
    // The report reads what was seen, nothing more.
    let reported = app.update(vcx, |app, _| {
        app.history.reported_visible.clone().map(|(_, ids)| ids)
    });
    assert_eq!(reported, Some(seen));
}

#[gpui_kit::test]
fn a_row_counts_as_on_screen_as_soon_as_any_of_it_shows(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item(40, cx));
    });
    frames(vcx);
    let (seen, _, cut_off) = snapshot(&app, vcx);
    assert!(
        !cut_off.is_empty(),
        "a row is cut off by the viewport's edge"
    );
    let on_screen = app.update(vcx, |app, _| app.history.on_screen.borrow().clone());
    assert!(seen.iter().all(|id| on_screen.contains(id)));
    assert!(cut_off.iter().any(|id| on_screen.contains(id)));
}
