//! The tab a profile opens on (`ProfileTab`, schema 1.8.68 line 766).
//!
//! tdesktop keeps this as `main_profile_tab` on the user and channel
//! (`data_user.cpp`, `info_profile_tabs_host.cpp`): a profile opens on that
//! tab when the peer chose one. Quill's profile shows the shared-media
//! gallery for the media-like tabs; Posts and Gifts have no gallery tab here
//! yet, so they fall back to the default tab.
use crate::state::SharedMediaTab;
use serde_json::Value;

/// One `profileTab*` constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileTab {
    Posts,
    Gifts,
    Media,
    Files,
    Links,
    Music,
    Voice,
    Gifs,
}

impl ProfileTab {
    /// Order of the chooser.
    pub const ALL: [ProfileTab; 8] = [
        ProfileTab::Posts,
        ProfileTab::Gifts,
        ProfileTab::Media,
        ProfileTab::Files,
        ProfileTab::Links,
        ProfileTab::Music,
        ProfileTab::Voice,
        ProfileTab::Gifs,
    ];

    /// The TDLib constructor name.
    pub fn type_name(self) -> &'static str {
        match self {
            ProfileTab::Posts => "profileTabPosts",
            ProfileTab::Gifts => "profileTabGifts",
            ProfileTab::Media => "profileTabMedia",
            ProfileTab::Files => "profileTabFiles",
            ProfileTab::Links => "profileTabLinks",
            ProfileTab::Music => "profileTabMusic",
            ProfileTab::Voice => "profileTabVoice",
            ProfileTab::Gifs => "profileTabGifs",
        }
    }

    pub fn from_type_name(name: &str) -> Option<ProfileTab> {
        Self::ALL.into_iter().find(|tab| tab.type_name() == name)
    }

    /// Parses a `ProfileTab` object; `null` or an unknown type gives `None`.
    pub fn from_value(value: Option<&Value>) -> Option<ProfileTab> {
        let name = value?.get("@type")?.as_str()?;
        Self::from_type_name(name)
    }

    /// Label in the chooser.
    pub fn label(self) -> &'static str {
        match self {
            ProfileTab::Posts => "Posts",
            ProfileTab::Gifts => "Gifts",
            ProfileTab::Media => "Media",
            ProfileTab::Files => "Files",
            ProfileTab::Links => "Links",
            ProfileTab::Music => "Music",
            ProfileTab::Voice => "Voice messages",
            ProfileTab::Gifs => "GIFs",
        }
    }

    /// The shared-media gallery tab that shows this profile tab, if Quill
    /// has one.
    pub fn shared_media_tab(self) -> Option<SharedMediaTab> {
        match self {
            ProfileTab::Posts | ProfileTab::Gifts => None,
            ProfileTab::Media => Some(SharedMediaTab::Media),
            ProfileTab::Files => Some(SharedMediaTab::Files),
            ProfileTab::Links => Some(SharedMediaTab::Links),
            ProfileTab::Music => Some(SharedMediaTab::Music),
            ProfileTab::Voice => Some(SharedMediaTab::Voice),
            ProfileTab::Gifs => Some(SharedMediaTab::Gifs),
        }
    }

    /// The `ProfileTab` object for `setMainProfileTab`.
    pub fn to_value(self) -> Value {
        serde_json::json!({ "@type": self.type_name() })
    }
}

#[cfg(test)]
mod tests {
    use super::ProfileTab;
    use crate::state::SharedMediaTab;
    use serde_json::json;

    #[test]
    fn parses_every_constructor_and_round_trips() {
        for tab in ProfileTab::ALL {
            assert_eq!(ProfileTab::from_value(Some(&tab.to_value())), Some(tab));
        }
        assert_eq!(ProfileTab::from_value(None), None);
        assert_eq!(ProfileTab::from_value(Some(&json!(null))), None);
        assert_eq!(
            ProfileTab::from_value(Some(&json!({"@type": "profileTabFuture"}))),
            None
        );
    }

    #[test]
    fn gallery_mapping_skips_posts_and_gifts() {
        assert_eq!(ProfileTab::Posts.shared_media_tab(), None);
        assert_eq!(ProfileTab::Gifts.shared_media_tab(), None);
        assert_eq!(
            ProfileTab::Voice.shared_media_tab(),
            Some(SharedMediaTab::Voice)
        );
    }
}
