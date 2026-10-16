//! The multi-select action bar above the chat list.
//!
//! Every action is an icon button with a tooltip; the ones that do not fit
//! a 240 px sidebar next to the count live in a kit overflow menu. The bar is
//! built from `chat_list.selected` on each render and holds no state.

use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use quill::lang::{keys, tr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectBarAction {
    Pin,
    Read,
    Mute,
    Archive,
    SelectUnread,
    Delete,
}

/// What the bulk handlers will do for the current selection. Each toggle
/// is all-or-nothing: if any checked chat lacks the state, apply it to all,
/// otherwise remove it from all (the same `any` predicates as
/// `toggle_selected_pins`, `toggle_selected_mute` and
/// `toggle_selected_archive`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SelectionFacts {
    any_unpinned: bool,
    any_unmuted: bool,
    any_unarchived: bool,
}

impl SelectionFacts {
    /// `(pinned, muted, archived)` for each checked chat.
    pub(super) fn from_states(states: impl IntoIterator<Item = (bool, bool, bool)>) -> Self {
        let mut facts = Self {
            any_unpinned: false,
            any_unmuted: false,
            any_unarchived: false,
        };
        for (pinned, muted, archived) in states {
            facts.any_unpinned |= !pinned;
            facts.any_unmuted |= !muted;
            facts.any_unarchived |= !archived;
        }
        facts
    }

    pub(super) fn pin_all(self) -> bool {
        self.any_unpinned
    }

    pub(super) fn mute_all(self) -> bool {
        self.any_unmuted
    }

    pub(super) fn archive_all(self) -> bool {
        self.any_unarchived
    }
}

impl SelectBarAction {
    /// Icon buttons shown in the bar, in focus order.
    pub(super) const INLINE: [SelectBarAction; 4] = [
        SelectBarAction::Pin,
        SelectBarAction::Read,
        SelectBarAction::Mute,
        SelectBarAction::Archive,
    ];
    /// Entries of the "More" menu.
    pub(super) const OVERFLOW: [SelectBarAction; 2] =
        [SelectBarAction::SelectUnread, SelectBarAction::Delete];

    fn label(self, facts: SelectionFacts) -> &'static str {
        match self {
            Self::Pin if facts.pin_all() => tr(keys::CONTEXT_PIN_MSG),
            Self::Pin => tr(keys::Q_CHAT_UNPIN),
            Self::Read => tr(keys::Q_CHATLIST_READ),
            Self::Mute if facts.mute_all() => tr(keys::Q_CHATLIST_MUTE),
            Self::Mute => tr(keys::Q_CHAT_UNMUTE_NOTIFICATIONS),
            Self::Archive if facts.archive_all() => tr(keys::Q_CHATLIST_ARCHIVE),
            Self::Archive => tr(keys::Q_CHAT_UNARCHIVE),
            Self::SelectUnread => tr(keys::Q_CHATLIST_SELECT_UNREAD),
            Self::Delete => tr(keys::CHAT_LINK_DELETE),
        }
    }

    fn icon(self, facts: SelectionFacts) -> IconName {
        match self {
            Self::Pin if facts.pin_all() => IconName::Pin,
            Self::Pin => IconName::PinOff,
            Self::Read => IconName::CheckCheck,
            Self::Mute if facts.mute_all() => IconName::BellOff,
            Self::Mute => IconName::Bell,
            Self::Archive if facts.archive_all() => IconName::Archive,
            Self::Archive => IconName::ArchiveRestore,
            Self::SelectUnread => IconName::MessageSquareDot,
            Self::Delete => IconName::Trash,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Pin => "select-pin",
            Self::Read => "select-read",
            Self::Mute => "select-mute",
            Self::Archive => "select-archive",
            Self::SelectUnread => "select-unread",
            Self::Delete => "select-delete",
        }
    }

    fn run(self, app: &mut QuillApp, cx: &mut Context<QuillApp>) {
        match self {
            Self::Pin => app.toggle_selected_pins(cx),
            Self::Read => app.mark_selected_read(cx),
            Self::Mute => app.toggle_selected_mute(cx),
            Self::Archive => app.toggle_selected_archive(cx),
            Self::SelectUnread => app.select_unread_chats(cx),
            Self::Delete => app.delete_selected_chats(cx),
        }
    }
}

impl QuillApp {
    /// Facts for the checked chats, read from the live session on each call.
    fn selection_facts(&self) -> SelectionFacts {
        let pins = self.selected_chat_pin_states();
        SelectionFacts::from_states(pins.into_iter().map(|(id, pinned)| {
            let chat = self.session().and_then(|s| s.chats.get(&id.0));
            (
                pinned,
                chat.is_some_and(|chat| chat.is_muted()),
                chat.is_some_and(|chat| chat.in_archive),
            )
        }))
    }

