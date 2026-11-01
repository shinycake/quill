#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Leaving a chat mid-history and coming back lands on the same row, through
//! the real app, scroller and render loop (`docs/decisions/sweep-scroll-restore.md`).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase,
    VisualTestContext, px, size,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::SavedScroll;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

pub(in crate::ui) const A: ChatId = ChatId(11);
const B: ChatId = ChatId(12);

pub(in crate::ui) fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The ready-chats demo with `messages` more messages in Demo chat A.
pub(in crate::ui) fn open(
    cx: &mut TestAppContext,
    messages: usize,
) -> (Entity<QuillApp>, VisualTestContext) {
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
            for i in 0..messages {
                let json = format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":11,"is_outgoing":{},"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Message {i} with enough words to make a row","entities":[]}}}}}}}}"#,
                    2_000 + i,
                    i % 2 == 0,
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
    (app, vcx)
}

pub(in crate::ui) fn select(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: ChatId) {
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(chat, window, cx)
    });
    frames(vcx);
}

/// The top row's message and offset as the scroller reports them.
fn top(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(MessageId, f32)> {
    app.update(vcx, |app, cx| {
        let (ix, offset) = app.history.scroller.read(cx).top_anchor()?;
        Some((app.history.rows.get(ix)?.first_id()?, offset))
    })
}

fn saved(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: ChatId) -> Option<SavedScroll> {
    app.update(vcx, |app, _| {
        app.history.scroll_memory.get((chat.0, None, 0))
    })
}

#[gpui_kit::test]
fn returning_to_a_chat_lands_on_the_row_it_was_left_at(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 120);
    let vcx = &mut vcx;
    select(&app, vcx, A);
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item_offset(40, 12., cx));
    });
    frames(vcx);
    let left = top(&app, vcx).expect("scrolled away from the tail");
    assert_eq!(left.1, 12.);
    assert_eq!(
        saved(&app, vcx, A),
        Some(SavedScroll {
            anchor: left.0,
            offset: 12.
        })
    );
    // Two round trips: the first restore must not drift or overwrite it.
    for _ in 0..2 {
        select(&app, vcx, B);
        assert_eq!(saved(&app, vcx, A).map(|s| s.anchor), Some(left.0));
        select(&app, vcx, A);
        assert_eq!(top(&app, vcx), Some(left));
    }
}

#[gpui_kit::test]
fn an_explicit_target_wins_over_the_saved_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 120);
    let vcx = &mut vcx;
    select(&app, vcx, A);
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item_offset(40, 12., cx));
    });
    frames(vcx);
    let left = top(&app, vcx).expect("scrolled away from the tail");
    select(&app, vcx, B);
    // "Go To Message" on a peeked chat: the message, not the saved row.
    let target = MessageId(2_100);
    app.update_in(vcx, |app, window, cx| {
        app.go_to_message_in(A, target, window, cx)
    });
    frames(vcx);
    assert_ne!(top(&app, vcx).map(|t| t.0), Some(left.0));
    let target_row = app.update(vcx, |app, _| {
        app.history.rows.iter().position(|row| row.contains(target))
    });
    let shown = app.update(vcx, |app, _| app.history.scroll_view_probe.get());
    assert!(
        target_row
            .is_some_and(|row| shown.is_some_and(|(first, last)| first <= row && row <= last)),
        "target row {target_row:?} not in {shown:?}"
    );
    assert_ne!(saved(&app, vcx, A).map(|s| s.anchor), Some(left.0));
}

#[gpui_kit::test]
fn a_saved_row_that_is_gone_opens_the_default_and_is_forgotten(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 120);
    let vcx = &mut vcx;
    select(&app, vcx, B);
    app.update(vcx, |app, _| {
        app.history.scroll_memory.record(
            (A.0, None, 0),
            Some(SavedScroll {
                anchor: MessageId(999_999),
                offset: 4.,
            }),
        );
    });
    select(&app, vcx, A);
    // The default open: a little above the "Unread messages" divider. What
    // is saved now is that row, as for any position away from the tail.
    let divider = app.update(vcx, |app, _| {
        app.history
            .rows
            .iter()
            .position(|row| row.unread_divider())
            .and_then(|ix| app.history.rows.get(ix.saturating_sub(1))?.first_id())
    });
    assert!(divider.is_some());
    assert_eq!(top(&app, vcx).map(|t| t.0), divider);
    assert_ne!(
        saved(&app, vcx, A).map(|s| s.anchor),
        Some(MessageId(999_999))
    );
}

fn swipe(
    vcx: &mut VisualTestContext,
    at: gpui_kit::Point<gpui_kit::Pixels>,
    dy: f32,
    moves: usize,
) {
    let mut send = |phase, dy| {
        vcx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(gpui_kit::point(px(0.), px(dy))),
            modifiers: Modifiers::default(),
            touch_phase: phase,
            source: gpui_kit::ScrollSource::Pointer,
        });
    };
    send(TouchPhase::Started, dy);
    for _ in 0..moves {
        send(TouchPhase::Moved, dy);
    }
    send(TouchPhase::Ended, 0.);
}

/// A trackpad swipe down to the newest message reaches the end, even
/// through the kit's scroll bounce: the list follows its tail again and the
/// jump-to-latest button goes away (`docs/decisions/gpui-kit-0-8-0.md`).
#[gpui_kit::test]
fn swiping_back_to_the_newest_message_resumes_following(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 40);
    select(&app, &mut vcx, A);
    let at = app.read_with(&vcx, |app, _| {
        app.history
            .viewport
            .get()
            .expect("history painted")
            .center()
    });
    swipe(&mut vcx, at, 300., 4);
    frames(&mut vcx);
    let scrolled_up = app.read_with(&vcx, |app, cx| {
        app.history.scroller.read(cx).is_scrolled_up()
    });
    assert!(scrolled_up, "the swipe up left the newest message");

    swipe(&mut vcx, at, -300., 12);
    for _ in 0..30 {
        frames(&mut vcx);
    }
    let (scrolled_up, following) = app.read_with(&vcx, |app, cx| {
        let scroller = app.history.scroller.read(cx);
        (scroller.is_scrolled_up(), scroller.is_following_tail())
    });
    assert!(
        !scrolled_up,
        "the jump-to-latest button must go away at the end"
    );
    assert!(following, "the list follows new messages again");
}
