//! A request that will never be answered settles through the failure path
//! of its domain: no loading flag survives the sweep or the account.
use super::common::*;
use super::*;
use crate::premium_hub::page_stage::{PageKind, PageStage};
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
    media.open_for(chat);
    let tab = SharedMediaTab::default();
    let state = media.tab_state(tab);
    state.status = SharedMediaTabStatus::ready();
    state.next_from = MessageId(40);
    let (generation, _) = media.begin_fetch_more(tab).expect("page starts");
    session.request(
        RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { tab, generation }),
        Some(chat),
    );
    assert!(!session.media.shared_media.tab_state(tab).can_load_more());

    assert!(sweep(&mut session));

    assert!(session.requests.is_empty());
    let state = session.media.shared_media.tab_state(tab);
    assert!(state.more_error().is_some(), "the list shows its retry row");
    assert!(state.more_askable());
}

#[test]
fn sweeping_an_in_chat_search_page_clears_its_loading_flag() {
    let mut session = ready_session();
    let chat = ChatId(5);
    session.search.chat_search.open_for(chat);
    let generation = session.search.chat_search.generation;
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
    session.settings.privacy_data.network.begin();
    session.request(RequestPurpose::GetNetworkStatistics, None);
    let subscriptions = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { append: false }),
        None,
    );
    session
        .payments
        .star_subscriptions
        .begin_load(subscriptions);
    let history = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append: false }),
        None,
    );
    session.payments.hub.stars.page = PageStage::Loading {
        request: history,
        kind: PageKind::First,
    };
    let gifts = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append: false }),
        None,
    );
    session.payments.hub.received.page = PageStage::Loading {
        request: gifts,
        kind: PageKind::First,
    };
    let audios = session.request_for_user(
        RequestPurpose::Media(MediaPurpose::GetProfileAudios { append: false }),
        9,
    );
    session
        .media
        .playlists
        .entry(9)
        .or_default()
        .list
        .begin(audios, false);
    let user_action = session.request(RequestPurpose::SendMessage, Some(ChatId(5)));

    session.begin_logout();

    assert!(!session.settings.privacy_data.network.is_loading());
    assert!(!session.payments.star_subscriptions.is_loading());
    assert!(!session.payments.hub.stars.page.is_loading());
    assert!(!session.payments.hub.received.page.is_loading());
    assert!(!session.media.playlists[&9].loading());
    assert!(session.requests.is_empty());
    assert!(session.requests.get(user_action).is_none());
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

#[test]
fn sweeping_a_widget_public_data_poll_tells_the_widget_to_poll_again() {
    let mut session = ready_session();
    let (chat, message) = (ChatId(5), MessageId(9));
    session.request_for_message(
        RequestPurpose::Bots(BotsPurpose::GetWidgetPublicData),
        chat,
        message,
    );

    assert!(sweep(&mut session));

    assert!(session.requests.is_empty());
    assert!(
        matches!(
            session.bots.widget_answers.as_slice(),
            [crate::message_widget::WidgetAnswer::PublicData { data: None, .. }]
        ),
        "the answer that returns the widget to Due"
    );
}

/// Sweep 05-193: a lost `canTransferOwnership` answer settles as a failed
/// check, so the next dialog open asks again.
#[test]
fn sweeping_a_lost_transfer_check_lets_the_next_open_ask_again() {
    let mut session = ready_session();
    let request = session.request(RequestPurpose::CanTransferOwnership, None);
    session.begin_ownership_check(request);

    assert!(sweep(&mut session));

    assert!(!session.groups.ownership.check.is_loading());
    assert!(session.groups.ownership.check.error().is_some());
}

/// Sweep 05-202: a lost recommended-channels answer ends its loading state.
#[test]
fn sweeping_lost_recommended_channels_clears_their_loading_flag() {
    let mut session = ready_session();
    session.search.search.tabs.recommended.begin();
    session.request(RequestPurpose::GetRecommendedChats, None);

    assert!(sweep(&mut session));

    assert!(!session.search.search.tabs.recommended.is_loading());
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
    assert!(!session.chats_state.chat_action_error.is_empty());
}

/// An answer dropped as out of order still settles what asked for it: the
/// download is no longer counted as running, so the file can be asked again.
#[test]
fn an_answer_dropped_as_out_of_order_does_not_leave_its_download_running() {
    let (mut session, sink) = session();
    session.connection = ConnectionState::Ready;
    session.last_seq = 50;
    let file = FileId(7);
    let extra = session.request_download(file);
    assert!(session.requests.has_download(file));
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.requests.is_empty());
    assert!(!session.requests.has_download(file));
}

/// Sweep r2-17 (05-011): a swept `getLinkPreview` leaves "Getting link
/// info" for "could not load", so the URL asks again when typed again.
#[test]
fn sweeping_a_link_preview_request_settles_its_chip() {
    let mut session = ready_session();
    let url = "https://example.com/slow";
    let request = session.request(RequestPurpose::GetLinkPreview, None);
    session.messages.composer_previews.asked(url, request);
    assert!(matches!(
        session.messages.composer_previews.get(url).map(|p| &p.fetch),
        Some(LinkPreviewFetch::Asked(id)) if *id == request
    ));

    assert!(sweep(&mut session));

    assert!(matches!(
        session
            .messages
            .composer_previews
            .get(url)
            .map(|p| &p.fetch),
        Some(LinkPreviewFetch::Failed)
    ));
}

