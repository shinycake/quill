//! The "Archived chats" row of the chat list, ported from tdesktop.
//!
//! tdesktop shows the archive as a pinned-top entry of the main list
//! (`Data::Folder`, fixed on top) with the archive userpic, the names of
//! the most recent archived chats as its preview (`ComposeFolderListEntryText`,
//! `data/data_folder.cpp:38-108`; unread chats' names in semibold) and a
//! muted unread badge counting unread *chats* (`Folder::chatListBadgesState`,
//! `data/data_folder.cpp:385-399`). Two settings change it
//! (`main/main_session_settings.h:102-108`, context menu
//! `window/window_peer_menu.cpp:2034-2072`):
//!
//! - `archiveCollapsed`: the row shrinks to a slim 37px bar
//!   (`st::dialogsImportantBarHeight`, `InnerWidget::paintCollapsedRow`);
//! - `archiveInMainMenu`: the row leaves the list and an "Archived chats"
//!   entry appears in the main menu (`window/window_main_menu.cpp:508-526`).
//!
//! This module is the pure part: what the row says and which form it takes.
#![deny(clippy::indexing_slicing)]

/// `kShowChatNamesCount`.
pub const SHOWN_NAMES: usize = 8;
/// `st::dialogsImportantBarHeight`.
pub const COLLAPSED_BAR_HEIGHT: f32 = 37.0;

/// How the archive appears in the chat list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveRowMode {
    /// Not in the list (no archived chats, moved to the main menu, or a
    /// filtered view such as a folder).
    Hidden,
    /// The full row on top.
    Row,
    /// The slim bar on top.
    Collapsed,
}

/// `InnerWidget::refreshWithCollapsedRows` / `needCollapsedRowsRefresh`:
/// `in_main_menu` wins over `collapsed`; filtered lists never show it.
pub fn row_mode(
    has_archived: bool,
    collapsed: bool,
    in_main_menu: bool,
    unfiltered: bool,
) -> ArchiveRowMode {
    if !has_archived || !unfiltered || in_main_menu {
        ArchiveRowMode::Hidden
    } else if collapsed {
        ArchiveRowMode::Collapsed
    } else {
        ArchiveRowMode::Row
    }
}

/// Whether the main menu carries the "Archived chats" entry
/// (`checkArchive` in `window_main_menu.cpp`).
pub fn show_in_main_menu(has_archived: bool, in_main_menu: bool) -> bool {
    has_archived && in_main_menu
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveName {
    pub title: String,
    pub unread: bool,
}

/// What the row prints: the newest archived chats' names plus a "more"
/// tail, and the unread-chat count for the badge.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchiveRowSummary {
    pub names: Vec<ArchiveName>,
    /// Archived chats not named in the preview.
    pub more: usize,
    /// Chats with unread messages (or marked unread).
    pub unread_chats: usize,
}

/// One archived chat as the summary needs it.
#[derive(Debug, Clone)]
pub struct ArchivedChat {
    pub title: String,
    /// Last message date (0 when there is none).
    pub date: i32,
    pub unread: bool,
}

impl ArchiveRowSummary {
    /// `None` for an empty archive. `Folder::reorderLastHistories` keeps
    /// the `SHOWN_NAMES` newest chats by last-message date;
    /// `ComposeFolderListEntryText` drops the last name when exactly one
    /// chat would remain ("and 1 more") and appends the count otherwise.
    pub fn build(chats: &[ArchivedChat]) -> Option<Self> {
        if chats.is_empty() {
            return None;
        }
        let mut newest: Vec<&ArchivedChat> = chats.iter().collect();
        // Stable: ties keep list order.
        newest.sort_by_key(|chat| std::cmp::Reverse(chat.date));
        newest.truncate(SHOWN_NAMES);
        let count = chats.len();
        let throw_away_last = newest.len() > 1 && count == newest.len() + 1;
        if throw_away_last {
            newest.pop();
        }
        let names: Vec<ArchiveName> = newest
            .iter()
            .map(|chat| ArchiveName {
                title: chat.title.clone(),
                unread: chat.unread,
            })
            .collect();
        Some(Self {
            more: count - names.len(),
            names,
            unread_chats: chats.iter().filter(|chat| chat.unread).count(),
        })
    }

    /// The plain preview text ("A, B, C and 2 more chats").
    pub fn text(&self) -> String {
        self.text_and_unread().0
    }

    /// [`Self::text`] and the byte ranges of the unread chats' names in it.
    /// Each name is isolated (a Hebrew title cannot carry the commas or the
    /// "and 2 more chats" over to its side), and the tail is the language
    /// pack's `lng_archived_last`.
    pub fn text_and_unread(&self) -> (String, Vec<std::ops::Range<usize>>) {
        let mut joined = String::new();
        let mut unread = Vec::new();
        for (index, name) in self.names.iter().enumerate() {
            if index > 0 {
                joined.push_str(", ");
            }
            let start = joined.len();
            joined.push_str(&crate::text::isolate(&name.title));
            if name.unread {
                unread.push(start..joined.len());
            }
        }
        if self.more == 0 {
            return (joined, unread);
        }
        let text = crate::lang::trn_with(
            crate::lang::keys::ARCHIVED_LAST,
            self.more,
            &[("chats", joined.as_str())],
        );
        let shift = text.find(joined.as_str()).unwrap_or_default();
        let unread = unread
            .into_iter()
            .map(|range| range.start + shift..range.end + shift)
            .collect();
        (text, unread)
    }

