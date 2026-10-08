//! Shared-media gallery state types.
use super::*;

/// Slice media-shared-gallery: the per-chat shared-media gallery tabs (README
/// tab order: Media / Files / Music / Links / Voice / GIFs). Each tab maps to
/// one TDLib `searchChatMessages` filter constructor — verified concept-level
/// against every `searchMessagesFilter*` constructor in `schema/td_api.tl`
/// (lines 6275-6326), never a single-name grep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SharedMediaTab {
    #[default]
    Media,
    Files,
    Music,
    Links,
    Voice,
    Gifs,
}

impl SharedMediaTab {
    pub const ALL: [SharedMediaTab; 6] = [
        SharedMediaTab::Media,
        SharedMediaTab::Files,
        SharedMediaTab::Music,
        SharedMediaTab::Links,
        SharedMediaTab::Voice,
        SharedMediaTab::Gifs,
    ];

    /// Index into `SharedMediaState::tabs` (discriminant order = `ALL` order).
    pub fn index(self) -> usize {
        match self {
            SharedMediaTab::Media => 0,
            SharedMediaTab::Files => 1,
            SharedMediaTab::Music => 2,
            SharedMediaTab::Links => 3,
            SharedMediaTab::Voice => 4,
            SharedMediaTab::Gifs => 5,
        }
    }

    /// Tab label in the gallery tab bar.
    pub fn label(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "Media",
            SharedMediaTab::Files => "Files",
            SharedMediaTab::Music => "Music",
            SharedMediaTab::Links => "Links",
            SharedMediaTab::Voice => "Voice",
            SharedMediaTab::Gifs => "GIFs",
        }
    }

    /// `searchChatMessages` `filter` constructor (`schema/td_api.tl:11864`).
    pub fn filter_constructor(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "searchMessagesFilterPhotoAndVideo",
            SharedMediaTab::Files => "searchMessagesFilterDocument",
            SharedMediaTab::Music => "searchMessagesFilterAudio",
            SharedMediaTab::Links => "searchMessagesFilterUrl",
            SharedMediaTab::Voice => "searchMessagesFilterVoiceNote",
            SharedMediaTab::Gifs => "searchMessagesFilterAnimation",
        }
    }

    /// Empty-state glyph (TGX's `EmptySmartView` uses 96dp drawables; a
    /// single glyph keeps the one-shared-renderer rule).
    pub fn glyph(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "🖼",
            SharedMediaTab::Files => "📄",
            SharedMediaTab::Music => "🎵",
            SharedMediaTab::Links => "🔗",
            SharedMediaTab::Voice => "🎙",
            SharedMediaTab::Gifs => "▶",
        }
    }

    /// TGX empty-state title (`NoMediaToShow`, `NoDocumentsToShow`, … —
    /// `~/workspace/telegram-x/app/src/main/res/values/strings.xml:1601-1609`).
    pub fn empty_title(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "No media to show",
            SharedMediaTab::Files => "No documents to show",
            SharedMediaTab::Music => "No music to show",
            SharedMediaTab::Links => "No links to show",
            SharedMediaTab::Voice => "No voice messages to show",
            SharedMediaTab::Gifs => "No GIFs to show",
        }
    }

    /// TGX empty-state description (`No*ToShowInChat` / `No*ToShowInChannel`,
    /// strings.xml:1610-1627) — `is_channel` selects the channel wording,
    /// exactly as `EmptySmartView.setMode(mode, isChannel, …)` does.
    pub fn empty_hint(self, is_channel: bool) -> &'static str {
        if is_channel {
            return match self {
                SharedMediaTab::Media => "Published photos and videos\nwill be shown here.",
                SharedMediaTab::Files => "Published documents and files\nwill be shown here.",
                SharedMediaTab::Music => "Published music and audio files\nwill be shown here.",
                SharedMediaTab::Links => "Published links and articles\nwill be shown here.",
                SharedMediaTab::Voice => "Published voice messages\nwill be shown here.",
                SharedMediaTab::Gifs => "Published GIFs will be shown here.",
            };
        }
        match self {
            SharedMediaTab::Media => {
                "Share photos and videos in this chat and\naccess them on any of your devices."
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
            SharedMediaTab::Voice => {
                "Share voice messages in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Gifs => {
                "Share GIFs in this chat and\naccess them on any device you have."
            }
        }
    }
}

/// Slice media-shared-gallery: per-tab fetch state. The renderer keys "still
/// loading" / "empty" / "failed" off this enum — it never shows the empty
/// state while a fetch is in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SharedMediaTabStatus {
    #[default]
    Idle,
    Loading,
    Ready,
    Empty,
    Failed,
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
}

impl SharedMediaItem {
    /// Row label from the parsed message: caption first, then file name /
    /// URL, then a kind fallback — never empty.
    pub fn from_parsed(tab: SharedMediaTab, message: &ParsedMessage) -> Self {
        let glyph = tab.glyph();
        let label = match &message.content {
            MessageContent::Photo(c) => caption_or(&[&c.caption], "Photo"),
            MessageContent::Video(c) => caption_or(&[&c.caption, &c.file_name], "Video"),
            MessageContent::Document(c) => caption_or(&[&c.caption, &c.file_name], "File"),
            MessageContent::Audio(c) => {
                let performer_title = if !c.performer.is_empty() || !c.title.is_empty() {
                    format!(
                        "{} — {}",
                        non_empty_or(&c.performer, "Unknown artist"),
                        non_empty_or(&c.title, "Unknown track"),
                    )
                } else {
                    String::new()
                };
                caption_or(&[&performer_title, &c.file_name], "Music")
            }
            MessageContent::VoiceNote(c) => caption_or(&[&c.caption], "Voice message"),
            MessageContent::Animation(c) => caption_or(&[&c.caption, &c.file_name], "GIF"),
            MessageContent::Text(c) => {
                let url = c
                    .entities
                    .iter()
                    .find_map(|entity| entity.open_href(&c.text))
                    .unwrap_or_default();
                let preview: String = c.text.chars().take(60).collect();
                caption_or(&[url, &preview], "Link")
            }
            _ => tab.label().to_string(),
        };
        let message_copy = matches!(tab, SharedMediaTab::Media | SharedMediaTab::Gifs)
            .then(|| Box::new(history_message(message.clone(), false)));
        SharedMediaItem {
            message_id: message.id,
            glyph,
            label,
            message: message_copy,
        }
    }
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
    pub error: String,
    /// `foundChatMessages.next_from_message_id` of the last page; `0` once
    /// the oldest message has been reached.
    pub next_from: MessageId,
    /// An older page is in flight (the viewer pages toward the end of the
    /// list and asks for more).
    pub loading_more: bool,
}

