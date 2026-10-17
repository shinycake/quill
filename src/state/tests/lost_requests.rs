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
    session.search.chat_search.next_from_message_id = MessageId(9);
    session.search.chat_search.loading_more = true;
    session.request_chat_search(RequestPurpose::SearchChatMessagesMore, chat, generation);

    assert!(sweep(&mut session));

    assert!(!session.search.chat_search.loading_more);
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
    session.payments.star_subscriptions_loading = true;
    session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append: false }),
        None,
    );
    session.payments.hub.tx_loading = true;
    session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append: false }),
        None,
    );
    session.payments.hub.gifts_loading = true;
    session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append: false }),
        None,
    );
    session.media.playlists.entry(9).or_default().loading = true;
    session.request_for_user(
        RequestPurpose::Media(MediaPurpose::GetProfileAudios { append: false }),
        9,
    );
    let user_action = session.request(RequestPurpose::SendMessage, Some(ChatId(5)));
    session.stale_history_requests.insert(77);

    session.begin_logout();

    assert!(!session.settings.privacy_data.network_loading);
    assert!(!session.payments.star_subscriptions_loading);
    assert!(!session.payments.hub.tx_loading);
    assert!(!session.payments.hub.gifts_loading);
    assert!(!session.media.playlists[&9].loading);
    assert!(session.requests.is_empty());
    assert!(session.requests.get(user_action).is_none());
    assert!(session.stale_history_requests.is_empty());
}
