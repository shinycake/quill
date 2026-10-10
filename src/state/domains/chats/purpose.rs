//! Request purposes for chat-level look and actions: backgrounds, themes, deep links, action bar.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for chat-level look and actions: backgrounds, themes, deep links, action bar; wrapped as
/// [`RequestPurpose::Chats`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatsPurpose {
    /// `parity:platform-deep-links`: `getDeepLinkInfo` for a launch-time
    /// `t.me` / `tg:` link (schema 1.8.67, line 16189). The answer lands
    /// in `Session::deep_link`; `generation` drops stale answers.
    DeepLinkInfo { generation: u64 },
    /// `getInternalLinkType` for a link the local parsers do not cover
    /// (`parity:deeplink-internal-link-type`). Same slot as
    /// [`Self::DeepLinkInfo`] (`ResolvingInfo`).
    DeepLinkInternalType { generation: u64 },
    /// `parity:platform-deep-links`: deep-link follow-up resolving to a
    /// chat (`searchPublicChat` / `createPrivateChat` / `getChat`). The
    /// `chat` answer is picked up in `apply_update_new_chat` and opens
    /// via `ChatReady`.
    DeepLinkResolve { generation: u64 },
    /// Check an invite without joining; answer becomes a guarded preview.
    DeepLinkCheckInvite { generation: u64 },
    /// `parity:platform-deep-links`: explicitly confirmed `joinChatByInviteLink`
    /// (schema 1.8.67, line 14166); answer is a `chatJoinResult`.
    DeepLinkJoin { generation: u64 },
    /// Slice CL3: `reportChat` (schema 1.8.67, line 15693). Response is
    /// `ReportChatResult`; `Ok` reports the chat, any other variant
    /// surfaces as "more info required" (never success).
    ReportChat,
    /// Batch 8: `removeChatActionBar` (the bar's close button). Response
    /// is `ok`; the bar is dropped optimistically.
    RemoveChatActionBar,
    /// Slice CL2: `createPrivateChat` for Saved Messages (schema
    /// 1.8.67, line 9590 — "Call createPrivateChat with
    /// getOption("my_id") and open the chat"). The `chat` answer opens
    /// the chat.
    CreatePrivateChat,
    /// `getInstalledBackgrounds`. Response is `backgrounds`
    /// (`Session::installed_backgrounds`).
    GetInstalledBackgrounds,
    /// `setDefaultBackground`. Response is `background`
    /// (`Session::default_backgrounds`).
    SetDefaultBackground,
    /// `deleteDefaultBackground`. Response is `ok`.
    DeleteDefaultBackground,
    /// `removeInstalledBackground`. Response is `ok`; the entry leaves the
    /// list optimistically at send time.
    RemoveInstalledBackground,
    /// `setDefaultBackground` with a local image (`inputBackgroundLocal`).
    /// Response is `background`, stored like `SetDefaultBackground`.
    SetDefaultBackgroundLocal,
    /// `searchBackground` for a `bg/` link. Response is `background`
    /// (`Session::searched_background`).
    SearchBackground,
    /// `setChatBackground`. Response is `ok`; `updateChatBackground`
    /// carries the new wallpaper.
    SetChatBackground,
    /// `deleteChatBackground`. Response is `ok`.
    DeleteChatBackground,
    /// `setChatTheme`. Response is `ok`; `updateChatTheme` carries it.
    SetChatTheme,
}

flat_purposes!(Chats(ChatsPurpose) {
    ReportChat,
    RemoveChatActionBar,
    CreatePrivateChat,
    GetInstalledBackgrounds,
    SetDefaultBackground,
    DeleteDefaultBackground,
    RemoveInstalledBackground,
    SetDefaultBackgroundLocal,
    SearchBackground,
    SetChatBackground,
    DeleteChatBackground,
    SetChatTheme,
});
