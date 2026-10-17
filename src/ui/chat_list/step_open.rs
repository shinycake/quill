//! Keyboard chat stepping (Alt+Up/Down, Ctrl+Tab, Ctrl+PgUp/PgDn): which
//! chat comes next and which list item to reveal.
//!
//! Telegram Desktop (`Dialogs::InnerWidget::chatListEntryAfter/Before`)
//! returns nothing past the last or before the first row, so the keys stop at
//! the ends instead of wrapping.

use super::*;

/// The list item to open when stepping by `step` from the open chat. `chats`
/// holds the list item index of each chat row in order; `open` is the open
/// chat's position among them. Returns the position in `chats`, or `None`
/// when the step would run past an end.
pub(super) fn step_target(chats: usize, open: Option<usize>, step: isize) -> Option<usize> {
    if chats == 0 {
        return None;
    }
    match open {
        Some(index) => {
            let next = isize::try_from(index).ok()?.checked_add(step)?;
            usize::try_from(next).ok().filter(|next| *next < chats)
        }
        None if step >= 0 => Some(0),
        None => chats.checked_sub(1),
    }
}

impl QuillApp {
    /// Open the chat `step` rows away from the open one in the list as
    /// currently shown (folder, filter and archive state included), stopping
    /// at the ends, and scroll the list so the new row is visible. With no
    /// chat open, starts at the top (or the bottom for a backwards step).
    pub(in crate::ui) fn step_open_chat(
        &mut self,
        step: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chats: Vec<(usize, ChatId)> = self
            .chat_list
            .items
            .iter()
            .enumerate()
            .filter_map(|(item, entry)| match entry {
                ChatListItem::Chat { id, .. } => Some((item, *id)),
                _ => None,
            })
            .collect();
        let open = self.session().and_then(|s| s.open_chat);
        let current = open.and_then(|open| chats.iter().position(|(_, id)| *id == open));
        let Some(&(item, id)) =
            step_target(chats.len(), current, step).and_then(|next| chats.get(next))
        else {
            return;
        };
        self.select_listed_chat(id, window, cx);
        self.chat_list
            .scroll
            .scroll_to_item(item, ScrollStrategy::Nearest);
    }
}

#[cfg(test)]
mod tests {
    use super::step_target;

    #[test]
    fn steps_within_the_list() {
        assert_eq!(step_target(5, Some(2), 1), Some(3));
        assert_eq!(step_target(5, Some(2), -1), Some(1));
    }

    #[test]
    fn stops_at_the_ends_like_tdesktop() {
        assert_eq!(step_target(5, Some(4), 1), None);
        assert_eq!(step_target(5, Some(0), -1), None);
    }

    #[test]
    fn starts_at_an_end_when_no_chat_is_open() {
        assert_eq!(step_target(5, None, 1), Some(0));
        assert_eq!(step_target(5, None, -1), Some(4));
        assert_eq!(step_target(0, None, 1), None);
    }
}
