//! The profile-style tabs of the shared-media gallery (tdesktop
//! `info/profile/tabs`): which tabs a chat shows, the Photos / Videos split
//! (tdesktop's `profile-media-tabs-expanded` option), the tabs that load
//! from their own TDLib methods (Stories and Gifts) instead of
//! `searchChatMessages`, and the saved-music Playlist page.
//!
//! tdesktop lists the tabs in this order: Posts / Stories, Gifts, Media
//! (or Photos and Videos once expanded), Saved, Files, Links, Music, Polls,
//! Voice, GIFs. A tab with no items is hidden (`shown = count > 0`). Saved
//! Messages and the common-groups / similar tabs are not shared media.
use super::*;
use crate::ids::FileId;
use crate::premium_hub::GiftsPage;
use crate::telegram::envelope::{ChatKind, ProfileAudio, ProfileAudiosPage};
use std::collections::HashMap;

/// What a click on a gallery row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SharedMediaAction {
    /// Jump to the message (viewable media opens the viewer first).
    #[default]
    Message,
    /// Open the story in the story viewer.
    Story(i32),
    /// Open the chat's received gifts.
    Gift,
}

/// What a grid tile of the Stories and Gifts tabs draws.
#[derive(Debug, Clone, Default)]
pub enum SharedMediaVisual {
    #[default]
    None,
    /// A story's thumbnail: file ids to try in order, and the picture size.
    Story {
        thumbs: Vec<FileId>,
        width: i32,
        height: i32,
    },
    /// A received gift: its sticker, backdrop and price.
    Gift(Box<crate::premium_hub::ReceivedGift>),
}

/// `getChatMessageCount` / `MEDIA_COUNT_FILTERS` index of the extra
/// filters the tab bar needs (see `EXTRA_COUNT_FILTERS`).
pub const COUNT_KEY_POLLS: u8 = 7;
pub const COUNT_KEY_VOICE: u8 = 8;
/// Count keys of the tabs that load from their own method; their total
/// comes with their first page.
pub const COUNT_KEY_STORIES: u8 = 9;
pub const COUNT_KEY_GIFTS: u8 = 10;

impl SharedMediaTab {
    /// The order the tab bar shows tabs in (tdesktop's profile tab order).
    pub const DISPLAY: [SharedMediaTab; Self::COUNT] = [
        SharedMediaTab::Stories,
        SharedMediaTab::Gifts,
        SharedMediaTab::Media,
        SharedMediaTab::Photos,
        SharedMediaTab::Videos,
        SharedMediaTab::Files,
        SharedMediaTab::Links,
        SharedMediaTab::Music,
        SharedMediaTab::Polls,
        SharedMediaTab::Voice,
        SharedMediaTab::Gifs,
    ];

    /// The count keys (`chat_media_counts`) that add up to this tab's size.
    pub fn count_keys(self) -> &'static [u8] {
        match self {
            SharedMediaTab::Media => &[0, 1],
            SharedMediaTab::Photos => &[0],
            SharedMediaTab::Videos => &[1],
            SharedMediaTab::Files => &[2],
            SharedMediaTab::Music => &[3],
            SharedMediaTab::Links => &[4],
            SharedMediaTab::Gifs => &[6],
            SharedMediaTab::Polls => &[COUNT_KEY_POLLS],
            SharedMediaTab::Voice => &[COUNT_KEY_VOICE],
            SharedMediaTab::Stories => &[COUNT_KEY_STORIES],
            SharedMediaTab::Gifts => &[COUNT_KEY_GIFTS],
        }
    }

    /// Whether the tab lists messages (`searchChatMessages`).
    pub fn lists_messages(self) -> bool {
        self.filter_constructor().is_some()
    }

    /// Tab label for the chat: a channel's stories are its "Posts".
    pub fn label_in(self, is_channel: bool) -> &'static str {
        if is_channel && self == SharedMediaTab::Stories {
            "Posts"
        } else {
            self.label()
        }
    }

    /// Whether the chat can have this tab at all: stories and gifts belong
    /// to users and channels.
    pub fn available_in(self, kind: &ChatKind) -> bool {
        let user = matches!(kind, ChatKind::Private { user_id } if user_id.0 > 0);
        let channel = matches!(
            kind,
            ChatKind::Supergroup {
                is_channel: true,
                ..
            }
        );
        match self {
            SharedMediaTab::Stories | SharedMediaTab::Gifts => user || channel,
            _ => true,
        }
    }
}

