//! Phase 9.7 story-page state: per-chat album lists, chat-page stories,
//! archive pagination, and the honest mutation-op state (`StoryPageOp`).
//! Split out of `state.rs` per the file-size directive — `state.rs` keeps
//! the `Session` fields, the `RequestPurpose` variants, the reducer match
//! arms, and the `Session` methods.
#![deny(clippy::indexing_slicing)]

use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;

/// Phase 9.7: one chat's `getChatPostedToChatPageStories` state — the
/// loaded story ids (paginated, accumulated), the pinned story ids (from
/// the first page's `pinned_story_ids`, or from a `setChatPinnedStories`
/// `ok`), and the server-side total.
#[derive(Debug, Clone, Default)]
pub struct ChatPageStories {
    pub story_ids: Vec<i32>,
    pub pinned_story_ids: Vec<i32>,
    pub total_count: i32,
}

/// Phase 9.7: one chat's `getChatArchivedStories` state — accumulated
/// story ids, the server total, and the cursor for the next page (the
/// smallest loaded story id; `None` before the first page).
#[derive(Debug, Clone, Default)]
pub struct ArchivedStories {
    pub story_ids: Vec<i32>,
    pub total_count: i32,
    pub next_from_story_id: Option<i32>,
}

/// Phase 9.7: honest album/pin mutation states on the story page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryPageOpState {
    /// A read is in flight as `request` (`getChatStoryAlbums`,
    /// `getStoryAlbumStories`, `getChatArchivedStories`,
    /// `getChatPostedToChatPageStories`).
    Checking { request: RequestId },
    /// A mutation is in flight as `request` (create/reorder/delete/rename
    /// album, add/remove/reorder album stories, set pinned stories).
    Sending { request: RequestId },
    /// `request` was answered.
    Succeeded { request: RequestId },
    /// `request` was refused, or could not be sent.
    Failed { request: RequestId, reason: String },
}

impl StoryPageOpState {
    /// The call this state is about.
    pub fn request(&self) -> RequestId {
        match self {
            Self::Checking { request }
            | Self::Sending { request }
            | Self::Succeeded { request }
            | Self::Failed { request, .. } => *request,
        }
    }

    /// The same call answered with success.
    pub fn succeeded(&self) -> Self {
        Self::Succeeded {
            request: self.request(),
        }
    }

    /// The same call refused with `reason`.
    pub fn failed(&self, reason: String) -> Self {
        Self::Failed {
            request: self.request(),
            reason,
        }
    }
}

/// Which story-page call an op is, so a status line and a "this read is in
/// flight" check match the call and not a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryPageOpKind {
    LoadAlbums,
    LoadAlbumStories,
    CreateAlbum,
    ReorderAlbums,
    DeleteAlbum,
    RenameAlbum,
    AddAlbumStories,
    RemoveAlbumStories,
    ReorderAlbumStories,
    LoadArchive,
    LoadChatPage,
    PinStories,
    LoadCloseFriends,
    SaveCloseFriends,
    MoveStories,
    ToggleProfileStory,
    Other,
}

impl StoryPageOpKind {
    /// The status-line wording.
    pub fn label(self) -> &'static str {
        match self {
            Self::LoadAlbums => "loading albums",
            Self::LoadAlbumStories => "loading album stories",
            Self::CreateAlbum => "creating album",
            Self::ReorderAlbums => "reordering albums",
            Self::DeleteAlbum => "deleting album",
            Self::RenameAlbum => "renaming album",
            Self::AddAlbumStories => "adding stories to album",
            Self::RemoveAlbumStories => "removing stories from album",
            Self::ReorderAlbumStories => "reordering album stories",
            Self::LoadArchive => "loading archived stories",
            Self::LoadChatPage => "loading chat page stories",
            Self::PinStories => "pinning stories",
            Self::LoadCloseFriends => "loading close friends",
            Self::SaveCloseFriends => "saving close friends",
            Self::MoveStories => "moving stories",
            Self::ToggleProfileStory => "updating profile stories",
            Self::Other => "story request",
        }
    }
}

