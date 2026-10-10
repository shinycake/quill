//! TDLib updates and answers for the chat list: loading, folders, archive and pins.
mod parse;

use crate::ids::ChatId;
use crate::telegram::envelope::*;
use crate::telegram::requests::ArchiveChatListSettings;
pub(crate) use parse::parse_chat_list_payload;

/// Payloads for the chat list: loading, folders, archive and pins; wrapped as
/// [`EnvelopePayload::ChatList`].
// Sized like the flat enum before the domain split; boxing the big
// variant would change every constructor and pattern.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ChatListPayload {
    UpdateChatPosition(ChatPositionUpdate),
    UpdateChatLastMessage {
        chat_id: ChatId,
        last_message: Option<ParsedMessage>,
        positions: Vec<ChatPositionUpdate>,
    },
    UpdateChatAddedToList {
        chat_id: ChatId,
        list: ChatList,
    },
    UpdateChatRemovedFromList {
        chat_id: ChatId,
        list: ChatList,
    },
    /// `updateUnreadMessageCount`: server-side unread-message totals for a
    /// whole chat list (schema 1.8.67, line 10877).
    UpdateUnreadMessageCount {
        list: ChatList,
        unread_count: i32,
        unread_unmuted_count: i32,
    },
    /// `updateUnreadChatCount`: server-side unread-chat totals for a whole
    /// chat list, marked-as-unread chats included (schema line 10886).
    UpdateUnreadChatCount {
        list: ChatList,
        total_count: i32,
        unread_count: i32,
        unread_unmuted_count: i32,
        marked_as_unread_count: i32,
        marked_as_unread_unmuted_count: i32,
    },
    /// Slice CL1: `updateChatIsMarkedAsUnread` (schema 1.8.67, line
    /// 10588) — the chat was marked as unread or was read.
    UpdateChatIsMarkedAsUnread {
        chat_id: ChatId,
        is_marked_as_unread: bool,
    },
    /// `chats` — `searchChats` / `searchRecentlyFoundChats` / similar.
    Chats {
        total_count: i32,
        chat_ids: Vec<ChatId>,
    },
    /// Slice CL2: `archiveChatListSettings` — `getArchiveChatListSettings`
    /// response (schema 1.8.67, line 3512); stored in
    /// `Session::archive_chat_list_settings` when the pending purpose is
    /// `GetArchiveChatListSettings`.
    ArchiveChatListSettings {
        settings: ArchiveChatListSettings,
    },
    /// `updateChatFolders` (TDLib 1.8.67, `schema/td_api.tl:10606`) — the
    /// full ordered folder list. There is no `getChatFolders` function in
    /// 1.8.67; TDLib pushes this update after authorization and whenever
    /// folders change. `main_chat_list_position` is dropped (folder reorder
    /// always sends position 0); `are_tags_enabled` is kept (parity slice:
    /// folder tags UI).
    UpdateChatFolders {
        folders: Vec<ChatFolderInfo>,
        are_tags_enabled: bool,
    },
    /// Parity slice: `chatFolderInfo` as the response of `createChatFolder`
    /// / `editChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13358` /
    /// `:13361`). The reducer upserts it into `Session::chat_folders`;
    /// `updateChatFolders` stays the source of truth.
    ChatFolderInfo(ChatFolderInfo),
    /// Parity slice: `chatFolder` as the response of `getChatFolder`
    /// (TDLib 1.8.67, `schema/td_api.tl:13355`) — the full editable spec
    /// for the edit dialog prefill / remove-from-folder chain.
    ChatFolder {
        spec: ChatFolderSpec,
    },
    /// `chatFolderInviteLink` — the answer of `createChatFolderInviteLink`
    /// / `editChatFolderInviteLink` (`schema/td_api.tl:13785` / `:13795`).
    ChatFolderInviteLink(ChatFolderInviteLink),
    /// `chatFolderInviteLinks` — the answer of `getChatFolderInviteLinks`
    /// (`schema/td_api.tl:13788`).
    ChatFolderInviteLinks(Vec<ChatFolderInviteLink>),
    /// `premiumLimit` — the answer of `getPremiumLimit`
    /// (`schema/td_api.tl:8559`); `type_name` is the `premiumLimitType*`
    /// constructor.
    PremiumLimit {
        type_name: String,
        default_value: i32,
        premium_value: i32,
    },
    /// `recommendedChatFolders` — the answer of `getRecommendedChatFolders`
    /// (`schema/td_api.tl:13773`).
    RecommendedChatFolders(Vec<RecommendedChatFolder>),
    /// `chatFolderInviteLinkInfo` — the answer of `checkChatFolderInviteLink`
    /// (`schema/td_api.tl:13803`).
    ChatFolderInviteLinkInfo(ChatFolderInviteLinkInfo),
    /// Parity slice: `chatLists` as the response of `getChatListsToAddChat`
    /// (TDLib 1.8.67, `schema/td_api.tl:13347`) — the chat lists a chat may
    /// be added to via `addChatToList`. Correlated to the chat by the
    /// request's `PendingRequest::chat_id`.
    ChatLists {
        lists: Vec<ChatList>,
    },
}
