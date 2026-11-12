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
    assert_eq!(
        ban_step(&Fetch::Loading {
            stale: None,
            request: None
        }),
        BanStep::Wait
    );
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
    let undo = UndoAction::new(7, true, started, None);
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
    UndoAction::new(chat_id, true, Instant::now(), None)
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
        ban_start(Ok(Some(RequestId(9))), None),
        BanStart::Wait(RequestId(9))
    ));
    assert!(
        matches!(
            ban_start(Ok(None), Some(RequestId(4))),
            BanStart::Wait(RequestId(4))
        ),
        "the same ask is already in flight: the wait is for its request"
    );
    assert!(
        matches!(ban_start(Ok(None), None), BanStart::Refused),
        "nothing went: the community is gone or not accessible"
    );
    assert!(matches!(
        ban_start(Err(ConnectSendError::InvalidRequest), None),
        BanStart::Failed(_)
    ));
}

/// Ungroup through a real driver: one toggle goes out and the asked
/// state shows at once, a second press while it is out sends nothing and
/// has no note (the box closes either way, in `confirm_ungroup`), and
/// the wait ends on its own answer, after which a press sends again.
#[test]
fn ungroup_sends_one_toggle_and_a_second_press_sends_nothing() {
    use super::send_ungroup;
    use crate::ui::auth_state::driver_fixture::{driver_with, sent_types};
    const ID: i64 = 42;
    let community = |collapsed: bool| {
        format!(
            r#"{{"@type":"updateCommunity","community":{{"@type":"community","id":{ID},"have_access":true,"name":"R","date":1,"status":{{"@type":"communityMemberStatusMember"}},"permissions":{{"@type":"communityPermissions","can_edit_chat_list":false}},"are_chats_collapsed":{collapsed}}}}}"#
        )
    };
    let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    let (mut driver, recorder) = driver_with(&[ready.to_string(), community(true)]);
    assert!(driver.session.community_chats_collapsed(ID));
    let before = recorder.snapshot().len();

    assert_eq!(send_ungroup(&mut driver, ID), None);
    assert_eq!(
        sent_types(&recorder, before),
        ["toggleCommunityAreChatsCollapsed"]
    );
    assert!(driver.session.community_collapse_in_flight(ID));
    assert!(
        !driver.session.community_chats_collapsed(ID),
        "the asked state shows at once"
    );
    let sent = recorder.snapshot().last().cloned().unwrap_or_default();
    assert!(sent.contains(r#""are_chats_collapsed":false"#), "{sent}");
    let extra = sent
        .split(r#""@extra":""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_string();
    assert!(!extra.is_empty(), "{sent}");

    assert_eq!(send_ungroup(&mut driver, ID), None, "no note");
    assert_eq!(
        sent_types(&recorder, before).len(),
        1,
        "a second press while it is out sends nothing"
    );

    let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
        std::sync::Arc::new(quill::diagnostics::MemorySink::new());
    let seq = std::sync::atomic::AtomicU64::new(1000);
    let ingest = |driver: &mut quill::connect::ConnectDriver<_>, json: &str| {
        if let Some(parsed) = quill::telegram::client::copy_and_parse(json, &seq, &sink) {
            let _ = driver.ingest(parsed);
        }
    };
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{extra}x"}}"#),
    );
    assert!(
        driver.session.community_collapse_in_flight(ID),
        "another request's answer does not end the wait"
    );
    ingest(&mut driver, &community(false));
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#),
    );
    assert!(
        !driver.session.community_collapse_in_flight(ID),
        "the wait ends on its own answer"
    );
    assert!(
        !driver.session.community_chats_collapsed(ID),
        "the stored flag holds once nothing is out"
    );
    assert_eq!(send_ungroup(&mut driver, ID), None);
    assert_eq!(
        sent_types(&recorder, before).len(),
        2,
        "a press after the answer sends a second toggle"
    );
}
