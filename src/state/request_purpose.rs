//! Request purposes: every in-flight TDLib request, typed.
//!
//! `RequestPurpose` is a thin hub: each variant wraps one domain's enum
//! (`src/state/domains/<domain>/purpose.rs`), so adding a request only
//! edits that domain's file. Unit variants keep their flat spelling
//! through aliases — `RequestPurpose::SendMessage` is
//! `RequestPurpose::Messages(MessagesPurpose::SendMessage)` and works in
//! expressions and patterns alike. Variants with fields are written in
//! full: `RequestPurpose::Calls(CallsPurpose::CreateCall { .. })`, or
//! `CallsPurpose::CreateCall { .. }.into()` when building one.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RequestPurpose {
    /// Requests for sign-in, registration and the TDLib session lifecycle.
    Auth(AuthPurpose),
    /// Requests for bots: inline queries, callback buttons, commands, games and login URLs.
    Bots(BotsPurpose),
    /// Requests for one-to-one calls, group calls and video chats.
    Calls(CallsPurpose),
    /// Requests for the chat list: loading, folders, archive and pins.
    ChatList(ChatListPurpose),
    /// Requests for chat-level look and actions: backgrounds, themes, deep links, action bar.
    Chats(ChatsPurpose),
    /// Requests for groups and channels: members, admin rights, invite links, join requests, boosts, communities.
    Groups(GroupsPurpose),
    /// Requests for files, downloads, uploads and the shared-media gallery.
    Media(MediaPurpose),
    /// Requests for history, sending, editing, reactions, polls, translation and message menus.
    Messages(MessagesPurpose),
    /// Requests for payments, Premium, Stars and gifts.
    Payments(PaymentsPurpose),
    /// Requests for global and in-chat search, top chats and date jumps.
    Search(SearchPurpose),
    /// Requests for settings: privacy, notifications, sessions, storage, proxy and the account.
    Settings(SettingsPurpose),
    /// Requests for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library.
    Stickers(StickersPurpose),
    /// Requests for stories, story albums and close friends.
    Stories(StoriesPurpose),
    /// Requests for forum topics, comment threads and Saved Messages.
    Threads(ThreadsPurpose),
    /// Requests for users, contacts, profiles and secret chats.
    Users(UsersPurpose),
    Other,
    // Kept flat: the screenshot demo fixtures (`src/ui/*_demo.rs`) build
    // these by name with field syntax, which an alias cannot provide.
    // Move each into its domain enum once those fixtures use the domain
    // path (`ThreadsPurpose`, `GroupsPurpose`, `MediaPurpose`, ...).
    /// `getRepliedMessage` for a bubble's reply strip; the `message`
    /// answer lands in `Session::reply_targets`, not in the history.
    GetRepliedMessage {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// Slice media-shared-gallery: one `searchChatMessages` page for a
    /// gallery tab. `generation` is the `SharedMediaState` generation at
    /// send time — late answers drop on mismatch.
    GetSharedMedia {
        tab: SharedMediaTab,
        generation: u64,
    },
    /// `getChatInviteLinks` with another admin's `creator_user_id`.
    GetAdminChatInviteLinks {
        revoked: bool,
    },
    /// `getChatJoinRequests` filtered by one invite link; `append` marks
    /// a later page.
    GetLinkJoinRequests {
        append: bool,
    },
    /// `getChatBoosts`; `append` marks a later page.
    GetChatBoosts {
        append: bool,
    },
    /// `getSavedMessagesTopicHistory` (schema 1.8.67, line 11773): `messages`.
    GetSavedMessagesTopicHistory {
        topic_id: i64,
    },
    /// `getSavedMessagesTags` (schema 1.8.67, line 12856): `savedMessagesTags`.
    GetSavedMessagesTags {
        topic_id: i64,
    },
    /// `searchSavedMessages` (schema 1.8.67, line 11897): `foundChatMessages`.
    SearchSavedMessages {
        topic_id: i64,
    },
    /// `getMessageThread` (schema 1.8.67, line 11566) — resolves the
    /// comment / reply thread of `message_id`. Response is
    /// `messageThreadInfo`; correlated to the origin chat via
    /// `PendingRequest::chat_id`.
    GetMessageThread {
        message_id: i64,
    },
    /// `getMessageThreadHistory` (schema 1.8.67, line 11839) — one page of
    /// the open thread. Response is `messages`; `message_id` identifies the
    /// thread's origin message, correlated to the chat via
    /// `PendingRequest::chat_id`.
    GetMessageThreadHistory {
        message_id: i64,
    },
    /// B10: a chat-id list for a profile panel — groups in common
    /// (`getGroupsInCommon`, pending `user_id`), similar channels
    /// (`getChatSimilarChats`, pending `chat_id`) or the channels that
    /// can be a personal channel (`getSuitablePersonalChats`). Response
    /// is `chats`; ids land in `Session::profile_chat_lists`.
    GetProfileChats(ProfileChatsKind),
}

