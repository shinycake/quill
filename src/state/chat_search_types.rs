//! In-chat search state: the bar's surface, its results and its pages.
use super::*;

/// In-chat search (tdesktop `searchInChat` / ComposeSearch): `searchChatMessages`.
/// What survives closing the bar (the generation that drops late answers,
/// the jump a hit starts, the quiet window of a typed query) lives here;
/// everything the open bar owns is replaced whole with its surface.
#[derive(Debug, Clone)]
pub struct ChatSearchState {
    pub surface: ChatSearchSurface,
    pub generation: u64,
    pub jump: ChatSearchJump,
    /// Bumped by every jump request, so the UI can restart its highlight
    /// fade when the same message is jumped to again.
    pub jump_serial: u64,
    /// The typed query waiting out its quiet window; a new generation
    /// drops it.
    pub(crate) typed: super::debounce::Debounce<String>,
}

/// The in-chat search bar: shut, or open on one chat with everything it
/// holds. Opening and closing replace the value, so nothing of a search
/// outlives its bar.
#[derive(Debug, Clone, Default)]
pub enum ChatSearchSurface {
    #[default]
    Closed,
    Open(OpenChatSearch),
}

/// Where the open bar stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatSearchPhase {
    #[default]
    Idle,
    Searching,
    Ready,
    Empty,
    Failed,
}

impl From<ChatSearchPhase> for SearchStatus {
    fn from(phase: ChatSearchPhase) -> Self {
        match phase {
            ChatSearchPhase::Idle => Self::Idle,
            ChatSearchPhase::Searching => Self::Searching,
            ChatSearchPhase::Ready => Self::Ready,
            ChatSearchPhase::Empty => Self::Empty,
            ChatSearchPhase::Failed => Self::Failed,
        }
    }
}

/// An open in-chat search.
#[derive(Debug, Clone)]
pub struct OpenChatSearch {
    pub chat_id: ChatId,
    pub query: String,
    pub phase: ChatSearchPhase,
    pub hits: Vec<SearchMessageHit>,
    pub total_count: i32,
    /// The older pages: where they continue from, and whether one is in
    /// flight or failed.
    pub paging: ChatSearchPaging,
    pub selected: Option<usize>,
    /// "From: member" (`sender_id` of `searchChatMessages`).
    pub sender: Option<MessageSender>,
    /// The media tab (`filter` of `searchChatMessages`).
    pub media: crate::search_filters::SearchMediaKind,
    /// The member picker replacing the results while choosing a sender.
    pub from_picker: Option<FromPicker>,
}

impl OpenChatSearch {
    fn new(chat_id: ChatId) -> Self {
        Self {
            chat_id,
            query: String::new(),
            phase: ChatSearchPhase::Idle,
            hits: Vec::new(),
            total_count: 0,
            paging: ChatSearchPaging::End,
            selected: None,
            sender: None,
            media: crate::search_filters::SearchMediaKind::All,
            from_picker: None,
        }
    }

    fn clear_results(&mut self) {
        self.hits.clear();
        self.total_count = 0;
        self.paging = ChatSearchPaging::End;
        self.selected = None;
    }
}

/// The older pages of an in-chat search (`foundChatMessages` pages by
/// `next_from_message_id`, 0 = no more). A failed page waits for a Retry
/// instead of ending the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatSearchPaging {
    /// No older page (the server sent cursor 0, or nothing is loaded).
    #[default]
    End,
    /// An older page can be asked from this cursor.
    More { from: MessageId },
    /// The page from this cursor is in flight.
    Loading { from: MessageId },
    /// The page from this cursor failed; asking again is the user's Retry.
    Failed { from: MessageId },
}

impl ChatSearchPaging {
    /// The page after the server's answer: its cursor, 0 ending the list.
    fn after(next_from_message_id: MessageId) -> Self {
        if next_from_message_id.0 == 0 {
            Self::End
        } else {
            Self::More {
                from: next_from_message_id,
            }
        }
    }
}

/// The "From:" picker: group members filtered by the field's text
/// (tdesktop `dialogs_search_from_controllers`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FromPicker {
    /// The text the members were last searched with.
    pub query: String,
    /// The members for `query`: the in-flight `searchChatMembers` holds its
    /// request (other answers are dropped) and the previous list meanwhile.
    pub members: crate::state::fetch::Fetch<Vec<MessageSender>>,
}

