//! The deep-link flow's one owner: `ChatsState` moves it, the UI only asks.
use crate::ids::RequestId;
use crate::state::{ChatsState, DeepLinkState};

fn invite(request: u64) -> DeepLinkState {
    DeepLinkState::InvitePreview {
        hash: "h".into(),
        title: "Club".into(),
        member_count: 3,
        creates_join_request: false,
        is_channel: false,
        request: RequestId(request),
    }
}

#[test]
fn a_request_in_flight_stays_and_a_terminal_step_is_taken_once() {
    let mut chats = ChatsState::new();
    assert!(chats.take_deep_link_step().is_none());
    chats.start_deep_link(
        DeepLinkState::ResolvingInfo {
            request: RequestId(1),
            original: String::new(),
        },
        true,
    );
    assert!(chats.take_deep_link_step().is_none());
    assert!(chats.deep_link_stage().is_some(), "the request stays");
    chats.enter_deep_link_stage(DeepLinkState::ShowText("hi".into()));
    let step = chats.take_deep_link_step().expect("terminal");
    assert!(step.from_agent, "the origin rides with the flow");
    assert!(chats.take_deep_link_step().is_none(), "consumed once");
}

#[test]
fn an_invite_stays_until_its_own_box_cancels_it() {
    let mut chats = ChatsState::new();
    chats.start_deep_link(invite(4), false);
    assert!(chats.take_deep_link_step().is_some());
    assert!(chats.deep_link_stage().is_some(), "the preview stays");
    chats.cancel_deep_link_invite(RequestId(3));
    assert!(chats.deep_link_stage().is_some(), "another flow's box");
    chats.cancel_deep_link_invite(RequestId(4));
    assert!(chats.deep_link_stage().is_none());
}