    pub(super) fn chat_select_bar(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let facts = self.selection_facts();
        let mut actions = div().flex().flex_none().items_center();
        for action in SelectBarAction::INLINE {
            actions = actions.child(
                Button::new(action.id())
                    .icon(action.icon(facts))
                    .ghost()
                    .xsmall()
                    .tooltip(action.label(facts))
                    .on_click(cx.listener(move |this, _, _, cx| action.run(this, cx))),
            );
        }
        let owner = cx.entity().downgrade();
        let more = Button::new("select-more")
            .icon(IconName::Ellipsis)
            .ghost()
            .xsmall()
            .tooltip(tr(keys::PROFILE_ACTION_SHORT_MORE))
            .dropdown_menu(move |mut menu, _, _| {
                for action in SelectBarAction::OVERFLOW {
                    let owner = owner.clone();
                    let item = if action == SelectBarAction::Delete {
                        // The kit menu item has no danger style, so draw the
                        // row in the destructive colour, like the chat row
                        // menu's "Delete chat".
                        let label = action.label(facts);
                        PopupMenuItem::element(move |_, _| {
                            div().text_color(danger_bright()).child(label)
                        })
                        .icon(Icon::new(action.icon(facts)).text_color(danger_bright()))
                    } else {
                        PopupMenuItem::new(action.label(facts)).icon(action.icon(facts))
                    };
                    menu = menu.item(item.on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| action.run(this, cx));
                    }));
                }
                menu
            });
        div()
            .id("select-bar")
            .flex()
            .items_center()
            .gap_0p5()
            .px_1p5()
            .py_1()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.12))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_semibold()
                    .child(format!("{count} selected")),
            )
            .child(actions)
            .child(more)
            .child(
                Button::new("select-cancel")
                    .icon(IconName::X)
                    .ghost()
                    .xsmall()
                    .tooltip(tr(keys::Q_CHATLIST_EXIT_SELECTION))
                    .on_click(cx.listener(|this, _, _, cx| this.exit_select_mode(cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::SelectBarAction as A;

    #[test]
    fn every_action_is_reachable_exactly_once() {
        let all = [
            A::Pin,
            A::Read,
            A::Mute,
            A::Archive,
            A::SelectUnread,
            A::Delete,
        ];
        for action in all {
            let inline = A::INLINE.iter().filter(|a| **a == action).count();
            let overflow = A::OVERFLOW.iter().filter(|a| **a == action).count();
            assert_eq!(inline + overflow, 1, "{action:?}");
        }
        assert_eq!(A::INLINE.len() + A::OVERFLOW.len(), all.len());
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = A::INLINE
            .iter()
            .chain(A::OVERFLOW.iter())
            .map(|a| a.id())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), A::INLINE.len() + A::OVERFLOW.len());
    }
}

#[cfg(test)]
mod facts_tests {
    use super::{SelectBarAction as A, SelectionFacts};
    use gpui_kit::assets::IconName;
    use quill::lang::{keys, tr};

    #[test]
    fn all_pinned_selection_offers_unpin() {
        let facts = SelectionFacts::from_states([(true, false, false), (true, false, false)]);
        assert_eq!(A::Pin.label(facts), tr(keys::Q_CHAT_UNPIN));
        assert_eq!(A::Pin.icon(facts), IconName::PinOff);
    }

    #[test]
    fn mixed_selection_offers_pin_mute_archive() {
        let facts = SelectionFacts::from_states([(true, true, true), (false, false, false)]);
        assert_eq!(A::Pin.label(facts), tr(keys::CONTEXT_PIN_MSG));
        assert_eq!(A::Pin.icon(facts), IconName::Pin);
        assert_eq!(A::Mute.icon(facts), IconName::BellOff);
        assert_eq!(A::Archive.icon(facts), IconName::Archive);
    }

    #[test]
    fn all_muted_and_archived_offer_the_reverse() {
        let facts = SelectionFacts::from_states([(false, true, true), (true, true, true)]);
        assert_eq!(A::Mute.label(facts), tr(keys::Q_CHAT_UNMUTE_NOTIFICATIONS));
        assert_eq!(A::Mute.icon(facts), IconName::Bell);
        assert_eq!(A::Archive.label(facts), tr(keys::Q_CHAT_UNARCHIVE));
        assert_eq!(A::Archive.icon(facts), IconName::ArchiveRestore);
        assert_eq!(A::Pin.icon(facts), IconName::Pin);
    }
}
