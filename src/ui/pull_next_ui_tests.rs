//! The pull to the next unread channel and the wide chat layout on a demo
//! `QuillApp`. Nothing is sent: the demo session has no sender.

use crate::ui::app::QuillApp;
use crate::ui::history_row::HistoryRow;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, Pixels, Point, ScrollDelta, ScrollEvent, TestAppContext,
    TouchPhase, VisualTestContext, point, px, size,
};
use quill::chat_wide::ChatLayout;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn new_app(cx: &mut TestAppContext, width: f32) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(width), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

/// The demo's channel (13) open, and a second channel (14) with unread
/// posts below it in the chat list.
fn open_channel_with_a_next_one(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let seq = AtomicU64::new(9_000);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        for json in [
            r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Next channel","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":true},"unread_count":3}}"#,
            r#"{"@type":"updateChatPosition","chat_id":14,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":13,"is_outgoing":false,"is_channel_post":true,"date":1790631720,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"First post.","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":401,"chat_id":14,"is_outgoing":false,"is_channel_post":true,"date":1790631900,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Unread post.","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":13,"is_outgoing":false,"is_channel_post":true,"date":1790631840,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Second post.","entities":[]}}}}"#,
        ] {
            if let Some(owned) = quill::telegram::client::copy_and_parse(json, &seq, &sink) {
                session.apply(owned);
            }
        }
        session.open_chat(ChatId(13));
        cx.notify();
    });
    frames(cx);
}

fn history_centre(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Point<Pixels> {
    app.read_with(cx, |app, _| {
        app.history
            .viewport
            .get()
            .expect("history painted")
            .center()
    })
}

fn wheel(cx: &mut VisualTestContext, at: Point<Pixels>, phase: TouchPhase, down: f32) {
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(-down))),
        modifiers: Modifiers::default(),
        touch_phase: phase,
        source: gpui_kit::ScrollSource::Pointer,
    });
}

/// A finger-down pull of `steps` × 25 px; no frame is drawn, so the
/// dwell has not fired yet.
fn pull(cx: &mut VisualTestContext, at: Point<Pixels>, steps: usize) {
    wheel(cx, at, TouchPhase::Started, 0.);
    for _ in 0..steps {
        wheel(cx, at, TouchPhase::Moved, 25.);
    }
}

#[gpui_kit::test]
fn pulling_past_the_bottom_of_a_channel_opens_the_next_unread_one(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    frames(vcx);
    open_channel_with_a_next_one(&app, vcx);
    let at = history_centre(&app, vcx);
    pull(vcx, at, 12);
    app.read_with(vcx, |app, _| {
        let pull = &app.history.pull;
        assert!(pull.machine_for_tests().pulling());
        assert!(pull.machine_for_tests().overscroll() >= quill::pull_next::PULL_THRESHOLD);
        assert_eq!(pull.next_chat_for_tests(), Some(ChatId(14)));
    });
    // Held past the threshold for the dwell, the pull is ready.
    std::thread::sleep(std::time::Duration::from_millis(
        quill::pull_next::READY_DWELL_MS + 30,
    ));
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(app.read_with(vcx, |app, _| app.history.pull.machine_for_tests().reached()));
    // Letting go opens the next channel.
    wheel(vcx, at, TouchPhase::Ended, 0.);
    vcx.run_until_parked();
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.open_chat_id()),
        Some(ChatId(14))
    );
    // The new chat starts with no pull.
    app.read_with(vcx, |app, _| {
        assert!(!app.history.pull.machine_for_tests().pulling());
        assert!(!app.history.pull.machine_for_tests().jumping());
    });
}

#[gpui_kit::test]
fn a_quick_flick_or_a_chat_springs_back_without_opening_anything(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    frames(vcx);
    open_channel_with_a_next_one(&app, vcx);
    let at = history_centre(&app, vcx);
    // A long dwell keeps a loaded machine from reaching "ready" mid-pull.
    quill::pull_next::set_ready_dwell_for_tests(Some(60_000));
    // Released before the dwell: nothing opens.
    pull(vcx, at, 12);
    wheel(vcx, at, TouchPhase::Ended, 0.);
    vcx.run_until_parked();
    assert_eq!(
        app.read_with(vcx, |app, _| app.open_chat_id()),
        Some(ChatId(13))
    );
    // A private chat never pulls.
    app.update(vcx, |app, cx| {
        app.demo_session.as_mut().unwrap().open_chat(ChatId(11));
        cx.notify();
    });
    frames(vcx);
    pull(vcx, at, 12);
    assert!(!app.read_with(vcx, |app, _| app.history.pull.machine_for_tests().pulling()));
}

#[gpui_kit::test]
fn the_setting_turns_the_pull_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    frames(vcx);
    app.update(vcx, |app, cx| {
        app.set_media_pref(|prefs| prefs.pull_to_next_channel = false, cx);
    });
    open_channel_with_a_next_one(&app, vcx);
    let at = history_centre(&app, vcx);
    pull(vcx, at, 12);
    assert!(!app.read_with(vcx, |app, _| app.history.pull.machine_for_tests().pulling()));
}

#[gpui_kit::test]
fn the_wide_layout_puts_every_message_on_the_left_with_photos(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, 1600.);
    let vcx = &mut vcx;
    frames(vcx);
    // Wide enough, but off by default: the look is unchanged.
    app.read_with(vcx, |app, _| {
        assert_eq!(app.history.chat_layout.get(), ChatLayout::Wide);
        assert!(!app.chat_wide());
    });
    let outgoing_photo = |app: &QuillApp| {
        app.history.rows.iter().any(|row| match row {
            HistoryRow::Single(inputs) => {
                inputs.message.is_outgoing && inputs.chrome.sender_avatar.is_some()
            }
            _ => false,
        })
    };
    assert!(!app.read_with(vcx, |app, _| outgoing_photo(app)));
    // The Appearance checkbox shows while the chat is wide.
    app.update(vcx, |app, cx| {
        assert!(app.adaptive_wide_checkbox(cx).is_some());
        app.appearance.adaptive_for_wide = true;
        cx.notify();
    });
    frames(vcx);
    app.read_with(vcx, |app, cx| {
        assert!(app.chat_wide());
        assert!(app.bubble_look(cx).wide);
        assert!(outgoing_photo(app), "own messages get a photo too");
    });
}

#[gpui_kit::test]
fn a_narrow_chat_keeps_the_normal_layout(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    frames(vcx);
    app.update(vcx, |app, cx| {
        app.appearance.adaptive_for_wide = true;
        cx.notify();
    });
    frames(vcx);
    app.update(vcx, |app, cx| {
        assert_eq!(app.history.chat_layout.get(), ChatLayout::Normal);
        assert!(!app.chat_wide());
        // tdesktop hides the checkbox while the chat is narrow.
        assert!(app.adaptive_wide_checkbox(cx).is_none());
    });
}
