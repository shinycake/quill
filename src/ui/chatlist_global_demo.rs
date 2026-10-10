//! Screenshot fixtures of the "chatlist-global" work: the contacts index
//! bar, the Clear calls box, the stories menu, the main menu, the
//! suggestions block and the search tabs (injected through the reducer,
//! no live Telegram).

use super::ScreenshotDemo as D;
use super::app::QuillApp;
use super::calls::apply_ready_calls_settings;
use gpui_kit::{Context, Window};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Thirty English-named contacts over fifteen letters, with a spread of
/// last-seen statuses (ids 201..).
const CONTACT_NAMES: [(&str, &str); 30] = [
    ("Adam", "Reed"),
    ("Ana", "Ruiz"),
    ("Bianca", "Rossi"),
    ("Ben", "Carter"),
    ("Chloe", "Zhang"),
    ("Carlos", "Mendez"),
    ("Daniel", "Okafor"),
    ("Dina", "Haddad"),
    ("Elena", "Petrova"),
    ("Ethan", "Wright"),
    ("Farah", "Aziz"),
    ("Grace", "Hopper"),
    ("George", "Boole"),
    ("Hana", "Kimura"),
    ("Ivan", "Novak"),
    ("Isla", "Murray"),
    ("Jonas", "Berg"),
    ("Julia", "Stein"),
    ("Kai", "Nakamura"),
    ("Leo", "Park"),
    ("Lena", "Fischer"),
    ("Maya", "Chen"),
    ("Mateo", "Silva"),
    ("Noor", "Haddad"),
    ("Oscar", "Lindqvist"),
    ("Priya", "Nair"),
    ("Quinn", "Foster"),
    ("Rosa", "Ibarra"),
    ("Sam", "Whitaker"),
    ("Tara", "Singh"),
];

