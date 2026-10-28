//! Every setting Quill shows, declared once: its stable id, the page and
//! section it sits in, its title and extra search words. The settings pages
//! render rows from these entries and search ranks the same entries, so a
//! setting can't be shown without being searchable (tdesktop's
//! `settings_builder.h` pairs widgets and search entries the same way).
//!
//! Each page's entries live in `entries/<page>.rs`.

mod entries;
pub mod fuzzy;
mod recent;
mod search;
mod synonyms;

pub use recent::{MAX_RECENT, RecentSearches};
pub use search::{Hit, SearchContext, Target, search};

use crate::lang::{Key, keys};

/// A page of the settings window, in sidebar order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Account,
    Notifications,
    Privacy,
    Devices,
    AgentAccess,
    Chats,
    Appearance,
    Folders,
    DataStorage,
    Connection,
    Calls,
    Power,
    Language,
    Shortcuts,
    System,
    Telegram,
    About,
    Advanced,
}

impl Page {
    pub const ALL: [Page; 18] = [
        Page::Account,
        Page::Notifications,
        Page::Privacy,
        Page::Devices,
        Page::AgentAccess,
        Page::Chats,
        Page::Appearance,
        Page::Folders,
        Page::DataStorage,
        Page::Connection,
        Page::Calls,
        Page::Power,
        Page::Language,
        Page::Shortcuts,
        Page::System,
        Page::Telegram,
        Page::About,
        Page::Advanced,
    ];

    /// Stable id, the prefix of its entries' ids.
    pub fn id(self) -> &'static str {
        match self {
            Page::Account => "account",
            Page::Notifications => "notifications",
            Page::Privacy => "privacy",
            Page::Devices => "devices",
            Page::AgentAccess => "agent_access",
            Page::Chats => "chats",
            Page::Appearance => "appearance",
            Page::Folders => "folders",
            Page::DataStorage => "data",
            Page::Connection => "connection",
            Page::Calls => "calls",
            Page::Power => "power",
            Page::Language => "language",
            Page::Shortcuts => "shortcuts",
            Page::System => "system",
            Page::Telegram => "telegram",
            Page::About => "about",
            Page::Advanced => "advanced",
        }
    }

    /// The page with this id.
    pub fn from_id(id: &str) -> Option<Page> {
        Page::ALL.into_iter().find(|page| page.id() == id)
    }

    pub fn title(self) -> Key {
        match self {
            Page::Account => keys::Q_SETTINGS_PAGE_ACCOUNT,
            Page::Notifications => keys::SETTINGS_SECTION_NOTIFY,
            Page::Privacy => keys::SETTINGS_SECTION_PRIVACY,
            Page::Devices => keys::Q_SETTINGS_PAGE_DEVICES,
            Page::AgentAccess => keys::Q_AGENT_ACCESS,
            Page::Chats => keys::Q_SETTINGS_PAGE_CHATS,
            Page::Appearance => keys::Q_SETTINGS_PAGE_APPEARANCE,
            Page::Folders => keys::Q_SETTINGS_PAGE_FOLDERS,
            Page::DataStorage => keys::Q_SETTINGS_PAGE_DATA,
            Page::Connection => keys::Q_SETTINGS_PAGE_CONNECTION,
            Page::Calls => keys::Q_SETTINGS_PAGE_CALLS,
            Page::Power => keys::Q_SETTINGS_PAGE_POWER,
            Page::Language => keys::Q_SETTINGS_PAGE_LANGUAGE,
            Page::Shortcuts => keys::Q_SETTINGS_PAGE_SHORTCUTS,
            Page::System => keys::Q_SETTINGS_PAGE_SYSTEM,
            Page::Telegram => keys::Q_SETTINGS_PAGE_TELEGRAM,
            Page::About => keys::Q_SETTINGS_PAGE_ABOUT,
            Page::Advanced => keys::Q_SETTINGS_PAGE_ADVANCED,
        }
    }

    /// The page's own entries, in display order.
    pub fn entries(self) -> &'static [Entry] {
        entries::of(self)
    }

    /// Whether the page is in the sidebar and in search. Agent access shows
    /// only while it is on; off, its one trace is the switch on Advanced
    /// ([`AGENT_SWITCH`]).
    pub fn shown(self, agent_access: bool) -> bool {
        self != Page::AgentAccess || agent_access
    }
}

/// The id of Advanced's "Agent access" switch, the way to turn Agent access
/// on. It shows only while Agent access is off; on, the switch is the first
/// row of the Agent access page.
pub const AGENT_SWITCH: &str = "advanced.agent_access";

/// Operating systems a setting is offered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Platforms(u8);

impl Platforms {
    pub const MACOS: Platforms = Platforms(1);
    pub const WINDOWS: Platforms = Platforms(2);
    pub const LINUX: Platforms = Platforms(4);
    pub const ALL: Platforms = Platforms(7);

    pub const fn or(self, other: Platforms) -> Platforms {
        Platforms(self.0 | other.0)
    }

    pub fn contains(self, other: Platforms) -> bool {
        self.0 & other.0 == other.0
    }

    /// The platform this build runs on.
    pub fn current() -> Platforms {
        if cfg!(target_os = "macos") {
            Platforms::MACOS
        } else if cfg!(target_os = "windows") {
            Platforms::WINDOWS
        } else {
            Platforms::LINUX
        }
    }
}

/// One setting (a row, a group of controls or a link to a screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Stable id, `<page id>.<name>` (`privacy.last_seen`). Search history
    /// and deep links store it.
    pub id: &'static str,
    pub page: Page,
    /// The group title the row sits under, if any.
    pub section: Option<Key>,
    pub title: Key,
    /// Extra English words people search with ("dark" for night mode).
    pub keywords: &'static [&'static str],
    pub platforms: Platforms,
}

impl Entry {
    pub const fn new(id: &'static str, page: Page, title: Key) -> Entry {
        Entry {
            id,
            page,
            section: None,
            title,
            keywords: &[],
            platforms: Platforms::ALL,
        }
    }

    pub const fn section(mut self, section: Key) -> Entry {
        self.section = Some(section);
        self
    }

    pub const fn keywords(mut self, keywords: &'static [&'static str]) -> Entry {
        self.keywords = keywords;
        self
    }

    pub const fn only(mut self, platforms: Platforms) -> Entry {
        self.platforms = platforms;
        self
    }

    /// Whether the row shows, given whether Agent access is on.
    pub fn shown(&self, agent_access: bool) -> bool {
        self.page.shown(agent_access) && (self.id != AGENT_SWITCH || !agent_access)
    }

    /// How deep the entry sits: rows on a page are 1, rows in a section 2.
    pub fn depth(&self) -> u8 {
        if self.section.is_some() { 2 } else { 1 }
    }
}

/// Every entry of every page, in sidebar then display order.
pub fn all() -> impl Iterator<Item = &'static Entry> {
    Page::ALL.into_iter().flat_map(|page| page.entries().iter())
}

/// The entry with this id.
pub fn find(id: &str) -> Option<&'static Entry> {
    all().find(|entry| entry.id == id)
}

/// The page or entry with this id (search history, links).
pub fn target(id: &str) -> Option<Target> {
    Page::from_id(id)
        .map(Target::Page)
        .or_else(|| find(id).map(Target::Entry))
}

#[cfg(test)]
mod tests;