/// The tabs the tab bar shows for a chat, in display order. A tab shows
/// once its count is known to be above zero; the active tab always shows,
/// and so does Media (or, when split, Photos and Videos) while its count is
/// still unknown so the bar is never empty.
pub fn visible_tabs(
    kind: Option<&ChatKind>,
    counts: Option<&HashMap<u8, i32>>,
    media_split: bool,
    active: SharedMediaTab,
) -> Vec<SharedMediaTab> {
    let known = |tab: SharedMediaTab| -> Option<i32> {
        let counts = counts?;
        let mut sum = 0;
        for key in tab.count_keys() {
            sum += *counts.get(key)?;
        }
        Some(sum)
    };
    SharedMediaTab::DISPLAY
        .into_iter()
        .filter(|&tab| {
            if tab == active {
                return true;
            }
            if let Some(kind) = kind
                && !tab.available_in(kind)
            {
                return false;
            }
            match tab {
                SharedMediaTab::Media => !media_split && known(tab).is_none_or(|n| n > 0),
                SharedMediaTab::Photos | SharedMediaTab::Videos => {
                    media_split && known(tab).is_some_and(|n| n > 0)
                }
                _ => known(tab).is_some_and(|n| n > 0),
            }
        })
        .collect()
}

impl SharedMediaState {
    /// Split (`true`) or merge Media into Photos and Videos. Returns the
    /// tab the gallery should now show when the active one went away, and
    /// whether it still has to be fetched.
    pub fn set_media_split(
        &mut self,
        split: bool,
        counts: Option<&HashMap<u8, i32>>,
    ) -> Option<(SharedMediaTab, bool)> {
        if self.media_split == split {
            return None;
        }
        self.media_split = split;
        let target = match (split, self.active_tab) {
            (true, SharedMediaTab::Media) => {
                let photos = counts.and_then(|c| c.get(&0)).copied().unwrap_or(1);
                if photos > 0 {
                    SharedMediaTab::Photos
                } else {
                    SharedMediaTab::Videos
                }
            }
            (false, SharedMediaTab::Photos | SharedMediaTab::Videos) => SharedMediaTab::Media,
            _ => return None,
        };
        let needs_fetch = self.select_tab(target);
        Some((target, needs_fetch))
    }

    /// A first page of a profile tab is on its way. No generation bump:
    /// the profile tabs load side by side when the gallery opens.
    pub fn begin_profile_fetch(&mut self, tab: SharedMediaTab) {
        let state = self.tab_state(tab);
        state.status = SharedMediaTabStatus::Loading;
        state.error.clear();
    }

    /// The next page of a profile tab is on its way; `false` when there is
    /// nothing more or a page is already loading.
    pub fn begin_profile_more(&mut self, tab: SharedMediaTab) -> bool {
        let state = self.tab_state(tab);
        if !state.can_load_more() {
            return false;
        }
        state.loading_more = true;
        true
    }

    /// A page of a profile tab landed. `append` adds to the loaded items.
    pub fn accept_profile(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        items: Vec<SharedMediaItem>,
        total_count: i32,
        has_more: bool,
        append: bool,
    ) {
        if !self.open || self.chat_id != Some(chat_id) {
            return;
        }
        let state = self.tab_state(tab);
        if append {
            state.items.extend(items);
        } else {
            state.items = items;
        }
        state.total_count = total_count.max(state.items.len() as i32);
        state.has_more = has_more;
        state.loading_more = false;
        state.error.clear();
        state.status = if state.items.is_empty() {
            SharedMediaTabStatus::Empty
        } else {
            SharedMediaTabStatus::Ready
        };
    }

    /// A profile tab failed to load.
    pub fn fail_profile(&mut self, chat_id: ChatId, tab: SharedMediaTab, error: String) {
        if !self.open || self.chat_id != Some(chat_id) {
            return;
        }
        let state = self.tab_state(tab);
        state.loading_more = false;
        if state.status == SharedMediaTabStatus::Ready {
            return;
        }
        state.items.clear();
        state.total_count = 0;
        state.error = error;
        state.status = SharedMediaTabStatus::Failed;
    }
}

