//! Group/channel admins replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

/// Phase D3b: `getChatAdministrators` response populates the admin-list
/// cache (owner with custom title + editable admin). The gate is
/// deny-by-default: closed until the viewer's own `can_promote_members`
/// right is known.
#[test]
fn replay_admin_list_load_and_gate() {
    use quill::state::{AdminListFetch, MemberStatusChange};

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    // Deny-by-default: no membership known yet.
    assert!(!session.chat_can_manage_admins(chat_id));

    let list_extra = session.request(RequestPurpose::GetChatAdministrators, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatAdministrators","@extra":"{}","administrators":[{{"@type":"chatAdministrator","user_id":1,"custom_title":"Founder","is_owner":true,"can_be_edited":false}},{{"@type":"chatAdministrator","user_id":2,"custom_title":"Mod","is_owner":false,"can_be_edited":true}}]}}"#,
            list_extra.0
        )],
    );
    let list = match session.admin_lists.get(&13) {
        Some(AdminListFetch::Loaded(list)) => list,
        other => panic!("expected loaded admin list, got {other:?}"),
    };
    assert_eq!(list.len(), 2);
    assert!(list[0].is_owner);
    assert_eq!(list[0].custom_title, "Founder");
    assert!(!list[0].can_be_edited);
    assert_eq!(list[1].user_id, 2);
    assert!(list[1].can_be_edited);

    // The viewer is admin 2 with `can_promote_members`: gate opens via
    // the supergroup's own status block.
    let me_extra = session.request(RequestPurpose::GetMe, None);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"user","@extra":"{}","id":2}}"#, me_extra.0),
            r#"{"@type":"updateSupergroup","supergroup":{"id":13,"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_promote_members":true}}}}"#,
        ],
    );
    assert!(session.chat_can_manage_admins(chat_id));

    // `ok` to `setChatMemberStatus` invalidates the cached admin list.
    let promote_extra = session.request(
        RequestPurpose::SetChatMemberStatus {
            user_id: 5,
            kind: MemberStatusChange::Promote,
        },
        Some(chat_id),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"ok","@extra":"{}"}}"#,
            promote_extra.0
        )],
    );
    assert!(!session.admin_lists.contains_key(&13));
}

/// Phase D3b: any `updateChatMember` invalidates the cached admin list,
/// and the viewer's own membership refresh keeps the
/// `can_promote_members` gate honest in both directions.
#[test]
fn replay_admin_update_member_invalidates_and_own_gate() {
    use quill::state::AdminListFetch;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
        ],
    );

    // Seed a loaded list via a direct insert shaped like the reducer's.
    let list_extra = session.request(RequestPurpose::GetChatAdministrators, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatAdministrators","@extra":"{}","administrators":[{{"@type":"chatAdministrator","user_id":1,"custom_title":"","is_owner":true,"can_be_edited":false}}]}}"#,
            list_extra.0
        )],
    );
    assert!(matches!(
        session.admin_lists.get(&13),
        Some(AdminListFetch::Loaded(_))
    ));

    // Someone else is promoted: the cached list is dropped.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":9},"status":{"@type":"chatMemberStatusMember"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":9},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_promote_members":false}}}}"#,
        ],
    );
    assert!(!session.admin_lists.contains_key(&13));

    // Own membership: gate opens with can_promote_members=true…
    let me_extra = session.request(RequestPurpose::GetMe, None);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(r#"{{"@type":"user","@extra":"{}","id":7}}"#, me_extra.0),
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":7},"status":{"@type":"chatMemberStatusMember"}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":7},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_promote_members":true}}}}"#,
        ],
    );
    assert!(session.chat_can_manage_admins(chat_id));
    // …and closes again when the right is revoked.
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":1,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":7},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_promote_members":true}}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":7},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_promote_members":false}}}}"#,
        ],
    );
    assert!(!session.chat_can_manage_admins(chat_id));
}

/// Phase D3b: `getChatMember` for one administrator populates the
/// per-admin rights cache backing the edit-rights dialog; an `ok` to
/// demote clears the admin list like promote does.
#[test]
fn replay_admin_rights_lookup_and_demote() {
    use quill::state::{AdminListFetch, AdminRightsFetch, MemberStatusChange};

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );

    let rights_extra =
        session.request(RequestPurpose::GetAdminRights { user_id: 2 }, Some(chat_id));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":2}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_post_messages":true,"can_delete_messages":true,"can_promote_members":false}}}}}}"#,
            rights_extra.0
        )],
    );
    let rights = match session.admin_rights.get(&(13, 2)) {
        Some(AdminRightsFetch::Loaded(rights)) => rights,
        other => panic!("expected loaded admin rights, got {other:?}"),
    };
    assert!(rights.can_post_messages);
    assert!(rights.can_delete_messages);
    assert!(!rights.can_promote_members);
    assert!(!rights.can_pin_messages);

    // Demote's `ok` invalidates the admin list too.
    let demote_extra = session.request(
        RequestPurpose::SetChatMemberStatus {
            user_id: 2,
            kind: MemberStatusChange::Demote,
        },
        Some(chat_id),
    );
    session.admin_lists.insert(13, AdminListFetch::Loading);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"ok","@extra":"{}"}}"#,
            demote_extra.0
        )],
    );
    assert!(!session.admin_lists.contains_key(&13));
}

/// Phase D3b: `getSupergroupMembers` populates the promote picker's
/// member cache; a failed lookup records the failure honestly.
#[test]
fn replay_supergroup_members_picker_cache() {
    use quill::state::SupergroupMembersFetch;

    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let chat_id = quill::ids::ChatId(13);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":false},"unread_count":0}}"#,
        ],
    );

    let members_extra = session.request(
        RequestPurpose::GetSupergroupMembers {
            filter: MemberListFilter::Recent,
        },
        Some(chat_id),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":2,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":5}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusMember"}}}},{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":6}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights"}}}}}}]}}"#,
            members_extra.0
        )],
    );
    let page = match session
        .supergroup_members
        .get(&(13, MemberListFilter::Recent))
    {
        Some(SupergroupMembersFetch::Loaded { members, .. }) => members,
        other => panic!("expected loaded members, got {other:?}"),
    };
    use quill::telegram::envelope::MessageSender;
    fn member_user_id(member: &quill::telegram::envelope::ParsedChatMember) -> i64 {
        match member.member_id {
            MessageSender::User { user_id } => user_id,
            _ => panic!("expected user member"),
        }
    }
    assert_eq!(page.len(), 2);
    assert_eq!(member_user_id(&page[0]), 5);
    assert_eq!(member_user_id(&page[1]), 6);
    assert!(page[1].admin_rights.is_some());

    // A TDLib error records a failed fetch with the action label.
    let retry_extra = session.request(
        RequestPurpose::GetSupergroupMembers {
            filter: MemberListFilter::Recent,
        },
        Some(chat_id),
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
            retry_extra.0
        )],
    );
    assert!(matches!(
        session
            .supergroup_members
            .get(&(13, MemberListFilter::Recent)),
        Some(SupergroupMembersFetch::Failed(_))
    ));
}