/// Phase 9.7: one story-page call — `kind` names the operation, `state`
/// the honest TDLib round-trip state and the call it tracks (only its
/// answer moves `state`) and `chat_id` the chat page it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryPageOp {
    pub kind: StoryPageOpKind,
    pub state: StoryPageOpState,
    pub chat_id: Option<ChatId>,
}

/// Most story-page ops kept; past it the oldest go, finished ones first.
const STORY_PAGE_OPS_CAP: usize = 64;

/// Every story-page call the session is tracking, one entry per request:
/// a read (paging the archive) and a mutation (deleting an album) never
/// share a slot, so neither one's answer hides the other's.
#[derive(Debug, Clone, Default)]
pub struct StoryPageOps {
    ops: Vec<StoryPageOp>,
}

impl StoryPageOps {
    /// Track `op`. A call tracked under the same request is replaced, and
    /// the finished outcome of the same call on the same page is dropped:
    /// repeating it makes the earlier line stale. Past the cap the oldest
    /// finished op goes; a call still in flight stays (its answer would find
    /// nothing).
    pub fn begin(&mut self, op: StoryPageOp) {
        self.ops.retain(|tracked| {
            tracked.state.request() != op.state.request()
                && !(tracked.is_finished()
                    && tracked.kind == op.kind
                    && tracked.chat_id == op.chat_id)
        });
        if self.ops.len() >= STORY_PAGE_OPS_CAP {
            // A call in flight goes only when the list is twice the cap, a
            // size real use (64 calls out at once) never reaches: calls
            // that never answer still cannot grow it without end.
            let oldest = self
                .ops
                .iter()
                .position(|tracked| tracked.is_finished())
                .or((self.ops.len() >= 2 * STORY_PAGE_OPS_CAP).then_some(0));
            if let Some(oldest) = oldest {
                self.ops.remove(oldest);
            }
        }
        self.ops.push(op);
    }

    pub fn get(&self, request: RequestId) -> Option<&StoryPageOp> {
        self.ops.iter().find(|op| op.state.request() == request)
    }

    pub(crate) fn get_mut(&mut self, request: RequestId) -> Option<&mut StoryPageOp> {
        self.ops
            .iter_mut()
            .find(|op| op.state.request() == request)
    }

    pub(crate) fn remove(&mut self, request: RequestId) -> bool {
        let before = self.ops.len();
        self.ops.retain(|op| op.state.request() != request);
        self.ops.len() != before
    }

    /// Drop the ops that finished: their line was about what was left.
    pub fn settle_finished(&mut self) {
        self.ops.retain(|op| !op.is_finished());
    }

    /// The page of `chat_id` was left: nothing it asked for is tracked
    /// any more, so a late answer changes no status line.
    pub fn forget_chat(&mut self, chat_id: ChatId) {
        self.ops.retain(|op| op.chat_id != Some(chat_id));
    }

