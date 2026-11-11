//! A request that will never be answered settles through the failure path
//! of its domain: no loading flag survives the sweep or the account.
use super::common::*;
use super::*;
use std::time::{Duration, Instant};

fn ready_session() -> Session {
    let (mut session, _sink) = session();
    session.connection = ConnectionState::Ready;
    session
}

fn sweep(session: &mut Session) -> bool {
    session.sweep_stale_requests(
        Instant::now() + PENDING_REQUEST_TTL + Duration::from_secs(1),
        PENDING_REQUEST_TTL,
        |_| false,
    )
}

#[test]
fn sweeping_a_shared_media_page_allows_the_next_page_request() {
    let mut session = ready_session();
    let chat = ChatId(5);
    let media = &mut session.media.shared_media;
    media.open = true;
    media.chat_id = Some(chat);
    let tab = SharedMediaTab::default();
    let state = media.tab_state(tab);
    state.status = SharedMediaTabStatus::Ready;
    state.next_from = MessageId(40);
    let (generation, _) = media.begin_fetch_more(tab).expect("page starts");
    session.request(
        RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { tab, generation }),
        Some(chat),
    );
    assert!(!session.media.shared_media.tab_state(tab).can_load_more());

    assert!(sweep(&mut session));

    assert!(session.requests.is_empty());
    assert!(session.media.shared_media.tab_state(tab).can_load_more());
}

#[test]
fn sweeping_an_in_chat_search_page_clears_its_loading_flag() {
    let mut session = ready_session();
    let chat = ChatId(5);
    let generation = session.search.chat_search.generation;
    session.search.chat_search.open = true;
    session.search.chat_search.chat_id = Some(chat);
    session.search.chat_search.begin_more(MessageId(9));
    session.request_chat_search(RequestPurpose::SearchChatMessagesMore, chat, generation);

    assert!(sweep(&mut session));

    assert!(!session.search.chat_search.loading_more());
    assert!(session.search.chat_search.more_failed(), "Retry is offered");
}

#[test]
fn every_sweepable_purpose_leaves_the_registry_and_the_session_alive() {
    let purposes = [
        RequestPurpose::GetHistory,
        RequestPurpose::GetHistoryAround,
        RequestPurpose::GetHistoryNewer,
        RequestPurpose::GetTopicHistory,
        RequestPurpose::GetMessageThreadHistory { message_id: 3 },
        RequestPurpose::SearchMessages,
        RequestPurpose::SearchChatMessages,
        RequestPurpose::SearchChatMessagesMore,
        RequestPurpose::SearchMessagesMore,
        RequestPurpose::SearchPublicPosts,
        RequestPurpose::SearchPublicPostsMore,
        RequestPurpose::SearchPublicMessagesByTag,
        RequestPurpose::GetRecommendedChats,
        RequestPurpose::CanTransferOwnership,
        RequestPurpose::SearchChats,
        RequestPurpose::SearchPublicChats,
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::default(),
            generation: 0,
        },
        RequestPurpose::Media(MediaPurpose::GetSharedMediaMore {
            tab: SharedMediaTab::default(),
            generation: 0,
        }),
        RequestPurpose::GetPinnedMessages,
        RequestPurpose::GetUserFullInfo,
        RequestPurpose::GetSupergroupFullInfo,
        RequestPurpose::GetBasicGroupFullInfo,
        RequestPurpose::GetChatMember,
    ];
    for purpose in purposes {
        assert!(purpose.is_sweepable(), "{purpose:?}");
        let mut session = ready_session();
        session.request(purpose, Some(ChatId(5)));
        assert!(sweep(&mut session), "{purpose:?}");
        assert!(session.requests.is_empty(), "{purpose:?}");
    }
}