/// Debug prints the inner variant only (`SendMessage`, not
/// `Messages(SendMessage)`): request names in logs and the user-action
/// classifier read the bare name.
impl std::fmt::Debug for RequestPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth(inner) => inner.fmt(f),
            Self::Bots(inner) => inner.fmt(f),
            Self::Calls(inner) => inner.fmt(f),
            Self::ChatList(inner) => inner.fmt(f),
            Self::Chats(inner) => inner.fmt(f),
            Self::Groups(inner) => inner.fmt(f),
            Self::Media(inner) => inner.fmt(f),
            Self::Messages(inner) => inner.fmt(f),
            Self::Payments(inner) => inner.fmt(f),
            Self::Search(inner) => inner.fmt(f),
            Self::Settings(inner) => inner.fmt(f),
            Self::Stickers(inner) => inner.fmt(f),
            Self::Stories(inner) => inner.fmt(f),
            Self::Threads(inner) => inner.fmt(f),
            Self::Users(inner) => inner.fmt(f),
            Self::GetRepliedMessage {
                chat_id,
                message_id,
            } => f
                .debug_struct("GetRepliedMessage")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .finish(),
            Self::GetSharedMedia { tab, generation } => f
                .debug_struct("GetSharedMedia")
                .field("tab", tab)
                .field("generation", generation)
                .finish(),
            Self::GetAdminChatInviteLinks { revoked } => f
                .debug_struct("GetAdminChatInviteLinks")
                .field("revoked", revoked)
                .finish(),
            Self::GetLinkJoinRequests { append } => f
                .debug_struct("GetLinkJoinRequests")
                .field("append", append)
                .finish(),
            Self::GetChatBoosts { append } => f
                .debug_struct("GetChatBoosts")
                .field("append", append)
                .finish(),
            Self::GetSavedMessagesTopicHistory { topic_id } => f
                .debug_struct("GetSavedMessagesTopicHistory")
                .field("topic_id", topic_id)
                .finish(),
            Self::GetSavedMessagesTags { topic_id } => f
                .debug_struct("GetSavedMessagesTags")
                .field("topic_id", topic_id)
                .finish(),
            Self::SearchSavedMessages { topic_id } => f
                .debug_struct("SearchSavedMessages")
                .field("topic_id", topic_id)
                .finish(),
            Self::GetMessageThread { message_id } => f
                .debug_struct("GetMessageThread")
                .field("message_id", message_id)
                .finish(),
            Self::GetMessageThreadHistory { message_id } => f
                .debug_struct("GetMessageThreadHistory")
                .field("message_id", message_id)
                .finish(),
            Self::GetProfileChats(value) => f.debug_tuple("GetProfileChats").field(value).finish(),
            Self::Other => f.write_str("Other"),
        }
    }
}

macro_rules! purpose_domains {
    ($($domain:ident($ty:ident)),* $(,)?) => {
        $(
            impl From<$ty> for RequestPurpose {
                fn from(purpose: $ty) -> Self {
                    Self::$domain(purpose)
                }
            }
        )*
    };
}

purpose_domains!(
    Auth(AuthPurpose),
    Bots(BotsPurpose),
    Calls(CallsPurpose),
    ChatList(ChatListPurpose),
    Chats(ChatsPurpose),
    Groups(GroupsPurpose),
    Media(MediaPurpose),
    Messages(MessagesPurpose),
    Payments(PaymentsPurpose),
    Search(SearchPurpose),
    Settings(SettingsPurpose),
    Stickers(StickersPurpose),
    Stories(StoriesPurpose),
    Threads(ThreadsPurpose),
    Users(UsersPurpose),
);

/// Flat aliases for a domain's unit variants, declared next to the domain
/// enum: `RequestPurpose::SendMessage` names
/// `RequestPurpose::Messages(MessagesPurpose::SendMessage)`.
macro_rules! flat_purposes {
    ($domain:ident($ty:ident) { $($name:ident),* $(,)? }) => {
        #[allow(non_upper_case_globals)]
        impl $crate::state::RequestPurpose {
            $(
                #[doc = concat!("Alias of [`", stringify!($ty), "::", stringify!($name), "`].")]
                pub const $name: Self = Self::$domain($ty::$name);
            )*
        }
    };
}
pub(crate) use flat_purposes;
