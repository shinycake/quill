//! Request purposes for stories, story albums and close friends.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for stories, story albums and close friends; wrapped as
/// [`RequestPurpose::Stories`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoriesPurpose {
    /// Phase 9.1: `loadActiveStories` (`storyListMain`). The stories
    /// arrive as `updateChatActiveStories` updates; feed the story tray.
    LoadActiveStories,
    /// Phase 9.1: `getChatActiveStories`. Response is `chatActiveStories`;
    /// handled like `updateChatActiveStories`.
    GetChatActiveStories,
    /// Phase 9.1: `getStory`. Response is `story`; the viewer prefetches
    /// every story in the tray entry before opening. `Ok` answers of
    /// `openStory` / `closeStory` need no handling (fire-and-forget).
    GetStory,
    OpenStory,
    CloseStory,
    /// Phase 9.2: `getStoryAvailableReactions`. Response is
    /// `availableReactions`; cached in
    /// `Session::story_available_reactions` for the viewer picker.
    GetStoryAvailableReactions,
    /// Phase 9.2+: `getCustomEmojiStickers` for the story reaction picker.
    /// Response is `stickers`; cached in
    /// `Session::story_custom_emoji_stickers` keyed by sticker id (=
    /// custom emoji id).
    GetStoryCustomEmojiStickers,
    /// Phase 9.2: `setStoryReaction` (set) / removing the chosen reaction.
    /// Responses are `ok`; the new state arrives via `updateStory`.
    SetStoryReaction,
    RemoveStoryReaction,
    /// Phase 9.2: `deleteStory`. Response is `ok`; the deletion lands as
    /// `updateStoryDeleted`.
    DeleteStory,
    /// Phase 9.2: story reply — `sendMessage` with
    /// `inputMessageReplyToStory`. Response is `message`; the normal
    /// message-send updates handle it.
    SendStoryReply,
    /// Phase 9.5: `getStoryInteractions` — an own story's viewers list.
    /// Response is `storyInteractions`; pages accumulate in
    /// `Session::story_viewers` (the previous page's `next_offset`
    /// starts the next request).
    GetStoryInteractions,
    /// `getStoryStatistics` for the story open in the viewer. Response is
    /// `storyStatistics`, stored in `Session::story_insights`.
    GetStoryStatistics,
    /// `getStoryPublicForwards` pages for the same story. Response is
    /// `publicForwards`.
    GetStoryPublicForwards,
    /// `searchPublicStoriesByTag/Location/Venue`. Response is
    /// `foundStories`, stored in `Session::story_search`.
    SearchPublicStories,
    /// Phase 9.5: `reportStory`. Response is `ReportStoryResult`
    /// (`Ok` / `OptionRequired` / `TextRequired`); driven by
    /// `Session::story_report`.
    ReportStory,
    /// Phase 9.5: `activateStoryStealthMode`. Response is `ok`; the new
    /// state lands as `updateStoryStealthMode` in
    /// `Session::story_stealth`.
    ActivateStoryStealthMode,
    /// Phase 9.3: `canPostStory`. Response is a `canPostStoryResult*`;
    /// the reducer stores it in `Session::story_post.eligibility`.
    CheckCanPostStory,
    /// Phase 9.3: `postStory`. Response is a `story` (the pending
    /// story, id = temporary); success/failure lands via
    /// `updateStoryPostSucceeded` / `updateStoryPostFailed`.
    PostStory,
    /// Phase 9.5: `editStory`. Response is `ok`; the edited story
    /// arrives via `updateStory`.
    EditStory,
    /// Phase 9.5: `editStoryCover`. Response is `ok`.
    EditStoryCover,
    /// Phase 9.5: `setStoryPrivacySettings`. Response is `ok`.
    SetStoryPrivacySettings,
    /// Phase 9.5: `getChatsToPostStories`. Response is `chats`;
    /// stored in `Session::story_post_as_chats`.
    GetChatsToPostStories,
    /// Phase 9.7: `getChatStoryAlbums`. Response is `storyAlbums`;
    /// replaces `Session::story_albums[chat_id]`.
    GetChatStoryAlbums,
    /// Phase 9.7: `getStoryAlbumStories`. Response is `stories`;
    /// accumulated into `Session::story_album_stories[(chat_id, album_id)]`.
    GetStoryAlbumStories,
    /// Phase 9.7: `createStoryAlbum`. Response is the new `storyAlbum`;
    /// upserted into `Session::story_albums[chat_id]`.
    CreateStoryAlbum,
    /// Phase 9.7: `reorderStoryAlbums`. Response is `ok`; the sent order
    /// is applied to `Session::story_albums[chat_id]` (correlated via
    /// `PendingRequest::story_ids`, which carries album ids here).
    ReorderStoryAlbums,
    /// Phase 9.7: `deleteStoryAlbum`. Response is `ok`; the album is
    /// dropped from `Session::story_albums[chat_id]` (album id rides on
    /// `PendingRequest::story_album_id`).
    DeleteStoryAlbum,
    /// Phase 9.7: `setStoryAlbumName`. Response is the changed
    /// `storyAlbum`; upserted into `Session::story_albums[chat_id]`.
    SetStoryAlbumName,
    /// Phase 9.7: `addStoryAlbumStories` / `removeStoryAlbumStories` /
    /// `reorderStoryAlbumStories`. Response is the changed `storyAlbum`;
    /// upserted; the album's story id list is refreshed from the next
    /// `getStoryAlbumStories` page.
    AddStoryAlbumStories,
    RemoveStoryAlbumStories,
    ReorderStoryAlbumStories,
    /// Phase 9.7: `getChatArchivedStories`. Response is `stories`;
    /// accumulated into `Session::archived_stories[chat_id]`.
    GetChatArchivedStories,
    /// Phase 9.7: `getChatPostedToChatPageStories`. Response is `stories`
    /// (with `pinned_story_ids` on the first page); replaces
    /// `Session::chat_page_stories[chat_id]`.
    GetChatPostedToChatPageStories,
    /// Phase 9.7: `setChatPinnedStories`. Response is `ok`; the sent
    /// story ids (correlated via `PendingRequest::story_ids`) replace
    /// the chat's `pinned_story_ids`.
    SetChatPinnedStories,
    /// B14: `getCloseFriends`. Response is `users`; the ids land in
    /// `Session::close_friends` (user objects arrive via `updateUser`).
    GetCloseFriends,
    /// B14: `setCloseFriends`. Response is `ok`; the sent ids (staged in
    /// `Session::close_friends_pending`) become `Session::close_friends`.
    SetCloseFriends,
    /// B14: `setChatActiveStoriesList` (hide / unhide a peer's stories).
    /// Response is `ok`; the tray moves via `updateChatActiveStories`.
    SetChatActiveStoriesList,
    /// B14: `toggleStoryIsPostedToChatPage` (post to / remove from
    /// profile). Response is `ok`; the flag arrives via `updateStory`.
    ToggleStoryIsPostedToChatPage,
}

flat_purposes!(Stories(StoriesPurpose) {
    LoadActiveStories,
    GetChatActiveStories,
    GetStory,
    OpenStory,
    CloseStory,
    GetStoryAvailableReactions,
    GetStoryCustomEmojiStickers,
    SetStoryReaction,
    RemoveStoryReaction,
    DeleteStory,
    SendStoryReply,
    GetStoryInteractions,
    GetStoryStatistics,
    GetStoryPublicForwards,
    SearchPublicStories,
    ReportStory,
    ActivateStoryStealthMode,
    CheckCanPostStory,
    PostStory,
    EditStory,
    EditStoryCover,
    SetStoryPrivacySettings,
    GetChatsToPostStories,
    GetChatStoryAlbums,
    GetStoryAlbumStories,
    CreateStoryAlbum,
    ReorderStoryAlbums,
    DeleteStoryAlbum,
    SetStoryAlbumName,
    AddStoryAlbumStories,
    RemoveStoryAlbumStories,
    ReorderStoryAlbumStories,
    GetChatArchivedStories,
    GetChatPostedToChatPageStories,
    SetChatPinnedStories,
    GetCloseFriends,
    SetCloseFriends,
    SetChatActiveStoriesList,
    ToggleStoryIsPostedToChatPage,
});
