//! Fix round 2 of sweep bundle r4-05: a refused change never overwrites a
//! fetch in flight, and answers are matched to the request that asked.
use super::common::*;
use super::*;
use crate::state::fetch::Fetch;

fn error_json(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"BROKEN"}}"#,
        extra.0,
    )
}

#[test]
fn a_refused_unblock_leaves_the_blocked_page_in_flight_and_puts_the_user_back() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.blocked_senders = Fetch::Ready(vec![61, 62]);
    session.settings.blocked_total = 5;
    // The user scrolls (a page is out) and unblocks 61 (optimistic).
    let page = session.request(
        RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders { offset: 2 }),
        None,
    );
    session.settings.blocked_senders.begin_with(page);
    let unblock = session.request(
        RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList {
            user_id: 61,
            block: false,
        }),
        None,
    );
    if let Some(list) = session.settings.blocked_senders.data_mut() {
        list.retain(|id| *id != 61);
    }
    session.settings.blocked_total = 4;
    apply_json(&mut session, &seq, &sink, &error_json(unblock));
    assert!(
        session.settings.blocked_senders.waits_for(page),
        "the page keeps its place"
    );
    assert_eq!(session.settings.blocked_total, 5);
    assert_eq!(session.settings.blocked_senders.data(), Some(&vec![62, 61]));
    // The page then lands.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messageSenders","@extra":"{}","total_count":5,"senders":[{{"@type":"messageSenderUser","user_id":70}}]}}"#,
            page.0,
        ),
    );
    assert_eq!(
        session.settings.blocked_senders.data(),
        Some(&vec![62, 61, 70])
    );
}

#[test]
fn a_refused_block_without_a_page_out_reports_and_keeps_the_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.blocked_senders = Fetch::Ready(vec![61, 99]);
    session.settings.blocked_total = 2;
    let block = session.request(
        RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList {
            user_id: 99,
            block: true,
        }),
        None,
    );
    apply_json(&mut session, &seq, &sink, &error_json(block));
    assert_eq!(session.settings.blocked_senders.data(), Some(&vec![61]));
    assert_eq!(session.settings.blocked_total, 1);
    assert!(session.settings.blocked_senders.error().is_some());
}

#[test]
fn a_refused_terminate_leaves_a_session_refetch_in_flight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let refetch = session.request(RequestPurpose::GetActiveSessions, None);
    session.settings.sessions.begin_with(refetch);
    let terminate = session.request(
        RequestPurpose::Settings(SettingsPurpose::TerminateSession { session_id: 4 }),
        None,
    );
    session.settings.sessions_op.begin(terminate);
    apply_json(&mut session, &seq, &sink, &error_json(terminate));
    assert!(session.settings.sessions.waits_for(refetch));
    assert!(!session.settings.sessions_op.is_mutating());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"sessions","@extra":"{}","sessions":[],"inactive_session_ttl_days":180}}"#,
            refetch.0,
        ),
    );
    assert!(session.settings.sessions.data().is_some());
    assert!(session.settings.sessions.error().is_none());

    // With no refetch out, the refusal is the list's error line.
    let again = session.request(
        RequestPurpose::Settings(SettingsPurpose::TerminateSession { session_id: 4 }),
        None,
    );
    session.settings.sessions_op.begin(again);
    apply_json(&mut session, &seq, &sink, &error_json(again));
    assert!(session.settings.sessions.error().is_some());
}

#[test]
fn a_late_mask_set_answer_after_a_change_leaves_no_entry_behind() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let ask = session.request(StickersPurpose::GetMaskSet { set_id: 5 }.into(), None);
    session.stickers.masks.sets.entry(5).or_default().begin_with(ask);
    // A change cleared the sets meanwhile.
    let change = session.request(
        StickersPurpose::ChangeMaskSet {
            set_id: 5,
            installed: false,
            archived: false,
        }
        .into(),
        None,
    );
    session
        .stickers
        .masks
        .begin_change(change, crate::state::MaskChangeTarget::Set(5));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, change.0),
    );
    assert!(session.stickers.masks.sets.is_empty());
    assert!(!session.stickers.masks.change_pending());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stickerSet","@extra":"{}","id":"5","title":"T","name":"t","stickers":[]}}"#,
            ask.0
        ),
    );
    assert!(
        session.stickers.masks.sets.is_empty(),
        "no Idle entry hides the set from the strip"
    );
    assert!(session.stickers.masks.set_wanted(5));
}

#[test]
fn a_refused_mask_change_leaves_a_list_fetch_in_flight_and_ends_the_change() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let list = session.request(StickersPurpose::GetInstalledMaskSets.into(), None);
    session.stickers.masks.installed.begin_with(list);
    let change = session.request(
        StickersPurpose::ChangeMaskSet {
            set_id: 5,
            installed: false,
            archived: false,
        }
        .into(),
        None,
    );
    session
        .stickers
        .masks
        .begin_change(change, crate::state::MaskChangeTarget::Set(5));
    apply_json(&mut session, &seq, &sink, &error_json(change));
    assert!(session.stickers.masks.installed.waits_for(list));
    assert!(!session.stickers.masks.change_pending());
}

#[test]
fn the_account_write_ends_with_its_own_answer_or_error() {
    use crate::state::AccountWrite;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let set = |session: &mut Session, days| {
        let id = session.request(
            RequestPurpose::Settings(SettingsPurpose::SetAccountTtl { days }),
            None,
        );
        session.settings.account_ttl.begin_with(id);
        session.settings.account_write = AccountWrite::SetTtl { request: id };
        id
    };
    let older = session.request(RequestPurpose::DeleteAccount, None);
    let first = set(&mut session, 90);
    // An unrelated request's answer leaves the write out.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, older.0),
    );
    assert!(session.settings.account_write.is_out());
    apply_json(&mut session, &seq, &sink, &error_json(first));
    assert!(!session.settings.account_write.is_out());
    let second = set(&mut session, 180);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, second.0),
    );
    assert!(!session.settings.account_write.is_out());
}

#[test]
fn member_action_answers_leave_the_open_dialogs_list_and_an_old_open_is_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let tag = RequestPurpose::Groups(GroupsPurpose::SetChatMemberTag { user_id: 42 });
    let old = session.request(tag, Some(ChatId(13)));
    session.groups.open_member_actions(13);
    let confirmed = session.request(tag, Some(ChatId(13)));
    let refused = session.request(tag, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, confirmed.0),
    );
    // The old open's refusal lands nowhere; the new one shows once.
    apply_json(&mut session, &seq, &sink, &error_json(old));
    assert_eq!(session.groups.member_action_error(13), None);
    apply_json(&mut session, &seq, &sink, &error_json(refused));
    assert!(session.groups.member_action_error(13).is_some());
    session.groups.dismiss_member_action_error(13);
    // A refusal of an answered request is not a new line.
    apply_json(&mut session, &seq, &sink, &error_json(refused));
    assert_eq!(session.groups.member_action_error(13), None);
}

#[test]
fn an_unpin_outdates_a_pinned_list_fetch_in_flight() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let ask = session.request(RequestPurpose::GetPinnedMessages, Some(ChatId(11)));
    session
        .messages
        .pinned_messages
        .entry(11)
        .or_default()
        .begin_with(ask);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":false}"#,
    );
    let fetch = &session.messages.pinned_messages[&11];
    assert!(fetch.is_outdated(), "asked again, not answered with the old");
    assert!(!fetch.waits_for(ask));
}
