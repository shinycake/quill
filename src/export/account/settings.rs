//! What a full account export includes (tdesktop `Settings::Type` and
//! `fullChats` in `export/export_settings.h`, and the options of
//! `export/view/export_view_settings.cpp`).
use crate::export::settings::{DEFAULT_SIZE_LIMIT, ExportFormat, MediaKind, MediaTypes, Skip};
use std::path::PathBuf;

/// The parts of the account an export can include. tdesktop's
/// "Miscellaneous data" (`OtherData`) is missing: it is a takeout-only
/// file TDLib cannot fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    PersonalInfo,
    Userpics,
    Contacts,
    Stories,
    ProfileMusic,
    Sessions,
    PersonalChats,
    BotChats,
    PrivateGroups,
    PrivateChannels,
    PublicGroups,
    PublicChannels,
}

impl Section {
    /// The chat types, in the box's order.
    pub const CHATS: [Section; 6] = [
        Self::PersonalChats,
        Self::BotChats,
        Self::PrivateGroups,
        Self::PrivateChannels,
        Self::PublicGroups,
        Self::PublicChannels,
    ];

    fn flag(self) -> u16 {
        1 << (self as u16)
    }

    pub fn is_chats(self) -> bool {
        Self::CHATS.contains(&self)
    }

    /// The checkbox label (tdesktop's wording).
    pub fn label(self) -> &'static str {
        match self {
            Self::PersonalInfo | Self::Userpics => "Account information",
            Self::Contacts => "Contacts list",
            Self::Stories => "Story archive",
            Self::ProfileMusic => "Music on Profiles",
            Self::Sessions => "Active sessions",
            Self::PersonalChats => "Personal chats",
            Self::BotChats => "Bot chats",
            Self::PrivateGroups => "Private groups",
            Self::PrivateChannels => "Private channels",
            Self::PublicGroups => "Public groups",
            Self::PublicChannels => "Public channels",
        }
    }

    /// The line under the checkbox, for the sections that have one.
    pub fn about(self) -> Option<&'static str> {
        Some(match self {
            Self::PersonalInfo | Self::Userpics => {
                "Your chosen display name, username, phone number and profile photos."
            }
            Self::Contacts => {
                "If you allow access, contacts are continuously synced with Telegram. You can \
                 adjust this in Settings › Privacy & Security on mobile devices."
            }
            Self::Stories => "All stories you posted from Telegram mobile apps.",
            Self::ProfileMusic => "All tracks you saved to your playlist.",
            Self::Sessions => {
                "Telegram keeps this to show your connected devices in Active sessions."
            }
            _ => return None,
        })
    }
}

/// A set of [`Section`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sections(u16);

impl Sections {
    pub fn none() -> Self {
        Self(0)
    }

    pub fn of(list: &[Section]) -> Self {
        list.iter().fold(Self::none(), |set, s| set.with(*s, true))
    }

    pub fn contains(self, section: Section) -> bool {
        self.0 & section.flag() != 0
    }

