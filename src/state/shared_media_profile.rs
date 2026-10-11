//! The profile-style tabs of the shared-media gallery (tdesktop
//! `info/profile/tabs`): which tabs a chat shows, the Photos / Videos split,
//! and the tabs that load from their own TDLib methods (Stories, Gifts, and
//! the saved-music Playlist) instead of `searchChatMessages`.
//!
//! tdesktop lists the tabs in this order: Posts / Stories, Gifts, Media
//! (or Photos and Videos once expanded), Saved, Files, Links, Music, Polls,
//! Voice, GIFs. A tab with no items is hidden (`shown = count > 0`). Saved
//! Messages and the common-groups / similar tabs are not shared media.
use super::*;
use crate::premium_hub::GiftsPage;
use crate::telegram::envelope::{ChatKind, ProfileAudiosPage};
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
    /// A saved track; read-only.
    Audio,
}

/// `getChatMessageCount` / `MEDIA_COUNT_FILTERS` index of the extra
/// filters the tab bar needs (see `EXTRA_COUNT_FILTERS`).
pub const COUNT_KEY_POLLS: u8 = 7;
pub const COUNT_KEY_VOICE: u8 = 8;
/// Count keys of the tabs that load from their own method; their total
/// comes with their first page.
pub const COUNT_KEY_STORIES: u8 = 9;
pub const COUNT_KEY_GIFTS: u8 = 10;
pub const COUNT_KEY_PLAYLIST: u8 = 11;

impl SharedMediaTab {
    /// The order the tab bar shows tabs in (tdesktop's profile tab order).
    pub const DISPLAY: [SharedMediaTab; Self::COUNT] = [
        SharedMediaTab::Stories,
        SharedMediaTab::Gifts,
        SharedMediaTab::Media,
        SharedMediaTab::Photos,
        SharedMediaTab::Videos,
        SharedMediaTab::SavedMusic,
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
            SharedMediaTab::SavedMusic => &[COUNT_KEY_PLAYLIST],
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
    /// to users and channels, saved music to users (`getUserProfileAudios`).
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
            SharedMediaTab::SavedMusic => user,
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

/// The row for a story posted to the profile.
pub fn story_item(
    story: Option<&crate::telegram::envelope::ParsedStory>,
    id: i32,
) -> SharedMediaItem {
    let caption: String = story
        .map(|story| story.caption.chars().take(60).collect())
        .unwrap_or_default();
    SharedMediaItem {
        message_id: MessageId(0),
        glyph: SharedMediaTab::Stories.glyph(),
        label: caption_or(&[&caption], "Story"),
        message: None,
        date: story.map_or(0, |story| story.date),
        action: SharedMediaAction::Story(id),
    }
}

/// The row for a received gift: its price or collectible name, with the
/// sender's note when there is one.
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
    }
}

/// The row for a saved track: "Performer — Title" and its length.
pub fn audio_item(audio: &crate::telegram::envelope::ProfileAudio) -> SharedMediaItem {
    let name = if !audio.performer.is_empty() || !audio.title.is_empty() {
        format!(
            "{} — {}",
            non_empty_or(&audio.performer, "Unknown artist"),
            non_empty_or(&audio.title, "Unknown track"),
        )
    } else {
        String::new()
    };
    let name = caption_or(&[&name, &audio.file_name], "Music");
    let label = if audio.duration > 0 {
        format!(
            "{name} · {}:{:02}",
            audio.duration / 60,
            audio.duration % 60
        )
    } else {
        name
    };
    SharedMediaItem {
        message_id: MessageId(0),
        glyph: SharedMediaTab::SavedMusic.glyph(),
        label,
        message: None,
        date: 0,
        action: SharedMediaAction::Audio,
    }
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

    fn set_profile_count(&mut self, chat_id: ChatId, key: u8, count: i32) {
        self.media
            .chat_media_counts
            .entry(chat_id.0)
            .or_default()
            .insert(key, count);
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

    /// A `getUserProfileAudios` page for the Playlist tab.
    pub(crate) fn apply_profile_audios(
        &mut self,
        page: ProfileAudiosPage,
        pending: Option<&PendingRequest>,
    ) {
        let Some(pending) = pending else {
            return;
        };
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let append = matches!(
            pending.purpose,
            RequestPurpose::Media(MediaPurpose::GetProfileAudios { append: true })
        );
        self.remember_files(&page.files);
        let items: Vec<SharedMediaItem> = page.audios.iter().map(audio_item).collect();
        let before = if append {
            self.media
                .shared_media
                .tab_state(SharedMediaTab::SavedMusic)
                .items
                .len()
        } else {
            0
        };
        let loaded = before + items.len();
        self.set_profile_count(chat_id, COUNT_KEY_PLAYLIST, page.total_count);
        self.media.shared_media.accept_profile(
            chat_id,
            SharedMediaTab::SavedMusic,
            items,
            page.total_count,
            (loaded as i32) < page.total_count,
            append,
        );
    }

    /// A profile tab's request failed: the tab shows the error and hides
    /// from the bar unless it is the active one.
    pub(crate) fn fail_profile_tab(&mut self, chat_id: ChatId, tab: SharedMediaTab, error: String) {
        let key = tab.count_keys()[0];
        self.set_profile_count(chat_id, key, 0);
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
        for tab in [
            SharedMediaTab::Stories,
            SharedMediaTab::Gifts,
            SharedMediaTab::SavedMusic,
        ] {
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
        let counts = counts(&[(COUNT_KEY_PLAYLIST, 3), (COUNT_KEY_GIFTS, 2)]);
        let group = ChatKind::BasicGroup { basic_group_id: 1 };
        let tabs = visible_tabs(Some(&group), Some(&counts), false, SharedMediaTab::Files);
        assert!(!tabs.contains(&SharedMediaTab::SavedMusic));
        assert!(!tabs.contains(&SharedMediaTab::Gifts));
        assert!(tabs.contains(&SharedMediaTab::Files));
        let channel = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        let tabs = visible_tabs(Some(&channel), Some(&counts), false, SharedMediaTab::Media);
        assert!(tabs.contains(&SharedMediaTab::Gifts));
        assert!(!tabs.contains(&SharedMediaTab::SavedMusic));
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
            action: SharedMediaAction::Audio,
        };
        let tab = SharedMediaTab::SavedMusic;
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
    fn rows_for_gifts_and_tracks() {
        let audio = crate::telegram::envelope::ProfileAudio {
            file_id: crate::ids::FileId(1),
            title: "Night Drive".into(),
            performer: "Ada".into(),
            file_name: String::new(),
            duration: 214,
        };
        assert_eq!(audio_item(&audio).label, "Ada — Night Drive · 3:34");
        assert_eq!(story_item(None, 7).action, SharedMediaAction::Story(7));
        assert_eq!(story_item(None, 7).label, "Story");
    }
}
