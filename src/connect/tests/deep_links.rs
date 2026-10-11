//! Invite deep links must be checked before an explicit, generation-guarded join.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::{DeepLinkAction, DeepLinkState};
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn invite_check_confirm_cancel_and_stale_answers() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let action = DeepLinkAction::JoinInvite {
        hash: "AbCd".into(),
    };
    let extra = driver.resolve_deep_link(action.clone()).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "checkChatInviteLink")["invite_link"],
        "https://t.me/+AbCd"
    );
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("joinChatByInviteLink"))
    );
    let generation = driver.session.chats_state.deep_link_seq;
    assert_eq!(driver.confirm_deep_link_invite(generation).unwrap(), None);
    let preview = |extra: crate::ids::RequestId| {
        json!({
            "@type": "chatInviteLinkInfo", "@extra": extra.as_extra(),
            "title": "Rust Community", "member_count": 1248,
            "creates_join_request": true, "type": {"@type": "inviteLinkChatTypeChannel"}
        })
        .to_string()
    };
    driver
        .ingest(copy_and_parse(&preview(extra), &seq, &sink).unwrap())
        .unwrap();
    assert!(
        matches!(driver.session.chats_state.deep_link, Some(DeepLinkState::InvitePreview {
        ref title, member_count: 1248, creates_join_request: true, is_channel: true, ..
    }) if title == "Rust Community")
    );
    assert_eq!(
        driver.confirm_deep_link_invite(generation + 1).unwrap(),
        None
    );
    let join = driver
        .confirm_deep_link_invite(generation)
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "joinChatByInviteLink")["invite_link"],
        "https://t.me/+AbCd"
    );
    assert_eq!(driver.confirm_deep_link_invite(generation).unwrap(), None);
    driver
        .ingest(
            copy_and_parse(
                &json!({"@type": "chatJoinResultRequestSent", "@extra": join.as_extra()})
                    .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        matches!(driver.session.chats_state.deep_link, Some(DeepLinkState::ShowText(ref text)) if text.contains("Join request sent"))
    );

    let old = driver.resolve_deep_link(action.clone()).unwrap().unwrap();
    let current = driver.resolve_deep_link(action).unwrap().unwrap();
    driver
        .ingest(copy_and_parse(&preview(old), &seq, &sink).unwrap())
        .unwrap();
    assert!(matches!(
        driver.session.chats_state.deep_link,
        Some(DeepLinkState::ResolvingChat { .. })
    ));
    driver
        .ingest(copy_and_parse(&preview(current), &seq, &sink).unwrap())
        .unwrap();
    // Cancel clears the retained preview; a later confirmation cannot join.
    driver.session.chats_state.deep_link = None;
    let before = recorder.snapshot().len();
    assert_eq!(
        driver
            .confirm_deep_link_invite(driver.session.chats_state.deep_link_seq)
            .unwrap(),
        None
    );
    assert_eq!(recorder.snapshot().len(), before);
    std::fs::remove_dir_all(dir).unwrap();
}
