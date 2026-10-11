//! The tab strip of the global search (tdesktop `Suggestions::TabKeysFor`
//! and its `setupTabs` labels): Chats, Channels, Apps, Posts, then the
//! media tabs Photos, Videos, Links, Files, Music and Voice. Pure: which
//! tab a set of search filters means, and the filters a tab stands for.
//!
//! tdesktop also lists a Downloads tab between Videos and Links, shown only
//! while the download manager has entries; Quill keeps downloads in their
//! own list and leaves that tab out.

use crate::search_filters::{GlobalSearchFilters, SearchChatType, SearchMediaKind, SearchScope};
use crate::state::SharedMediaTab;

/// One tab of the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SearchTab {
    #[default]
    Chats,
    Channels,
    Apps,
    Posts,
    Photos,
    Videos,
    Links,
    Files,
    Music,
    Voice,
}

impl SearchTab {
    /// The order tdesktop shows them in.
    pub const ALL: [Self; 10] = [
        Self::Chats,
        Self::Channels,
        Self::Apps,
        Self::Posts,
        Self::Photos,
        Self::Videos,
        Self::Links,
        Self::Files,
        Self::Music,
        Self::Voice,
    ];

    /// `lng_recent_chats` / `lng_recent_channels` / `lng_recent_apps` /
    /// `lng_recent_posts` and the `lng_all_*` media labels.
    pub fn label(self) -> &'static str {
        match self {
            Self::Chats => "Chats",
            Self::Channels => "Channels",
            Self::Apps => "Apps",
            Self::Posts => "Posts",
            Self::Photos => "Photos",
            Self::Videos => "Videos",
            Self::Links => "Links",
            Self::Files => "Files",
            Self::Music => "Music",
            Self::Voice => "Voice",
        }
    }

    /// The tab a filter set is on.
    pub fn of(filters: &GlobalSearchFilters) -> Self {
        match filters.scope {
            SearchScope::PublicPosts => return Self::Posts,
            SearchScope::Apps => return Self::Apps,
            SearchScope::MyMessages => {}
        }
        match filters.media {
            SearchMediaKind::Photos => Self::Photos,
            SearchMediaKind::Videos => Self::Videos,
            SearchMediaKind::Links => Self::Links,
            SearchMediaKind::Files => Self::Files,
            SearchMediaKind::Music => Self::Music,
            SearchMediaKind::Voice => Self::Voice,
            // The in-chat "Media" join is not a global tab; it reads as
            // chats.
            SearchMediaKind::All | SearchMediaKind::Media => {
                if filters.chat_type == SearchChatType::Channels {
                    Self::Channels
                } else {
                    Self::Chats
                }
            }
        }
    }

    /// `filters` moved onto this tab. The date window and the archive
    /// toggle carry over; the chat type only where the tab lists chats.
    pub fn apply(self, mut filters: GlobalSearchFilters) -> GlobalSearchFilters {
        filters.scope = match self {
            Self::Posts => SearchScope::PublicPosts,
            Self::Apps => SearchScope::Apps,
            _ => SearchScope::MyMessages,
        };
        filters.media = match self {
            Self::Photos => SearchMediaKind::Photos,
            Self::Videos => SearchMediaKind::Videos,
            Self::Links => SearchMediaKind::Links,
            Self::Files => SearchMediaKind::Files,
            Self::Music => SearchMediaKind::Music,
            Self::Voice => SearchMediaKind::Voice,
            _ => SearchMediaKind::All,
        };
        match self {
            Self::Channels => filters.chat_type = SearchChatType::Channels,
            // Leaving the Channels tab must not leave its chat type behind.
            Self::Chats if filters.chat_type == SearchChatType::Channels => {
                filters.chat_type = SearchChatType::All;
            }
            _ => {}
        }
        filters
    }

    /// The media tab whose rows and glyph the results reuse.
    pub fn media_tab(self) -> Option<SharedMediaTab> {
        match self {
            Self::Photos => Some(SharedMediaTab::Photos),
            Self::Videos => Some(SharedMediaTab::Videos),
            Self::Links => Some(SharedMediaTab::Links),
            Self::Files => Some(SharedMediaTab::Files),
            Self::Music => Some(SharedMediaTab::Music),
            Self::Voice => Some(SharedMediaTab::Voice),
            _ => None,
        }
    }

    /// Lists messages of one kind across every chat, with or without a
    /// query (tdesktop's `Info::Section::Type::GlobalMedia`).
    pub fn is_media(self) -> bool {
        self.media_tab().is_some()
    }

    /// Whether a typed query runs `searchMessages` (and so honours the
    /// date window).
    pub fn searches_messages(self) -> bool {
        !matches!(self, Self::Apps | Self::Posts)
    }

    /// Whether the chat-type chips make sense on the tab.
    pub fn has_chat_type(self) -> bool {
        self == Self::Chats
    }

    /// The hint of an empty field.
    pub fn idle_hint(self) -> &'static str {
        match self {
            Self::Chats => "Type to search chats and messages.",
            Self::Channels => "Type to search channels.",
            Self::Apps => "",
            Self::Posts => "Type to search posts of public channels.",
            Self::Photos => "No photos to show.",
            Self::Videos => "No videos to show.",
            Self::Links => "No links to show.",
            Self::Files => "No files to show.",
            Self::Music => "No music to show.",
            Self::Voice => "No voice messages to show.",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::search_filters::{
        GlobalSearchFilters, SearchChatType, SearchDateRange, SearchMediaKind, SearchScope,
    };
    use crate::search_tabs::SearchTab;

    #[test]
    fn the_strip_follows_tdesktop() {
        let labels: Vec<_> = SearchTab::ALL.iter().map(|tab| tab.label()).collect();
        assert_eq!(
            labels,
            [
                "Chats", "Channels", "Apps", "Posts", "Photos", "Videos", "Links", "Files",
                "Music", "Voice"
            ]
        );
    }

    #[test]
    fn every_tab_reads_back_from_its_filters() {
        for tab in SearchTab::ALL {
            let filters = tab.apply(GlobalSearchFilters::default());
            assert_eq!(SearchTab::of(&filters), tab, "{tab:?}");
        }
    }

    #[test]
    fn media_tabs_map_to_search_filters() {
        let constructor = |tab: SearchTab| {
            tab.apply(GlobalSearchFilters::default())
                .media
                .constructor()
        };
        assert_eq!(
            constructor(SearchTab::Photos),
            Some("searchMessagesFilterPhoto")
        );
        assert_eq!(
            constructor(SearchTab::Videos),
            Some("searchMessagesFilterVideo")
        );
        assert_eq!(
            constructor(SearchTab::Links),
            Some("searchMessagesFilterUrl")
        );
        assert_eq!(
            constructor(SearchTab::Files),
            Some("searchMessagesFilterDocument")
        );
        assert_eq!(
            constructor(SearchTab::Music),
            Some("searchMessagesFilterAudio")
        );
        assert_eq!(
            constructor(SearchTab::Voice),
            Some("searchMessagesFilterVoiceAndVideoNote")
        );
        assert_eq!(constructor(SearchTab::Chats), None);
        assert_eq!(constructor(SearchTab::Channels), None);
    }

    #[test]
    fn media_tabs_reuse_shared_media_filters() {
        for tab in SearchTab::ALL {
            let Some(shared) = tab.media_tab() else {
                continue;
            };
            let filters = tab.apply(GlobalSearchFilters::default());
            assert_eq!(filters.media.constructor(), shared.filter_constructor());
        }
    }

    #[test]
    fn channels_tab_is_the_channel_chat_type() {
        let filters = SearchTab::Channels.apply(GlobalSearchFilters::default());
        assert_eq!(filters.chat_type, SearchChatType::Channels);
        assert_eq!(filters.scope, SearchScope::MyMessages);
        let back = SearchTab::Chats.apply(filters);
        assert_eq!(back.chat_type, SearchChatType::All);
        assert_eq!(SearchTab::of(&back), SearchTab::Chats);
    }

    #[test]
    fn date_window_and_archive_survive_a_tab_change() {
        let filters = GlobalSearchFilters {
            date: SearchDateRange::Month,
            archived: true,
            chat_type: SearchChatType::Groups,
            ..Default::default()
        };
        let next = SearchTab::Files.apply(filters);
        assert_eq!(next.date, SearchDateRange::Month);
        assert!(next.archived);
        assert_eq!(next.media, SearchMediaKind::Files);
        // The Groups chat type stays so the Chats tab comes back as it was.
        assert_eq!(
            SearchTab::Chats.apply(next).chat_type,
            SearchChatType::Groups
        );
    }

    #[test]
    fn only_message_tabs_search_messages() {
        let searching = SearchTab::ALL
            .iter()
            .filter(|tab| tab.searches_messages())
            .count();
        assert_eq!(searching, 8);
        assert!(!SearchTab::Apps.searches_messages());
        assert!(!SearchTab::Posts.searches_messages());
        assert!(SearchTab::Voice.is_media());
        assert!(!SearchTab::Channels.is_media());
        assert!(SearchTab::Chats.has_chat_type());
    }
}