#[test]
fn account_loss_resets_the_loading_flags_of_pending_lookups() {
    let mut session = ready_session();
    session.settings.privacy_data.network_loading = true;
    session.request(RequestPurpose::GetNetworkStatistics, None);
    let subscriptions = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append: false }),
        None,
    );
    session.payments.star_subscriptions_request = Some(subscriptions);
    session.payments.hub.tx_loading = true;
    let history = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append: false }),
        None,
    );
    session.payments.hub.tx_request = history.0;
    session.payments.hub.gifts_loading = true;
    let gifts = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append: false }),
        None,
    );
    session.payments.hub.gifts_request = gifts.0;
    session.media.playlists.entry(9).or_default().loading = true;
    session.request_for_user(
        RequestPurpose::Media(MediaPurpose::GetProfileAudios { append: false }),
        9,
    );
    let user_action = session.request(RequestPurpose::SendMessage, Some(ChatId(5)));
    session.stale_history_requests.insert(77);

    session.begin_logout();

    assert!(!session.settings.privacy_data.network_loading);
    assert!(!session.payments.star_subscriptions_loading());
    assert!(!session.payments.hub.tx_loading);
    assert!(!session.payments.hub.gifts_loading);
    assert!(!session.media.playlists[&9].loading);
    assert!(session.requests.is_empty());
    assert!(session.requests.get(user_action).is_none());
    assert!(session.stale_history_requests.is_empty());
}

/// Sweep 05-200: a request dropped by logout settles (its busy marker
/// clears) without leaving a failure note that shows during logout or after
/// the next login.
#[test]
fn logout_does_not_leave_failure_notes_of_dropped_user_actions() {
    let mut session = ready_session();
    session
        .messages
        .message_action_note
        .push("earlier note".into());
    session.request(RequestPurpose::SetChatMessageSender, Some(ChatId(5)));
    session.request(RequestPurpose::SendMessage, Some(ChatId(5)));

    session.begin_logout();

    assert!(session.requests.is_empty());
    assert_eq!(
        session.messages.message_action_note.last(),
        Some("earlier note"),
        "no 'could not change the sender' note from the dropped request"
    );
    assert!(session.messages.flood_notice.is_empty());
}

/// A request lost to its TTL is the user's to hear about; only logout is
/// quiet.
#[test]
fn a_swept_user_action_still_says_it_failed() {
    let mut session = ready_session();
    let id = session.request(RequestPurpose::SetChatMessageSender, Some(ChatId(5)));
    let pending = session.requests.take(id).expect("registered");
    session.settle_lost_request(pending);
    assert!(!session.messages.message_action_note.is_empty());
}

/// Sweep 05-193: a lost `canTransferOwnership` answer settles as a failed
/// check, so the next dialog open asks again.
#[test]
fn sweeping_a_lost_transfer_check_lets_the_next_open_ask_again() {
    let mut session = ready_session();
    session.begin_ownership_check();
    session.request(RequestPurpose::CanTransferOwnership, None);

    assert!(sweep(&mut session));

    assert!(!session.groups.ownership.check_in_flight);
    assert!(session.groups.ownership.check_error.is_some());
}

/// Sweep 05-202: a lost recommended-channels answer ends its loading state.
#[test]
fn sweeping_lost_recommended_channels_clears_their_loading_flag() {
    let mut session = ready_session();
    session.search.search.tabs.recommended_loading = true;
    session.request(RequestPurpose::GetRecommendedChats, None);

    assert!(sweep(&mut session));

    assert!(!session.search.search.tabs.recommended_loading);
}

/// A swept upgrade or history toggle takes its waiting step with it and
/// tells the person (02-074).
#[test]
fn a_swept_request_drops_its_admin_followups_and_says_so() {
    let mut session = ready_session();
    let id = session.request(RequestPurpose::UpgradeBasicGroup, Some(ChatId(5)));
    session.queue_admin_followup(
        id,
        AdminFollowup::HistoryAfterUpgrade {
            old: ChatId(5),
            visible: true,
        },
    );
    let pending = session.requests.take(id).expect("registered");
    session.settle_lost_request(pending);
    assert!(session.take_admin_followup(id).is_none());
    assert!(session.chats_state.chat_action_error.is_some());
}
