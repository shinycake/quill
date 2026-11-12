//! Fix round 3 of sweep bundle r4-05.
use super::common::*;
use super::*;

/// Every owner of a [`Holds`](crate::state::held::Holds) lets go of a lost
/// request: one request in each of the six owners, all lost, none left.
#[test]
fn held_requests_all_settle_when_lost() {
    let (mut session, _sink) = session();
    let mut ids = Vec::new();
    let mut ask = |session: &mut Session| {
        let id = session.request(RequestPurpose::GetActiveSessions, None);
        ids.push(id);
        id
    };
    let id = ask(&mut session);
    session.settings.quick_replies.sent.begin(
        id,
        crate::state::QuickReplySend {
            purpose: RequestPurpose::GetActiveSessions,
            chat_id: None,
            subject: "hi".into(),
        },
    );
    let id = ask(&mut session);
    session.stories.page_asks.begin(id, 7);
    let id = ask(&mut session);
    session.calls.creates.begin(id, ());
    let id = ask(&mut session);
    session.stickers.edits.begin(id, ());
    let id = ask(&mut session);
    session.bots.web_apps.answers_due.begin(id, ());
    let id = ask(&mut session);
    session
        .communities
        .sent
        .begin(id, (5, crate::state::CommunitiesPurpose::GetJoined));
    assert_eq!(ids.len(), 6);

    for id in ids {
        let pending = session.requests.take(id).expect("registered");
        session.settle_lost_request(pending);
    }
    assert!(session.settings.quick_replies.sent.is_empty());
    assert!(session.stories.page_asks.is_empty());
    assert!(session.calls.creates.is_empty());
    assert!(session.stickers.edits.is_empty());
    assert!(session.bots.web_apps.answers_due.is_empty());
    assert!(session.communities.sent.is_empty());
}

/// A refusal of an older `getRecommendedChatFolders` is not a new line: only
/// the request the list waits for says why.
#[test]
fn an_older_recommended_folders_refusal_pushes_no_error() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let older = session.request(RequestPurpose::GetRecommendedChatFolders, None);
    let newer = session.request(RequestPurpose::GetRecommendedChatFolders, None);
    session.chat_list.recommended_folders.begin_with(newer);
    let refused = |id: RequestId| {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BROKEN"}}"#,
            id.0
        )
    };
    apply_json(&mut session, &seq, &sink, &refused(older));
    assert!(session.chat_list.recommended_folders.waits_for(newer));
    assert!(session.chats_state.chat_action_error.is_empty());
    apply_json(&mut session, &seq, &sink, &refused(newer));
    assert!(session.chat_list.recommended_folders.error().is_some());
    assert!(session.chats_state.chat_action_error.last().is_some());
}
