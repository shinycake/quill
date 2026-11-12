//! What the received-gifts screen shows over the list: nothing, one gift's
//! panel, that panel's convert confirmation, or its Upgrade or resale form.
//! The request in flight is the hub's `gifts_op`, not a mode of the view: it
//! outlives the panel.
#![deny(clippy::indexing_slicing)]

use crate::gift_manage::FlowAsk;
use crate::ids::RequestId;
use crate::state::PaymentsPurpose;

/// The price form or confirmation open inside one gift's panel. It lives in
/// [`GiftView::Form`], next to the id of the gift it is for, so it cannot
/// outlive that panel or name a gift that is not shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftForm {
    /// Confirm `upgradeGift` (costs Stars unless prepaid).
    Upgrade { keep_details: bool, ask: FlowAsk },
    /// Edit the resale price.
    Resale { ask: FlowAsk },
}

impl GiftForm {
    pub fn upgrade(keep_details: bool) -> Self {
        Self::Upgrade {
            keep_details,
            ask: FlowAsk::Editing,
        }
    }

    pub fn resale() -> Self {
        Self::Resale {
            ask: FlowAsk::Editing,
        }
    }

    fn ask_mut(&mut self) -> &mut FlowAsk {
        match self {
            Self::Upgrade { ask, .. } | Self::Resale { ask } => ask,
        }
    }

    /// Whether `purpose` is the request this form sends.
    fn sends(&self, purpose: PaymentsPurpose) -> bool {
        matches!(
            (self, purpose),
            (Self::Upgrade { .. }, PaymentsPurpose::UpgradeGift)
                | (Self::Resale { .. }, PaymentsPurpose::SetGiftResalePrice)
        )
    }

    /// Whether `request` is the one this form sent.
    fn answered_by(&self, request: Option<RequestId>) -> bool {
        let ask = match self {
            Self::Upgrade { ask, .. } | Self::Resale { ask } => *ask,
        };
        matches!((ask, request), (FlowAsk::Sent(sent), Some(request)) if sent == request)
    }
}

/// The mode of the open gifts list. One value, so a confirmation cannot
/// outlive the panel it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiftView {
    /// The gifts box is not open.
    #[default]
    Closed,
    /// The list alone.
    List,
    /// The panel of the gift with this id.
    Detail(String),
    /// "Convert to Stars?" for the gift with this id (its panel is open).
    ConfirmConvert(String),
    /// The Upgrade or resale form of the gift with this id (its panel is
    /// open).
    Form { id: String, form: GiftForm },
}

impl GiftView {
    /// The gifts box is open (the list, or a panel over it).
    pub fn is_open(&self) -> bool {
        *self != Self::Closed
    }

    /// The gift whose panel is open.
    pub fn selected(&self) -> Option<&str> {
        match self {
            Self::Closed | Self::List => None,
            Self::Detail(id) | Self::ConfirmConvert(id) | Self::Form { id, .. } => Some(id),
        }
    }

    /// The form open in the selected gift's panel.
    pub fn form(&self) -> Option<&GiftForm> {
        match self {
            Self::Form { form, .. } => Some(form),
            _ => None,
        }
    }

    pub fn form_mut(&mut self) -> Option<&mut GiftForm> {
        match self {
            Self::Form { form, .. } => Some(form),
            _ => None,
        }
    }

    /// The form open for the gift `id`, and only for it.
    pub fn form_of(&self, id: &str) -> Option<&GiftForm> {
        match self {
            Self::Form { id: open, form } if open == id => Some(form),
            _ => None,
        }
    }

    /// Open (`Some`) or close (`None`) a form in the open panel. No panel,
    /// no form.
    pub fn set_form(&mut self, form: Option<GiftForm>) {
        let Some(id) = self.selected().map(str::to_owned) else {
            return;
        };
        *self = match form {
            Some(form) => Self::Form { id, form },
            None => Self::Detail(id),
        };
    }

    /// The form open for gift `gift` sent `request` for `purpose`: its
    /// answer closes it. A request for another gift, or of another kind,
    /// is not this form's.
    pub fn form_sent(&mut self, gift: &str, purpose: PaymentsPurpose, request: RequestId) {
        if let Self::Form { id, form } = self
            && id == gift
            && form.sends(purpose)
        {
            *form.ask_mut() = FlowAsk::Sent(request);
        }
    }

    /// `request` was refused: its form is being filled in again.
    pub fn form_refused(&mut self, request: Option<RequestId>) {
        if let Self::Form { form, .. } = self
            && form.answered_by(request)
        {
            *form.ask_mut() = FlowAsk::Editing;
        }
    }

    /// The form that sent `request` is done and the panel stays; any other
    /// form stays too.
    pub fn close_form_answered_by(&mut self, request: Option<RequestId>) {
        if let Self::Form { id, form } = self
            && form.answered_by(request)
        {
            *self = Self::Detail(std::mem::take(id));
        }
    }

    /// The gift whose convert confirmation is showing.
    pub fn confirming(&self) -> Option<&str> {
        match self {
            Self::ConfirmConvert(id) => Some(id),
            _ => None,
        }
    }

    /// Open (`Some`) or close (`None`) a gift's panel. A confirmation or
    /// form goes with the panel it was for (the same gift keeps its form).
    /// A closed box shows no panel.
    pub fn select(&mut self, id: Option<String>) {
        if *self == Self::Closed || id.as_deref().is_some_and(|id| self.form_of(id).is_some()) {
            return;
        }
        *self = match id {
            Some(id) => Self::Detail(id),
            None => Self::List,
        };
    }

    /// Ask (`Some`) or drop (`None`) the convert confirmation for the open
    /// panel.
    pub fn confirm_convert(&mut self, id: Option<String>) {
        match (&*self, id) {
            (Self::Closed, _) => {}
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
    fn a_form_belongs_to_its_gift_alone() {
        let mut view = GiftView::List;
        view.select(Some("a".into()));
        view.set_form(Some(GiftForm::upgrade(true)));
        view.select(Some("a".into()));
        assert!(view.form_of("a").is_some(), "the same gift keeps its form");
        view.select(Some("b".into()));
        assert_eq!((view.form(), view.selected()), (None, Some("b")));
        view.set_form(Some(GiftForm::resale()));
        view.select(None);
        assert_eq!((view.form(), view.selected()), (None, None));
        view.set_form(Some(GiftForm::resale()));
        assert_eq!(view.form(), None, "no panel, no form");
    }

    #[test]
    fn only_the_form_that_sent_a_request_closes_on_its_answer() {
        let mut view = GiftView::Form {
            id: "a".into(),
            form: GiftForm::resale(),
        };
        view.form_sent("a", PaymentsPurpose::UpgradeGift, RequestId(1));
        assert_eq!(view.form(), Some(&GiftForm::resale()), "not its request");
        view.form_sent("b", PaymentsPurpose::SetGiftResalePrice, RequestId(9));
        assert_eq!(
            view.form(),
            Some(&GiftForm::resale()),
            "another gift's request"
        );
        view.form_sent("a", PaymentsPurpose::SetGiftResalePrice, RequestId(2));
        view.close_form_answered_by(Some(RequestId(1)));
        assert!(view.form().is_some());
        view.form_refused(Some(RequestId(2)));
        assert_eq!(view.form(), Some(&GiftForm::resale()));
        view.form_sent("a", PaymentsPurpose::SetGiftResalePrice, RequestId(3));
        view.close_form_answered_by(Some(RequestId(3)));
        assert_eq!(view, GiftView::Detail("a".into()));
    }

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