/// Thumbnail candidates of a story, best first: the photo size nearest a
/// grid tile, or the video's thumbnail.
fn story_thumbs(story: &crate::telegram::envelope::ParsedStory) -> (Vec<FileId>, i32, i32) {
    use crate::telegram::envelope::StoryContentView;
    const TILE: i32 = 200;
    match &story.content {
        StoryContentView::Photo { sizes } => {
            let mut sorted: Vec<_> = sizes.iter().collect();
            sorted.sort_by_key(|size| (size.width - TILE).abs());
            let (width, height) = sorted.first().map_or((0, 0), |s| (s.width, s.height));
            (
                sorted.iter().map(|size| size.file_id).collect(),
                width,
                height,
            )
        }
        StoryContentView::Video {
            thumb_file_id,
            thumb_width,
            thumb_height,
            ..
        } => (
            thumb_file_id.iter().copied().collect(),
            *thumb_width,
            *thumb_height,
        ),
        _ => (Vec::new(), 0, 0),
    }
}

/// The tile for a story posted to the profile.
pub fn story_item(
    story: Option<&crate::telegram::envelope::ParsedStory>,
    id: i32,
) -> SharedMediaItem {
    let caption: String = story
        .map(|story| story.caption.chars().take(60).collect())
        .unwrap_or_default();
    let (thumbs, width, height) = story.map(story_thumbs).unwrap_or_default();
    SharedMediaItem {
        message_id: MessageId(0),
        glyph: SharedMediaTab::Stories.glyph(),
        label: caption_or(&[&caption], "Story"),
        message: None,
        date: story.map_or(0, |story| story.date),
        action: SharedMediaAction::Story(id),
        visual: SharedMediaVisual::Story {
            thumbs,
            width,
            height,
        },
        meta: SharedMediaMeta::None,
        file: FileId(0),
    }
}

/// The tile for a received gift: its price or collectible name, with the
/// sender's note as the label when there is one.
pub fn gift_item(gift: &crate::premium_hub::ReceivedGift) -> SharedMediaItem {
    let label = if gift.text.is_empty() {
        gift.headline()
    } else {
        format!("{} — {}", gift.headline(), gift.text)
    };
    SharedMediaItem {
        message_id: MessageId(0),
        glyph: SharedMediaTab::Gifts.glyph(),
        label,
        message: None,
        date: gift.date,
        action: SharedMediaAction::Gift,
        visual: SharedMediaVisual::Gift(Box::new(gift.clone())),
        meta: SharedMediaMeta::None,
        file: FileId(0),
    }
}

/// A user's saved music (`getUserProfileAudios`), shown on the Playlist
/// page and as the profile's music button.
#[derive(Debug, Clone, Default)]
pub struct Playlist {
    pub tracks: Vec<ProfileAudio>,
    pub total_count: i32,
    /// The first page arrived (or failed).
    pub loaded: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// A change of the playlist failed: the driver loads it again.
    pub stale: bool,
}

impl Playlist {
    pub fn has_more(&self) -> bool {
        self.loaded && !self.loading && (self.tracks.len() as i32) < self.total_count
    }
}

/// The message id the player uses for a playlist track. Tracks have no
/// message, so the (positive) file id is negated; real message ids are
/// never negative.
pub fn playlist_message_id(file_id: FileId) -> MessageId {
    MessageId(-i64::from(file_id.0))
}

/// "Performer — Title" for a saved track, or its file name.
pub fn track_name(audio: &ProfileAudio) -> String {
    let name = if !audio.performer.is_empty() || !audio.title.is_empty() {
        format!(
            "{} — {}",
            non_empty_or(&audio.performer, "Unknown artist"),
            non_empty_or(&audio.title, "Unknown track"),
        )
    } else {
        String::new()
    };
    caption_or(&[&name, &audio.file_name], "Music")
}

impl Session {
    /// The tabs the gallery shows for the open chat.
    pub fn shared_media_visible_tabs(&self) -> Vec<SharedMediaTab> {
        let gallery = &self.media.shared_media;
        let chat_id = gallery.chat_id;
        let kind = chat_id
            .and_then(|id| self.chats.get(&id.0))
            .map(|chat| &chat.kind);
        let counts = chat_id.and_then(|id| self.media.chat_media_counts.get(&id.0));
        visible_tabs(kind, counts, gallery.media_split, gallery.active_tab)
    }

