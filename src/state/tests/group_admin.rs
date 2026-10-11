//! State reducer tests: B7 group admin toggles (gating and reducer).
use super::common::*;
use super::*;
use crate::telegram::envelope::{ChatAvailableReactions, ReactionType};

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const MEMBER: &str = r#"{"@type":"chatMemberStatusMember"}"#;

fn admin_status(rights: &str) -> String {
    format!(
        r#"{{"@type":"chatMemberStatusAdministrator","rights":{{"@type":"chatAdministratorRights",{rights}}}}}"#
    )
}

/// A non-channel supergroup `chat_id` (supergroup id `chat_id + 100`)
/// with the viewer's `status` and extra `supergroup` fields.
fn megagroup(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    chat_id: i64,
    status: &str,
    supergroup_fields: &str,
) -> i64 {
    let sg = chat_id + 100;
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Group","type":{{"@type":"chatTypeSupergroup","supergroup_id":{sg},"is_channel":false}},"unread_count":0}}}}"#
        ),
    );
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{sg},"member_count":500,"status":{status}{supergroup_fields}}}}}"#
        ),
    );
    sg
}

fn full_info(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    sg: i64,
    fields: &str,
) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{sg},"supergroup_full_info":{{"description":"d","member_count":500{fields}}}}}"#
        ),
    );
}

fn channel(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    chat_id: i64,
    status: &str,
) -> i64 {
    let sg = chat_id + 100;
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Channel","type":{{"@type":"chatTypeSupergroup","supergroup_id":{sg},"is_channel":true}},"unread_count":0}}}}"#
        ),
    );
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{sg},"is_channel":true,"status":{status}}}}}"#
        ),
    );
    sg
}

#[test]
fn private_megagroup_owner_gets_every_toggle() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sg = megagroup(&mut session, &seq, &sink, 13, CREATOR, "");
    full_info(
        &mut session,
        &seq,
        &sink,
        sg,
        r#","can_hide_members":true,"has_hidden_members":false,"is_all_history_available":true"#,
    );
    let controls = session.group_admin_controls(ChatId(13));
    let topics = controls.topics.expect("owner sees topics");
    assert!(!topics.enabled);
    assert!(!topics.needs_upgrade);
    assert_eq!(topics.locked, None);
    assert_eq!(controls.history.map(|h| h.visible), Some(true));
    assert!(controls.hide_members);
    assert!(controls.protected_content);
    assert!(controls.reactions);
    // No linked channel: no join-to-send and nothing to unlink.
    assert!(!controls.join_to_send);
    assert_eq!(controls.discussion, None);
    assert!(!controls.upgrade);
}

#[test]
fn topics_lock_with_few_members_or_a_linked_channel() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    // 150 members: below tdesktop's 200.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"G","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"member_count":150,"status":{CREATOR}}}}}"#
        ),
    );
    assert_eq!(
        session
            .group_admin_controls(ChatId(13))
            .topics
            .unwrap()
            .locked,
        Some(TopicsLock::TooFewMembers)
    );
    // A discussion group can't become a forum, whatever its size.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"member_count":900,"status":{CREATOR}}}}}"#
        ),
    );
    full_info(
        &mut session,
        &seq,
        &sink,
        113,
        r#","linked_chat_id":-1007,"member_count":900"#,
    );
    let controls = session.group_admin_controls(ChatId(13));
    assert_eq!(
        controls.topics.unwrap().locked,
        Some(TopicsLock::LinkedDiscussion)
    );
    // tdesktop hides the history row for discussion groups.
    assert_eq!(controls.history, None);
    // The owner can unlink from the group side.
    assert_eq!(
        controls.discussion,
        Some(DiscussionControl::Group {
            channel: ChatId(-1007)
        })
    );
}

#[test]
fn an_enabled_forum_can_always_be_switched_off() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    megagroup(
        &mut session,
        &seq,
        &sink,
        13,
        CREATOR,
        r#","is_forum":true"#,
    );
    let topics = session.group_admin_controls(ChatId(13)).topics.unwrap();
    assert!(topics.enabled);
    assert_eq!(topics.locked, None);
    // tdesktop hides the history row in forums.
    full_info(
        &mut session,
        &seq,
        &sink,
        113,
        r#","is_all_history_available":true"#,
    );
    assert_eq!(session.group_admin_controls(ChatId(13)).history, None);
}

