//! What the Devices page has open over its list, and where the "link a
//! device" flow stands. One value each, so the details box, its rename
//! form and the terminate question cannot be open in a combination that
//! means nothing.
#![deny(clippy::indexing_slicing)]

use super::dialogs::confirm::{SessionsConfirm, WebsitesConfirm};

/// The box over the sessions list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DeviceLayer {
    /// The list alone.
    #[default]
    None,
    /// The details box of this session.
    Details(i64),
    /// The details box of this device, on its rename form.
    Renaming(i64),
    /// "Terminate?" for one session or all others.
    Confirm(SessionsConfirm),
}

impl DeviceLayer {
    /// The session whose box is open (details or rename form).
    pub(crate) fn details(&self) -> Option<i64> {
        match self {
            Self::Details(id) | Self::Renaming(id) => Some(*id),
            Self::None | Self::Confirm(_) => None,
        }
    }

    pub(crate) fn renaming(&self) -> bool {
        matches!(self, Self::Renaming(_))
    }

    pub(crate) fn confirm(&self) -> Option<SessionsConfirm> {
        match self {
            Self::Confirm(confirm) => Some(*confirm),
            _ => None,
        }
    }

    pub(crate) fn open_details(&mut self, id: i64) {
        *self = Self::Details(id);
    }

    /// Close the details box (and its rename form); a terminate question
    /// stays.
    pub(crate) fn close_details(&mut self) {
        if self.details().is_some() {
            *self = Self::None;
        }
    }

    /// Open the rename form of the open details box.
    pub(crate) fn start_rename(&mut self) {
        if let Self::Details(id) = *self {
            *self = Self::Renaming(id);
        }
    }

    /// Back from the rename form to the details.
    pub(crate) fn stop_rename(&mut self) {
        if let Self::Renaming(id) = *self {
            *self = Self::Details(id);
        }
    }

    /// Ask "Terminate?"; the details box (if any) closes first.
    pub(crate) fn ask(&mut self, confirm: SessionsConfirm) {
        *self = Self::Confirm(confirm);
    }

    /// Answer or drop the terminate question.
    pub(crate) fn take_confirm(&mut self) -> Option<SessionsConfirm> {
        let confirm = self.confirm()?;
        *self = Self::None;
        Some(confirm)
    }

    /// Leave the page: everything closes.
    pub(crate) fn close_all(&mut self) {
        *self = Self::None;
    }

    /// Esc: the question first, then the details.
    pub(crate) fn close_top(&mut self) {
        *self = Self::None;
    }
}

/// The Connected Websites box: closed, open on its list, or asking
/// whether to disconnect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WebsitesScreen {
    #[default]
    Closed,
    Open,
    Confirm(WebsitesConfirm),
}

impl WebsitesScreen {
    pub(crate) fn is_open(&self) -> bool {
        *self != Self::Closed
    }

    pub(crate) fn confirm(&self) -> Option<WebsitesConfirm> {
        match self {
            Self::Confirm(confirm) => Some(*confirm),
            _ => None,
        }
    }

    pub(crate) fn open(&mut self) {
        *self = Self::Open;
    }

    /// Close; a pending question goes with it.
    pub(crate) fn close(&mut self) {
        *self = Self::Closed;
    }

    /// Ask "Disconnect?" over the open list.
    pub(crate) fn ask(&mut self, confirm: WebsitesConfirm) {
        if self.is_open() {
            *self = Self::Confirm(confirm);
        }
    }

    /// Drop the question; the list stays.
    pub(crate) fn drop_confirm(&mut self) {
        if matches!(self, Self::Confirm(_)) {
            *self = Self::Open;
        }
    }

    /// Answer the question: the list stays.
    pub(crate) fn take_confirm(&mut self) -> Option<WebsitesConfirm> {
        let confirm = self.confirm()?;
        *self = Self::Open;
        Some(confirm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminate_question_replaces_the_details_and_escape_clears_it() {
        let mut layer = DeviceLayer::None;
        layer.open_details(4);
        layer.start_rename();
        assert!(layer.renaming());
        layer.stop_rename();
        assert_eq!(layer, DeviceLayer::Details(4));
        layer.ask(SessionsConfirm::TerminateAll);
        assert_eq!(layer.details(), None, "one box at a time");
        layer.close_details();
        assert_eq!(layer.confirm(), Some(SessionsConfirm::TerminateAll));
        assert_eq!(layer.take_confirm(), Some(SessionsConfirm::TerminateAll));
        assert_eq!(layer, DeviceLayer::None);
        assert_eq!(layer.take_confirm(), None);
    }

    #[test]
    fn closing_the_websites_box_drops_its_question() {
        let mut screen = WebsitesScreen::Closed;
        screen.ask(WebsitesConfirm::DisconnectAll);
        assert_eq!(
            screen,
            WebsitesScreen::Closed,
            "no question without the box"
        );
        screen.open();
        screen.ask(WebsitesConfirm::DisconnectAll);
        assert_eq!(screen.confirm(), Some(WebsitesConfirm::DisconnectAll));
        assert_eq!(screen.take_confirm(), Some(WebsitesConfirm::DisconnectAll));
        assert_eq!(screen, WebsitesScreen::Open, "the list stays");
        screen.ask(WebsitesConfirm::DisconnectAll);
        screen.close();
        assert!(!screen.is_open());
    }
}
