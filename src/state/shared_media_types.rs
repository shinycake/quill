//! Shared-media gallery state types.
use super::*;
use crate::lang::{keys, tr};

/// Slice media-shared-gallery: the per-chat shared-media gallery tabs. The
/// message tabs map to one TDLib `searchChatMessages` filter constructor
/// (`schema/td_api.tl` lines 6610-6664); Stories and Gifts load
/// from their own TDLib methods (see `shared_media_profile.rs`). The
/// declaration order is the index into `SharedMediaState::tabs`; the order
/// the tab bar shows them in is `SharedMediaTab::DISPLAY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SharedMediaTab {
    #[default]
    Media,
    Files,
    Music,
    Links,
    Voice,
    Gifs,
    Photos,
    Videos,
    Polls,
    Stories,
    Gifts,
}

impl SharedMediaTab {
    /// Number of tabs.
    pub const COUNT: usize = 11;

    pub const ALL: [SharedMediaTab; Self::COUNT] = [
        SharedMediaTab::Media,
        SharedMediaTab::Files,
        SharedMediaTab::Music,
        SharedMediaTab::Links,
        SharedMediaTab::Voice,
        SharedMediaTab::Gifs,
        SharedMediaTab::Photos,
        SharedMediaTab::Videos,
        SharedMediaTab::Polls,
        SharedMediaTab::Stories,
        SharedMediaTab::Gifts,
    ];

    /// Index into `SharedMediaState::tabs` (declaration order = `ALL` order).
    pub fn index(self) -> usize {
        self as usize
    }

    /// Tab label in the gallery tab bar (tdesktop `lng_media_type_*`).
    pub fn label(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "Media",
            SharedMediaTab::Photos => "Photos",
            SharedMediaTab::Videos => "Videos",
            SharedMediaTab::Files => "Files",
            SharedMediaTab::Music => "Music",
            SharedMediaTab::Links => "Links",
            SharedMediaTab::Polls => "Polls",
            SharedMediaTab::Voice => "Voice",
            SharedMediaTab::Gifs => "GIFs",
            SharedMediaTab::Stories => "Stories",
            SharedMediaTab::Gifts => "Gifts",
        }
    }

    /// `searchChatMessages` `filter` constructor (`schema/td_api.tl:11864`);
    /// `None` for the tabs that do not list messages. Voice also lists
    /// round videos, as tdesktop's `RoundVoiceFile` does.
    pub fn filter_constructor(self) -> Option<&'static str> {
        Some(match self {
            SharedMediaTab::Media => "searchMessagesFilterPhotoAndVideo",
            SharedMediaTab::Photos => "searchMessagesFilterPhoto",
            SharedMediaTab::Videos => "searchMessagesFilterVideo",
            SharedMediaTab::Files => "searchMessagesFilterDocument",
            SharedMediaTab::Music => "searchMessagesFilterAudio",
            SharedMediaTab::Links => "searchMessagesFilterUrl",
            SharedMediaTab::Polls => "searchMessagesFilterPoll",
            SharedMediaTab::Voice => "searchMessagesFilterVoiceAndVideoNote",
            SharedMediaTab::Gifs => "searchMessagesFilterAnimation",
            SharedMediaTab::Stories | SharedMediaTab::Gifts => return None,
        })
    }

    /// Empty-state glyph (TGX's `EmptySmartView` uses 96dp drawables; a
    /// single glyph keeps the one-shared-renderer rule).
    pub fn glyph(self) -> &'static str {
        match self {
            SharedMediaTab::Media | SharedMediaTab::Photos => "🖼",
            SharedMediaTab::Videos => "🎞",
            SharedMediaTab::Files => "📄",
            SharedMediaTab::Music => "🎵",
            SharedMediaTab::Links => "🔗",
            SharedMediaTab::Polls => "📊",
            SharedMediaTab::Voice => "🎙",
            SharedMediaTab::Gifs => "▶",
            SharedMediaTab::Stories => "◎",
            SharedMediaTab::Gifts => "🎁",
        }
    }

    /// TGX empty-state title (`NoMediaToShow`, `NoDocumentsToShow`, … —
    /// `~/workspace/telegram-x/app/src/main/res/values/strings.xml:1601-1609`).
    pub fn empty_title(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "No media to show",
            SharedMediaTab::Photos => "No photos to show",
            SharedMediaTab::Videos => "No videos to show",
            SharedMediaTab::Files => "No documents to show",
            SharedMediaTab::Music => "No music to show",
            SharedMediaTab::Links => "No links to show",
            SharedMediaTab::Polls => "No polls to show",
            SharedMediaTab::Voice => "No voice messages to show",
            SharedMediaTab::Gifs => "No GIFs to show",
            SharedMediaTab::Stories => "No stories to show",
            SharedMediaTab::Gifts => "No gifts to show",
        }
    }

    /// TGX empty-state description (`No*ToShowInChat` / `No*ToShowInChannel`,
    /// strings.xml:1610-1627) — `is_channel` selects the channel wording,
    /// exactly as `EmptySmartView.setMode(mode, isChannel, …)` does.
    pub fn empty_hint(self, is_channel: bool) -> &'static str {
        if is_channel {
            return match self {
                SharedMediaTab::Media => "Published photos and videos\nwill be shown here.",
                SharedMediaTab::Photos => "Published photos will be shown here.",
                SharedMediaTab::Videos => "Published videos will be shown here.",
                SharedMediaTab::Files => "Published documents and files\nwill be shown here.",
                SharedMediaTab::Music => "Published music and audio files\nwill be shown here.",
                SharedMediaTab::Links => "Published links and articles\nwill be shown here.",
                SharedMediaTab::Polls => "Published polls will be shown here.",
                SharedMediaTab::Voice => "Published voice messages\nwill be shown here.",
                SharedMediaTab::Gifs => "Published GIFs will be shown here.",
                SharedMediaTab::Stories => {
                    "Stories posted to this channel's page\nwill be shown here."
                }
                SharedMediaTab::Gifts => "Gifts this channel received\nwill be shown here.",
            };
        }
        match self {
            SharedMediaTab::Media => {
                "Share photos and videos in this chat and\naccess them on any of your devices."
            }
            SharedMediaTab::Photos => {
                "Share photos in this chat and\naccess them on any of your devices."
            }
            SharedMediaTab::Videos => {
                "Share videos in this chat and\naccess them on any of your devices."
            }
            SharedMediaTab::Files => {
                "Share files and documents in this chat and\naccess them on any of your devices."
            }
            SharedMediaTab::Music => {
                "Share music and audio files in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Links => {
                "Share links in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Polls => "Polls sent in this chat\nwill be shown here.",
            SharedMediaTab::Voice => {
                "Share voice messages in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Gifs => {
                "Share GIFs in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Stories => "Stories posted to the profile\nwill be shown here.",
            SharedMediaTab::Gifts => "Gifts shown on the profile\nwill be listed here.",
        }
    }
}

