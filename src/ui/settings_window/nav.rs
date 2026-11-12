//! Where the settings window is: closed, open on its page list, on a page,
//! or on a subpage. One value, so a subpage cannot outlive its parent page
//! and a closed window has no page.

use super::SubPage;
use quill::settings_catalog::Page;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::ui) enum SettingsNav {
    #[default]
    Closed,
    /// Open with no page chosen yet: a wide window shows Account, a narrow
    /// one the page list.
    List,
    Page(Page),
    /// A subpage, which brings its parent page with it.
    Sub(SubPage),
}

impl SettingsNav {
    pub(in crate::ui) fn is_open(self) -> bool {
        self != Self::Closed
    }

    pub(in crate::ui) fn page(self) -> Option<Page> {
        match self {
            Self::Closed | Self::List => None,
            Self::Page(page) => Some(page),
            Self::Sub(sub) => Some(sub.parent()),
        }
    }

    pub(in crate::ui) fn sub(self) -> Option<SubPage> {
        match self {
            Self::Sub(sub) => Some(sub),
            Self::Closed | Self::List | Self::Page(_) => None,
        }
    }

    /// Opening: a closed window opens on the list, an open one keeps its
    /// page; a subpage steps back to its page.
    pub(super) fn opened(self) -> Self {
        match self {
            Self::Closed => Self::List,
            Self::Sub(sub) => Self::Page(sub.parent()),
            other => other,
        }
    }

    /// The window moves to `page`. A closed window stays closed.
    pub(super) fn on_page(self, page: Page) -> Self {
        match self {
            Self::Closed => Self::Closed,
            _ => Self::Page(page),
        }
    }

    /// The window moves to `sub`. A closed window stays closed.
    pub(super) fn on_sub(self, sub: SubPage) -> Self {
        match self {
            Self::Closed => Self::Closed,
            _ => Self::Sub(sub),
        }
    }

    /// The subpage steps back to its page.
    pub(super) fn without_sub(self) -> Self {
        match self {
            Self::Sub(sub) => Self::Page(sub.parent()),
            other => other,
        }
    }

    /// The page goes, the window stays open (a narrow window's list).
    pub(super) fn to_list(self) -> Self {
        match self {
            Self::Closed => Self::Closed,
            _ => Self::List,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Page, SettingsNav, SubPage};

    #[test]
    fn a_subpage_brings_its_page_and_a_closed_window_has_none() {
        let nav = SettingsNav::Sub(SubPage::Experimental);
        assert_eq!(nav.page(), Some(Page::Advanced));
        assert_eq!(nav.sub(), Some(SubPage::Experimental));
        assert_eq!(nav.without_sub(), SettingsNav::Page(Page::Advanced));
        assert_eq!(SettingsNav::Closed.page(), None);
        assert!(!SettingsNav::Closed.is_open());
    }

    #[test]
    fn a_closed_window_stays_closed_when_a_page_is_chosen() {
        assert_eq!(
            SettingsNav::Closed.on_page(Page::About),
            SettingsNav::Closed
        );
        assert_eq!(SettingsNav::Closed.opened(), SettingsNav::List);
        assert_eq!(
            SettingsNav::List.on_page(Page::About),
            SettingsNav::Page(Page::About)
        );
        assert_eq!(SettingsNav::Page(Page::About).to_list(), SettingsNav::List);
    }
}
