//! The message context menu as one value: closed, or open with everything
//! that was true when it opened (the message, the text selection, the link
//! and the checklist task or poll option under the pointer) and the page and
//! reaction strip it is showing now. Closing replaces it whole, so nothing
//! of one menu reaches the next.
#![deny(clippy::indexing_slicing)]

use super::menu_states::MessageMenuState;
use quill::composer::ReplyItem;
use quill::telegram::envelope::ReactionType;
use quill::text::LinkTarget;

/// Which page of the message menu is showing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum MessageMenuPage {
    #[default]
    Main,
    /// A song's "Save to..." destinations.
    SaveTo,
    /// "Add to Sticker Set" / "Add to Emoji Set": the sets the user
    /// created.
    AddToSet { emoji: bool },
    /// Who saw / reacted to the message, on the "who reacted" tab
    /// (`None` = All).
    Audience { tab: Option<ReactionType> },
    /// The page of the poll option the menu was opened over (tdesktop
    /// `FillPollOptionPage`).
    PollOption,
}

/// Message text selected when the menu opened, if the selection lies in
/// that message (Quote & Reply, Copy Selected Text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MenuSelection {
    pub(super) text: String,
    /// Which occurrence of `text` in the message the selection started at
    /// (0 for the first); where Quote & Reply points.
    pub(super) nth: usize,
}

#[derive(Debug, Clone)]
pub(super) struct OpenMenu {
    pub(super) at: MessageMenuState,
    pub(super) selection: Option<MenuSelection>,
    /// The link the menu was opened over.
    pub(super) link: Option<LinkTarget>,
    /// The checklist task or poll option the menu was opened over (the
    /// whole message when none).
    pub(super) item: ReplyItem,
    pub(super) page: MessageMenuPage,
    /// The reaction strip is expanded to every reaction.
    pub(super) reactions_expanded: bool,
    /// A right-clicked reaction chip asked for its own "who reacted" tab:
    /// it loads once the message's audience has begun
    /// (`drain_wanted_reactor_tab`), and goes with the menu if that never
    /// happens.
    pub(super) first_tab: Option<ReactionType>,
}

#[derive(Debug, Clone, Default)]
pub(super) enum MessageMenu {
    #[default]
    Closed,
    Open(OpenMenu),
}

impl MessageMenu {
    /// Where and for which message the menu is open.
    pub(super) fn at(&self) -> Option<MessageMenuState> {
        self.open().map(|open| open.at)
    }

    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    pub(super) fn open(&self) -> Option<&OpenMenu> {
        match self {
            Self::Open(open) => Some(open),
            Self::Closed => None,
        }
    }

    pub(super) fn open_mut(&mut self) -> Option<&mut OpenMenu> {
        match self {
            Self::Open(open) => Some(open),
            Self::Closed => None,
        }
    }

    /// Open for `at` with nothing else known: the main page over the whole
    /// message.
    pub(super) fn open_at(&mut self, at: MessageMenuState) {
        *self = Self::Open(OpenMenu {
            at,
            selection: None,
            link: None,
            item: ReplyItem::Message,
            page: MessageMenuPage::Main,
            reactions_expanded: false,
            first_tab: None,
        });
    }

    pub(super) fn close(&mut self) {
        *self = Self::Closed;
    }

    pub(super) fn page(&self) -> MessageMenuPage {
        self.open()
            .map(|open| open.page.clone())
            .unwrap_or_default()
    }

    /// Move to `page` (no effect while closed).
    pub(super) fn show_page(&mut self, page: MessageMenuPage) {
        if let Some(open) = self.open_mut() {
            open.page = page;
        }
    }

    /// Hand over the reaction whose tab a chip asked for, once.
    pub(super) fn take_first_tab(&mut self) -> Option<ReactionType> {
        self.open_mut()?.first_tab.take()
    }

    /// The "who reacted" tab on the audience page.
    pub(super) fn audience_tab(&self) -> Option<ReactionType> {
        match self.open().map(|open| &open.page) {
            Some(MessageMenuPage::Audience { tab }) => tab.clone(),
            _ => None,
        }
    }

    pub(super) fn selection_text(&self) -> Option<String> {
        self.open()
            .and_then(|open| open.selection.as_ref())
            .map(|selection| selection.text.clone())
    }

    pub(super) fn selection_nth(&self) -> usize {
        self.open()
            .and_then(|open| open.selection.as_ref())
            .map_or(0, |selection| selection.nth)
    }

    pub(super) fn link(&self) -> Option<LinkTarget> {
        self.open().and_then(|open| open.link.clone())
    }

    pub(super) fn item(&self) -> ReplyItem {
        self.open()
            .map_or(ReplyItem::Message, |open| open.item.clone())
    }

    pub(super) fn reactions_expanded(&self) -> bool {
        self.open().is_some_and(|open| open.reactions_expanded)
    }

    pub(super) fn expand_reactions(&mut self) {
        if let Some(open) = self.open_mut() {
            open.reactions_expanded = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill::ids::{ChatId, MessageId};

    fn at() -> MessageMenuState {
        MessageMenuState {
            chat_id: ChatId(1),
            message_id: MessageId(2),
            position: gpui_kit::point(gpui_kit::px(1.), gpui_kit::px(2.)),
        }
    }

    #[test]
    fn closing_drops_the_page_selection_and_strip_together() {
        let mut menu = MessageMenu::default();
        menu.open_at(at());
        menu.show_page(MessageMenuPage::Audience {
            tab: Some(ReactionType::Paid),
        });
        menu.expand_reactions();
        if let Some(open) = menu.open_mut() {
            open.selection = Some(MenuSelection {
                text: "hi".into(),
                nth: 2,
            });
        }
        assert_eq!(menu.selection_nth(), 2);
        assert!(menu.audience_tab().is_some() && menu.reactions_expanded());
        menu.close();
        assert!(!menu.is_open());
        assert_eq!(menu.page(), MessageMenuPage::Main);
        assert_eq!(menu.selection_text(), None);
        menu.open_at(at());
        assert!(!menu.reactions_expanded() && menu.audience_tab().is_none());
    }
}
