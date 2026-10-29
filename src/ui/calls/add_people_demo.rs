//! Screenshot demo of "Add People" over a 1:1 voice call (injected, no
//! live Telegram): the call window opens with the box over it, every
//! contact listed by last seen and Maya, the other side, checked and
//! disabled. `QUILL_DEMO_QUERY=<text>` types a search: the contacts are
//! filtered by name and two people outside your contacts come from the
//! "server". The call window opens beside the main window; capture it
//! with `QUILL_DEMO_CAPTURE_WINDOWS=1`.
#![deny(clippy::indexing_slicing)]

use super::call_video_demo::{PEER, seed_video_call};
use crate::ui::app::QuillApp;
use crate::ui::call_add_people::AddPeopleBox;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::engine::RemoteVideoState;
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, RequestId};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-call-add-people",
        "screenshot demo — Add People over a 1:1 call, with its search"
    )
    .setup(QuillApp::demo_call_add_people),
];

/// Contacts: id, first and last name, and status.
const CONTACTS: &[(i64, &str, &str, &str)] = &[
    (
        4510,
        "Daniel",
        "Katz",
        r#"{"@type":"userStatusOnline","expires":4102444800}"#,
    ),
    (4511, "Yael", "Dagan", r#"{"@type":"userStatusRecently"}"#),
    (
        4512,
        "Omer",
        "Ben David",
        r#"{"@type":"userStatusRecently"}"#,
    ),
    (4513, "Dana", "Levi", r#"{"@type":"userStatusLastWeek"}"#),
    (4514, "Tamar", "Golan", r#"{"@type":"userStatusLastWeek"}"#),
    (
        4515,
        "Lior",
        "Avraham",
        r#"{"@type":"userStatusLastMonth"}"#,
    ),
    (
        4516,
        "Noam",
        "Shapiro",
        r#"{"@type":"userStatusLastMonth"}"#,
    ),
];

/// People only the server search finds.
const GLOBAL: &[(i64, &str, &str)] = &[(4520, "Dafna", "Rosen"), (4521, "Dalia", "Mizrahi")];

impl QuillApp {
    fn demo_call_add_people(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let query = std::env::var("QUILL_DEMO_QUERY").unwrap_or_default();
        self.demo_high_contrast(cx);
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            seed_video_call(session, false, &sink, &self.demo_ui.seq);
            if let Some(call) = session.calls.active_call.as_mut() {
                call.is_video = false;
                call.camera_on = false;
                call.remote_video = RemoteVideoState::Inactive;
            }
            seed_people(session, &query, &sink, &self.demo_ui.seq);
        }
        self.calls.add_people = Some(AddPeopleBox::with_query(query));
        cx.notify();
    }
}

fn user_json(id: i64, first: &str, last: &str, status: &str, contact: bool) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{status},"profile_photo":null,"is_contact":{contact},"is_mutual_contact":{contact},"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
    )
}

/// The contacts, and the "server's" answer to `query`.
fn seed_people(
    session: &mut Session,
    query: &str,
    sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
) {
    let mut updates: Vec<String> = CONTACTS
        .iter()
        .map(|(id, first, last, status)| user_json(*id, first, last, status, true))
        .collect();
    for (id, first, last) in GLOBAL {
        updates.push(user_json(
            *id,
            first,
            last,
            r#"{"@type":"userStatusRecently"}"#,
            false,
        ));
        updates.push(format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{first} {last}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
        ));
    }
    for update in updates {
        if let Some(owned) = copy_and_parse(&update, seq, sink) {
            session.apply(owned);
        }
    }
    let mut contacts = vec![PEER];
    contacts.extend(CONTACTS.iter().map(|(id, ..)| *id));
    session.users_state.contacts = Some(contacts);
    if !query.trim().is_empty() {
        let search = &mut session.calls.add_people_search;
        search.begin(query, RequestId(1), RequestId(2));
        search.accept(RequestId(1), &[]);
        let found: Vec<ChatId> = GLOBAL.iter().map(|(id, ..)| ChatId(*id)).collect();
        search.accept(RequestId(2), &found);
    }
}