/// Thirty contacts through `updateUser` and a `users` answer to
/// `getContacts`.
fn apply_contacts_fixture(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now: i64 = 1_790_000_000;
    let mut ids = Vec::new();
    for (i, (first, last)) in CONTACT_NAMES.iter().enumerate() {
        let id = 201 + i as i64;
        ids.push(id.to_string());
        let status = match i % 5 {
            0 => r#"{"@type":"userStatusOnline","expires":9999999999}"#.to_string(),
            1 => format!(
                r#"{{"@type":"userStatusOffline","was_online":{}}}"#,
                now - 600 * (i as i64 + 1)
            ),
            2 => r#"{"@type":"userStatusRecently","by_my_privacy_settings":false}"#.to_string(),
            3 => r#"{"@type":"userStatusLastWeek","by_my_privacy_settings":false}"#.to_string(),
            _ => r#"{"@type":"userStatusLastMonth","by_my_privacy_settings":false}"#.to_string(),
        };
        let json = format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","usernames":{{"@type":"usernames","active_usernames":[],"disabled_usernames":[],"editable_username":"","collectible_usernames":[]}},"phone_number":"","status":{status},"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let extra = session.request(RequestPurpose::GetContacts, None);
    let json = format!(
        r#"{{"@type":"users","@extra":"{}","total_count":{},"user_ids":[{}]}}"#,
        extra.0,
        ids.len(),
        ids.join(",")
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    /// Fixtures of the chatlist-global captures.
    pub(super) fn demo_setup_chatlist_global(
        &mut self,
        demo: Option<D>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(demo) = demo else { return };
        if !matches!(
            demo,
            D::ReadyChatlistContactsIndex
                | D::ReadyChatlistCallsClear
                | D::ReadyChatlistStoriesMenu
                | D::ReadyChatlistBirthdays
                | D::ReadyChatlistSuggestions
                | D::ReadyChatlistSuggestionsPhone
        ) {
            return;
        }
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_seq.store(session.last_seq, Ordering::SeqCst);
        match demo {
            D::ReadyChatlistContactsIndex => {
                apply_contacts_fixture(session, &self.demo_sink, &self.demo_seq);
                self.contacts_tab_open = true;
                self.global.contacts_sort = quill::contacts_index::SortMode::Alphabet;
                // The pointer rests over the "L" area of the bar.
                self.global.index_cursor = Some((46., 250.));
                self.status_note = "screenshot demo — contacts by name with the index bar".into();
            }
            D::ReadyChatlistCallsClear => {
                apply_ready_calls_settings(session, &self.demo_sink, &self.demo_seq);
                self.calls_tab_open = true;
                self.global.clear_calls_open = true;
                self.status_note = "screenshot demo — Clear calls box".into();
            }
            D::ReadyChatlistStoriesMenu => {
                super::chatlist_demo::apply_ready_archive_row(
                    session,
                    &self.demo_sink,
                    &self.demo_seq,
                );
                self.global.story_menu =
                    Some((11, gpui_kit::point(gpui_kit::px(150.), gpui_kit::px(190.))));
                self.status_note = "screenshot demo — story tile menu".into();
            }
            D::ReadyChatlistBirthdays => {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                let now = quill::local_time::now_unix();
                let day = |offset: i64| quill::local_time::civil_local(now + offset * 86_400);
                let entry = |id: i64,
                             first: &str,
                             last: &str,
                             when: quill::local_time::CivilTime| {
                    [
                        format!(
                            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","phone_number":"","type":{{"@type":"userTypeRegular"}}}}}}"#
                        ),
                        format!(r#"{{"id":{id},"day":{},"month":{}}}"#, when.day, when.month),
                    ]
                };
                let people = [
                    entry(401, "Ada", "Lovelace", day(0)),
                    entry(402, "Grace", "Hopper", day(1)),
                    entry(403, "Alan", "Turing", day(-1)),
                ];
                let mut closes = Vec::new();
                for [user, date] in &people {
                    if let Some(owned) = copy_and_parse(user, &self.demo_seq, &dyn_sink) {
                        session.apply(owned);
                    }
                    closes.push(date.clone());
                }
                let list: Vec<String> = closes
                    .iter()
                    .map(|date| {
                        let v: serde_json::Value = serde_json::from_str(date).unwrap();
                        format!(
                            r#"{{"@type":"closeBirthdayUser","user_id":{},"birthdate":{{"@type":"birthdate","day":{},"month":{},"year":0}}}}"#,
                            v["id"], v["day"], v["month"]
                        )
                    })
                    .collect();
                let json = format!(
                    r#"{{"@type":"updateContactCloseBirthdays","close_birthday_users":[{}]}}"#,
                    list.join(",")
                );
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
                self.settings_open = true;
                self.settings_page = Some("Contacts");
                self.status_note = "screenshot demo — birthday contacts in Settings".into();
            }
            D::ReadyChatlistSuggestions | D::ReadyChatlistSuggestionsPhone => {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                let today = quill::local_time::civil_local(quill::local_time::now_unix());
                let jsons = [
                    r#"{"@type":"updateUser","user":{"id":401,"first_name":"Ada","last_name":"Lovelace","phone_number":"","type":{"@type":"userTypeRegular"}}}"#.to_string(),
                    r#"{"@type":"updateUser","user":{"id":1,"first_name":"Demo","last_name":"Me","phone_number":"15550100123","is_contact":false,"type":{"@type":"userTypeRegular"}}}"#.to_string(),
                    r#"{"@type":"updateSuggestedActions","added_actions":[{"@type":"suggestedActionCheckPhoneNumber"},{"@type":"suggestedActionSetProfilePhoto"}],"removed_actions":[]}"#.to_string(),
                    format!(
                        r#"{{"@type":"updateContactCloseBirthdays","close_birthday_users":[{{"@type":"closeBirthdayUser","user_id":401,"birthdate":{{"@type":"birthdate","day":{},"month":{},"year":1815}}}}]}}"#,
                        today.day, today.month
                    ),
                ];
                for json in jsons {
                    if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                        session.apply(owned);
                    }
                }
                session.my_user_id = Some(1);
                if demo == D::ReadyChatlistSuggestions {
                    // The phone check outranks a birthday: it was answered.
                    session
                        .suggestions
                        .actions
                        .remove("suggestedActionCheckPhoneNumber");
                }
                self.status_note = "screenshot demo — chat-list suggestion".into();
            }
            _ => {}
        }
        cx.notify();
    }
}