impl FromPicker {
    /// The members to list: the answer, or the last one while a new
    /// query loads or after it failed.
    pub fn members(&self) -> &[MessageSender] {
        self.members.data().map_or(&[], Vec::as_slice)
    }

    /// A member search is out.
    pub fn loading(&self) -> bool {
        self.members.is_loading()
    }
}

impl Default for ChatSearchState {
    fn default() -> Self {
        Self {
            surface: ChatSearchSurface::Closed,
            generation: 0,
            jump: ChatSearchJump::None,
            jump_serial: 0,
            typed: Default::default(),
        }
    }
}

impl ChatSearchState {
    /// A new generation: the answers asked before it are dropped, and a
    /// typed query still waiting its quiet window with them.
    fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.typed.cancel();
    }

    /// The open search, when the bar is open.
    pub fn open(&self) -> Option<&OpenChatSearch> {
        match &self.surface {
            ChatSearchSurface::Open(open) => Some(open),
            ChatSearchSurface::Closed => None,
        }
    }

    pub fn open_mut(&mut self) -> Option<&mut OpenChatSearch> {
        match &mut self.surface {
            ChatSearchSurface::Open(open) => Some(open),
            ChatSearchSurface::Closed => None,
        }
    }

    /// The in-chat search bar is open.
    pub fn is_open(&self) -> bool {
        self.open().is_some()
    }

    /// The chat the open bar searches.
    pub fn chat_id(&self) -> Option<ChatId> {
        self.open().map(|open| open.chat_id)
    }

    pub fn query(&self) -> &str {
        self.open().map_or("", |open| open.query.as_str())
    }

    /// What the bar shows (`Closed` when it is not open).
    pub fn status(&self) -> SearchStatus {
        self.open()
            .map_or(SearchStatus::Closed, |open| open.phase.into())
    }

    pub fn hits(&self) -> &[SearchMessageHit] {
        self.open().map_or(&[], |open| open.hits.as_slice())
    }

    pub fn total_count(&self) -> i32 {
        self.open().map_or(0, |open| open.total_count)
    }

    pub fn selected(&self) -> Option<usize> {
        self.open().and_then(|open| open.selected)
    }

    pub fn sender(&self) -> Option<MessageSender> {
        self.open().and_then(|open| open.sender)
    }

    pub fn media(&self) -> crate::search_filters::SearchMediaKind {
        self.open()
            .map_or(crate::search_filters::SearchMediaKind::All, |open| {
                open.media
            })
    }

    pub fn from_picker(&self) -> Option<&FromPicker> {
        self.open().and_then(|open| open.from_picker.as_ref())
    }

    /// Choose (or clear) the "From:" member of the open bar.
    pub fn set_sender(&mut self, sender: Option<MessageSender>) {
        if let Some(open) = self.open_mut() {
            open.sender = sender;
        }
    }

    /// Choose the media tab of the open bar.
    pub fn set_media(&mut self, media: crate::search_filters::SearchMediaKind) {
        if let Some(open) = self.open_mut() {
            open.media = media;
        }
    }

    /// Open or close the member picker of the open bar.
    pub fn set_from_picker(&mut self, picker: Option<FromPicker>) {
        if let Some(open) = self.open_mut() {
            open.from_picker = picker;
        }
    }

    /// A search runs for a non-empty query, a chosen sender or a media tab.
    pub fn has_criteria(&self) -> bool {
        self.open().is_some_and(|open| {
            !open.query.is_empty()
                || open.sender.is_some()
                || open.media != crate::search_filters::SearchMediaKind::All
        })
    }

    /// An older page can be fetched (the results are a partial first page).
    pub fn can_load_more(&self) -> bool {
        self.open().is_some_and(|open| {
            matches!(open.paging, ChatSearchPaging::More { .. })
                && (open.hits.len() as i32) < open.total_count
        })
    }

    /// The cursor of the page to ask: the next one, or the failed one the
    /// user retries.
    pub fn page_to_ask(&self) -> Option<MessageId> {
        match self.open()?.paging {
            ChatSearchPaging::More { from } if self.can_load_more() => Some(from),
            ChatSearchPaging::Failed { from } => Some(from),
            _ => None,
        }
    }

    /// The page from `from` goes out.
    pub(crate) fn begin_more(&mut self, from: MessageId) {
        if let Some(open) = self.open_mut() {
            open.paging = ChatSearchPaging::Loading { from };
        }
    }

    /// The page that was going out never left, or failed: back to waiting
    /// for the user's Retry.
    pub(crate) fn fail_more(&mut self) {
        if let Some(open) = self.open_mut()
            && let ChatSearchPaging::Loading { from } = open.paging
        {
            open.paging = ChatSearchPaging::Failed { from };
        }
    }

    /// An older page is in flight.
    pub fn loading_more(&self) -> bool {
        self.open()
            .is_some_and(|open| matches!(open.paging, ChatSearchPaging::Loading { .. }))
    }

    /// The last older page failed (the Retry strip shows).
    pub fn more_failed(&self) -> bool {
        self.open()
            .is_some_and(|open| matches!(open.paging, ChatSearchPaging::Failed { .. }))
    }

    /// Append an older page (deduped; `selected` stays on its message).
    pub(crate) fn append_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
    ) {
        let Some(open) = self.open_mut() else {
            return;
        };
        for hit in hits {
            if !open.hits.iter().any(|h| h.message_id == hit.message_id) {
                open.hits.push(hit);
            }
        }
        open.total_count = total_count.max(open.hits.len() as i32);
        open.paging = ChatSearchPaging::after(next_from_message_id);
    }

    /// Open the bar on `chat_id`: a fresh search, unless it already is.
    pub fn open_for(&mut self, chat_id: ChatId) {
        if self.chat_id() == Some(chat_id) {
            return;
        }
        self.surface = ChatSearchSurface::Open(OpenChatSearch::new(chat_id));
        self.bump_generation();
        self.jump = ChatSearchJump::None;
    }

    pub fn close(&mut self) {
        self.surface = ChatSearchSurface::Closed;
        self.bump_generation();
        self.jump = ChatSearchJump::None;
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.bump_generation();
        self.jump = ChatSearchJump::None;
        if let Some(open) = self.open_mut() {
            open.query = query.to_string();
            open.phase = ChatSearchPhase::Searching;
            open.clear_results();
        }
        self.generation
    }

    pub fn clear_query(&mut self) {
        self.bump_generation();
        self.jump = ChatSearchJump::None;
        if let Some(open) = self.open_mut() {
            open.query.clear();
            open.phase = ChatSearchPhase::Idle;
            open.clear_results();
        }
    }

    pub(crate) fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.is_open()
            && pending.and_then(|p| p.chat_id) == self.chat_id()
    }

    pub(crate) fn accept_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
        error: bool,
    ) {
        self.jump = ChatSearchJump::None;
        let Some(open) = self.open_mut() else {
            return;
        };
        open.hits = hits;
        open.total_count = total_count;
        open.paging = ChatSearchPaging::after(next_from_message_id);
        open.selected = if open.hits.is_empty() { None } else { Some(0) };
        open.phase = if !open.hits.is_empty() {
            ChatSearchPhase::Ready
        } else if error {
            ChatSearchPhase::Failed
        } else {
            ChatSearchPhase::Empty
        };
    }

    pub fn selected_hit(&self) -> Option<&SearchMessageHit> {
        let open = self.open()?;
        open.selected.and_then(|i| open.hits.get(i))
    }

    /// Newer hit (Unigram `NextExecute`: lower index; results are newest-first).
    pub fn select_newer(&mut self) -> Option<MessageId> {
        let open = self.open_mut()?;
        let index = open.selected?;
        if index == 0 {
            return None;
        }
        open.selected = Some(index - 1);
        open.hits.get(index - 1).map(|hit| hit.message_id)
    }

    /// Older hit (Unigram `PreviousExecute`: higher index).
    pub fn select_older(&mut self) -> Option<MessageId> {
        let open = self.open_mut()?;
        let index = open.selected?;
        if index + 1 >= open.hits.len() {
            return None;
        }
        open.selected = Some(index + 1);
        open.hits.get(index + 1).map(|hit| hit.message_id)
    }

    pub fn select_message(&mut self, message_id: MessageId) -> bool {
        let Some(open) = self.open_mut() else {
            return false;
        };
        let Some(index) = open.hits.iter().position(|hit| hit.message_id == message_id) else {
            return false;
        };
        open.selected = Some(index);
        true
    }

    /// tdesktop `lng_search_messages_n_of_amount`: "N of M" with M the
    /// server's total, not just the pages loaded so far.
    pub fn position_label(&self) -> String {
        match (self.selected(), self.hits().len()) {
            (Some(i), n) if n > 0 => {
                format!("{} of {}", i + 1, (self.total_count().max(0) as usize).max(n))
            }
            _ => String::new(),
        }
    }
}
