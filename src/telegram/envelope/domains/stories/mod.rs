//! TDLib updates and answers for stories, story albums and close friends.
mod parse;

use crate::telegram::envelope::*;
use crate::telegram::envelope_story::ParsedStoryAlbum;
pub(crate) use parse::parse_stories_payload;

/// Payloads for stories, story albums and close friends; wrapped as
/// [`EnvelopePayload::Stories`].
#[derive(Debug, Clone, PartialEq)]
pub enum StoriesPayload {
    /// Phase 9.5: `ReportStoryResult` — `reportStory` response.
    ReportStoryResult(ReportStoryResult),
    /// Phase 9.5: `storyInteractions` — one page of `getStoryInteractions`
    /// results (an own story's viewers).
    StoryInteractions { interactions: StoryInteractionsView },
    /// `storyStatistics` — the `getStoryStatistics` answer.
    StoryStatistics { statistics: StoryStatisticsView },
    /// `publicForwards` — a page of `getStoryPublicForwards`.
    PublicForwards { forwards: PublicForwardsView },
    /// `foundStories` — a page of the `searchPublicStoriesBy*` family.
    FoundStories { found: FoundStoriesView },
    /// Phase 9.5: `updateStoryStealthMode` — stealth-mode state changed.
    UpdateStoryStealthMode {
        active_until_date: i32,
        cooldown_until_date: i32,
    },
    /// Phase 9.1: `updateChatActiveStories` (TDLib 1.8.67,
    /// `schema/td_api.tl:10911`) — the active stories of a chat changed.
    /// The reducer keeps it in `Session::story_tray`; only entries with
    /// `list == Main` are shown in the story tray above the chat list.
    UpdateChatActiveStories {
        active_stories: ChatActiveStoriesView,
    },
    /// Phase 9.1: `chatActiveStories` — the `getChatActiveStories` response
    /// (TDLib 1.8.67, `schema/td_api.tl:13768`); handled like the update.
    ChatActiveStories {
        active_stories: ChatActiveStoriesView,
    },
    /// Phase 9.1: `story` — the `getStory` response (TDLib 1.8.67,
    /// `schema/td_api.tl:13695`); also the `updateStory` update (line
    /// 10895). The reducer keeps it in `Session::stories` keyed by
    /// `(poster_chat_id, id)` for the story viewer.
    Story {
        story: ParsedStory,
        files: Vec<ParsedFile>,
    },
    /// Phase 9.7: `storyAlbums` — the `getChatStoryAlbums` response
    /// (TDLib 1.8.67, `schema/td_api.tl:13850`). The reducer replaces the
    /// chat's album list (`Session::story_albums`), correlated via
    /// `PendingRequest::chat_id`.
    StoryAlbums { albums: Vec<ParsedStoryAlbum> },
    /// Phase 9.7: `storyAlbum` — the `createStoryAlbum` /
    /// `setStoryAlbumName` / `addStoryAlbumStories` /
    /// `removeStoryAlbumStories` / `reorderStoryAlbumStories` response
    /// (schema `td_api.tl:13863` / `13879` / `13887` / `13894` / `13901`;
    /// each returns "the changed album"). The reducer upserts it into
    /// the chat's album list.
    StoryAlbum { album: ParsedStoryAlbum },
    /// Phase 9.7: `stories` — the `getStoryAlbumStories` /
    /// `getChatArchivedStories` / `getChatPostedToChatPageStories`
    /// response (schema `td_api.tl:13857` / `13784` / `13776`).
    /// `pinned_story_ids` is populated only by
    /// `getChatPostedToChatPageStories` with `from_story_id == 0`
    /// (schema comment at `td_api.tl:6747`). Stories are cached in
    /// `Session::stories`; the reducer accumulates the ids per purpose.
    Stories {
        total_count: i32,
        stories: Vec<(ParsedStory, Vec<ParsedFile>)>,
        pinned_story_ids: Vec<i32>,
    },
    /// Phase 9.2: `updateStoryDeleted` (TDLib 1.8.67, `schema/td_api.tl:10898`)
    /// — a story was deleted. The reducer drops it from `Session::stories`
    /// and from the poster's tray entry.
    UpdateStoryDeleted { poster_chat_id: i64, story_id: i32 },
    /// Phase 9.2: `updateStoryPostSucceeded` (TDLib 1.8.67,
    /// `schema/td_api.tl:10901`) — a story posted from another client is
    /// live. The reducer upserts it into `Session::stories` and asks the
    /// driver to refresh the poster's tray (`getChatActiveStories`), so an
    /// own story appears in the tray.
    UpdateStoryPostSucceeded {
        story: ParsedStory,
        files: Vec<ParsedFile>,
        old_story_id: i32,
    },
    /// Phase 9.2: `updateStoryPostFailed` (TDLib 1.8.67,
    /// `schema/td_api.tl:10907`) — a story failed to post. The reducer drops
    /// the failed story from `Session::stories` and the poster's tray entry
    /// (it never went live). Phase 9.3: also feeds the composer's
    /// `StoryPostOutcome::Failed` for our own pending post.
    UpdateStoryPostFailed { story: ParsedStory, error: TdError },
    /// Phase 9.2: `availableReactions` — the `getStoryAvailableReactions`
    /// response (TDLib 1.8.67, `schema/td_api.tl:13802`). The reducer keeps
    /// it in `Session::story_available_reactions` for the viewer picker.
    StoryAvailableReactions {
        reactions: Vec<StoryAvailableReactionView>,
        /// `recent_reactions` / `popular_reactions` and
        /// `allow_custom_emoji` — used by the message reaction picker
        /// (`getMessageAvailableReactions` answers the same type).
        recent: Vec<StoryAvailableReactionView>,
        popular: Vec<StoryAvailableReactionView>,
        allow_custom_emoji: bool,
    },
    /// Phase 9.3: `canPostStory` answer (TDLib 1.8.67,
    /// `schema/td_api.tl:8535` – `td_api.tl:8553`). The reducer honors it
    /// only when the pending purpose is `CheckCanPostStory`.
    CanPostStoryResult { result: CanPostStoryResult },
}
