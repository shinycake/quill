//! A channel's sponsored message scrolls with its history (#1427): the
//! reader can scroll up from it, and reaching the bottom shows it without
//! moving the view.

use crate::ui::app::QuillApp;
use crate::ui::history::demo_chat::frames;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext, Entity, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase,
    VisualTestContext, px, size,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The sponsored demo channel with 40 more posts, so its history scrolls.
fn open_channel(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-sponsored");
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
            for i in 0..40 {
                let json = format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":13,"sender_id":{{"@type":"messageSenderChat","chat_id":13}},"is_outgoing":false,"is_channel_post":true,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Post {i} with enough words to make a row","entities":[]}}}}}}}}"#,
                    100 + i,
                    1_690_000_000 + i * 600
                );
                if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                    session.apply(owned);
                }
            }
        }
        cx.notify();
    });
    for _ in 0..5 {
        frames(&mut vcx);
    }
    (app, vcx)
}

fn wheel(vcx: &mut VisualTestContext, at: gpui_kit::Point<gpui_kit::Pixels>, dy: f32) {
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(gpui_kit::point(px(0.), px(dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: gpui_kit::ScrollSource::Pointer,
    });
}

fn scrolled_up(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, cx| {
        app.history.scroller.read(cx).is_scrolled_up()
    })
}

/// Scrolling up a little from the ad stays where the reader put it; before
/// #1427 the ad left the layout, the list grew into its space, landed at
/// the end again and the ad pulled the view back down.
#[gpui_kit::test]
fn scrolling_up_from_the_ad_stays_up(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_channel(cx);
    let vcx = &mut vcx;
    let shown = app.read_with(vcx, |app, _| app.history.sponsored_tail.is_some());
    assert!(shown, "the channel ends with its ad");
    let footer = vcx.debug_bounds("sponsored-footer").expect("ad painted");
    let viewport = app
        .read_with(vcx, |app, _| app.history.viewport.get())
        .expect("history painted");
    assert!(
        footer.bottom() <= viewport.bottom() + px(0.5),
        "the bottom of the history shows the whole ad"
    );
    assert!(!scrolled_up(&app, vcx));

    for step in 1..=6 {
        wheel(vcx, viewport.center(), 30.);
        for _ in 0..4 {
            frames(vcx);
        }
        assert!(
            scrolled_up(&app, vcx),
            "wheel step {step} up from the ad was pulled back to the bottom"
        );
    }
}