#[test]
fn public_groups_have_no_history_row() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sg = megagroup(
        &mut session,
        &seq,
        &sink,
        13,
        CREATOR,
        r#","usernames":{"@type":"usernames","active_usernames":["pubgroup"],"disabled_usernames":[],"editable_username":"pubgroup"}"#,
    );
    full_info(
        &mut session,
        &seq,
        &sink,
        sg,
        r#","is_all_history_available":true"#,
    );
    assert_eq!(session.group_admin_controls(ChatId(13)).history, None);
}

#[test]
fn restrict_admin_gets_history_and_join_to_send_but_not_owner_toggles() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sg = megagroup(
        &mut session,
        &seq,
        &sink,
        13,
        &admin_status(r#""can_restrict_members":true"#),
        "",
    );
    full_info(
        &mut session,
        &seq,
        &sink,
        sg,
        r#","is_all_history_available":false"#,
    );
    let controls = session.group_admin_controls(ChatId(13));
    assert_eq!(controls.history.map(|h| h.visible), Some(false));
    assert!(controls.topics.is_none(), "topics are owner-only");
    assert!(
        !controls.protected_content,
        "protected content is owner-only"
    );
    assert!(!controls.reactions, "reactions need can_change_info");
    // Linked to a channel: join-to-send needs can_restrict_members.
    full_info(&mut session, &seq, &sink, sg, r#","linked_chat_id":-1009"#);
    assert!(session.group_admin_controls(ChatId(13)).join_to_send);
}

#[test]
fn hide_members_follows_the_server_capability() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sg = megagroup(&mut session, &seq, &sink, 13, CREATOR, "");
    full_info(
        &mut session,
        &seq,
        &sink,
        sg,
        r#","can_hide_members":false"#,
    );
    assert!(!session.group_admin_controls(ChatId(13)).hide_members);
    full_info(
        &mut session,
        &seq,
        &sink,
        sg,
        r#","can_hide_members":true,"has_hidden_members":true"#,
    );
    assert!(session.group_admin_controls(ChatId(13)).hide_members);
    assert!(
        session.groups.supergroup_full_infos[&sg]
            .admin
            .has_hidden_members
    );
}

#[test]
fn plain_members_and_unknown_rights_see_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sg = megagroup(&mut session, &seq, &sink, 13, MEMBER, "");
    full_info(
        &mut session,
        &seq,
        &sink,
        sg,
        r#","can_hide_members":true,"is_all_history_available":true"#,
    );
    let controls = session.group_admin_controls(ChatId(13));
    // `can_hide_members` is the server's own capability flag.
    assert!(controls.hide_members);
    assert!(controls.topics.is_none());
    assert!(controls.history.is_none());
    assert!(!controls.protected_content);
    assert!(!controls.reactions);
    assert!(!controls.join_to_send);
    assert!(session.group_admin_controls(ChatId(999)).is_empty());
}

#[test]
fn channel_admin_links_a_discussion_group_and_sets_reactions() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sg = channel(
        &mut session,
        &seq,
        &sink,
        20,
        &admin_status(r#""can_change_info":true"#),
    );
    let controls = session.group_admin_controls(ChatId(20));
    assert_eq!(
        controls.discussion,
        Some(DiscussionControl::Channel { linked: None })
    );
    assert!(controls.reactions);
    assert!(!controls.protected_content, "owner-only");
    assert!(controls.topics.is_none());
    assert!(controls.history.is_none());
    full_info(&mut session, &seq, &sink, sg, r#","linked_chat_id":-1005"#);
    assert_eq!(
        session.group_admin_controls(ChatId(20)).discussion,
        Some(DiscussionControl::Channel {
            linked: Some(ChatId(-1005))
        })
    );
    // An admin without can_change_info gets nothing.
    let sg2 = channel(
        &mut session,
        &seq,
        &sink,
        21,
        &admin_status(r#""can_post_messages":true"#),
    );
    let _ = sg2;
    let plain = session.group_admin_controls(ChatId(21));
    assert!(plain.discussion.is_none() && !plain.reactions && !plain.usernames);
    // Every administrator may open the boosts list.
    assert!(plain.boosts);
}

fn basic_group(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    status: &str,
    members: i32,
) {
    apply_json(
        session,
        seq,
        sink,
        r#"{"@type":"updateNewChat","chat":{"id":30,"title":"Basic","type":{"@type":"chatTypeBasicGroup","basic_group_id":7},"unread_count":0}}"#,
    );
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":7,"member_count":{members},"status":{status},"is_active":true}}}}"#
        ),
    );
}

