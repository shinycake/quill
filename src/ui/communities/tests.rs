//! The community screens' rules: when a ban warns, how long an answer
//! can be undone, and which section a community chat sits in.
use super::UndoAction;
use super::ban::{BanStep, ban_step};
use super::info::{ChatSection, chat_section};
use super::requests::UNDO_DURATION;
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
    assert_eq!(ban_step(&Fetch::Idle), BanStep::Wait);
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