    /// Only into the entry the gallery's counts fetch opened, as a `count`
    /// answer does: an answer for a chat whose counts went meanwhile would
    /// make a partial entry that stops the next fetch.
    fn set_profile_count(&mut self, chat_id: ChatId, key: u8, count: i32) {
        if let Some(counts) = self.media.chat_media_counts.get_mut(&chat_id.0) {
            counts.insert(key, count);
        }
    }

    /// Ask for the thumbnails of the files (the next ingest downloads them).
    fn want_profile_thumbs(&mut self, ids: impl IntoIterator<Item = FileId>) {
        self.media
            .profile_thumbs_due
            .extend(ids.into_iter().filter(|id| id.0 != 0));
    }

    /// Thumbnails the Stories and Gifts grids still need.
    pub fn take_profile_thumbs(&mut self) -> Vec<FileId> {
        let mut seen = std::collections::HashSet::new();
        std::mem::take(&mut self.media.profile_thumbs_due)
            .into_iter()
            .filter(|id| seen.insert(*id) && self.should_download(*id))
            .collect()
    }

    /// A page of `getChatPostedToChatPageStories` landed (the loaded ids
    /// are already merged into `stories.chat_page_stories`).
    pub(crate) fn apply_profile_stories(&mut self, chat_id: ChatId, total_count: i32) {
        let ids = self
            .stories
            .chat_page_stories
            .get(&chat_id.0)
            .map(|entry| entry.story_ids.clone())
            .unwrap_or_default();
        let items: Vec<SharedMediaItem> = ids
            .iter()
            .map(|id| story_item(self.stories.stories.get(&(chat_id.0, *id)), *id))
            .collect();
        let thumbs: Vec<FileId> = items
            .iter()
            .filter_map(|item| match &item.visual {
                SharedMediaVisual::Story { thumbs, .. } => thumbs.first().copied(),
                _ => None,
            })
            .collect();
        self.want_profile_thumbs(thumbs);
        let has_more = (items.len() as i32) < total_count;
        self.set_profile_count(chat_id, COUNT_KEY_STORIES, total_count);
        self.media.shared_media.accept_profile(
            chat_id,
            SharedMediaTab::Stories,
            items,
            total_count,
            has_more,
            false,
        );
    }

    /// A `receivedGifts` page asked for by the gallery's Gifts tab.
    pub(crate) fn apply_profile_gifts(&mut self, page: GiftsPage, pending: &PendingRequest) {
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let append = matches!(
            pending.purpose,
            RequestPurpose::Media(MediaPurpose::GetProfileGifts { append: true })
        );
        let files: Vec<_> = page
            .gifts
            .iter()
            .flat_map(|gift| gift.gift.files.iter().cloned())
            .collect();
        self.remember_files(&files);
        self.want_profile_thumbs(files.iter().map(|file| file.id));
        let items = page.gifts.iter().map(gift_item).collect();
        self.set_profile_count(chat_id, COUNT_KEY_GIFTS, page.total_count);
        self.media.shared_media.accept_profile(
            chat_id,
            SharedMediaTab::Gifts,
            items,
            page.total_count,
            !page.next_offset.is_empty(),
            append,
        );
        let state = self.media.shared_media.tab_state(SharedMediaTab::Gifts);
        state.next_offset = page.next_offset;
    }

