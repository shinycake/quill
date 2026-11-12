//! What the keys and the pointer selected in one list (the chat list, or
//! its search results), as one value.
//!
//! tdesktop paints the keyboard's row with the hover fill and takes it back
//! on the next mouse move (`mouseMoveEvent` → `selectByMouse`). The pointer's
//! row is remembered under the keys' row, so a mouse move hands the
//! selection back to the row the pointer rests on even though no new hover
//! event fires for it.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Pick<R> {
    #[default]
    None,
    /// The pointer rests on `R`.
    Pointer(R),
    /// The keys selected `row`; `pointer` is the row under the pointer, if any.
    Keys { row: R, pointer: Option<R> },
}

impl<R: Copy + PartialEq> Pick<R> {
    /// The row the keys selected, painted until the mouse moves.
    pub(super) fn keys(self) -> Option<R> {
        match self {
            Self::Keys { row, .. } => Some(row),
            _ => None,
        }
    }

    /// The row under the pointer.
    pub(super) fn pointer(self) -> Option<R> {
        match self {
            Self::None => None,
            Self::Pointer(row) => Some(row),
            Self::Keys { pointer, .. } => pointer,
        }
    }

    /// The row Enter opens and the arrows move from: the keys', else the
    /// pointer's.
    pub(super) fn current(self) -> Option<R> {
        self.keys().or(self.pointer())
    }

    /// The keys select `row`.
    pub(super) fn select_by_keys(&mut self, row: R) {
        *self = Self::Keys {
            row,
            pointer: self.pointer(),
        };
    }

    /// The keys' selection is dropped; the pointer's stays.
    pub(super) fn drop_keys(&mut self) {
        *self = match self.pointer() {
            Some(row) => Self::Pointer(row),
            None => Self::None,
        };
    }

    /// The pointer entered or left `row`.
    pub(super) fn hover(&mut self, row: R, hovered: bool) {
        let pointer = if hovered {
            Some(row)
        } else if self.pointer() == Some(row) {
            None
        } else {
            return;
        };
        *self = match (*self, pointer) {
            (Self::Keys { row: keys, .. }, pointer) => Self::Keys { row: keys, pointer },
            (_, Some(row)) => Self::Pointer(row),
            (_, None) => Self::None,
        };
    }

    /// The pointer moved over the list: it takes the selection back.
    /// Returns whether the keys had one.
    pub(super) fn mouse_moved(&mut self) -> bool {
        let had = self.keys().is_some();
        self.drop_keys();
        had
    }

    /// Nothing is selected (a row was opened, or the list was replaced).
    pub(super) fn clear(&mut self) {
        *self = Self::None;
    }
}

#[cfg(test)]
mod tests {
    use super::Pick;

    #[test]
    fn the_keys_cover_the_pointer_and_a_mouse_move_gives_it_back() {
        let mut pick = Pick::default();
        pick.hover(3, true);
        assert_eq!(pick.current(), Some(3));
        pick.select_by_keys(5);
        assert_eq!(pick.current(), Some(5), "Enter opens the keys' row");
        assert_eq!(pick.pointer(), Some(3));
        assert!(pick.mouse_moved());
        assert_eq!(pick.current(), Some(3), "the pointer's row, no new hover");
        assert!(!pick.mouse_moved(), "nothing left to take back");
    }

    #[test]
    fn leaving_a_row_forgets_only_that_row() {
        let mut pick = Pick::default();
        pick.hover(1, true);
        pick.hover(2, true);
        pick.hover(1, false);
        assert_eq!(pick.pointer(), Some(2), "a late leave of the old row");
        pick.hover(2, false);
        assert_eq!(pick, Pick::None);
        pick.select_by_keys(4);
        pick.hover(9, true);
        pick.hover(9, false);
        assert_eq!(pick.keys(), Some(4));
        assert_eq!(pick.pointer(), None);
    }

    #[test]
    fn dropping_the_keys_keeps_the_pointer() {
        let mut pick = Pick::default();
        pick.select_by_keys(4);
        pick.drop_keys();
        assert_eq!(pick, Pick::None);
        pick.hover(7, true);
        pick.select_by_keys(8);
        pick.drop_keys();
        assert_eq!(pick, Pick::Pointer(7));
        pick.clear();
        assert_eq!(pick, Pick::None);
    }
}
