//! What the received-gifts screen shows over the list: nothing, one gift's
//! panel, that panel's convert confirmation, or a mutation in flight.
#![deny(clippy::indexing_slicing)]

/// The mode of the open gifts list. One value, so a confirmation cannot
/// outlive the panel it belongs to and the in-flight lock cannot be left
/// set by a writer that cleared the selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiftView {
    /// The list alone.
    #[default]
    List,
    /// The panel of the gift with this id.
    Detail(String),
    /// "Convert to Stars?" for the gift with this id (its panel is open).
    ConfirmConvert(String),
    /// A toggle, sale or management request is out. The lock outlives the
    /// panel: opening another list keeps it until the request's own answer
    /// (a second request would pay twice). `detail` is the open panel.
    Mutating { detail: Option<String> },
}

impl GiftView {
    /// The gift whose panel is open.
    pub fn selected(&self) -> Option<&str> {
        match self {
            Self::List => None,
            Self::Detail(id) | Self::ConfirmConvert(id) => Some(id),
            Self::Mutating { detail } => detail.as_deref(),
        }
    }

    /// The gift whose convert confirmation is showing.
    pub fn confirming(&self) -> Option<&str> {
        match self {
            Self::ConfirmConvert(id) => Some(id),
            _ => None,
        }
    }

    pub fn is_mutating(&self) -> bool {
        matches!(self, Self::Mutating { .. })
    }

    /// Open (`Some`) or close (`None`) a gift's panel. A confirmation goes
    /// with the panel it was for; a request in flight keeps its lock.
    pub fn select(&mut self, id: Option<String>) {
        *self = match (std::mem::take(self), id) {
            (Self::Mutating { .. }, detail) => Self::Mutating { detail },
            (_, Some(id)) => Self::Detail(id),
            (_, None) => Self::List,
        };
    }

    /// Ask (`Some`) or drop (`None`) the convert confirmation for the open
    /// panel. Nothing changes while a request is out.
    pub fn confirm_convert(&mut self, id: Option<String>) {
        match (&*self, id) {
            (Self::Mutating { .. }, _) => {}
            (_, Some(id)) => *self = Self::ConfirmConvert(id),
            (Self::ConfirmConvert(open), None) => *self = Self::Detail(open.clone()),
            (_, None) => {}
        }
    }

    /// A request goes out; the panel stays as it was.
    pub fn begin_mutation(&mut self) {
        let detail = self.selected().map(str::to_string);
        *self = Self::Mutating { detail };
    }

    /// The request answered or failed: the lock is gone, the panel stays.
    pub fn end_mutation(&mut self) {
        if let Self::Mutating { detail } = self {
            *self = match detail.take() {
                Some(id) => Self::Detail(id),
                None => Self::List,
            };
        }
    }

    /// The request failed: the lock and any confirmation are gone, the
    /// panel stays.
    pub fn abandon_confirmation(&mut self) {
        if let Self::ConfirmConvert(id) = self {
            *self = Self::Detail(std::mem::take(id));
        }
        self.end_mutation();
    }

    /// Close the panel of `id` when it is the open one (the gift was sold or
    /// deleted; the user may have opened another meanwhile).
    pub fn close_if_selected(&mut self, id: Option<&str>) {
        if self.selected() == id {
            self.select(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_the_panel_takes_the_confirmation_with_it() {
        let mut view = GiftView::List;
        view.select(Some("a".into()));
        view.confirm_convert(Some("a".into()));
        assert_eq!(view.confirming(), Some("a"));
        view.select(None);
        assert_eq!(view, GiftView::List);
        view.select(Some("a".into()));
        assert_eq!(view.confirming(), None, "a new panel starts unconfirmed");
    }

    #[test]
    fn ending_a_request_that_is_not_out_leaves_the_panel_alone() {
        let mut view = GiftView::Detail("a".into());
        view.end_mutation();
        assert_eq!(view, GiftView::Detail("a".into()));
        let mut view = GiftView::ConfirmConvert("a".into());
        view.end_mutation();
        assert_eq!(view, GiftView::ConfirmConvert("a".into()));
    }

    #[test]
    fn the_lock_outlives_the_panel_until_the_request_ends() {
        let mut view = GiftView::Detail("a".into());
        view.begin_mutation();
        assert!(view.is_mutating());
        assert_eq!(view.selected(), Some("a"));
        view.select(None);
        assert!(view.is_mutating(), "closing the panel does not unlock");
        view.confirm_convert(Some("b".into()));
        assert!(view.is_mutating(), "no confirmation under a request");
        view.end_mutation();
        assert_eq!(view, GiftView::List);
    }

    #[test]
    fn a_failed_sale_keeps_the_panel_and_drops_the_confirmation() {
        let mut view = GiftView::ConfirmConvert("a".into());
        view.begin_mutation();
        view.abandon_confirmation();
        assert_eq!(view, GiftView::Detail("a".into()));
        let mut asking = GiftView::ConfirmConvert("a".into());
        asking.abandon_confirmation();
        assert_eq!(asking, GiftView::Detail("a".into()));
    }

    #[test]
    fn a_sold_gift_closes_only_its_own_panel() {
        let mut view = GiftView::Detail("b".into());
        view.close_if_selected(Some("a"));
        assert_eq!(view.selected(), Some("b"));
        view.close_if_selected(Some("b"));
        assert_eq!(view, GiftView::List);
    }
}