#[test]
fn basic_group_owner_upgrades_before_topics_and_history() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    basic_group(&mut session, &seq, &sink, CREATOR, 12);
    let controls = session.group_admin_controls(ChatId(30));
    let topics = controls.topics.unwrap();
    assert!(topics.needs_upgrade);
    assert_eq!(topics.locked, Some(TopicsLock::TooFewMembers));
    assert!(controls.history.unwrap().needs_upgrade);
    assert!(controls.upgrade);
    assert!(controls.protected_content);
    assert!(controls.reactions);
    assert!(!controls.hide_members);
    // Deactivated after the upgrade: nothing left to manage.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":7,"member_count":12,"status":{CREATOR},"is_active":false}}}}"#
        ),
    );
    assert!(session.group_admin_controls(ChatId(30)).is_empty());
}

#[test]
fn basic_group_member_and_change_info_admin() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    basic_group(&mut session, &seq, &sink, MEMBER, 12);
    assert!(session.group_admin_controls(ChatId(30)).is_empty());
    basic_group(
        &mut session,
        &seq,
        &sink,
        &admin_status(r#""can_change_info":true"#),
        12,
    );
    let controls = session.group_admin_controls(ChatId(30));
    assert!(controls.reactions);
    assert!(controls.topics.is_none() && controls.history.is_none() && !controls.upgrade);
}

#[test]
fn supergroup_update_carries_join_to_send() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    megagroup(
        &mut session,
        &seq,
        &sink,
        13,
        CREATOR,
        r#","join_to_send_messages":true"#,
    );
    assert!(session.chat_join_to_send(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"status":{CREATOR}}}}}"#
        ),
    );
    assert!(!session.chat_join_to_send(ChatId(13)));
}

#[test]
fn available_reactions_arrive_with_the_chat_and_update() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"G","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},"unread_count":0,"available_reactions":{"@type":"chatAvailableReactionsSome","reactions":[{"@type":"reactionTypeEmoji","emoji":"👍"}],"max_reaction_count":3}}}"#,
    );
    assert_eq!(
        session.chat_available_reactions(ChatId(13)),
        Some(&ChatAvailableReactions::Some {
            reactions: vec![ReactionType::emoji("👍")],
            max_reaction_count: 3
        })
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAvailableReactions","chat_id":13,"available_reactions":{"@type":"chatAvailableReactionsAll","max_reaction_count":11}}"#,
    );
    assert_eq!(
        session.chat_available_reactions(ChatId(13)),
        Some(&ChatAvailableReactions::All {
            max_reaction_count: 11
        })
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateActiveEmojiReactions","emojis":["👍","🔥"]}"#,
    );
    assert_eq!(session.reaction_picker_emoji(), vec!["👍", "🔥"]);
}

#[test]
fn picker_falls_back_to_the_default_row_and_toggles_reactions() {
    let (session, _sink) = session();
    let picker = session.reaction_picker_emoji();
    assert!(picker.len() >= 6);
    // From "all", picking one reaction narrows to the full list minus it.
    let all = ChatAvailableReactions::All {
        max_reaction_count: 5,
    };
    let narrowed = Session::toggled_reaction(Some(&all), &picker, &picker[0]);
    match &narrowed {
        ChatAvailableReactions::Some {
            reactions,
            max_reaction_count,
        } => {
            assert_eq!(reactions.len(), picker.len() - 1);
            assert_eq!(*max_reaction_count, 5);
        }
        other => panic!("{other:?}"),
    }
    // Removing the last one leaves "none".
    let one = ChatAvailableReactions::Some {
        reactions: vec![ReactionType::emoji("🔥")],
        max_reaction_count: 11,
    };
    assert!(Session::toggled_reaction(Some(&one), &picker, "🔥").is_none());
    assert!(Session::reaction_allowed(&all, "🔥"));
    assert!(!Session::reaction_allowed(
        &ChatAvailableReactions::none(),
        "🔥"
    ));
}

