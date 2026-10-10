//! Request purposes for global and in-chat search, top chats and date jumps.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for global and in-chat search, top chats and date jumps; wrapped as
/// [`RequestPurpose::Search`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchPurpose {
    SearchChats,
    SearchMessages,
    SearchRecentlyFoundChats,
    /// Phase 7.2: `searchPublicChats` — public username/title lookup across
    /// all public chats (not just known ones). Sent alongside `searchChats`.
    SearchPublicChats,
    AddRecentlyFoundChat,
    /// `searchChatsOnServer`: the server's title/username hits, merged
    /// behind `SearchChats` (never gates the search status).
    SearchChatsOnServer,
    /// `searchPublicPosts` (the "Public posts" scope of a plain query).
    SearchPublicPosts,
    /// `searchPublicMessagesByTag` (the "Public posts" scope of a hashtag).
    SearchPublicMessagesByTag,
    /// `getTopChats(topChatCategoryUsers)`: the "Frequent contacts" strip.
    GetTopChats,
    /// `removeTopChat`; response is `ok`, the entry left the strip
    /// optimistically.
    RemoveTopChat,
    /// `removeRecentlyFoundChat`; response is `ok`, the row left the Recent
    /// list optimistically.
    RemoveRecentlyFoundChat,
    /// `setOption(disable_top_chats)`; the truth arrives as `updateOption`.
    SetTopChatsDisabled,
    SearchChatMessages,
    /// The next older page of the open in-chat search (appended to the
    /// hits; carries the search generation).
    SearchChatMessagesMore,
    /// `searchChatMembers` behind the in-chat "From:" picker.
    SearchFromMembers,
    /// `getChatMessageByDate` of a jump to date.
    GetChatMessageByDate,
    /// `getChatMessageCalendar` page of the calendar box (the box's
    /// generation drops late answers).
    GetChatMessageCalendar {
        generation: u64,
    },
    /// `viewSponsoredChat`. Response is `ok`.
    ViewSponsoredChat,
}

flat_purposes!(Search(SearchPurpose) {
    SearchChats,
    SearchMessages,
    SearchRecentlyFoundChats,
    SearchPublicChats,
    AddRecentlyFoundChat,
    SearchChatsOnServer,
    SearchPublicPosts,
    SearchPublicMessagesByTag,
    GetTopChats,
    RemoveTopChat,
    RemoveRecentlyFoundChat,
    SetTopChatsDisabled,
    SearchChatMessages,
    SearchChatMessagesMore,
    SearchFromMembers,
    GetChatMessageByDate,
    ViewSponsoredChat,
});
