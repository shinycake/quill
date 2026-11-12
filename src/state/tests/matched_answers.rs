//! Sweep r2-17 (05-062): the fetches below take the answer of the request
//! they wait for. An older request's late answer never lands on a newer ask,
//! whichever order the two answers arrive in.
use super::common::*;
use super::*;
use crate::telegram::EmojiCategory;
use crate::telegram::envelope::{MessagesPayload, StickerItem, StickersPayload};

/// Two asks of one fetch, the second after the first: the requests, as the
/// stage holds them and as the driver's `PendingRequest`s.
struct Asks {
    first: PendingRequest,
    second: PendingRequest,
}

impl Asks {
    fn ids(&self) -> (Option<RequestId>, Option<RequestId>) {
        (Some(self.first.id), Some(self.second.id))
    }
}

/// Ask `purpose` twice (`ask` is the stage's `begin` + `sent`), taking both
/// pending requests the way the reducer reads them on their answers.
fn asked_twice(
    session: &mut Session,
    purpose: RequestPurpose,
    chat: Option<ChatId>,
    mut ask: impl FnMut(&mut Session, RequestId),
) -> Asks {
    let first = session.request(purpose, chat);
    ask(session, first);
    let second = session.request(purpose, chat);
    ask(session, second);
    let (Some(first), Some(second)) = (session.requests.take(first), session.requests.take(second))
    else {
        panic!("both requests are pending");
    };
    Asks { first, second }
}

#[test]
fn an_older_installed_emoji_list_never_answers_a_newer_ask() {
    for newest_first in [false, true] {
        let (mut session, _) = session();
        let asks = asked_twice(
            &mut session,
            RequestPurpose::GetInstalledEmojiSets,
            None,
            |s, id| {
                let list = &mut s.stickers.emoji.lists.installed;
                list.begin_with(id);
            },
        );
        let (first, second) = asks.ids();
        if newest_first {
            session.accept_installed_emoji_sets(second, Vec::new());
            assert!(!session.stickers.emoji.lists.installed.is_loading());
            session.accept_installed_emoji_sets(first, Vec::new());
        } else {
            session.accept_installed_emoji_sets(first, Vec::new());
            assert!(session.stickers.emoji.lists.installed.is_loading());
            session.accept_installed_emoji_sets(second, Vec::new());
        }
        assert!(!session.stickers.emoji.lists.installed.is_loading());
        assert!(!session.stickers.emoji.lists.installed.can_ask());
    }
}

#[test]
fn an_older_emoji_search_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    let asks = asked_twice(
        &mut session,
        RequestPurpose::SearchEmojiSets,
        None,
        |s, id| {
            let list = &mut s.stickers.emoji.lists.search;
            list.begin_with(id);
        },
    );
    let (first, second) = asks.ids();
    session.accept_found_emoji_sets(first, Vec::new());
    assert!(session.stickers.emoji.lists.search.is_loading());
    session.accept_found_emoji_sets(second, Vec::new());
    assert!(!session.stickers.emoji.lists.search.is_loading());
}

#[test]
fn an_older_sticker_category_strip_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    let asks = asked_twice(
        &mut session,
        RequestPurpose::GetStickerCategories,
        None,
        |s, id| {
            let strip = &mut s.stickers.emoji.sticker_categories;
            strip.begin_with(id);
        },
    );
    let (first, second) = asks.ids();
    let purpose = Some(RequestPurpose::GetStickerCategories);
    let strip = |name: &str| {
        vec![EmojiCategory {
            name: name.into(),
            icon: None,
            is_greeting: false,
            source: Default::default(),
        }]
    };
    session.accept_emoji_categories(purpose, second, strip("new"), Vec::new());
    session.accept_emoji_categories(purpose, first, strip("old"), Vec::new());
    let held = session.stickers.emoji.sticker_categories.data();
    assert_eq!(held.map(|rows| rows[0].name.as_str()), Some("new"));
}

#[test]
fn an_older_premium_sticker_preview_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    let asks = asked_twice(
        &mut session,
        RequestPurpose::GetPremiumStickerExamples,
        None,
        |s, id| {
            let list = &mut s.payments.hub.premium_stickers;
            list.begin_with(id);
        },
    );
    let payload = || StickersPayload::Stickers {
        stickers: Vec::<StickerItem>::new(),
        files: Vec::new(),
    };
    session.apply_stickers_payload(payload(), Some(&asks.first), None, 0);
    assert!(session.payments.hub.premium_stickers.is_loading());
    session.apply_stickers_payload(payload(), Some(&asks.second), None, 0);
    assert!(!session.payments.hub.premium_stickers.is_loading());
}

#[test]
fn an_older_default_auto_delete_answer_never_lands_on_a_newer_ask() {
    for newest_first in [false, true] {
        let (mut session, _) = session();
        let asks = asked_twice(
            &mut session,
            RequestPurpose::GetDefaultAutoDelete,
            None,
            |s, id| {
                let fetch = &mut s.settings.default_auto_delete;
                fetch.begin_with(id);
            },
        );
        let answer = |session: &mut Session, pending: &PendingRequest, seconds| {
            session.apply_messages_payload(
                MessagesPayload::MessageAutoDeleteTime { seconds },
                Some(pending),
                None,
                0,
            );
        };
        if newest_first {
            answer(&mut session, &asks.second, 86_400);
            answer(&mut session, &asks.first, 604_800);
        } else {
            answer(&mut session, &asks.first, 604_800);
            assert!(session.settings.default_auto_delete.is_loading());
            answer(&mut session, &asks.second, 86_400);
        }
        assert_eq!(session.settings.default_auto_delete.data(), Some(&86_400));
    }
}

