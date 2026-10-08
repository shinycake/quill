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
        let joined = self
            .names
            .iter()
            .map(|name| name.title.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        match self.more {
            0 => joined,
            1 => format!("{joined} and 1 more chat"),
            more => format!("{joined} and {more} more chats"),
        }
    }

    /// The tail after the names ("and 2 more chats"), if any.
    pub fn more_text(&self) -> Option<String> {
        match self.more {
            0 => None,
            1 => Some("and 1 more chat".into()),
            more => Some(format!("and {more} more chats")),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
