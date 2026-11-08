//! `QUILL_DEMO_SHOWCASE=calls` and `=contacts`: the showcase account's
//! Calls and Contacts tabs, filled with the showcase people so the rows
//! show real names, photos and statuses.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use quill::diagnostics::DiagnosticSink;
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Private showcase chats (the chat id is the user id).
const LEO: i64 = 105;
const SAM: i64 = 106;
const CHLOE: i64 = 107;

impl QuillApp {
    pub(super) fn demo_showcase_calls(&mut self) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let extra = session.request(RequestPurpose::SearchCallMessages, None);
        // Recent calls only take the page they wait for.
        session.calls.recent_calls_request = Some(extra);
        let now = quill::local_time::now_unix();
        // (chat, outgoing, missed, video, minutes ago, seconds long)
        let calls: [(i64, bool, bool, bool, i64, i64); 9] = [
            (SAM, false, true, false, 12, 0),
            (SAM, false, true, false, 14, 0),
            (LEO, true, false, true, 95, 754),
            (CHLOE, false, false, false, 210, 128),
            (CHLOE, true, false, false, 26 * 60, 47),
            (CHLOE, true, false, false, 26 * 60 + 5, 0),
            (CHLOE, true, false, false, 26 * 60 + 9, 0),
            (SAM, true, false, false, 50 * 60, 1_865),
            (LEO, false, true, true, 75 * 60, 0),
        ];
        let messages: Vec<String> = calls
            .iter()
            .enumerate()
            .map(|(n, (chat, out, missed, video, ago, len))| {
                let reason = if *missed {
                    "callDiscardReasonMissed"
                } else {
                    "callDiscardReasonHungUp"
                };
                format!(
                    r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":{out},"date":{date},"content":{{"@type":"messageCall","unique_id":{id},"is_video":{video},"discard_reason":{{"@type":"{reason}"}},"duration":{len}}}}}"#,
                    id = 7_000 + n,
                    date = now - ago * 60,
                )
            })
            .collect();
        let json = format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":{},"next_offset":"","messages":[{}]}}"#,
            extra.0,
            messages.len(),
            messages.join(","),
        );
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
            session.apply(owned);
        }
        self.chat_list.contacts_tab_open = false;
        self.chat_list.calls_tab_open = true;
    }

    pub(super) fn demo_showcase_contacts(&mut self) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let now = quill::local_time::now_unix();
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        // A mix of last-seen times, as a real list has.
        for (user, minutes) in [(101, 3), (103, 40), (104, 5 * 60), (106, 26 * 60), (108, 9)] {
            let json = format!(
                r#"{{"@type":"updateUserStatus","user_id":{user},"status":{{"@type":"userStatusOffline","was_online":{}}}}}"#,
                now - minutes * 60
            );
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        session.users_state.contacts = Some((101..=112).collect());
        self.chat_list.calls_tab_open = false;
        self.chat_list.contacts_tab_open = true;
    }
}