#[test]
fn an_older_pinned_list_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    let chat = ChatId(7);
    let asks = asked_twice(
        &mut session,
        RequestPurpose::GetPinnedMessages,
        Some(chat),
        |s, id| {
            let list = s.messages.pinned_messages.entry(chat.0).or_default();
            list.begin_with(id);
        },
    );
    session.apply_found_chat_messages(Vec::new(), 0, MessageId(0), Some(&asks.first), None, 0);
    assert!(session.messages.pinned_messages[&chat.0].is_loading());
    session.apply_found_chat_messages(Vec::new(), 0, MessageId(0), Some(&asks.second), None, 0);
    assert!(!session.messages.pinned_messages[&chat.0].is_loading());
}

#[test]
fn an_older_chat_preview_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    let chat = ChatId(7);
    let asks = asked_twice(
        &mut session,
        RequestPurpose::GetChatPreview,
        Some(chat),
        |s, id| {
            let peek = s.chat_list.chat_preview_fetch.entry(chat).or_default();
            peek.begin_with(id);
        },
    );
    session.apply_messages(Vec::new(), Some(&asks.first), None, 0);
    assert!(session.chat_list.chat_preview_fetch[&chat].is_loading());
    session.apply_messages(Vec::new(), Some(&asks.second), None, 0);
    assert!(!session.chat_list.chat_preview_fetch[&chat].is_loading());
}

#[test]
fn an_older_post_as_list_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    let asks = asked_twice(
        &mut session,
        RequestPurpose::GetChatsToPostStories,
        None,
        |s, id| {
            s.stories.post_as.begin_with(id);
        },
    );
    session.apply_chats(vec![ChatId(1)], Some(&asks.first));
    assert!(session.stories.post_as.is_loading());
    session.apply_chats(vec![ChatId(2)], Some(&asks.second));
    assert_eq!(session.stories.post_as.data(), Some(&vec![2]));
}

#[test]
fn an_older_folder_leave_list_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    let folder = 5;
    let ask = |s: &mut Session, id: RequestId| {
        let list = s.chat_list.folder_chats_to_leave.entry(folder).or_default();
        list.begin_with(id);
    };
    let first = session.request_for_folder(RequestPurpose::GetChatFolderChatsToLeave, folder);
    ask(&mut session, first);
    let second = session.request_for_folder(RequestPurpose::GetChatFolderChatsToLeave, folder);
    ask(&mut session, second);
    let (Some(first), Some(second)) = (session.requests.take(first), session.requests.take(second))
    else {
        panic!("both requests are pending");
    };
    session.apply_chats(vec![ChatId(1)], Some(&first));
    assert!(session.chat_list.folder_chats_to_leave[&folder].is_loading());
    session.apply_chats(vec![ChatId(2)], Some(&second));
    assert_eq!(
        session.chat_list.folder_chats_to_leave[&folder].data(),
        Some(&vec![2])
    );
}

#[test]
fn an_older_join_as_list_never_answers_a_newer_ask() {
    let (mut session, _) = session();
    session.calls.active_group_call = Some(ActiveGroupCall::fresh(9));
    let sender = |user_id| MessageSender::User { user_id };
    let ask = |s: &mut Session, id: RequestId| {
        if let Some(call) = s.calls.active_group_call.as_mut() {
            call.join_as_options.begin_with(id);
        }
    };
    let first = session.request(RequestPurpose::Other, None);
    ask(&mut session, first);
    let second = session.request(RequestPurpose::Other, None);
    ask(&mut session, second);
    session.set_group_call_join_as_options(9, Some(first), vec![sender(1)]);
    let held = |s: &Session| {
        s.calls
            .active_group_call
            .as_ref()
            .map(|call| call.join_as_options.clone())
    };
    assert!(held(&session).is_some_and(|options| options.is_loading()));
    session.set_group_call_join_as_options(9, Some(second), vec![sender(2)]);
    assert_eq!(
        held(&session).and_then(|options| options.data().cloned()),
        Some(vec![sender(2)])
    );
    // A failure of the older ask changes nothing either.
    session.fail_group_call_join_as_options(9, Some(first), "late");
    assert!(held(&session).is_some_and(|options| options.error().is_none()));
}

#[test]
fn an_ownership_transfer_that_fails_first_leaves_the_older_error_alone() {
    let (mut session, _) = session();
    let first = session.request(RequestPurpose::Other, None);
    let second = session.request(RequestPurpose::Other, None);
    let err = TdError::from_code(400);
    session.begin_ownership_transfer(first, 13, 888, false);
    session.begin_ownership_transfer(second, 13, 888, false);
    // The newest fails first: it ends the transfer; the older's late error
    // finds nothing waiting and changes nothing.
    session.fail_ownership_transfer(Some(second), &err);
    assert!(!session.groups.ownership.transfer.waits_for(Some(second)));
    let after_newest = session.groups.ownership.transfer.clone();
    session.fail_ownership_transfer(Some(first), &err);
    assert_eq!(session.groups.ownership.transfer, after_newest);
}
