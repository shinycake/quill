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
    assert_eq!(
        session.chat_list.folder_limit_hit.take(),
        Some(FolderLimitKind::SharedFolders)
    );
    assert!(session.chat_list.folder_invite_error.is_none());
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
        Some(FolderLimitKind::Folders)
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
        Some(FolderLimitKind::ChatsIncluded)
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
    assert!(session.chat_list.folder_invite_error.is_some());
}