#[test]
fn refused_toggles_roll_back_and_report() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sg = megagroup(
        &mut session,
        &seq,
        &sink,
        13,
        CREATOR,
        r#","join_to_send_messages":false"#,
    );
    full_info(
        &mut session,
        &seq,
        &sink,
        sg,
        r#","is_all_history_available":false,"has_hidden_members":false"#,
    );
    // Optimistic history toggle, then a refusal.
    let previous = session.set_group_toggle(sg, GroupToggle::HistoryVisible, true);
    assert_eq!(previous, Some(false));
    assert!(
        session.groups.supergroup_full_infos[&sg]
            .admin
            .is_all_history_available
    );
    let extra = session.request(
        RequestPurpose::ToggleSupergroupIsAllHistoryAvailable,
        Some(ChatId(13)),
    );
    session.requests.pending_mut(extra).unwrap().rollback = Some(RequestRollback::GroupToggle {
        supergroup_id: sg,
        toggle: GroupToggle::HistoryVisible,
        previous,
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
            extra.0
        ),
    );
    assert!(
        !session.groups.supergroup_full_infos[&sg]
            .admin
            .is_all_history_available
    );
    assert_eq!(
        session.chats_state.chat_action_error.as_deref(),
        Some("could not change the group setting (error 400)")
    );

    // Join-to-send with an unknown previous value goes back to unset.
    let previous = session.set_group_toggle(sg, GroupToggle::JoinToSend, true);
    assert_eq!(previous, Some(false));
    session.restore_group_toggle(sg, GroupToggle::JoinToSend, previous);
    assert!(!session.chat_join_to_send(ChatId(13)));
}

#[test]
fn refused_protected_content_and_reactions_roll_back() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    megagroup(&mut session, &seq, &sink, 13, CREATOR, "");
    session.set_chat_protected(13, true);
    let extra = session.request(
        RequestPurpose::ToggleChatHasProtectedContent,
        Some(ChatId(13)),
    );
    session.requests.pending_mut(extra).unwrap().rollback =
        Some(RequestRollback::ProtectedContent {
            chat_id: 13,
            previous: false,
        });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
            extra.0
        ),
    );
    assert!(!session.chat_has_protected_content(ChatId(13)));

    session.chats_state.chat_available_reactions.insert(
        13,
        ChatAvailableReactions::All {
            max_reaction_count: 11,
        },
    );
    let extra = session.request(RequestPurpose::SetChatAvailableReactions, Some(ChatId(13)));
    session.requests.pending_mut(extra).unwrap().rollback =
        Some(RequestRollback::AvailableReactions {
            chat_id: 13,
            previous: Some(ChatAvailableReactions::All {
                max_reaction_count: 5,
            }),
        });
    session
        .chats_state
        .chat_available_reactions
        .insert(13, ChatAvailableReactions::none());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chat_available_reactions(ChatId(13)),
        Some(&ChatAvailableReactions::All {
            max_reaction_count: 5
        })
    );
}

#[test]
fn upgrade_answer_records_the_new_chat_and_followups_queue() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    basic_group(&mut session, &seq, &sink, CREATOR, 300);
    let extra = session.request(RequestPurpose::UpgradeBasicGroup, Some(ChatId(30)));
    session.queue_admin_followup(
        extra,
        AdminFollowup::EnableTopicsAfterUpgrade {
            old: ChatId(30),
            tabs: true,
        },
    );
    assert!(session.group_admin_busy(ChatId(30)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chat","@extra":"{}","id":-100777,"title":"Basic","type":{{"@type":"chatTypeSupergroup","supergroup_id":777,"is_channel":false}},"unread_count":0}}"#,
            extra.0
        ),
    );
    assert_eq!(session.upgraded_chat_for(ChatId(30)), Some(ChatId(-100777)));
    assert!(!session.group_admin_busy(ChatId(30)));
    // The follow-up is handed out once.
    assert!(session.take_admin_followup(extra).is_some());
    assert!(session.take_admin_followup(extra).is_none());
    assert_eq!(
        session.take_chat_upgrades(),
        vec![(ChatId(30), ChatId(-100777))]
    );
    assert!(session.take_chat_upgrades().is_empty());
}