/// Sweep r2-17 (05-012): a swept reply-bar fetch is a failed attempt that
/// the bar asks again, not "Loading" for good.
#[test]
fn sweeping_a_reply_bar_fetch_leaves_loading() {
    let mut session = ready_session();
    let (chat, message) = (ChatId(7), MessageId(999));
    let request = session.request(
        RequestPurpose::Messages(MessagesPurpose::GetComposerReplyTarget {
            chat_id: chat,
            message_id: message,
        }),
        Some(chat),
    );
    session.begin_composer_reply_fetch(chat, message, request);

    assert!(sweep(&mut session));

    assert!(matches!(
        session.messages.composer_reply_fetch.get(&(7, 999)),
        Some(ComposerReplyFetch::Failed { attempts: 1, .. })
    ));
}

/// Sweep r2-17 (05-064): a lost request of a panel that draws "Loading"
/// ends in the panel's failed state, which draws Retry.
#[test]
fn sweeping_a_lost_panel_request_ends_its_loading_stage() {
    use crate::state::{
        AdminListFetch, AdminRightsFetch, ChatEventLogFetch, ChatStatisticsFetch, InlineQueryFetch,
        InlineQuerySlot, InviteLinkCountsFetch, OwnerLookup, PollVotersFetch,
    };
    let mut session = ready_session();
    let chat = ChatId(13);
    let asked = |session: &mut Session, purpose| session.request(purpose, Some(chat));
    let admins = asked(&mut session, RequestPurpose::GetChatAdministrators);
    session
        .groups
        .admin_lists
        .insert(13, AdminListFetch::Loading(admins));
    let rights = asked(
        &mut session,
        RequestPurpose::Groups(GroupsPurpose::GetAdminRights { user_id: 5 }),
    );
    session
        .groups
        .admin_rights
        .insert((13, 5), AdminRightsFetch::Loading(rights));
    let counts = asked(&mut session, RequestPurpose::GetChatInviteLinkCounts);
    session
        .groups
        .invite_link_counts
        .insert(13, InviteLinkCountsFetch::Loading(counts));
    let stats = asked(&mut session, RequestPurpose::GetChatStatistics);
    session.groups.chat_statistics.insert(
        13,
        ChatStatisticsFetch::Loading {
            is_dark: false,
            request: stats,
        },
    );
    let log = asked(
        &mut session,
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
    );
    session
        .groups
        .set_event_log_fetch(13, ChatEventLogFetch::Loading);
    session.groups.event_logs.entry(13).or_default().wait.page = Some(log);
    let owner = asked(&mut session, RequestPurpose::GetChatOwnerAfterLeaving);
    session.begin_owner_lookup(13, owner);
    let voters = asked(
        &mut session,
        RequestPurpose::Messages(MessagesPurpose::GetPollVoters {
            chat_id: chat,
            message_id: MessageId(4),
            option_id: 1,
            offset: 0,
        }),
    );
    session
        .messages
        .poll_voters
        .insert((13, 4, 1), PollVotersFetch::Loading(voters));
    let inline = asked(
        &mut session,
        RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
            chat_id: chat,
            bot_user_id: 77,
            first_page: true,
        }),
    );
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: chat,
        bot_user_id: 77,
        query: "cats".into(),
        fetch: InlineQueryFetch::Loading { request: inline },
    });

    assert!(sweep(&mut session));

    assert!(matches!(
        session.groups.admin_lists.get(&13),
        Some(AdminListFetch::Failed(_))
    ));
    assert!(matches!(
        session.groups.admin_rights.get(&(13, 5)),
        Some(AdminRightsFetch::Failed(_))
    ));
    assert!(matches!(
        session.groups.invite_link_counts.get(&13),
        Some(InviteLinkCountsFetch::Failed(_))
    ));
    assert!(matches!(
        session.groups.chat_statistics.get(&13),
        Some(ChatStatisticsFetch::Failed(_))
    ));
    assert!(matches!(
        session.groups.event_log_fetch(13),
        Some(ChatEventLogFetch::Failed(_))
    ));
    assert!(matches!(
        session.groups.ownership.owner_after_leaving.get(&13),
        Some(OwnerLookup::Failed(_))
    ));
    assert!(matches!(
        session.messages.poll_voters.get(&(13, 4, 1)),
        Some(PollVotersFetch::Failed(_))
    ));
    assert!(matches!(
        session.bots.inline_query.as_ref().map(|slot| &slot.fetch),
        Some(InlineQueryFetch::Failed(_))
    ));
}

#[test]
fn a_swept_request_settles_the_box_waiting_on_it() {
    let mut session = ready_session();
    let generation = session.account_generation;
    let request =
        session
            .requests
            .register(generation, RequestPurpose::GetUserFullInfo, None, None);
    session.acks.expect(request);
    assert_eq!(session.acks.take(request), None, "still out");
    assert!(sweep(&mut session));
    assert!(
        matches!(session.acks.take(request), Some(Answer::Failed(_))),
        "the sweep refuses it, so the dialog returns to editing"
    );
}