/// Slice media-shared-gallery: per-tab fetch state. The renderer keys "still
/// loading" / "empty" / "failed" off this enum — it never shows the empty
/// state while a fetch is in flight.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SharedMediaTabStatus {
    #[default]
    Idle,
    /// The first page is in flight as `request`; only its answer or error
    /// settles the tab.
    Loading {
        request: RequestId,
    },
    /// The first page is in; `more` is where the next page stands.
    Ready {
        more: NextPage,
    },
    Empty,
    /// The first page failed, with the line the tab shows.
    Failed(String),
}

/// Where the page after a tab's loaded ones stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NextPage {
    /// Nothing is out (there may be no more pages).
    #[default]
    Idle,
    /// An older page is in flight as `request` (the viewer pages toward the
    /// end of the list and asks for more); only its answer or error settles it.
    Loading { request: RequestId },
    /// The older page failed: the list shows a retry row, and nothing asks
    /// again on its own.
    Failed(String),
}

impl SharedMediaTabStatus {
    /// The first page loaded.
    pub fn ready() -> Self {
        Self::Ready {
            more: NextPage::Idle,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

/// One gallery row: enough to label the item and jump to its message.
#[derive(Debug, Clone)]
pub struct SharedMediaItem {
    pub message_id: MessageId,
    pub glyph: &'static str,
    pub label: String,
    /// The message itself for the viewable tabs (Media, GIFs): the media
    /// viewer pages over these (tdesktop `SharedMediaWithLastSlice`).
    /// `None` for tabs the viewer never opens.
    pub message: Option<Box<HistoryMessage>>,
    /// `message.date` (unix seconds); `0` when unknown. Groups the list
    /// into month sections.
    pub date: i32,
    /// What a click on the row does; messages jump to the message.
    pub action: SharedMediaAction,
    /// What the grid tiles of the Stories and Gifts tabs draw.
    pub visual: SharedMediaVisual,
    /// The facts a Files, Music, Voice or Links row prints.
    pub meta: SharedMediaMeta,
    /// The message's own file (`0` for none): what Cancel Download and
    /// Show in Folder act on.
    pub file: FileId,
}

impl SharedMediaItem {
    /// Row label of one message under a tab: caption first, then file name /
    /// URL, then a kind fallback — never empty. The global search tabs
    /// label their hits the same way.
    pub fn label_for(tab: SharedMediaTab, content: &MessageContent) -> String {
        match content {
            MessageContent::Photo(c) => caption_or(&[&c.caption], tr(keys::IN_DLG_PHOTO)),
            MessageContent::Video(c) => {
                caption_or(&[&c.caption, &c.file_name], tr(keys::IN_DLG_VIDEO))
            }
            MessageContent::Document(c) => {
                caption_or(&[&c.caption, &c.file_name], tr(keys::IN_DLG_FILE))
            }
            MessageContent::Audio(c) => caption_or(
                &[&super::shared_media_meta::song_name(
                    &c.performer,
                    &c.title,
                    &c.file_name,
                )],
                tr(keys::Q_OPS_UNKNOWN_TRACK),
            ),
            MessageContent::VoiceNote(c) => caption_or(&[&c.caption], tr(keys::IN_DLG_AUDIO)),
            MessageContent::Animation(c) => {
                caption_or(&[&c.caption, &c.file_name], tr(keys::Q_OPS_GIF))
            }
            MessageContent::VideoNote(_) => tr(keys::IN_DLG_VIDEO_MESSAGE).to_string(),
            MessageContent::Poll(c) => caption_or(&[&c.poll.question], tr(keys::IN_DLG_POLL)),
            MessageContent::Text(c) => {
                let url = c
                    .entities
                    .iter()
                    .find_map(|entity| entity.open_href(&c.text))
                    .unwrap_or_default();
                let preview: String = c.text.chars().take(60).collect();
                caption_or(&[url, &preview], tr(keys::INFO_LINK_LABEL))
            }
            _ => tab.label().to_string(),
        }
    }

    /// Row from the parsed message, labelled by [`Self::label_for`].
    pub fn from_parsed(tab: SharedMediaTab, message: &ParsedMessage) -> Self {
        let glyph = tab.glyph();
        let label = Self::label_for(tab, &message.content);
        let message_copy = matches!(
            tab,
            SharedMediaTab::Media
                | SharedMediaTab::Photos
                | SharedMediaTab::Videos
                | SharedMediaTab::Gifs
        )
        .then(|| Box::new(history_message(message.clone(), false)));
        SharedMediaItem {
            message_id: message.id,
            glyph,
            label,
            message: message_copy,
            date: message.date,
            action: SharedMediaAction::Message,
            visual: SharedMediaVisual::None,
            meta: SharedMediaMeta::from_parsed(message),
            file: primary_file(&message.content),
        }
    }
}

/// The file a row's actions (cancel, show in folder) work on.
fn primary_file(content: &MessageContent) -> FileId {
    let id = match content {
        MessageContent::Photo(c) => c.open_file_id(),
        MessageContent::Video(c) => c.play_file_id(),
        MessageContent::Document(c) => Some(c.file_id),
        MessageContent::Audio(c) => c.play_file_id(),
        MessageContent::VoiceNote(c) => Some(c.file_id),
        MessageContent::Animation(c) => c.play_file_id(),
        MessageContent::VideoNote(c) => c.play_file_id(),
        _ => None,
    };
    id.filter(|id| id.0 != 0).unwrap_or(FileId(0))
}

pub(crate) fn non_empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

pub(crate) fn caption_or(candidates: &[&str], fallback: &str) -> String {
    candidates
        .iter()
        .find(|candidate| !candidate.is_empty())
        .map(|candidate| candidate.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

#[derive(Debug, Clone, Default)]
pub struct SharedMediaTabState {
    pub status: SharedMediaTabStatus,
    /// Newest first, as `searchChatMessages` returns them.
    pub items: Vec<SharedMediaItem>,
    pub total_count: i32,
    /// `foundChatMessages.next_from_message_id` of the last page; `0` once
    /// the oldest message has been reached.
    pub next_from: MessageId,
    /// Local day number the list starts at after a calendar jump; `None`
    /// for the newest media.
    pub anchor_day: Option<i64>,
    /// `receivedGifts.next_offset` of the Gifts tab; empty at the end.
    pub next_offset: String,
    /// A profile tab (Stories, Gifts, Playlist) has more pages.
    pub has_more: bool,
    /// The search query the tab's items answer; empty for the plain list.
    pub query: String,
    /// Which list the tab holds, for the UI to keep a scroll place per list:
    /// it grows whenever the list is cleared or restarted (the gallery closed
    /// and opened again, a search, a calendar jump). Answers are never
    /// matched by it; they are matched by the request in `status`.
    pub list_epoch: u64,
}

impl SharedMediaTabState {
    pub(crate) fn clear(&mut self) {
        self.list_epoch = self.list_epoch.saturating_add(1);
        self.status = SharedMediaTabStatus::Idle;
        self.items.clear();
        self.total_count = 0;
        self.next_from = MessageId(0);
        self.anchor_day = None;
        self.next_offset.clear();
        self.has_more = false;
        self.query.clear();
    }

    /// The failed first page's line; empty otherwise.
    pub fn error(&self) -> &str {
        match &self.status {
            SharedMediaTabStatus::Failed(error) => error,
            _ => "",
        }
    }

    /// An older page is in flight.
    pub fn loading_more(&self) -> bool {
        matches!(
            self.status,
            SharedMediaTabStatus::Ready {
                more: NextPage::Loading { .. }
            }
        )
    }

    /// Whether the first page waits for `request`.
    pub(crate) fn first_page_waits_for(&self, request: RequestId) -> bool {
        matches!(self.status, SharedMediaTabStatus::Loading { request: asked } if asked == request)
    }

    /// Whether an older page waits for `request`.
    pub(crate) fn more_page_waits_for(&self, request: RequestId) -> bool {
        matches!(
            self.status,
            SharedMediaTabStatus::Ready {
                more: NextPage::Loading { request: asked }
            } if asked == request
        )
    }

    /// The failed older page's line, while the list shows its retry row.
    pub fn more_error(&self) -> Option<&str> {
        match &self.status {
            SharedMediaTabStatus::Ready {
                more: NextPage::Failed(error),
            } => Some(error),
            _ => None,
        }
    }

    /// Whether an older page exists and nothing asks for it: the scroll asks
    /// for it on its own. A failed page waits for the retry row.
    pub fn can_load_more(&self) -> bool {
        matches!(
            self.status,
            SharedMediaTabStatus::Ready {
                more: NextPage::Idle
            }
        ) && self.has_next_page()
    }

    /// Whether the tab offers its next page: the scroll asks on its own
    /// when `can_load_more`; after a failure the "Load more" control asks.
    pub fn more_askable(&self) -> bool {
        matches!(
            self.status,
            SharedMediaTabStatus::Ready {
                more: NextPage::Idle | NextPage::Failed(_)
            }
        ) && self.has_next_page()
    }

    fn has_next_page(&self) -> bool {
        self.next_from.0 != 0 || self.has_more
    }

    /// An older page went out as `request` (scrolled to, or the retry row
    /// pressed). `false`, changing nothing, when there is none to ask for or
    /// one is in flight.
    pub(crate) fn start_more(&mut self, request: RequestId) -> bool {
        let askable = self.more_askable();
        if askable {
            self.status = SharedMediaTabStatus::Ready {
                more: NextPage::Loading { request },
            };
        }
        askable
    }

    /// The older page `request` failed. No effect when it is not the one
    /// in flight.
    pub(crate) fn fail_more_page(&mut self, request: RequestId, error: String) {
        if self.more_page_waits_for(request)
            && let SharedMediaTabStatus::Ready { more } = &mut self.status
        {
            *more = NextPage::Failed(error);
        }
    }

    /// An older page could not be sent: the list shows its retry row.
    pub(crate) fn fail_more_unsent(&mut self, error: String) {
        if let SharedMediaTabStatus::Ready { more } = &mut self.status {
            *more = NextPage::Failed(error);
        }
    }

    /// The older page landed or was superseded: the next one may be asked.
    pub(crate) fn end_more(&mut self) {
        if let SharedMediaTabStatus::Ready { more } = &mut self.status {
            *more = NextPage::Idle;
        }
    }
}

/// What the shared-media panel shows: nothing, a chat's gallery, or a
/// user's Playlist page, which may sit over a gallery (Esc and the back
/// button return to it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SharedMediaView {
    #[default]
    Closed,
    Gallery(ChatId),
    Playlist {
        user_id: i64,
        /// The gallery under the page, if one was open.
        under: Option<ChatId>,
    },
}

/// Slice media-shared-gallery: the gallery panel state. Opening for another
/// chat or closing clears every tab, so late `foundChatMessages` answers
/// (their request is no longer the one a tab waits for) drop; a new fetch
/// replaces only its own tab's request.
#[derive(Debug, Clone, Default)]
pub struct SharedMediaState {
    /// The gallery, or the Playlist page over it, that is open.
    pub view: SharedMediaView,
    pub active_tab: SharedMediaTab,
    pub tabs: [SharedMediaTabState; SharedMediaTab::COUNT],
    /// Show Photos and Videos as two tabs instead of Media (tdesktop's
    /// "Expand" / "Collapse" action on the media tab).
    pub media_split: bool,
    /// The ticked rows and tiles, in the order they were ticked.
    pub selection: Vec<SelectedMedia>,
}

impl SharedMediaState {
    /// Whether the gallery is open (for [`Self::chat_id`]).
    pub fn open(&self) -> bool {
        self.chat_id().is_some()
    }

    /// The chat the gallery is open for; `None` while it is closed.
    pub fn chat_id(&self) -> Option<ChatId> {
        match &self.view {
            SharedMediaView::Closed => None,
            SharedMediaView::Gallery(chat) => Some(*chat),
            SharedMediaView::Playlist { under, .. } => *under,
        }
    }

    /// The user whose Playlist page is open instead of the gallery
    /// (tdesktop opens it from the profile's music button).
    pub fn playlist_user(&self) -> Option<i64> {
        match &self.view {
            SharedMediaView::Playlist { user_id, .. } => Some(*user_id),
            _ => None,
        }
    }

    /// The Playlist page of `user_id` opens over whatever is open.
    pub fn open_playlist(&mut self, user_id: i64) {
        self.view = SharedMediaView::Playlist {
            user_id,
            under: self.chat_id(),
        };
    }

    /// The Playlist page closes back to the gallery under it, or to
    /// nothing when it was opened without one.
    pub fn close_playlist(&mut self) {
        if let SharedMediaView::Playlist { under, .. } = &self.view {
            self.view = under.map_or(SharedMediaView::Closed, SharedMediaView::Gallery);
        }
    }

    /// The state of `tab`, `None` only if `tabs` is shorter than the enum.
    pub fn tab_at(&self, tab: SharedMediaTab) -> Option<&SharedMediaTabState> {
        self.tabs.get(tab.index())
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "`tabs` has one slot per `SharedMediaTab` variant and `index()` is the discriminant"
    )]
    pub(crate) fn tab_state(&mut self, tab: SharedMediaTab) -> &mut SharedMediaTabState {
        &mut self.tabs[tab.index()]
    }

    /// Open the gallery for `chat_id`; returns the tab to fetch. Reopening
    /// for the same chat keeps already-fetched tabs.
    pub fn open_for(&mut self, chat_id: ChatId) -> SharedMediaTab {
        self.open_for_tab(chat_id, SharedMediaTab::Media)
    }

    /// Like [`Self::open_for`], opening on `initial` (the profile's main
    /// tab) when the gallery is not already open for this chat.
    pub fn open_for_tab(&mut self, chat_id: ChatId, initial: SharedMediaTab) -> SharedMediaTab {
        if self.chat_id() == Some(chat_id) {
            return self.active_tab;
        }
        self.view = SharedMediaView::Gallery(chat_id);
        // With Photos and Videos split there is no Media tab.
        self.active_tab = match initial {
            SharedMediaTab::Media if self.media_split => SharedMediaTab::Photos,
            other => other,
        };
        self.selection.clear();
        self.clear_all_tabs();
        self.active_tab
    }

    /// Clear every tab: no request in flight is the one any of them waits for.
    fn clear_all_tabs(&mut self) {
        for tab in self.tabs.iter_mut() {
            tab.clear();
        }
    }

    /// Whether `request` is the first page `tab` waits for in the gallery
    /// open for `chat_id`.
    pub(crate) fn accepts_first(
        &self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        request: RequestId,
    ) -> bool {
        self.chat_id() == Some(chat_id)
            && self
                .tab_at(tab)
                .is_some_and(|state| state.first_page_waits_for(request))
    }

    /// Whether `request` is the older page `tab` waits for in the gallery
    /// open for `chat_id`.
    pub(crate) fn accepts_more(
        &self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        request: RequestId,
    ) -> bool {
        self.chat_id() == Some(chat_id)
            && self
                .tab_at(tab)
                .is_some_and(|state| state.more_page_waits_for(request))
    }

    pub fn close(&mut self) {
        self.view = SharedMediaView::Closed;
        self.selection.clear();
        self.clear_all_tabs();
    }

    /// Select a tab; returns `true` when it was never fetched and the caller
    /// must send `searchChatMessages`.
    pub fn select_tab(&mut self, tab: SharedMediaTab) -> bool {
        self.active_tab = tab;
        self.selection.clear();
        self.tabs
            .get(tab.index())
            .is_some_and(|t| t.status.is_idle())
    }

    /// The first page of `tab` went out as `request`: the tab waits for it
    /// alone. A page of this tab still on its way is superseded (its answer
    /// is dropped). Other tabs keep their requests.
    pub fn begin_fetch(&mut self, tab: SharedMediaTab, request: RequestId) {
        let state = self.tab_state(tab);
        state.list_epoch = state.list_epoch.saturating_add(1);
        state.status = SharedMediaTabStatus::Loading { request };
        state.anchor_day = None;
    }

    /// A `foundChatMessages` answer: applied only when the chat matches the
    /// open gallery and `request` is the first page the tab waits for.
    pub fn accept(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        request: RequestId,
        items: Vec<SharedMediaItem>,
        total_count: i32,
        next_from: MessageId,
    ) {
        if !self.accepts_first(chat_id, tab, request) {
            return;
        }
        let state = self.tab_state(tab);
        let items = match state.anchor_day {
            Some(day) => trim_to_day(items, day),
            None => items,
        };
        state.items = items;
        state.total_count = total_count;
        state.next_from = next_from;
        state.status = if state.items.is_empty() {
            SharedMediaTabStatus::Empty
        } else {
            SharedMediaTabStatus::ready()
        };
        // The list was replaced (a search, a calendar jump, a refetch): a
        // ticked row that is not in it any more is not selected.
        if tab == self.active_tab {
            let listed: Vec<MessageId> = self
                .tab_at(tab)
                .map(|state| state.items.iter().map(|item| item.message_id).collect())
                .unwrap_or_default();
            self.selection
                .retain(|selected| listed.contains(&selected.message_id));
        }
    }

    /// Where the next older page of `tab` starts (`from_message_id`), or
    /// `None` when there is nothing more to load or a page is in flight.
    pub fn more_from(&self, tab: SharedMediaTab) -> Option<MessageId> {
        let state = self.tab_at(tab)?;
        state.more_askable().then_some(state.next_from)
    }

    /// The next older page of `tab` went out as `request`: the tab waits for
    /// it alone. `false`, changing nothing, when no page is askable.
    pub fn begin_fetch_more(&mut self, tab: SharedMediaTab, request: RequestId) -> bool {
        self.tab_state(tab).start_more(request)
    }

    /// An older page landed: append the messages not yet listed. A page
    /// with nothing new ends the list.
    pub fn accept_more(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        request: RequestId,
        items: Vec<SharedMediaItem>,
        total_count: i32,
        next_from: MessageId,
    ) {
        if !self.accepts_more(chat_id, tab, request) {
            return;
        }
        let state = self.tab_state(tab);
        state.end_more();
        let before = state.items.len();
        for item in items {
            if !state
                .items
                .iter()
                .any(|old| old.message_id == item.message_id)
            {
                state.items.push(item);
            }
        }
        let grew = state.items.len() > before;
        state.total_count = total_count.max(state.items.len() as i32);
        // A page with nothing new cannot make progress: stop asking.
        state.next_from = if grew { next_from } else { MessageId(0) };
    }

    /// An older page failed: the list shows a retry row, which asks again.
    pub fn fail_more(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        request: RequestId,
        error: String,
    ) {
        if !self.accepts_more(chat_id, tab, request) {
            return;
        }
        self.tab_state(tab).fail_more_page(request, error);
    }

    /// The first page `request` failed.
    pub fn fail(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        request: RequestId,
        error: String,
    ) {
        if !self.accepts_first(chat_id, tab, request) {
            return;
        }
        let state = self.tab_state(tab);
        state.items.clear();
        state.total_count = 0;
        state.status = SharedMediaTabStatus::Failed(error);
        if tab == self.active_tab {
            self.selection.clear();
        }
    }
}