impl SharedMediaTabState {
    pub(crate) fn clear(&mut self) {
        self.status = SharedMediaTabStatus::Idle;
        self.items.clear();
        self.total_count = 0;
        self.error.clear();
        self.next_from = MessageId(0);
        self.loading_more = false;
    }

    /// Whether an older page exists and none is in flight.
    pub fn can_load_more(&self) -> bool {
        self.status == SharedMediaTabStatus::Ready && !self.loading_more && self.next_from.0 != 0
    }
}

/// Slice media-shared-gallery: the gallery panel state. Switching chats or
/// closing bumps `generation`, so late `foundChatMessages` answers drop.
#[derive(Debug, Clone, Default)]
pub struct SharedMediaState {
    pub open: bool,
    pub chat_id: Option<ChatId>,
    pub active_tab: SharedMediaTab,
    pub tabs: [SharedMediaTabState; 6],
    pub generation: u64,
}

impl SharedMediaState {
    pub(crate) fn tab_state(&mut self, tab: SharedMediaTab) -> &mut SharedMediaTabState {
        &mut self.tabs[tab.index()]
    }

    /// Open the gallery for `chat_id`; returns the tab to fetch. Reopening
    /// for the same chat keeps already-fetched tabs.
    pub fn open_for(&mut self, chat_id: ChatId) -> SharedMediaTab {
        if self.open && self.chat_id == Some(chat_id) {
            return self.active_tab;
        }
        self.open = true;
        self.chat_id = Some(chat_id);
        self.active_tab = SharedMediaTab::Media;
        self.generation = self.generation.saturating_add(1);
        for tab in self.tabs.iter_mut() {
            tab.clear();
        }
        self.active_tab
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chat_id = None;
        self.generation = self.generation.saturating_add(1);
        for tab in self.tabs.iter_mut() {
            tab.clear();
        }
    }

    /// Select a tab; returns `true` when it was never fetched and the caller
    /// must send `searchChatMessages`.
    pub fn select_tab(&mut self, tab: SharedMediaTab) -> bool {
        self.active_tab = tab;
        self.tabs[tab.index()].status == SharedMediaTabStatus::Idle
    }

    /// Mark the tab loading and bump the generation; the caller stamps the
    /// returned generation on the request purpose so late answers drop.
    pub fn begin_fetch(&mut self, tab: SharedMediaTab) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.tab_state(tab).status = SharedMediaTabStatus::Loading;
        self.generation
    }

    /// A `foundChatMessages` answer: applied only when the chat, tab, and
    /// generation all match the open gallery.
    pub fn accept(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        generation: u64,
        items: Vec<SharedMediaItem>,
        total_count: i32,
        next_from: MessageId,
    ) {
        if !self.open || self.chat_id != Some(chat_id) || self.generation != generation {
            return;
        }
        let state = self.tab_state(tab);
        state.items = items;
        state.total_count = total_count;
        state.next_from = next_from;
        state.loading_more = false;
        state.error.clear();
        state.status = if state.items.is_empty() {
            SharedMediaTabStatus::Empty
        } else {
            SharedMediaTabStatus::Ready
        };
    }

    /// Start fetching the next older page of `tab`: returns the generation
    /// to stamp and the `from_message_id` to send, or `None` when there is
    /// nothing more to load or a page is already in flight. The generation
    /// is not bumped, so the first-page answers of other tabs stay valid.
    pub fn begin_fetch_more(&mut self, tab: SharedMediaTab) -> Option<(u64, MessageId)> {
        let generation = self.generation;
        let state = self.tab_state(tab);
        if !state.can_load_more() {
            return None;
        }
        state.loading_more = true;
        Some((generation, state.next_from))
    }

    /// An older page landed: append the messages not yet listed. A page
    /// with nothing new ends the list.
    pub fn accept_more(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        generation: u64,
        items: Vec<SharedMediaItem>,
        total_count: i32,
        next_from: MessageId,
    ) {
        if !self.open || self.chat_id != Some(chat_id) || self.generation != generation {
            return;
        }
        let state = self.tab_state(tab);
        state.loading_more = false;
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

    /// An older page failed: allow a retry on the next request.
    pub fn fail_more(&mut self, chat_id: ChatId, tab: SharedMediaTab, generation: u64) {
        if !self.open || self.chat_id != Some(chat_id) || self.generation != generation {
            return;
        }
        self.tab_state(tab).loading_more = false;
    }

    pub fn fail(&mut self, chat_id: ChatId, tab: SharedMediaTab, generation: u64, error: String) {
        if !self.open || self.chat_id != Some(chat_id) || self.generation != generation {
            return;
        }
        let state = self.tab_state(tab);
        state.items.clear();
        state.total_count = 0;
        state.error = error;
        state.status = SharedMediaTabStatus::Failed;
    }
}
