//! State reducer tests: shared-folder new chats, folder limits.
use super::common::*;
use super::*;
use crate::folder_limits::FolderLimitKind;

#[test]
fn new_chats_answer_is_cached_and_an_empty_one_clears_it() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_folder(RequestPurpose::GetChatFolderNewChats, 4);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[8,9]}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_list.folder_new_chats.get(&4),
        Some(&vec![8, 9])
    );
    let extra = session.request_for_folder(RequestPurpose::GetChatFolderNewChats, 4);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            extra.0
        ),
    );
    assert!(!session.chat_list.folder_new_chats.contains_key(&4));
}

#[test]
fn folder_limit_options_and_premium_limits_are_folded() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"chat_folder_count_max","value":{"@type":"optionValueInteger","value":7}}"#,
    );
    assert_eq!(
        session
            .chat_list
            .folder_limits
            .current(FolderLimitKind::Folders, false),
        7
    );
    let extra = session.request_for_folder(RequestPurpose::GetPremiumLimit, 1);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"premiumLimit","@extra":"{}","type":{{"@type":"premiumLimitTypeChatFolderCount"}},"default_value":10,"premium_value":20}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session
            .chat_list
            .folder_limits
            .premium_value(FolderLimitKind::Folders),
        20
    );
}

#[test]
fn a_limit_error_opens_the_limit_box_instead_of_an_error_line() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // Joining shared folders past the limit.
    let extra = session.request(RequestPurpose::AddChatFolderByInviteLink, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHATLISTS_TOO_MUCH"}}"#,
            extra.0
        ),
    );
    // The limit box is for the box that sent this request.
    assert_eq!(
        session.chat_list.folder_limit_hit.take(),
        Some((FolderLimitKind::SharedFolders, Some(extra)))
    );
    // Too many folders when creating one.
    let extra = session.request(RequestPurpose::CreateChatFolder, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"DIALOG_FILTERS_TOO_MUCH"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_list.folder_limit_hit.take(),
        Some((FolderLimitKind::Folders, Some(extra)))
    );
    // Too many included chats when saving.
    let extra = session.request_for_folder(RequestPurpose::EditChatFolder, 3);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"FILTER_INCLUDE_TOO_MUCH"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_list.folder_limit_hit.take(),
        Some((FolderLimitKind::ChatsIncluded, Some(extra)))
    );
    // Any other failure is still an ordinary error.
    let extra = session.request(RequestPurpose::AddChatFolderByInviteLink, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"INVITE_SLUG_EXPIRED"}}"#,
            extra.0
        ),
    );
    assert!(session.chat_list.folder_limit_hit.is_none());
    assert!(matches!(
        session.chat_list.dialog_answers.take(extra),
        Some(crate::state::DialogAnswer::Failed(_))
    ));
}

fn refuse(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    extra: RequestId,
    message: &str,
) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"{message}"}}"#,
            extra.0
        ),
    );
}

#[test]
fn a_request_that_hit_a_limit_answers_its_box_with_the_limit_not_a_line() {
    use crate::state::DialogAnswer;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (purpose, message, other) in [
        (
            RequestPurpose::CreateChatFolder,
            "DIALOG_FILTERS_TOO_MUCH",
            "FILTER_TITLE_EMPTY",
        ),
        (
            RequestPurpose::CreateChatFolderInviteLink,
            "INVITES_TOO_MUCH",
            "INVITE_HASH_EXPIRED",
        ),
        (
            RequestPurpose::AddChatFolderByInviteLink,
            "CHATLISTS_TOO_MUCH",
            "INVITE_SLUG_EXPIRED",
        ),
    ] {
        let extra = session.request_for_folder(purpose, 3);
        refuse(&mut session, &seq, &sink, extra, message);
        assert!(session.chat_list.folder_limit_hit.take().is_some());
        assert_eq!(
            session.chat_list.dialog_answers.take(extra),
            Some(DialogAnswer::LimitHit),
            "{purpose:?}: the limit box alone says it"
        );
        let extra = session.request_for_folder(purpose, 3);
        refuse(&mut session, &seq, &sink, extra, other);
        assert!(matches!(
            session.chat_list.dialog_answers.take(extra),
            Some(DialogAnswer::Failed(_))
        ));
    }
}

#[test]
fn only_answers_a_box_waits_for_are_kept() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let ok = |extra: RequestId| format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0);
    // A box is waiting on this edit.
    let waited = session.request_for_folder(RequestPurpose::EditChatFolder, 3);
    apply_json(&mut session, &seq, &sink, &ok(waited));
    // The remove-from-folder chain sends its edits with no box: many of
    // them must not push the box's answer out.
    for _ in 0..40 {
        let removal = session.request_for_folder(RequestPurpose::RemoveChatFromFolder, 3);
        apply_json(&mut session, &seq, &sink, &ok(removal));
        assert_eq!(session.chat_list.dialog_answers.take(removal), None);
    }
    assert_eq!(
        session.chat_list.dialog_answers.take(waited),
        Some(crate::state::DialogAnswer::Done)
    );
}

#[test]
fn a_refused_folder_reorder_or_tags_switch_says_why() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for purpose in [
        RequestPurpose::ReorderChatFolders,
        RequestPurpose::ToggleChatFolderTags,
    ] {
        assert!(session.chats_state.chat_action_error.is_empty());
        let extra = session.request(purpose, None);
        refuse(&mut session, &seq, &sink, extra, "PREMIUM_ACCOUNT_REQUIRED");
        assert!(
            !session.chats_state.chat_action_error.take_all().is_empty(),
            "{purpose:?}: the toast is not silent"
        );
    }
}
