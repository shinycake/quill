//! The macOS Window menu, after Telegram Desktop's
//! `Platform::Manager::buildWindowMenu` (`platform/mac/global_menu_mac.mm`):
//!
//! Toggle Full Screen (⌃⌘F) / — / Contacts, Add Contact / — / New Group,
//! New Channel / — / Show Quill.
//!
//! tdesktop disables Contacts, Add Contact, New Group and New Channel while
//! no account is signed in or the passcode lock is on, and "Show Telegram"
//! while the window is the active one. GPUI enables a menu item when its
//! action has a handler, so a disabled item is bound to [`MenuUnavailable`],
//! which nothing handles, and the menu is rebuilt when the state changes.

use super::{
    MenuUnavailable, WindowMenuAddContact, WindowMenuContacts, WindowMenuNewChannel,
    WindowMenuNewGroup, WindowMenuShowMain,
};
use crate::ui::actions::ToggleFullscreen;
use gpui_kit::{Action, MenuItem};
use quill::lang::{keys, tr};

/// What the Window menu's enabled items depend on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WindowMenuState {
    /// An account is signed in and the passcode lock is off.
    pub(crate) session_usable: bool,
    /// The main window is the active (key) window.
    pub(crate) main_active: bool,
    /// The chat list column is hidden (the View menu offers Show Sidebar).
    pub(crate) sidebar_hidden: bool,
}

/// The Window menu's commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowCommand {
    FullScreen,
    Contacts,
    AddContact,
    NewGroup,
    NewChannel,
    ShowMain,
}

impl WindowCommand {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::FullScreen => tr(keys::Q_MISC_TOGGLE_FULL_SCREEN),
            Self::Contacts => tr(keys::MENU_CONTACTS),
            Self::AddContact => tr(keys::Q_MISC_ADD_CONTACT),
            Self::NewGroup => tr(keys::Q_MISC_NEW_GROUP),
            Self::NewChannel => tr(keys::Q_MISC_NEW_CHANNEL),
            Self::ShowMain => tr(keys::Q_MISC_SHOW_QUILL),
        }
    }

    fn action(self) -> Box<dyn Action> {
        match self {
            Self::FullScreen => Box::new(ToggleFullscreen),
            Self::Contacts => Box::new(WindowMenuContacts),
            Self::AddContact => Box::new(WindowMenuAddContact),
            Self::NewGroup => Box::new(WindowMenuNewGroup),
            Self::NewChannel => Box::new(WindowMenuNewChannel),
            Self::ShowMain => Box::new(WindowMenuShowMain),
        }
    }
}

/// One Window menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowMenuRow {
    Separator,
    Item {
        command: WindowCommand,
        enabled: bool,
    },
}

/// The rows of the Window menu for `state`, top to bottom.
pub(crate) fn window_menu_rows(state: WindowMenuState) -> Vec<WindowMenuRow> {
    let item = |command, enabled| WindowMenuRow::Item { command, enabled };
    let session = state.session_usable;
    vec![
        item(WindowCommand::FullScreen, true),
        WindowMenuRow::Separator,
        item(WindowCommand::Contacts, session),
        item(WindowCommand::AddContact, session),
        WindowMenuRow::Separator,
        item(WindowCommand::NewGroup, session),
        item(WindowCommand::NewChannel, session),
        WindowMenuRow::Separator,
        item(WindowCommand::ShowMain, !state.main_active),
    ]
}

/// The Window menu's items for the native menu bar.
pub(crate) fn window_menu_items(state: WindowMenuState) -> Vec<MenuItem> {
    window_menu_rows(state)
        .into_iter()
        .map(|row| match row {
            WindowMenuRow::Separator => MenuItem::separator(),
            WindowMenuRow::Item { command, enabled } => MenuItem::Action {
                name: command.title().into(),
                action: if enabled {
                    command.action()
                } else {
                    Box::new(MenuUnavailable)
                },
                os_action: None,
                checked: false,
                disabled: !enabled,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{WindowMenuRow, WindowMenuState, window_menu_rows};

    fn titles(state: WindowMenuState) -> Vec<(&'static str, bool)> {
        window_menu_rows(state)
            .into_iter()
            .map(|row| match row {
                WindowMenuRow::Separator => ("-", true),
                WindowMenuRow::Item { command, enabled } => (command.title(), enabled),
            })
            .collect()
    }

    #[test]
    fn matches_telegram_desktop_order_and_states() {
        let signed_out = WindowMenuState {
            session_usable: false,
            main_active: true,
            sidebar_hidden: false,
        };
        assert_eq!(
            titles(signed_out),
            vec![
                ("Toggle Full Screen", true),
                ("-", true),
                ("Contacts", false),
                ("Add Contact", false),
                ("-", true),
                ("New Group", false),
                ("New Channel", false),
                ("-", true),
                ("Show Quill", false),
            ]
        );
        let ready_in_background = WindowMenuState {
            session_usable: true,
            main_active: false,
            sidebar_hidden: false,
        };
        assert!(
            titles(ready_in_background)
                .iter()
                .all(|(_, enabled)| *enabled)
        );
    }
}
