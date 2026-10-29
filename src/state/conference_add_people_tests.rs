use super::*;
use crate::diagnostics::MemorySink;
use crate::telegram::client::copy_and_parse;
use std::sync::atomic::AtomicU64;

static SEQ: AtomicU64 = AtomicU64::new(0);

const PEER: i64 = 41;

fn session() -> Session {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    Session::new(crate::ids::AccountKey::primary(), sink)
}

fn apply(session: &mut Session, json: &str) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let owned = copy_and_parse(json, &SEQ, &sink).expect("envelope");
    session.apply(owned);
}

/// A user last seen at `was_online` (larger is more recent).
fn user(session: &mut Session, id: i64, first: &str, last: &str, was_online: i64, kind: &str) {
    apply(
        session,
        &format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusOffline","was_online":{was_online}}},"profile_photo":null,"is_contact":true,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"type":{{"@type":"{kind}"}}}}}}"#
        ),
    );
    apply(
        session,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{first} {last}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
        ),
    );
}

fn contact(session: &mut Session, id: i64, first: &str, last: &str, was_online: i64) {
    user(session, id, first, last, was_online, "userTypeRegular");
    session
        .users_state
        .contacts
        .get_or_insert_with(Vec::new)
        .push(id);
}

fn ids(rows: &[AddPeopleRow]) -> Vec<i64> {
    rows.iter().map(|row| row.user_id).collect()
}

/// You, the call's other side and a few contacts; a bot and a deleted
/// account among them are never offered.
fn seeded() -> Session {
    let mut session = session();
    session.my_user_id = Some(1);
    contact(&mut session, 1, "Me", "Myself", 900);
    contact(&mut session, PEER, "Noa", "Peretz", 800);
    contact(&mut session, 50, "Dana", "Levi", 500);
    contact(&mut session, 51, "Dan", "Avraham", 700);
    contact(&mut session, 52, "Yael", "Dagan", 600);
    user(&mut session, 53, "Helper", "Bot", 0, "userTypeBot");
    user(&mut session, 54, "", "", 0, "userTypeDeleted");
    if let Some(contacts) = session.users_state.contacts.as_mut() {
        contacts.extend([53, 54]);
    }
    apply(
        &mut session,
        r#"{"@type":"updateUser","user":{"@type":"user","id":52,"first_name":"Yael","last_name":"Dagan","usernames":{"@type":"usernames","active_usernames":["yd_music"],"disabled_usernames":[],"editable_username":"yd_music","collectible_usernames":[]},"phone_number":"","status":{"@type":"userStatusOffline","was_online":600},"profile_photo":null,"is_contact":true,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"type":{"@type":"userTypeRegular"}}}"#,
    );
    session
}

#[test]
fn every_contact_is_listed_by_last_seen_with_the_other_side_already_in() {
    let mut session = seeded();
    for n in 0..40 {
        contact(&mut session, 100 + n, "Guest", &format!("N{n}"), n);
    }
    let view = session.add_people_view(PEER, &[], &[], "");
    assert_eq!(view.rows.len(), 44);
    assert_eq!(
        ids(view.rows.get(..4).unwrap_or_default()),
        [PEER, 51, 52, 50]
    );
    let peer = view.rows.first().expect("peer row");
    assert!(peer.already_in && peer.checked);
    assert!(view.rows.iter().skip(1).all(|row| !row.already_in));
    assert!(!view.searching);
    assert_eq!(view.empty, None);
    assert_eq!(view.first_pickable(), Some(51));
}

#[test]
fn typing_filters_by_the_start_of_name_words_and_keeps_the_picks() {
    let session = seeded();
    let view = session.add_people_view(PEER, &[50, 52], &[], "da");
    assert!(view.searching);
    // "Dan Avraham", "Yael Dagan", "Dana Levi"; not "Noa Peretz".
    assert_eq!(ids(&view.rows), [51, 52, 50]);
    let checked: Vec<i64> = view
        .rows
        .iter()
        .filter(|row| row.checked)
        .map(|row| row.user_id)
        .collect();
    assert_eq!(checked, [52, 50]);
    let view = session.add_people_view(PEER, &[50, 52], &[], "da le");
    assert_eq!(ids(&view.rows), [50]);
    // The username counts as a name word.
    let view = session.add_people_view(PEER, &[], &[], "yd");
    assert_eq!(ids(&view.rows), [52]);
    // Back to the full list: the picks are still checked.
    let view = session.add_people_view(PEER, &[50, 52], &[], "");
    assert!(view.rows.iter().any(|row| row.user_id == 50 && row.checked));
}