    pub fn with(self, section: Section, on: bool) -> Self {
        if on {
            Self(self.0 | section.flag())
        } else {
            Self(self.0 & !section.flag())
        }
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn any_chats(self) -> bool {
        Section::CHATS.iter().any(|s| self.contains(*s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSettings {
    /// Parent folder; the export creates `DataExport_<date>` in it.
    pub dir: PathBuf,
    pub format: ExportFormat,
    pub sections: Sections,
    /// Chat types exported in full; the rest keep only the viewer's own
    /// messages (tdesktop's `fullChats`).
    pub full_chats: Sections,
    pub media: MediaTypes,
    pub size_limit: i64,
}

impl AccountSettings {
    /// tdesktop's defaults: account information, contacts, stories, music,
    /// personal chats and private groups; personal and bot chats in full.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            format: ExportFormat::default(),
            sections: Sections::of(&[
                Section::PersonalInfo,
                Section::Userpics,
                Section::Contacts,
                Section::Stories,
                Section::ProfileMusic,
                Section::PersonalChats,
                Section::PrivateGroups,
            ]),
            full_chats: Sections::of(&[Section::PersonalChats, Section::BotChats]),
            media: MediaTypes::default(),
            size_limit: DEFAULT_SIZE_LIMIT,
        }
    }

    /// Whether the box shows `section` checked. "Account information"
    /// stands for personal info and profile photos together.
    pub fn is_on(&self, section: Section) -> bool {
        match section {
            Section::PersonalInfo | Section::Userpics => {
                self.sections.contains(Section::PersonalInfo)
                    && self.sections.contains(Section::Userpics)
            }
            other => self.sections.contains(other),
        }
    }

    pub fn set(&mut self, section: Section, on: bool) {
        self.sections = match section {
            Section::PersonalInfo | Section::Userpics => self
                .sections
                .with(Section::PersonalInfo, on)
                .with(Section::Userpics, on),
            other => self.sections.with(other, on),
        };
    }

    /// Whether a chat type offers the "Only my messages" choice: groups and
    /// channels do; personal and bot chats are always exported in full.
    pub fn offers_only_my(section: Section) -> bool {
        matches!(
            section,
            Section::PrivateGroups
                | Section::PrivateChannels
                | Section::PublicGroups
                | Section::PublicChannels
        )
    }

    /// Public groups and channels can only export the viewer's own
    /// messages; the box shows the choice checked and locked.
    pub fn only_my_locked(section: Section) -> bool {
        matches!(section, Section::PublicGroups | Section::PublicChannels)
    }

    /// Whether chats of this type keep only the viewer's own messages.
    pub fn only_my(&self, section: Section) -> bool {
        Self::offers_only_my(section)
            && (Self::only_my_locked(section) || !self.full_chats.contains(section))
    }

    pub fn set_only_my(&mut self, section: Section, on: bool) {
        if Self::offers_only_my(section) && !Self::only_my_locked(section) {
            self.full_chats = self.full_chats.with(section, !on);
        }
    }

    /// The media choices matter only when chats or profile music are
    /// exported; the box hides them otherwise.
    pub fn shows_media(&self) -> bool {
        self.sections.any_chats() || self.sections.contains(Section::ProfileMusic)
    }

    /// The Export button needs at least one section.
    pub fn can_start(&self) -> bool {
        !self.sections.is_empty()
    }

    /// Why a chat or music file is left out, if it is (kind, then size).
    pub fn skip_reason(&self, kind: MediaKind, size: i64) -> Option<Skip> {
        if !self.media.contains(kind) {
            Some(Skip::NotIncluded)
        } else if size > self.size_limit {
            Some(Skip::TooLarge)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> AccountSettings {
        AccountSettings::new(PathBuf::from("/tmp"))
    }

    #[test]
    fn defaults_match_tdesktop() {
        let s = settings();
        for on in [
            Section::PersonalInfo,
            Section::Contacts,
            Section::Stories,
            Section::ProfileMusic,
            Section::PersonalChats,
            Section::PrivateGroups,
        ] {
            assert!(s.is_on(on), "{on:?}");
        }
        for off in [
            Section::Sessions,
            Section::BotChats,
            Section::PrivateChannels,
            Section::PublicGroups,
            Section::PublicChannels,
        ] {
            assert!(!s.is_on(off), "{off:?}");
        }
        assert_eq!(s.format, ExportFormat::Html);
        assert!(s.media.contains(MediaKind::Photo));
        assert_eq!(s.size_limit, DEFAULT_SIZE_LIMIT);
        // Private groups keep only my messages until switched to full.
        assert!(s.only_my(Section::PrivateGroups));
        assert!(!s.only_my(Section::PersonalChats));
        assert!(!s.only_my(Section::BotChats));
    }

    #[test]
    fn account_information_toggles_both_parts() {
        let mut s = settings();
        s.set(Section::PersonalInfo, false);
        assert!(!s.sections.contains(Section::PersonalInfo));
        assert!(!s.sections.contains(Section::Userpics));
        assert!(!s.is_on(Section::Userpics));
        s.set(Section::Userpics, true);
        assert!(s.sections.contains(Section::PersonalInfo));
        assert!(s.is_on(Section::PersonalInfo));
    }

    #[test]
    fn only_my_messages_follows_the_chat_type() {
        let mut s = settings();
        s.set_only_my(Section::PrivateGroups, false);
        assert!(!s.only_my(Section::PrivateGroups));
        s.set_only_my(Section::PrivateChannels, true);
        assert!(s.only_my(Section::PrivateChannels));
        // Public groups and channels are locked to my messages.
        s.set_only_my(Section::PublicGroups, false);
        assert!(s.only_my(Section::PublicGroups));
        assert!(AccountSettings::only_my_locked(Section::PublicChannels));
        // Personal chats never offer it.
        s.set_only_my(Section::PersonalChats, true);
        assert!(!s.only_my(Section::PersonalChats));
        assert!(!AccountSettings::offers_only_my(Section::BotChats));
    }

    #[test]
    fn media_shows_only_for_chats_or_music_and_export_needs_a_section() {
        let mut s = settings();
        assert!(s.shows_media());
        for section in Section::CHATS {
            s.set(section, false);
        }
        assert!(s.shows_media(), "profile music still needs media choices");
        s.set(Section::ProfileMusic, false);
        assert!(!s.shows_media());
        assert!(s.can_start());
        for section in [Section::PersonalInfo, Section::Contacts, Section::Stories] {
            s.set(section, false);
        }
        assert!(!s.can_start());
        s.set(Section::Sessions, true);
        assert!(s.can_start());
    }

    #[test]
    fn skip_reason_checks_kind_then_size() {
        let mut s = settings();
        s.media = MediaTypes::none().with(MediaKind::File, true);
        s.size_limit = 10;
        assert_eq!(s.skip_reason(MediaKind::Photo, 1), Some(Skip::NotIncluded));
        assert_eq!(s.skip_reason(MediaKind::File, 11), Some(Skip::TooLarge));
        assert_eq!(s.skip_reason(MediaKind::File, 10), None);
    }
}