    /// A `getUserProfileAudios` page for the user's playlist.
    pub(crate) fn apply_profile_audios(
        &mut self,
        page: ProfileAudiosPage,
        pending: Option<&PendingRequest>,
    ) {
        let Some(user_id) = pending.and_then(|p| p.user_id) else {
            return;
        };
        let append = matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::Media(MediaPurpose::GetProfileAudios {
                append: true
            }))
        );
        self.remember_files(&page.files);
        let playlist = self.media.playlists.entry(user_id).or_default();
        if !append {
            playlist.tracks.clear();
        }
        for track in page.audios {
            let known = track.file_id.0 != 0
                && playlist
                    .tracks
                    .iter()
                    .any(|old| old.file_id == track.file_id);
            if !known {
                playlist.tracks.push(track);
            }
        }
        playlist.total_count = page.total_count.max(playlist.tracks.len() as i32);
        playlist.loaded = true;
        playlist.loading = false;
        playlist.error = None;
    }

    /// A `getUserProfileAudios` request failed: no playlist for this user.
    pub(crate) fn fail_profile_audios(&mut self, user_id: i64, error: String) {
        let playlist = self.media.playlists.entry(user_id).or_default();
        playlist.loaded = true;
        playlist.loading = false;
        playlist.error = Some(error);
    }

    /// A profile tab's request failed: the tab shows the error and hides
    /// from the bar unless it is the active one.
    pub(crate) fn fail_profile_tab(&mut self, chat_id: ChatId, tab: SharedMediaTab, error: String) {
        if let Some(&key) = tab.count_keys().first() {
            self.set_profile_count(chat_id, key, 0);
        }
        self.media.shared_media.fail_profile(chat_id, tab, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::UserId;

    fn user() -> ChatKind {
        ChatKind::Private { user_id: UserId(5) }
    }

    fn counts(pairs: &[(u8, i32)]) -> HashMap<u8, i32> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn display_order_lists_every_tab_once() {
        let mut seen = SharedMediaTab::DISPLAY.to_vec();
        seen.sort_by_key(|tab| tab.index());
        assert_eq!(seen, SharedMediaTab::ALL.to_vec());
        for (index, tab) in SharedMediaTab::ALL.iter().enumerate() {
            assert_eq!(tab.index(), index);
        }
    }

    #[test]
    fn filters_match_tdesktop_media_types() {
        let filter = |tab: SharedMediaTab| tab.filter_constructor();
        assert_eq!(
            filter(SharedMediaTab::Photos),
            Some("searchMessagesFilterPhoto")
        );
        assert_eq!(
            filter(SharedMediaTab::Videos),
            Some("searchMessagesFilterVideo")
        );
        assert_eq!(
            filter(SharedMediaTab::Polls),
            Some("searchMessagesFilterPoll")
        );
        // tdesktop's `RoundVoiceFile`: round videos live under Voice.
        assert_eq!(
            filter(SharedMediaTab::Voice),
            Some("searchMessagesFilterVoiceAndVideoNote")
        );
        for tab in [SharedMediaTab::Stories, SharedMediaTab::Gifts] {
            assert!(!tab.lists_messages());
        }
    }

    #[test]
    fn tabs_hide_until_their_count_is_above_zero() {
        let counts = counts(&[
            (0, 4),
            (1, 0),
            (2, 0),
            (3, 2),
            (4, 0),
            (6, 0),
            (7, 1),
            (8, 0),
        ]);
        let tabs = visible_tabs(Some(&user()), Some(&counts), false, SharedMediaTab::Media);
        assert_eq!(
            tabs,
            vec![
                SharedMediaTab::Media,
                SharedMediaTab::Music,
                SharedMediaTab::Polls
            ]
        );
    }

    #[test]
    fn unknown_counts_keep_media_and_hide_the_rest() {
        let tabs = visible_tabs(Some(&user()), None, false, SharedMediaTab::Media);
        assert_eq!(tabs, vec![SharedMediaTab::Media]);
    }

    #[test]
    fn split_replaces_media_with_photos_and_videos() {
        let counts = counts(&[(0, 3), (1, 2)]);
        let tabs = visible_tabs(Some(&user()), Some(&counts), true, SharedMediaTab::Photos);
        assert_eq!(tabs, vec![SharedMediaTab::Photos, SharedMediaTab::Videos]);
        // Only videos: Photos is hidden.
        let counts = self::counts(&[(0, 0), (1, 2)]);
        let tabs = visible_tabs(Some(&user()), Some(&counts), true, SharedMediaTab::Videos);
        assert_eq!(tabs, vec![SharedMediaTab::Videos]);
    }

    #[test]
    fn active_tab_always_shows_and_unavailable_tabs_never() {
        let counts = counts(&[(COUNT_KEY_GIFTS, 2)]);
        let group = ChatKind::BasicGroup { basic_group_id: 1 };
        let tabs = visible_tabs(Some(&group), Some(&counts), false, SharedMediaTab::Files);
        assert!(!tabs.contains(&SharedMediaTab::Gifts));
        assert!(tabs.contains(&SharedMediaTab::Files));
        let channel = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        let tabs = visible_tabs(Some(&channel), Some(&counts), false, SharedMediaTab::Media);
        assert!(tabs.contains(&SharedMediaTab::Gifts));
        assert_eq!(SharedMediaTab::Stories.label_in(true), "Posts");
        assert_eq!(SharedMediaTab::Stories.label_in(false), "Stories");
    }

    #[test]
    fn story_tabs_lead_the_bar() {
        let counts = counts(&[(0, 1), (COUNT_KEY_STORIES, 5), (COUNT_KEY_GIFTS, 1)]);
        let tabs = visible_tabs(Some(&user()), Some(&counts), false, SharedMediaTab::Media);
        assert_eq!(
            tabs,
            vec![
                SharedMediaTab::Stories,
                SharedMediaTab::Gifts,
                SharedMediaTab::Media
            ]
        );
    }

    #[test]
    fn splitting_moves_the_active_tab() {
        let mut gallery = SharedMediaState::default();
        gallery.open_for(ChatId(1));
        let c = counts(&[(0, 0), (1, 3)]);
        let moved = gallery.set_media_split(true, Some(&c));
        assert_eq!(moved, Some((SharedMediaTab::Videos, true)));
        assert_eq!(gallery.active_tab, SharedMediaTab::Videos);
        assert_eq!(gallery.set_media_split(true, Some(&c)), None);
        let moved = gallery.set_media_split(false, Some(&c));
        assert_eq!(moved, Some((SharedMediaTab::Media, true)));
    }

    #[test]
    fn profile_pages_append_and_fail_without_losing_ready_items() {
        let mut gallery = SharedMediaState::default();
        gallery.open_for(ChatId(1));
        let row = |label: &str| SharedMediaItem {
            message_id: MessageId(0),
            glyph: "",
            label: label.into(),
            message: None,
            date: 0,
            action: SharedMediaAction::Gift,
            visual: SharedMediaVisual::None,
            meta: SharedMediaMeta::None,
            file: FileId(0),
        };
        let tab = SharedMediaTab::Gifts;
        gallery.begin_profile_fetch(tab);
        gallery.accept_profile(ChatId(1), tab, vec![row("a")], 2, true, false);
        assert_eq!(
            gallery.tabs[tab.index()].status,
            SharedMediaTabStatus::Ready
        );
        assert!(gallery.begin_profile_more(tab));
        assert!(!gallery.begin_profile_more(tab));
        gallery.accept_profile(ChatId(1), tab, vec![row("b")], 2, false, true);
        assert_eq!(gallery.tabs[tab.index()].items.len(), 2);
        assert!(!gallery.tabs[tab.index()].can_load_more());
        // A late failure keeps the list; another chat's answer is ignored.
        gallery.fail_profile(ChatId(1), tab, "boom".into());
        assert_eq!(
            gallery.tabs[tab.index()].status,
            SharedMediaTabStatus::Ready
        );
        gallery.accept_profile(ChatId(2), tab, vec![], 0, false, false);
        assert_eq!(gallery.tabs[tab.index()].items.len(), 2);
    }

    #[test]
    fn rows_for_stories_tracks_and_split_state() {
        let audio = ProfileAudio {
            file_id: crate::ids::FileId(1),
            title: "Night Drive".into(),
            performer: "Ada".into(),
            file_name: String::new(),
            duration: 214,
        };
        assert_eq!(track_name(&audio), "Ada — Night Drive");
        assert_eq!(playlist_message_id(audio.file_id), MessageId(-1));
        assert_eq!(story_item(None, 7).action, SharedMediaAction::Story(7));
        assert_eq!(story_item(None, 7).label, "Story");
        // With the option on, a fresh gallery opens on Photos, not Media.
        let mut gallery = SharedMediaState {
            media_split: true,
            ..Default::default()
        };
        assert_eq!(gallery.open_for(ChatId(1)), SharedMediaTab::Photos);
    }

    #[test]
    fn playlist_pages_until_the_total() {
        let mut playlist = Playlist::default();
        assert!(!playlist.has_more());
        playlist.loaded = true;
        playlist.total_count = 2;
        assert!(playlist.has_more());
        playlist.tracks = vec![
            ProfileAudio {
                file_id: crate::ids::FileId(1),
                title: String::new(),
                performer: String::new(),
                file_name: "a.mp3".into(),
                duration: 0,
            };
            2
        ];
        assert!(!playlist.has_more());
    }
}
