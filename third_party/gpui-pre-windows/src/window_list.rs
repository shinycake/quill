// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

use std::sync::{PoisonError, RwLock};

/// The platform's open windows, shared with the vsync thread. Only copies leave the lock, so no
/// caller can hold it across `SendMessageW`, which waits for the UI thread, which may be waiting
/// in `push` or `remove`.
pub(crate) struct WindowList<W> {
    windows: RwLock<Vec<W>>,
}

/// What `WindowList::remove` found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Removal {
    pub(crate) was_listed: bool,
    pub(crate) now_empty: bool,
}

impl<W: Clone> WindowList<W> {
    pub(crate) fn new() -> Self {
        Self {
            windows: RwLock::new(Vec::new()),
        }
    }

    pub(crate) fn push(&self, window: W) {
        self.windows
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(window);
    }

    /// Removes the first window `is_target` matches.
    pub(crate) fn remove(&self, is_target: impl Fn(&W) -> bool) -> Removal {
        let mut windows = self.windows.write().unwrap_or_else(PoisonError::into_inner);
        let index = windows.iter().position(is_target);
        if let Some(index) = index {
            windows.remove(index);
        }
        Removal {
            was_listed: index.is_some(),
            now_empty: windows.is_empty(),
        }
    }

    /// A copy of the list as it is now, in whatever collection the caller picks
    /// (`WindowList::snapshot` in `platform.rs` keeps a few windows off the heap).
    pub(crate) fn collect<C: FromIterator<W>>(&self) -> C {
        self.windows
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    pub(crate) fn find(&self, matches: impl Fn(&W) -> bool) -> Option<W> {
        self.windows
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|window| matches(window))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(windows: &[i32]) -> WindowList<i32> {
        let list = WindowList::new();
        windows.iter().for_each(|window| list.push(*window));
        list
    }

    #[test]
    fn the_list_can_change_while_its_copy_is_walked() {
        let list = list(&[1, 2, 3]);
        let mut sent = Vec::new();

        for window in list.collect::<Vec<_>>() {
            sent.push(window);
            if window == 1 {
                list.remove(|listed| *listed == 2);
                list.push(4);
            }
        }

        assert_eq!(sent, [1, 2, 3]);
        assert_eq!(list.collect::<Vec<_>>(), [1, 3, 4]);
    }

    #[test]
    fn a_window_opened_later_is_in_the_next_copy() {
        let list = list(&[1]);
        let before = list.collect::<Vec<_>>();
        list.push(2);

        assert_eq!(before, [1]);
        assert_eq!(list.collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn closing_the_last_window_empties_the_list() {
        let list = list(&[1, 2]);

        assert_eq!(
            list.remove(|window| *window == 1),
            Removal {
                was_listed: true,
                now_empty: false
            }
        );
        assert_eq!(
            list.remove(|window| *window == 2),
            Removal {
                was_listed: true,
                now_empty: true
            }
        );
    }

    #[test]
    fn closing_a_window_that_is_not_listed_changes_nothing() {
        let list = list(&[1]);

        assert_eq!(
            list.remove(|window| *window == 7),
            Removal {
                was_listed: false,
                now_empty: false
            }
        );
        assert_eq!(list.collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn find_hands_back_a_copy_of_the_first_match() {
        let list = list(&[1, 2, 3]);

        assert_eq!(list.find(|window| *window > 1), Some(2));
        assert_eq!(list.find(|window| *window > 3), None);
    }
}
