//! Share box rows: order and second lines (tdesktop forward box).
use super::*;
use crate::diagnostics::MemorySink;
use crate::telegram::client::copy_and_parse;
use std::sync::atomic::AtomicU64;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn session() -> Session {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    Session::new(crate::ids::AccountKey::primary(), sink)
}

fn apply(session: &mut Session, json: &str) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let owned = copy_and_parse(json, &SEQ, &sink).expect("envelope");
    session.apply(owned);
}

fn listed(session: &mut Session, id: i64, title: &str, kind: &str, order: i64) {
    apply(
        session,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"unread_count":0}}}}"#
        ),
    );
    apply(
        session,
        &format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        ),
    );
}

fn user(session: &mut Session, id: i64, status: &str, kind: &str) {
    apply(
        session,
        &format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"U{id}","last_name":"","usernames":null,"phone_number":"","status":{status},"profile_photo":null,"is_contact":true,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"type":{{"@type":"{kind}"}}}}}}"#
        ),
    );
}

fn private(id: i64) -> String {
    format!(r#"{{"@type":"chatTypePrivate","user_id":{id}}}"#)
}

fn ids(chats: &[&ChatSummary]) -> Vec<i64> {
    chats.iter().map(|chat| chat.id.0).collect()
}

#[test]
fn saved_messages_leads_the_list_wherever_the_chat_list_has_it() {
    let mut session = session();
    session.my_user_id = Some(5);
    listed(&mut session, 7, "Ada", &private(7), 30);
    listed(&mut session, 5, "Me", &private(5), 10);
    listed(&mut session, 8, "Bob", &private(8), 20);
    assert_eq!(ids(&session.share_destinations("")), [5, 7, 8]);
    // A query that leaves Saved Messages out keeps the plain order.
    assert_eq!(ids(&session.share_destinations("b")), [8]);
}

#[test]
fn rows_say_online_bot_members_and_nothing_for_saved_messages() {
    let mut session = session();
    session.my_user_id = Some(5);
    listed(&mut session, 5, "Me", &private(5), 10);
    listed(&mut session, 7, "Ada", &private(7), 30);
    listed(&mut session, 9, "Helper", &private(9), 25);
    user(
        &mut session,
        7,
        r#"{"@type":"userStatusOnline","expires":9999999999}"#,
        "userTypeRegular",
    );
    user(
        &mut session,
        9,
        r#"{"@type":"userStatusEmpty"}"#,
        "userTypeBot",
    );
    listed(
        &mut session,
        -100,
        "Team",
        r#"{"@type":"chatTypeBasicGroup","basic_group_id":100}"#,
        20,
    );
    apply(
        &mut session,
        r#"{"@type":"updateBasicGroup","basic_group":{"@type":"basicGroup","id":100,"member_count":12,"status":{"@type":"chatMemberStatusMember"},"is_active":true,"upgraded_to_supergroup_id":0}}"#,
    );
    let status = |session: &Session, id: i64| {
        session
            .chats
            .get(&id)
            .and_then(|chat| session.share_row_status(chat))
    };
    assert_eq!(status(&session, 5), None);
    assert_eq!(status(&session, 7).map(|(_, online)| online), Some(true));
    assert_eq!(status(&session, 9), Some(("bot".to_string(), false)));
    let (members, online) = status(&session, -100).expect("member line");
    assert!(members.contains("12"), "{members}");
    assert!(!online);
}
