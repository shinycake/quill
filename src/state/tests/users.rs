//! State reducer tests: users.
use super::common::*;
use super::*;

#[test]
fn update_user_populates_user_directory() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","phone_number":"+15550131","status":{"@type":"userStatusOnline","expires":1},"type":{"@type":"userTypeRegular"},"is_contact":true}}"#,
    );
    let user = session.user(31).expect("user cached");
    assert_eq!(user.display_name(), "Ada Lovelace");
    assert!(user.is_contact);
    assert!(!user.is_bot);
    assert!(user.status.is_online());
}

#[test]
fn update_user_status_refreshes_cached_status() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#,
    );
    assert!(session.user(31).unwrap().status.is_online());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUserStatus","user_id":31,"status":{"@type":"userStatusLastWeek","by_my_privacy_settings":false}}"#,
    );
    let user = session.user(31).unwrap();
    assert!(!user.status.is_online());
    assert_eq!(user.status.display(), "last seen within a week");
}

#[test]
fn update_user_carries_profile_accent_fields() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"},"profile_accent_color_id":3,"profile_background_custom_emoji_id":536870912}}"#,
    );
    let user = session.user(31).unwrap();
    assert_eq!(user.profile_accent_color_id, 3);
    assert_eq!(user.profile_background_custom_emoji_id, 536_870_912);

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":32,"first_name":"Bea","type":{"@type":"userTypeRegular"}}}"#,
    );
    let user = session.user(32).unwrap();
    assert_eq!(user.profile_accent_color_id, -1);
    assert_eq!(user.profile_background_custom_emoji_id, 0);
}

#[test]
fn get_contacts_accepts_matching_response() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(!session.contacts_settled());
    let extra = session.request(RequestPurpose::GetContacts, None);
    // A stray `users` payload without our `@extra` is ignored.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"users","total_count":1,"user_ids":[99]}"#,
    );
    assert!(!session.contacts_settled());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":32,"first_name":"Zed","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusRecently","by_my_privacy_settings":false}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"users","@extra":"{}","total_count":3,"user_ids":[31,32,33]}}"#,
            extra.0
        ),
    );
    assert!(session.contacts_settled());
    assert_eq!(session.contacts.as_deref(), Some([31, 32, 33].as_slice()));
    // User 33 never arrived via `updateUser` → no row yet.
    let rows = session.contact_rows();
    assert_eq!(rows.len(), 2);
    // Sorted by name: Ada before Zed, regardless of server order.
    assert_eq!(rows[0].user_id, 31);
    assert_eq!(rows[0].name, "Ada");
    assert!(rows[0].is_online);
    assert_eq!(rows[1].user_id, 32);
    assert_eq!(rows[1].status_text, "last seen recently");
    assert!(!rows[1].is_online);
}

#[test]
fn get_contacts_error_surfaces_retry() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetContacts, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            extra.0
        ),
    );
    assert!(session.contacts_error);
    assert!(session.contacts_settled());
}

#[test]
fn user_full_info_bio_resolves_user() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // User-scoped fetch (no chat).
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"CANARY bio","entities":[]}},"bot_info":null}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.user_full_info(31).map(|i| i.bio.as_str()),
        Some("CANARY bio")
    );
    // Chat-scoped fetch for a private chat.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":32},"unread_count":0}}"#,
    );
    let chat_extra = session.request(RequestPurpose::GetUserFullInfo, Some(ChatId(41)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"chat bio","entities":[]}},"bot_info":null}}"#,
            chat_extra.0
        ),
    );
    assert_eq!(
        session.user_full_info(32).map(|i| i.bio.as_str()),
        Some("chat bio")
    );
    // `updateUserFullInfo` refreshes the same cache.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUserFullInfo","user_id":31,"user_full_info":{"@type":"userFullInfo","bio":{"@type":"formattedText","text":"refreshed","entities":[]},"bot_info":null}}"#,
    );
    assert_eq!(
        session.user_full_info(31).map(|i| i.bio.as_str()),
        Some("refreshed")
    );
}

#[test]
fn add_contact_ok_invalidates_contacts() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let list_extra = session.request(RequestPurpose::GetContacts, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"users","@extra":"{}","total_count":0,"user_ids":[]}}"#,
            list_extra.0
        ),
    );
    assert!(session.contacts.is_some());
    let add_extra = session.request_for_user(RequestPurpose::AddContact, 55);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, add_extra.0),
    );
    assert!(session.contacts.is_none());
}

#[test]
fn a6_imported_contacts_response_invalidates_and_notices() {
    // Slice A6: `importContacts` answers `importedContacts`
    // (schema 1.8.67, line 14517), NOT `ok` — the reducer still
    // invalidates the list and records the notice.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.contacts = Some(vec![31]);
    let extra = session.request(RequestPurpose::ImportContacts, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"importedContacts","@extra":"{}","user_ids":[31,32],"importer_count":[2]}}"#,
            extra.0
        ),
    );
    assert!(session.contacts.is_none());
    assert_eq!(
        session.contacts_notice.as_deref(),
        Some("Contacts imported.")
    );
}

#[test]
fn a6_remove_contact_ok_clears_cached_is_contact() {
    // Slice A6: a confirmed `removeContacts` drops the contact flag
    // on the cached user (in addition to invalidating the list) so
    // the info panel stops offering "Delete contact".
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","phone_number":"+15550101031","status":{"@type":"userStatusRecently"},"is_contact":true,"type":{"@type":"userTypeRegular"}}}"#,
    );
    assert!(session.users.get(&31).expect("user").is_contact);
    session.contacts = Some(vec![31]);
    let extra = session.request_for_user(RequestPurpose::RemoveContact, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.contacts.is_none());
    assert!(!session.users.get(&31).expect("user").is_contact);
    assert_eq!(session.contacts_notice.as_deref(), Some("Contact deleted."));
}

#[test]
fn a6_block_ok_updates_cached_blocked() {
    // Slice A6: a user-scoped `setMessageSenderBlockList` `ok`
    // carries no state, but the confirmed request does — the cached
    // `UserFullInfoData.blocked` flips authoritatively (never
    // optimistically).
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","block_list":null,"bio":null,"bot_info":null}}"#,
            extra.0
        ),
    );
    assert!(!session.user_full_infos.get(&31).expect("info").blocked);
    let extra = session.request_for_user(
        RequestPurpose::SetMessageSenderBlockList { block: true },
        31,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.user_full_infos.get(&31).expect("info").blocked);
}
