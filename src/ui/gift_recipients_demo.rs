//! `ready-birthday-gift`: two contacts celebrate today, so the chat list's
//! birthday card opens "Gift Premium or Stars"; one more had a birthday
//! yesterday and one has it tomorrow. `ready-birthday-gift-one`: only one
//! celebrates today, so the card opens the gift picker for them
//! (injected, no live Telegram).
#![deny(clippy::indexing_slicing)]

use super::demo::seed_ready_chats_session;
use super::screenshot_demo::{DemoSpec, register_demos};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::local_time::{civil_local, now_unix};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-birthday-gift",
        seed_birthdays,
        "screenshot demo — birthday gifts: two contacts celebrate today (injected, no live Telegram)"
    ),
    DemoSpec::ready(
        "ready-birthday-gift-one",
        seed_one_birthday,
        "screenshot demo — birthday gifts: one contact celebrates today (injected, no live Telegram)"
    ),
];

/// The demo account.
pub(super) const ME: i64 = 900;
/// Contacts: today, today, yesterday, tomorrow, no birthday.
pub(super) const ADA: i64 = 901;
pub(super) const GRACE: i64 = 902;
pub(super) const ALAN: i64 = 903;
pub(super) const KATHERINE: i64 = 904;
pub(super) const LINUS: i64 = 905;

fn seed_birthdays(sink: Arc<MemorySink>) -> Session {
    seed(sink, &[ADA, GRACE])
}

fn seed_one_birthday(sink: Arc<MemorySink>) -> Session {
    seed(sink, &[ADA])
}

/// `today` celebrate today; Alan yesterday and Katherine tomorrow.
fn seed(sink: Arc<MemorySink>, today: &[i64]) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    let mut apply = |json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    let people = [
        (ME, "Quill", "Demo"),
        (ADA, "Ada", "Lovelace"),
        (GRACE, "Grace", "Hopper"),
        (ALAN, "Alan", "Turing"),
        (KATHERINE, "Katherine", "Johnson"),
        (LINUS, "Linus", "Pauling"),
    ];
    for (id, first, last) in people {
        apply(&format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ));
    }
    apply(&format!(
        r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
    ));
    let now = now_unix();
    let date = |offset_days: i64| civil_local(now.saturating_add(offset_days * 86_400));
    let mut birthdays: Vec<(i64, i64)> = today.iter().map(|id| (*id, 0)).collect();
    birthdays.extend([(ALAN, -1), (KATHERINE, 1)]);
    let entries: Vec<String> = birthdays
        .into_iter()
        .map(|(id, offset)| {
            let day = date(offset);
            format!(
                r#"{{"@type":"closeBirthdayUser","user_id":{id},"birthdate":{{"@type":"birthdate","day":{},"month":{},"year":0}}}}"#,
                day.day, day.month
            )
        })
        .collect();
    apply(&format!(
        r#"{{"@type":"updateContactCloseBirthdays","close_birthday_users":[{}]}}"#,
        entries.join(",")
    ));
    session.users_state.contacts = Some(vec![ADA, GRACE, ALAN, KATHERINE, LINUS]);
    session
}
