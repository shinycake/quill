//! The order of the folder tabs: the chat folders with the main list ("All
//! chats") at `main_chat_list_position` (TDLib `updateChatFolders`,
//! `schema/td_api.tl:11425`), and the drag-reorder rules of tdesktop's tab
//! strip and folder sidebar (`ui/widgets/chat_filters_tabs_strip.cpp`,
//! `window/window_filters_menu.cpp`).
//!
//! Only Premium accounts may move "All chats": without Premium tdesktop pins
//! it to the first slot (`addPinnedInterval(0, 1)`) and, before saving any
//! other move, puts it back in front (`ChatFilters::moveAllToFront`), so the
//! saved `main_chat_list_position` is 0, as `reorderChatFolders` requires
//! (`schema/td_api.tl:14361`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderTab {
    Main,
    Folder(i32),
}

impl FolderTab {
    /// The open-tab id the app keeps (`None` is the main list).
    pub fn folder_id(self) -> Option<i32> {
        match self {
            Self::Main => None,
            Self::Folder(id) => Some(id),
        }
    }
}

/// What `reorderChatFolders` takes: the folder ids in order and the index
/// of the main list among them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderOrder {
    pub folder_ids: Vec<i32>,
    pub main_position: usize,
}

impl FolderOrder {
    /// The tabs in display order, the main list at its position (clamped to
    /// the end, as tdesktop inserts it at the server index).
    pub fn tabs(&self) -> Vec<FolderTab> {
        let main_at = self.main_position.min(self.folder_ids.len());
        let mut tabs: Vec<FolderTab> = self
            .folder_ids
            .iter()
            .map(|id| FolderTab::Folder(*id))
            .collect();
        tabs.insert(main_at, FolderTab::Main);
        tabs
    }

    pub fn from_tabs(tabs: &[FolderTab]) -> Self {
        let mut folder_ids = Vec::with_capacity(tabs.len());
        let mut main_position = None;
        for tab in tabs {
            match tab {
                FolderTab::Main => main_position = main_position.or(Some(folder_ids.len())),
                FolderTab::Folder(id) => folder_ids.push(*id),
            }
        }
        Self {
            main_position: main_position.unwrap_or(0),
            folder_ids,
        }
    }

    /// Moves the tab `moved` to the slot of the tab `onto` (tdesktop
    /// `base::reorder`: the dragged tab takes the target's index and the
    /// tabs between shift by one). Without Premium the main list goes back
    /// to the front first and stays there: it cannot be moved, and nothing
    /// can take its slot. `None` when the move changes nothing or is not
    /// allowed.
    pub fn moved(&self, moved: FolderTab, onto: FolderTab, premium: bool) -> Option<Self> {
        let base = if premium {
            self.clone()
        } else {
            Self {
                folder_ids: self.folder_ids.clone(),
                main_position: 0,
            }
        };
        let mut tabs = base.tabs();
        let from = tabs.iter().position(|tab| *tab == moved)?;
        let to = tabs.iter().position(|tab| *tab == onto)?;
        if !premium && (moved == FolderTab::Main || to == 0) {
            return None;
        }
        if from == to {
            return None;
        }
        let tab = tabs.remove(from);
        tabs.insert(to, tab);
        let next = Self::from_tabs(&tabs);
        (next != *self).then_some(next)
    }

    /// Whether `tab` can be dragged: every folder, and the main list only
    /// with Premium.
    pub fn can_drag(tab: FolderTab, premium: bool) -> bool {
        premium || tab != FolderTab::Main
    }
}

#[cfg(test)]
mod tests {
    use super::{FolderOrder, FolderTab};

    fn order(ids: &[i32], main: usize) -> FolderOrder {
        FolderOrder {
            folder_ids: ids.to_vec(),
            main_position: main,
        }
    }

    #[test]
    fn tabs_put_main_at_its_position() {
        use FolderTab::*;
        assert_eq!(order(&[4, 9], 0).tabs(), vec![Main, Folder(4), Folder(9)]);
        assert_eq!(order(&[4, 9], 1).tabs(), vec![Folder(4), Main, Folder(9)]);
        assert_eq!(order(&[4, 9], 2).tabs(), vec![Folder(4), Folder(9), Main]);
        assert_eq!(order(&[4], 5).tabs(), vec![Folder(4), Main]);
        assert_eq!(order(&[], 3).tabs(), vec![Main]);
    }

    #[test]
    fn from_tabs_round_trips() {
        let o = order(&[4, 9, 2], 2);
        assert_eq!(FolderOrder::from_tabs(&o.tabs()), o);
    }

    #[test]
    fn premium_moves_main_among_folders() {
        use FolderTab::*;
        let o = order(&[4, 9], 0);
        assert_eq!(o.moved(Main, Folder(9), true), Some(order(&[4, 9], 2)));
        assert_eq!(o.moved(Main, Folder(4), true), Some(order(&[4, 9], 1)));
        let o = order(&[4, 9], 2);
        assert_eq!(o.moved(Folder(4), Main, true), Some(order(&[9, 4], 1)));
        assert_eq!(o.moved(Main, Folder(4), true), Some(order(&[4, 9], 0)));
    }

    #[test]
    fn free_accounts_keep_main_first() {
        use FolderTab::*;
        let o = order(&[4, 9], 0);
        assert_eq!(o.moved(Main, Folder(9), false), None);
        assert_eq!(o.moved(Folder(9), Main, false), None);
        assert_eq!(
            o.moved(Folder(9), Folder(4), false),
            Some(order(&[9, 4], 0))
        );
        assert!(!FolderOrder::can_drag(Main, false));
        assert!(FolderOrder::can_drag(Main, true));
        assert!(FolderOrder::can_drag(Folder(4), false));
    }

    #[test]
    fn free_account_move_puts_main_back_in_front() {
        use FolderTab::*;
        let o = order(&[4, 9, 2], 2);
        assert_eq!(
            o.moved(Folder(2), Folder(4), false),
            Some(order(&[2, 4, 9], 0))
        );
    }

    #[test]
    fn dropping_on_itself_changes_nothing() {
        use FolderTab::*;
        let o = order(&[4, 9], 1);
        assert_eq!(o.moved(Folder(4), Folder(4), true), None);
        assert_eq!(o.moved(Main, Main, true), None);
        assert_eq!(o.moved(Folder(7), Folder(4), true), None);
    }
}
