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

    fn label(self) -> &'static str {
        match self {
            Self::Pin => tr(keys::CONTEXT_PIN_MSG),
            Self::Read => tr(keys::Q_CHATLIST_READ),
            Self::Mute => tr(keys::Q_CHATLIST_MUTE),
            Self::Archive => tr(keys::Q_CHATLIST_ARCHIVE),
            Self::SelectUnread => tr(keys::Q_CHATLIST_SELECT_UNREAD),
            Self::Delete => tr(keys::CHAT_LINK_DELETE),
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Pin => IconName::Pin,
            Self::Read => IconName::CheckCheck,
            Self::Mute => IconName::BellOff,
            Self::Archive => IconName::Archive,
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
    pub(super) fn chat_select_bar(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let mut actions = div().flex().flex_none().items_center();
        for action in SelectBarAction::INLINE {
            actions = actions.child(
                Button::new(action.id())
                    .icon(action.icon())
                    .ghost()
                    .xsmall()
                    .tooltip(action.label())
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
                    menu = menu.item(
                        PopupMenuItem::new(action.label())
                            .icon(action.icon())
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| action.run(this, cx));
                            }),
                    );
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
