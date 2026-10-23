//! Screenshot demo of the Calls tab with a page of call history and more on
//! the server: the spinner row at the end, where a Load more button used to
//! be. Injected, no live Telegram, so no next page arrives.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-calls-history",
        "screenshot demo — call history loading more"
    )
    .setup(QuillApp::demo_calls_history),
];

impl QuillApp {
    fn demo_calls_history(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let extra = session.request(RequestPurpose::SearchCallMessages, None);
        let calls: Vec<String> = (0..14i64)
            .map(|n| {
                let reason = if n % 3 == 1 {
                    "callDiscardReasonMissed"
                } else {
                    "callDiscardReasonHungUp"
                };
                format!(
                    r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":{out},"date":{date},"content":{{"@type":"messageCall","unique_id":{id},"is_video":{video},"discard_reason":{{"@type":"{reason}"}},"duration":{dur}}}}}"#,
                    id = 900 + n,
                    chat = 11 + n % 2,
                    out = n % 2 == 0,
                    date = 1_760_000_000 - n * 40_000,
                    video = n % 4 == 0,
                    dur = if n % 3 == 1 { 0 } else { 60 + n * 37 },
                )
            })
            .collect();
        let json = format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":40,"next_offset":"page2","messages":[{}]}}"#,
            extra.0,
            calls.join(","),
        );
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
            session.apply(owned);
        }
        self.chat_list.contacts_tab_open = false;
        self.chat_list.calls_tab_open = true;
        cx.notify();
    }
}
