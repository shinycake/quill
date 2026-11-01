//! The ready-chats demo opened through the real app and window, shared by
//! the history UI tests. Over the demo; nothing reaches Telegram.

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

pub(in crate::ui) const A: ChatId = ChatId(11);

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
