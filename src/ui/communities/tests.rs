//! The community screens' rules: when a ban warns, how long an answer
//! can be undone, and which section a community chat sits in.
use super::UndoAction;
use super::ban::{BanStart, BanStep, ban_start, ban_step};
use super::info::{ChatSection, chat_section};
use super::requests::UNDO_DURATION;
use super::{CommunitiesUi, CommunityBox};
use quill::state::fetch::Fetch;
use quill::telegram::envelope::{CommunityMemberChats, ParsedCommunityChat};
use std::time::{Duration, Instant};

fn member_chats(owned: &[i64]) -> CommunityMemberChats {
    CommunityMemberChats {
        owned_chat_ids: owned.to_vec(),
        joined_chat_ids: vec![1, 2, 3],
    }
}

#[test]
fn a_ban_waits_for_the_member_chats_then_warns_only_about_owned_ones() {
    assert_eq!(
        ban_step(&Fetch::Idle),
        BanStep::Cancel,
        "no ask in flight and no answer: the wait ends"
    );
    assert_eq!(ban_step(&Fetch::Loading { stale: None }), BanStep::Wait);
    assert_eq!(ban_step(&Fetch::Ready(member_chats(&[]))), BanStep::BanNow);
    assert_eq!(
        ban_step(&Fetch::Ready(member_chats(&[7, 8]))),
        BanStep::Warn(vec![7, 8])
    );
}

#[test]
fn a_failed_member_chats_ask_bans_without_the_warning() {
    let failed = Fetch::Failed {
        error: "CHANNEL_PRIVATE".into(),
        stale: Some(member_chats(&[7])),
    };
    assert_eq!(ban_step(&failed), BanStep::BanNow);
}

#[test]
fn an_answer_counts_down_three_seconds() {
    let started = Instant::now();
    let undo = UndoAction {
        chat_id: 7,
        approve: true,
        started,
        _timer: None,
    };
    assert_eq!(undo.seconds_left(started), 3);
    assert_eq!(undo.seconds_left(started + Duration::from_millis(1_200)), 2);
    assert_eq!(
        undo.seconds_left(started + (UNDO_DURATION.saturating_sub(Duration::from_millis(1)))),
        1
    );
    assert_eq!(undo.seconds_left(started + UNDO_DURATION), 0);
}

fn chat(can_view_history: bool) -> ParsedCommunityChat {
    ParsedCommunityChat {
        chat_id: 1,
        can_view_history,
        is_hidden: false,
    }
}

#[test]
fn joined_chats_come_first_and_bots_are_never_just_viewable() {
    assert_eq!(chat_section(&chat(false), true, false), ChatSection::Joined);
    assert_eq!(chat_section(&chat(false), true, true), ChatSection::Joined);
    assert_eq!(
        chat_section(&chat(true), false, false),
        ChatSection::Viewable
    );
    assert_eq!(
        chat_section(&chat(true), false, true),
        ChatSection::Requestable
    );
    assert_eq!(
        chat_section(&chat(false), false, false),
        ChatSection::Requestable
    );
}

fn waiting(chat_id: i64) -> UndoAction {
    UndoAction {
        chat_id,
        approve: true,
        started: Instant::now(),
        _timer: None,
    }
}

#[test]
fn replacing_the_requests_list_hands_back_its_waiting_answer() {
    let mut ui = CommunitiesUi {
        open: Some(CommunityBox::Requests {
            community_id: 42,
            undo: Some(waiting(70)),
        }),
        ..Default::default()
    };
    let hidden = CommunityBox::HiddenChat {
        community_id: 42,
        chat_id: 71,
        owner_user_id: 5,
    };
    let sent = ui
        .replace_box(Some(hidden))
        .map(|(id, undo)| (id, undo.chat_id));
    assert_eq!(sent, Some((42, 70)), "the answer goes, not lost");
    assert!(matches!(
        ui.open,
        Some(CommunityBox::HiddenChat { chat_id: 71, .. })
    ));
    assert!(ui.replace_box(None).is_none(), "other boxes hold no answer");
    assert!(ui.open.is_none());

    ui.open = Some(CommunityBox::Requests {
        community_id: 42,
        undo: None,
    });
    assert!(
        ui.replace_box(None).is_none(),
        "an undone answer is not sent"
    );
}

#[test]
fn a_ban_waits_only_for_an_ask_that_will_answer() {
    use quill::connect::ConnectSendError;
    use quill::ids::RequestId;
    assert!(matches!(
        ban_start(Ok(Some(RequestId(9))), false),
        BanStart::Wait
    ));
    assert!(
        matches!(ban_start(Ok(None), true), BanStart::Wait),
        "the same ask is already in flight"
    );
    assert!(
        matches!(ban_start(Ok(None), false), BanStart::Refused),
        "nothing went: the community is gone or not accessible"
    );
    assert!(matches!(
        ban_start(Err(ConnectSendError::InvalidRequest), false),
        BanStart::Failed(_)
    ));
}
