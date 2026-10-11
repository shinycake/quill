//! Contacts replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

/// Phase 6: the contacts flow — `updateUser` objects land the user cache,
/// a `getContacts` `users` response lands the id list, `contact_rows`
/// renders name/status rows in server order sorted by name, and a
/// `getUserFullInfo` response (correlated via the pending request's
/// user id) caches the bio plus the preferred `chatPhoto` file. A later
/// `updateUserStatus` refreshes the row's status text.
#[test]
fn replay_contacts_list_and_user_full_info() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","phone_number":"+15550101031","status":{"@type":"userStatusOnline","expires":9999999999},"is_contact":true,"type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateUser","user":{"id":32,"first_name":"Zed","last_name":"Hopper","phone_number":"+15550101032","status":{"@type":"userStatusLastWeek","by_my_privacy_settings":false},"is_contact":false,"type":{"@type":"userTypeRegular"}}}"#,
        ],
    );
    assert!(!session.contacts_settled());
    let extra = session.request(RequestPurpose::GetContacts, None);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"users","@extra":"{}","total_count":2,"user_ids":[31,32]}}"#,
            extra.0
        )],
    );
    assert!(session.contacts_settled());
    let rows = session.contact_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Ada Lovelace");
    assert!(rows[0].is_online);
    assert!(rows[0].is_contact);
    assert_eq!(rows[1].name, "Zed Hopper");
    assert!(!rows[1].is_online);
    assert!(!rows[1].is_contact);
    assert_eq!(rows[1].status_text, "last seen within a week");

    // Full info for Zed: bio + chatPhoto (preferred "m" size wins).
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 32);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"CANARY_REPLAY_bio","entities":[]}},"bot_info":null,"photo":{{"@type":"chatPhoto","id":1,"added_date":1,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"s","photo":{{"@type":"file","id":901}},"width":90,"height":90,"progressive_sizes":[]}},{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":902}},"width":320,"height":320,"progressive_sizes":[]}}],"animation":null,"small_animation":null,"sticker":null}}}}"#,
            extra.0
        )],
    );
    let info = session.user_full_info(32).expect("full info cached");
    assert_eq!(info.bio, "CANARY_REPLAY_bio");
    assert_eq!(info.photo_file_id, Some(902));
    assert!(session.media.files.contains_key(&902));

    // A later status update refreshes the contact row.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateUserStatus","user_id":32,"status":{"@type":"userStatusOnline","expires":9999999999}}"#,
        ],
    );
    let rows = session.contact_rows();
    assert!(rows[1].is_online);
    assert_eq!(rows[1].status_text, "online");
    assert!(!sink.rendered().contains("CANARY_REPLAY_bio"));
}