    /// The account is going away.
    pub fn clear(&mut self) {
        self.ops.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// A read of `kind` for `chat_id` is in flight.
    pub fn loading(&self, chat_id: ChatId, kind: StoryPageOpKind) -> bool {
        self.ops.iter().any(|op| {
            op.chat_id == Some(chat_id)
                && matches!(op.state, StoryPageOpState::Checking { .. })
                && op.kind == kind
        })
    }

    /// A mutation for `chat_id` is in flight.
    pub fn mutating(&self, chat_id: ChatId) -> bool {
        self.ops
            .iter()
            .any(|op| {
                op.chat_id == Some(chat_id) && matches!(op.state, StoryPageOpState::Sending { .. })
            })
    }

    /// The one line a page shows: a failure first, then a mutation in
    /// flight, a finished one, and a read last; the newest of each kind.
    pub fn status_for(&self, chat_id: Option<ChatId>) -> Option<&StoryPageOp> {
        let rank = |op: &StoryPageOp| match op.state {
            StoryPageOpState::Failed { .. } => 3,
            StoryPageOpState::Sending { .. } => 2,
            StoryPageOpState::Succeeded { .. } => 1,
            StoryPageOpState::Checking { .. } => 0,
        };
        self.ops
            .iter()
            .filter(|op| op.chat_id == chat_id)
            .max_by_key(|op| rank(op))
    }

    /// The newest op of any chat, for tests and the viewer's own lookup.
    pub fn latest(&self) -> Option<&StoryPageOp> {
        self.ops.last()
    }
}

impl StoryPageOp {
    fn is_finished(&self) -> bool {
        matches!(
            self.state,
            StoryPageOpState::Succeeded { .. } | StoryPageOpState::Failed { .. }
        )
    }
}

/// Phase 9.7: the kind of story-page op a request purpose is — shared by
/// the driver's `begin_story_page_op` and the reducer's succeed/fail/clear
/// so both sides name the same op.
pub fn story_page_op_label(purpose: RequestPurpose) -> StoryPageOpKind {
    match purpose {
        RequestPurpose::GetChatStoryAlbums => StoryPageOpKind::LoadAlbums,
        RequestPurpose::GetStoryAlbumStories => StoryPageOpKind::LoadAlbumStories,
        RequestPurpose::CreateStoryAlbum => StoryPageOpKind::CreateAlbum,
        RequestPurpose::ReorderStoryAlbums => StoryPageOpKind::ReorderAlbums,
        RequestPurpose::DeleteStoryAlbum => StoryPageOpKind::DeleteAlbum,
        RequestPurpose::SetStoryAlbumName => StoryPageOpKind::RenameAlbum,
        RequestPurpose::AddStoryAlbumStories => StoryPageOpKind::AddAlbumStories,
        RequestPurpose::RemoveStoryAlbumStories => StoryPageOpKind::RemoveAlbumStories,
        RequestPurpose::ReorderStoryAlbumStories => StoryPageOpKind::ReorderAlbumStories,
        RequestPurpose::GetChatArchivedStories => StoryPageOpKind::LoadArchive,
        RequestPurpose::GetChatPostedToChatPageStories => StoryPageOpKind::LoadChatPage,
        RequestPurpose::SetChatPinnedStories => StoryPageOpKind::PinStories,
        RequestPurpose::GetCloseFriends => StoryPageOpKind::LoadCloseFriends,
        RequestPurpose::SetCloseFriends => StoryPageOpKind::SaveCloseFriends,
        RequestPurpose::SetChatActiveStoriesList => StoryPageOpKind::MoveStories,
        RequestPurpose::ToggleStoryIsPostedToChatPage => StoryPageOpKind::ToggleProfileStory,
        _ => StoryPageOpKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::{AccountKey, ChatId};
    use crate::state::Session;
    use crate::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }

    fn op(kind: StoryPageOpKind, state: StoryPageOpState, chat: i64) -> StoryPageOp {
        StoryPageOp {
            kind,
            state,
            chat_id: Some(ChatId(chat)),
        }
    }

    fn id(request: u64) -> RequestId {
        RequestId(request)
    }

    #[test]
    fn repeating_a_call_drops_its_finished_outcome_and_the_list_stays_bounded() {
        let mut ops = StoryPageOps::default();
        let kind = StoryPageOpKind::DeleteAlbum;
        ops.begin(op(
            kind,
            StoryPageOpState::Failed {
                request: id(1),
                reason: "no".into(),
            },
            11,
        ));
        ops.begin(op(
            StoryPageOpKind::CreateAlbum,
            StoryPageOpState::Succeeded { request: id(2) },
            11,
        ));
        // The same call on the same page again: its old failure is stale.
        ops.begin(op(kind, StoryPageOpState::Sending { request: id(3) }, 11));
        assert!(ops.get(crate::ids::RequestId(1)).is_none());
        assert!(
            ops.get(crate::ids::RequestId(2)).is_some(),
            "another call's line stays"
        );
        // A call still in flight is not dropped for the cap: the first one
        // of a full list keeps its place and its answer finds it.
        for request in 100..100 + STORY_PAGE_OPS_CAP as u64 {
            ops.begin(op(kind, StoryPageOpState::Sending { request: id(request) }, 12));
        }
        assert!(ops.get(id(100)).is_some(), "an op in flight is kept");
        // Calls that never answer still cannot grow the list without end.
        for request in 1000..1300 {
            ops.begin(op(kind, StoryPageOpState::Sending { request: id(request) }, 12));
        }
        assert!(ops.ops.len() <= 2 * STORY_PAGE_OPS_CAP);
        assert!(ops.get(id(1299)).is_some(), "the newest is kept");
    }

    #[test]
    fn story_page_albums_replace_and_stray_ignored() {
        // Phase 9.7: a `storyAlbums` answer with the matching
        // `GetChatStoryAlbums` purpose replaces the chat's album list; a
        // stray `storyAlbums` (no pending request) never flips the UI.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let albums = |extra: &str| {
            format!(
                r#"{{"@type":"storyAlbums","@extra":"{extra}","albums":[{{"@type":"storyAlbum","id":1,"name":"Travel"}},{{"@type":"storyAlbum","id":2,"name":"Food"}}]}}"#
            )
        };
        apply_json(&mut session, &seq, &sink, &albums("no-such-extra"));
        assert!(!session.stories.albums.contains_key(&11));
        let extra = session.request(RequestPurpose::GetChatStoryAlbums, Some(ChatId(11)));
        apply_json(&mut session, &seq, &sink, &albums(&extra.0.to_string()));
        let names: Vec<&str> = session.stories.albums[&11]
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["Travel", "Food"]);
        assert!(session.stories.page_ops.is_empty());
    }

    #[test]
    fn story_page_album_stories_accumulate_and_dedupe() {
        // Phase 9.7: `getStoryAlbumStories` pages accumulate per album and
        // dedupe; the loading op clears when a page lands.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let stories = |extra: &str, ids: &[i32]| {
            let list: Vec<String> = ids
                .iter()
                .map(|id| format!(r#"{{"@type":"story","id":{id},"poster_chat_id":11}}"#))
                .collect();
            format!(
                r#"{{"@type":"stories","@extra":"{extra}","total_count":3,"stories":[{}]}}"#,
                list.join(",")
            )
        };
        let fetch = |session: &mut Session, ids: &[i32]| {
            let extra = session.request_for_story_album(
                RequestPurpose::GetStoryAlbumStories,
                ChatId(11),
                None,
                Some(7),
            );
            session.begin_story_page_check(
                story_page_op_label(RequestPurpose::GetStoryAlbumStories),
                extra,
            );
            (extra, ids.to_vec())
        };
        let (extra, _) = fetch(&mut session, &[301, 302]);
        assert_eq!(
            session.stories.page_ops.latest().map(|op| &op.state),
            Some(&StoryPageOpState::Checking { request: extra })
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &stories(&extra.0.to_string(), &[301, 302]),
        );
        let (extra, _) = fetch(&mut session, &[302, 303]);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &stories(&extra.0.to_string(), &[302, 303]),
        );
        assert_eq!(session.stories.album_stories[&(11, 7)], vec![301, 302, 303]);
        assert!(session.stories.page_ops.is_empty());
    }

    #[test]
    fn story_page_chat_page_pinned_first_page_only() {
        // Phase 9.7: the first chat-page payload sets `pinned_story_ids`
        // (schema 1.8.67:6747 — populated only by the first page); a later
        // page without pins preserves them and appends ids.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let page = |extra: &str, pinned: Option<&str>, ids: &[i32]| {
            let list: Vec<String> = ids
                .iter()
                .map(|id| format!(r#"{{"@type":"story","id":{id},"poster_chat_id":11}}"#))
                .collect();
            let pins = pinned
                .map(|p| format!(r#""pinned_story_ids":[{p}],"#))
                .unwrap_or_default();
            format!(
                r#"{{"@type":"stories","@extra":"{extra}","total_count":3,{pins}"stories":[{}]}}"#,
                list.join(",")
            )
        };
        let extra = session.request(
            RequestPurpose::GetChatPostedToChatPageStories,
            Some(ChatId(11)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &page(&extra.0.to_string(), Some("301"), &[301, 302]),
        );
        let extra = session.request(
            RequestPurpose::GetChatPostedToChatPageStories,
            Some(ChatId(11)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &page(&extra.0.to_string(), None, &[303]),
        );
        let state = &session.stories.chat_page_stories[&11];
        assert_eq!(state.pinned_story_ids, vec![301]);
        assert_eq!(state.story_ids, vec![301, 302, 303]);
    }

    #[test]
    fn story_page_archive_next_cursor() {
        // Phase 9.7: the archive accumulates ids in server order (newest
        // first) and the next page starts from the smallest loaded id
        // (`from_story_id` returns stories below it).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let extra = session.request(RequestPurpose::GetChatArchivedStories, Some(ChatId(11)));
        session.begin_story_page_check(
            story_page_op_label(RequestPurpose::GetChatArchivedStories),
            extra,
        );
        assert_eq!(
            session.stories.page_ops.latest().map(|op| &op.state),
            Some(&StoryPageOpState::Checking { request: extra })
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stories","@extra":"{}","total_count":5,"stories":[{{"@type":"story","id":202,"poster_chat_id":11}},{{"@type":"story","id":201,"poster_chat_id":11}}]}}"#,
                extra.0
            ),
        );
        let archived = &session.stories.archived_stories[&11];
        assert_eq!(archived.story_ids, vec![202, 201]);
        assert_eq!(archived.next_from_story_id, Some(201));
        assert!(session.stories.page_ops.is_empty());
    }

    #[test]
    fn story_page_mutation_op_lifecycle() {
        // Phase 9.7: a mutation shows Sending while in flight, Succeeded on
        // the `storyAlbum` answer, Failed on a TDLib error.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        // An album answer for a chat whose albums were never fetched does
        // not start a one-album list (sweep 05-198).
        let first = session.request(RequestPurpose::CreateStoryAlbum, Some(ChatId(11)));
        session.begin_story_page_op(story_page_op_label(RequestPurpose::CreateStoryAlbum), first);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"storyAlbum","@extra":"{}","id":8,"name":"Early"}}"#,
                first.0
            ),
        );
        assert!(!session.stories.albums.contains_key(&11));
        session.stories.albums.insert(11, Vec::new());
        let extra = session.request(RequestPurpose::CreateStoryAlbum, Some(ChatId(11)));
        session.begin_story_page_op(story_page_op_label(RequestPurpose::CreateStoryAlbum), extra);
        assert_eq!(
            session.stories.page_ops.latest().map(|op| &op.state),
            Some(&StoryPageOpState::Sending { request: extra })
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"storyAlbum","@extra":"{}","id":9,"name":"New"}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.stories.page_ops.latest().map(|op| &op.state),
            Some(StoryPageOpState::Succeeded { .. })
        ));
        assert_eq!(session.stories.albums[&11][0].name, "New");
        let extra = session.request(RequestPurpose::DeleteStoryAlbum, Some(ChatId(11)));
        session.begin_story_page_op(story_page_op_label(RequestPurpose::DeleteStoryAlbum), extra);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"ALBUM_NOT_FOUND"}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.stories.page_ops.latest().map(|op| &op.state),
            Some(StoryPageOpState::Failed { .. })
        ));
    }
}
