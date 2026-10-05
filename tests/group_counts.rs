//! Header member/online counts from `updateBasicGroup`, `updateSupergroup`
//! and `updateChatOnlineMemberCount`.
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn apply(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64, json: &str) {
    session.apply(copy_and_parse(json, seq, sink).expect("parses"));
}

#[test]
fn group_counts_come_from_base_objects_and_online_updates() {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let mut session = Session::new(AccountKey::primary(), sink.clone());
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":-5,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":5},"unread_count":0}}"#,
        r#"{"@type":"updateBasicGroup","basic_group":{"@type":"basicGroup","id":5,"member_count":12,"status":{"@type":"chatMemberStatusMember","member_until_date":0},"is_active":true,"upgraded_to_supergroup_id":0}}"#,
        r#"{"@type":"updateChatOnlineMemberCount","chat_id":-5,"online_member_count":3}"#,
        r#"{"@type":"updateNewChat","chat":{"id":-1009,"title":"Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":true},"unread_count":0}}"#,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":9,"member_count":12400,"status":{"@type":"chatMemberStatusMember","member_until_date":0},"is_channel":true}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    ] {
        apply(&mut session, &sink, &seq, json);
    }
    let basic = session.chats.get(&-5).unwrap().clone();
    assert_eq!(session.group_member_counts(&basic), Some((12, 3)));
    let channel = session.chats.get(&-1009).unwrap().clone();
    assert_eq!(session.group_member_counts(&channel), Some((12400, 0)));
    let private = session.chats.get(&7).unwrap().clone();
    assert_eq!(session.group_member_counts(&private), None);
}