    /// Badge text of the muted unread counter; `None` when nothing is
    /// unread.
    pub fn badge(&self) -> Option<String> {
        (self.unread_chats > 0).then(|| {
            if self.unread_chats > 999 {
                format!("{}K", self.unread_chats / 1000)
            } else {
                self.unread_chats.to_string()
            }
        })
    }
}

/// Title of the "How does it work?" box (`lng_archive_hint_title`).
pub const ARCHIVE_HINT_TITLE: &str = "This is your Archive";

/// The three sections of the box (`lng_archive_hint_section_*`).
pub const ARCHIVE_HINT_SECTIONS: [(&str, &str); 3] = [
    (
        "Archived Chats",
        "Move any chat into your Archive and back by swiping on it.",
    ),
    (
        "Hiding Archive",
        "Hide the Archive from your Main screen by swiping on it.",
    ),
    (
        "Stories",
        "Archive Stories from your contacts separately from chats with them.",
    ),
];

/// Whether a new message pulls an unmuted archived chat back to the list.
/// tdesktop passes `unarchiveOnNewMessageCurrent() != None`, which is the
/// inverse of TDLib's `keep_unmuted_chats_archived`. Until the settings
/// arrive the account default (not kept) applies.
pub fn unarchive_on_new_message(keep_unmuted_chats_archived: Option<bool>) -> bool {
    !keep_unmuted_chats_archived.unwrap_or(false)
}

/// The explanation under the title (`lng_archive_hint_about` and
/// `lng_archive_hint_about_unmuted`, without the trailing link).
pub fn archive_hint_about(unarchive_on_new_message: bool) -> &'static str {
    if unarchive_on_new_message {
        "When you receive a new message, muted chats will remain in the Archive, while unmuted chats will be moved to Chats."
    } else {
        "Archived chats will remain in the Archive when you receive a new message."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_hint_text_follows_the_setting() {
        assert!(unarchive_on_new_message(None));
        assert!(unarchive_on_new_message(Some(false)));
        assert!(!unarchive_on_new_message(Some(true)));
        assert!(archive_hint_about(true).contains("unmuted chats will be moved"));
        assert!(archive_hint_about(false).starts_with("Archived chats will remain"));
        assert_eq!(ARCHIVE_HINT_SECTIONS.len(), 3);
    }

    fn chat(title: &str, date: i32, unread: bool) -> ArchivedChat {
        ArchivedChat {
            title: title.into(),
            date,
            unread,
        }
    }

    #[test]
    fn empty_archive_has_no_row() {
        assert!(ArchiveRowSummary::build(&[]).is_none());
        assert_eq!(row_mode(false, false, false, true), ArchiveRowMode::Hidden);
    }

    #[test]
    fn names_are_newest_first_and_unread_flagged() {
        let summary = ArchiveRowSummary::build(&[
            chat("Old", 10, false),
            chat("New", 30, true),
            chat("Mid", 20, false),
        ])
        .unwrap();
        assert_eq!(summary.text(), "New, Mid, Old");
        assert!(summary.names[0].unread);
        assert!(!summary.names[1].unread);
        assert_eq!(summary.unread_chats, 1);
        assert_eq!(summary.badge().as_deref(), Some("1"));
    }

    #[test]
    fn hebrew_titles_are_isolated_and_unread_ranges_follow_them() {
        let summary =
            ArchiveRowSummary::build(&[chat("Old", 10, false), chat("שלום", 30, true)]).unwrap();
        let (text, unread) = summary.text_and_unread();
        assert_eq!(text, "\u{2068}שלום\u{2069}, Old");
        assert_eq!(unread.len(), 1);
        assert_eq!(text.get(unread[0].clone()), Some("\u{2068}שלום\u{2069}"));
    }

    #[test]
    fn only_eight_names_then_a_count() {
        let chats: Vec<ArchivedChat> = (0..12)
            .map(|i| chat(&format!("C{i}"), 100 - i, false))
            .collect();
        let summary = ArchiveRowSummary::build(&chats).unwrap();
        assert_eq!(summary.names.len(), 8);
        assert_eq!(summary.more, 4);
        assert!(summary.text().ends_with("and 4 more chats"));
    }

    #[test]
    fn one_leftover_chat_is_named_instead_of_counted() {
        // 9 chats: tdesktop drops the 8th name and says "and 2 more".
        let chats: Vec<ArchivedChat> = (0..9)
            .map(|i| chat(&format!("C{i}"), 100 - i, false))
            .collect();
        let summary = ArchiveRowSummary::build(&chats).unwrap();
        assert_eq!(summary.names.len(), 7);
        assert_eq!(summary.more, 2);
    }

    #[test]
    fn badge_is_absent_without_unread_and_abbreviates() {
        let quiet = ArchiveRowSummary::build(&[chat("A", 1, false)]).unwrap();
        assert!(quiet.badge().is_none());
        let loud = ArchiveRowSummary {
            unread_chats: 1500,
            ..Default::default()
        };
        assert_eq!(loud.badge().as_deref(), Some("1K"));
    }

    #[test]
    fn modes_follow_the_two_settings() {
        assert_eq!(row_mode(true, false, false, true), ArchiveRowMode::Row);
        assert_eq!(row_mode(true, true, false, true), ArchiveRowMode::Collapsed);
        // In the main menu the row (and bar) are gone.
        assert_eq!(row_mode(true, true, true, true), ArchiveRowMode::Hidden);
        assert_eq!(row_mode(true, false, true, true), ArchiveRowMode::Hidden);
        // Folders and category filters never show it.
        assert_eq!(row_mode(true, false, false, false), ArchiveRowMode::Hidden);
        assert!(show_in_main_menu(true, true));
        assert!(!show_in_main_menu(false, true));
        assert!(!show_in_main_menu(true, false));
    }
}