#[test]
fn the_server_results_follow_the_contacts_without_repeating_them() {
    let mut session = seeded();
    user(&mut session, 60, "Daria", "Shalev", 10, "userTypeRegular");
    user(&mut session, 61, "Dalia", "Mor", 10, "userTypeRegular");
    user(&mut session, 62, "Dave", "Bot", 10, "userTypeBot");
    session
        .calls
        .add_people_search
        .begin("da", RequestId(7), RequestId(8));
    // Still searching, but the contacts already match.
    let view = session.add_people_view(PEER, &[], &[], "da");
    assert_eq!(view.global, Vec::new());
    assert_eq!(view.empty, None);
    session
        .calls
        .add_people_search
        .accept(RequestId(7), &[ChatId(50), ChatId(60)]);
    session
        .calls
        .add_people_search
        .accept(RequestId(8), &[ChatId(61), ChatId(60), ChatId(62)]);
    let view = session.add_people_view(PEER, &[61], &[], " da ");
    assert_eq!(ids(&view.rows), [51, 52, 50]);
    assert_eq!(ids(&view.global), [60, 61]);
    assert!(
        view.global
            .iter()
            .any(|row| row.user_id == 61 && row.checked)
    );
    // The answers belong to "da" only.
    let view = session.add_people_view(PEER, &[], &[], "dar");
    assert_eq!(view.global, Vec::new());
    assert_eq!(view.empty, Some(AddPeopleEmpty::NoResults));
}

#[test]
fn someone_picked_from_the_server_results_stays_at_the_top() {
    let mut session = seeded();
    user(&mut session, 60, "Daria", "Shalev", 10, "userTypeRegular");
    let view = session.add_people_view(PEER, &[60], &[60], "");
    assert_eq!(ids(view.rows.get(..2).unwrap_or_default()), [60, PEER]);
    assert!(view.rows.first().is_some_and(|row| row.checked));
    // A search that doesn't match drops them like any other row.
    let view = session.add_people_view(PEER, &[60], &[60], "yael");
    assert_eq!(ids(&view.rows), [52]);
}

#[test]
fn the_empty_texts_follow_the_contacts_and_the_search() {
    let mut session = session();
    let empty =
        |session: &Session, query: &str| session.add_people_view(PEER, &[], &[], query).empty;
    assert_eq!(empty(&session, ""), Some(AddPeopleEmpty::ContactsLoading));
    session.users_state.contacts = Some(Vec::new());
    assert_eq!(empty(&session, ""), Some(AddPeopleEmpty::NoContacts));
    assert_eq!(empty(&session, "zed"), Some(AddPeopleEmpty::NoResults));
    session
        .calls
        .add_people_search
        .begin("zed", RequestId(7), RequestId(8));
    assert_eq!(empty(&session, "zed"), Some(AddPeopleEmpty::SearchLoading));
    session.calls.add_people_search.accept(RequestId(7), &[]);
    session.calls.add_people_search.accept(RequestId(8), &[]);
    assert_eq!(empty(&session, "zed"), Some(AddPeopleEmpty::NoResults));
}

#[test]
fn the_invite_toast_names_one_person_and_counts_more() {
    let mut results = ConferenceInviteResults::default();
    for (request, user_id) in [(1, 50), (2, 51), (3, 52), (4, 53), (5, 54)] {
        results.sent(RequestId(request), user_id);
    }
    results.answered(
        RequestId(1),
        Some(InviteGroupCallParticipantResult::Success {
            chat_id: 50,
            message_id: 9,
        }),
    );
    results.answered(
        RequestId(2),
        Some(InviteGroupCallParticipantResult::UserAlreadyParticipant),
    );
    results.answered(
        RequestId(3),
        Some(InviteGroupCallParticipantResult::UserPrivacyRestricted),
    );
    results.answered(RequestId(4), None);
    assert!(!results.is_done());
    results.answered(
        RequestId(5),
        Some(InviteGroupCallParticipantResult::UserWasBanned),
    );
    assert!(results.is_done());
    let name = |user_id: i64| format!("P{user_id}");
    assert_eq!(
        conference_invite_toast(&results, name),
        "You're calling **P50** to join.\n\n**P51** is already in the call.\n\n\
         Couldn't call **P52** to join.\n\n**P54** was banned from the call.\n\n\
         Couldn't call **P53** to join."
    );
    results.invited.push(60);
    assert!(
        conference_invite_toast(&results, name).starts_with("You're calling **2 people** to join.")
    );
}

#[test]
fn server_results_never_repeat_a_contact_the_query_left_out() {
    let mut session = seeded();
    session
        .calls
        .add_people_search
        .begin("da", RequestId(7), RequestId(8));
    // "Noa Peretz" is a contact who doesn't match "da" (the server matched
    // something else of hers): she isn't added below the matches.
    session
        .calls
        .add_people_search
        .accept(RequestId(8), &[ChatId(PEER), ChatId(50)]);
    let view = session.add_people_view(PEER, &[], &[], "da");
    assert_eq!(ids(&view.rows), [51, 52, 50]);
    assert_eq!(view.global, Vec::new());
}

#[test]
fn the_other_side_is_listed_already_in_even_when_not_a_contact() {
    let mut session = seeded();
    session.users_state.contacts = Some(vec![50, 51]);
    let view = session.add_people_view(PEER, &[], &[], "");
    assert_eq!(ids(&view.rows), [PEER, 51, 50]);
    let peer = view.rows.first().expect("peer row");
    assert!(peer.already_in && peer.checked);
    assert_eq!(view.first_pickable(), Some(51));
    // The query filters them like anyone else.
    assert_eq!(
        ids(&session.add_people_view(PEER, &[], &[], "noa").rows),
        [PEER]
    );
    assert_eq!(
        ids(&session.add_people_view(PEER, &[], &[], "dan").rows),
        [51, 50]
    );
}
