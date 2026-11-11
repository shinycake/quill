//! What the received-gifts screen shows over the list: nothing, one gift's
//! panel, or that panel's convert confirmation. The request in flight is the
//! hub's `gifts_op`, not a mode of the view: it outlives the panel.
#![deny(clippy::indexing_slicing)]

/// The mode of the open gifts list. One value, so a confirmation cannot
/// outlive the panel it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiftView {
    /// The list alone.
    #[default]
    List,
    /// The panel of the gift with this id.
    Detail(String),
    /// "Convert to Stars?" for the gift with this id (its panel is open).
    ConfirmConvert(String),
}

impl GiftView {
    /// The gift whose panel is open.
    pub fn selected(&self) -> Option<&str> {
        match self {
            Self::List => None,
            Self::Detail(id) | Self::ConfirmConvert(id) => Some(id),
        }
    }

    /// The gift whose convert confirmation is showing.
    pub fn confirming(&self) -> Option<&str> {
        match self {
            Self::ConfirmConvert(id) => Some(id),
            _ => None,
        }
    }

    /// Open (`Some`) or close (`None`) a gift's panel. A confirmation goes
    /// with the panel it was for.
    pub fn select(&mut self, id: Option<String>) {
        *self = match id {
            Some(id) => Self::Detail(id),
            None => Self::List,
        };
    }

    /// Ask (`Some`) or drop (`None`) the convert confirmation for the open
    /// panel.
    pub fn confirm_convert(&mut self, id: Option<String>) {
        match (&*self, id) {
            (_, Some(id)) => *self = Self::ConfirmConvert(id),
            (Self::ConfirmConvert(open), None) => *self = Self::Detail(open.clone()),
            (_, None) => {}
        }
    }

    /// The sale failed or was cancelled: the confirmation is gone, the
    /// panel stays.
    pub fn abandon_confirmation(&mut self) {
        if let Self::ConfirmConvert(id) = self {
            *self = Self::Detail(std::mem::take(id));
        }
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
    fn a_failed_sale_keeps_the_panel_and_drops_the_confirmation() {
        let mut asking = GiftView::ConfirmConvert("a".into());
        asking.abandon_confirmation();
        assert_eq!(asking, GiftView::Detail("a".into()));
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
