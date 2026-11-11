#![deny(clippy::indexing_slicing)]
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
/// M1: right-click context menu state — the target message plus the
/// window position where the menu opens (`MouseDownEvent.position` is in
/// window coordinates, so the panel renders absolute at that point).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MessageMenuState {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub position: Point<Pixels>,
}

/// Slice CL1: right-click chat-row context menu target + window
/// position (same pattern as `MessageMenuState`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatMenuState {
    pub chat_id: ChatId,
    pub position: Point<Pixels>,
    /// The submenu opened beside its row (tdesktop opens Mute and Add to
    /// folder as submenus of the same popup).
    pub submenu: Option<ChatMenuSubmenu>,
    /// Which menu the row opened: a chat row's, or a top person's.
    pub kind: ChatMenuKind,
}

/// The menu a chat-list popup shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMenuKind {
    /// A chat row's context menu.
    Row,
    /// A person of the search's top-people strip: tdesktop's shorter entry
    /// menu with the "Remove from Recent" entries.
    TopPerson,
}

/// The chat-list popup menu: closed, or open on one target. Every way to
/// close it is [`Self::close`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ChatMenu {
    #[default]
    Closed,
    Open(ChatMenuState),
}

impl ChatMenu {
    pub fn open(&mut self, state: ChatMenuState) {
        *self = Self::Open(state);
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    /// The open menu's target and submenu.
    pub fn state(&self) -> Option<ChatMenuState> {
        match self {
            Self::Open(state) => Some(*state),
            Self::Closed => None,
        }
    }

    pub fn state_mut(&mut self) -> Option<&mut ChatMenuState> {
        match self {
            Self::Open(state) => Some(state),
            Self::Closed => None,
        }
    }
}

/// Which row of the chat menu has its submenu open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMenuSubmenu {
    Mute,
    Folders,
}

impl ChatMenuState {
    pub fn at(chat_id: ChatId, position: Point<Pixels>) -> Self {
        Self {
            chat_id,
            position,
            submenu: None,
            kind: ChatMenuKind::Row,
        }
    }

    /// The menu of a top-people strip tile.
    pub fn top_person(chat_id: ChatId, position: Point<Pixels>) -> Self {
        Self {
            kind: ChatMenuKind::TopPerson,
            ..Self::at(chat_id, position)
        }
    }
}
