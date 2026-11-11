use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::payment_card::NewCredentials;
use quill::telegram::envelope::{AddressData, OrderInfoData, PaymentFormData, PaymentFormTypeData};
/// Slice P1: the payment checkout dialog. The text inputs mirror the
/// `orderInfo` fields the invoice needs (`need_name` / `need_phone_number`
/// / `need_email_address` / `need_shipping_address`, schema:4655); only
/// the needed ones render. The credential choice is either a saved
/// credential (`inputCredentialsSaved`, schema:5199) or a new card the
/// provider tokenized in the card form (`inputCredentialsNew`, schema:5204).
pub struct PaymentDialog {
    pub(crate) name_input: Entity<TextareaState>,
    pub(crate) phone_input: Entity<TextareaState>,
    pub(crate) email_input: Entity<TextareaState>,
    pub(crate) street1_input: Entity<TextareaState>,
    pub(crate) street2_input: Entity<TextareaState>,
    pub(crate) city_input: Entity<TextareaState>,
    pub(crate) state_input: Entity<TextareaState>,
    /// The shipping country's ISO code, picked in the country box.
    pub(crate) country_iso: String,
    pub(crate) postal_input: Entity<TextareaState>,
    /// How the order is paid: a saved credential or the card the provider
    /// tokenized (tdesktop's `NewCredentials`). Kept only while the
    /// checkout is open.
    pub(crate) method: PaymentMethod,
    /// What fills the dialog: waiting for the form, the checkout, or the
    /// country box or "New Card" form open over it.
    pub(crate) page: PaymentPage,
    pub(crate) terms_accepted: bool,
    pub(crate) allow_save_order: bool,
}

/// The checkout's page. The dialog opens on Buy press before the form
/// arrives (`AwaitingForm`); the form's saved order info prefills the
/// inputs once, when it lands, and the page becomes the checkout itself.
/// A picker over the checkout leaves to the checkout: the pick, Cancel,
/// or the dialog closing.
pub(crate) enum PaymentPage {
    AwaitingForm,
    Checkout,
    Country(crate::ui::country_box::CountryBox),
    CardForm(Box<crate::ui::payments::card_form::CardForm>),
}

/// How the order is paid, as one value. `NeedsPassword` is the new card
/// after asking to save it when the account cannot save yet: "Turn on
/// Two-Step Verification" (tdesktop's `askSetPassword`) waits on it and Pay
/// stays off; choosing another method, dismissing the ask or setting the
/// password leaves it for `NewCard`. A new card stays listed while a saved
/// credential is chosen, so it is carried by `Saved` too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaymentMethod {
    /// "New Card" is chosen and none was entered yet.
    NoCard,
    Saved {
        id: String,
        card: Option<NewCard>,
    },
    NewCard(NewCard),
    NeedsPassword(NewCard),
}

impl PaymentMethod {
    pub(crate) fn card(&self) -> Option<&NewCard> {
        match self {
            Self::NewCard(card) | Self::NeedsPassword(card) => Some(card),
            Self::Saved { card, .. } => card.as_ref(),
            Self::NoCard => None,
        }
    }

    /// The choice shown selected in the method list.
    pub(crate) fn choice(&self) -> PaymentCredentialChoice {
        match self {
            Self::Saved { id, .. } => PaymentCredentialChoice::Saved(id.clone()),
            Self::NoCard | Self::NewCard(_) | Self::NeedsPassword(_) => {
                PaymentCredentialChoice::NewCard
            }
        }
    }

    /// "New Card" is the chosen method but no card was entered.
    pub(crate) fn lacks_card(&self) -> bool {
        matches!(self, Self::NoCard)
    }

    pub(crate) fn needs_password(&self) -> bool {
        matches!(self, Self::NeedsPassword(_))
    }

    /// Let go of the password ask; the card stays.
    pub(crate) fn drop_password_prompt(&mut self) {
        *self = match std::mem::replace(self, Self::NoCard) {
            Self::NeedsPassword(card) => Self::NewCard(card),
            other => other,
        };
    }

    /// Pick `choice` from the list. The entered card stays listed; picking
    /// it again asks for a password (when `ask_password`) if it is to be
    /// saved and cannot be yet.
    pub(crate) fn choose(&mut self, choice: PaymentCredentialChoice, ask_password: bool) {
        let card = self.take_card();
        *self = match (choice, card) {
            (PaymentCredentialChoice::Saved(id), card) => Self::Saved { id, card },
            (PaymentCredentialChoice::NewCard, None) => Self::NoCard,
            (PaymentCredentialChoice::NewCard, Some(card)) => Self::using(card, ask_password),
        };
    }

    /// The card just entered becomes the method.
    pub(crate) fn use_card(&mut self, card: NewCard, ask_password: bool) {
        *self = Self::using(card, ask_password);
    }

    fn using(card: NewCard, ask_password: bool) -> Self {
        if card.save && ask_password {
            Self::NeedsPassword(card)
        } else {
            Self::NewCard(card)
        }
    }

    fn take_card(&mut self) -> Option<NewCard> {
        match std::mem::replace(self, Self::NoCard) {
            Self::NewCard(card) | Self::NeedsPassword(card) => Some(card),
            Self::Saved { card, .. } => card,
            Self::NoCard => None,
        }
    }
}

/// A method picked from the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaymentCredentialChoice {
    Saved(String),
    /// The tokenized [`NewCard`].
    NewCard,
}

/// A card entered in the card form and tokenized by its provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewCard {
    pub(crate) credentials: NewCredentials,
    pub(crate) save: bool,
}

impl PaymentDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let mut input = |cx: &mut Context<QuillApp>, placeholder: &str| {
            cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 1)
                    .submit_on_enter(false)
            })
        };
        Self {
            name_input: input(cx, tr(keys::Q_MONEY_FULL_NAME)),
            phone_input: input(cx, tr(keys::Q_MONEY_PHONE_NUMBER)),
            email_input: input(cx, tr(keys::Q_MONEY_EMAIL_ADDRESS)),
            street1_input: input(cx, tr(keys::Q_MONEY_STREET_ADDRESS)),
            street2_input: input(cx, tr(keys::Q_MONEY_STREET_ADDRESS_LINE_2_OPTIONAL)),
            city_input: input(cx, tr(keys::PAYMENTS_ADDRESS_CITY)),
            state_input: input(cx, tr(keys::Q_MONEY_STATE)),
            country_iso: String::new(),
            postal_input: input(cx, tr(keys::Q_MONEY_POSTAL_CODE)),
            method: PaymentMethod::NoCard,
            page: PaymentPage::AwaitingForm,
            terms_accepted: false,
            allow_save_order: true,
            // ponytail: no `validated` flag — the session's
            // `payment_validated: Option<ValidatedOrderInfoData>` is the
            // single source of truth for whether validation ran.
        }
    }

    /// Slice P1 fix-up: prefill the order fields and credential choice
    /// from a regular form's `saved_order_info` (schema:4720), once. The
    /// dialog opens on Buy press before the form arrives, so this runs
    /// either at open (form already cached) or on the first frame after
    /// the form answer lands.
    pub(crate) fn prefill_from_form(
        &mut self,
        form: &PaymentFormData,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) {
        if !self.awaiting_form() {
            return;
        }
        if let PaymentFormTypeData::Regular(regular) = &form.form_type {
            self.prefill(&regular.saved_order_info, window, cx);
            if let Some(first) = regular.saved_credentials.first() {
                let card = self.method.take_card();
                self.method = PaymentMethod::Saved {
                    id: first.id.clone(),
                    card,
                };
            }
        }
        self.page = PaymentPage::Checkout;
    }

    /// The form has not landed yet.
    pub(crate) fn awaiting_form(&self) -> bool {
        matches!(self.page, PaymentPage::AwaitingForm)
    }

    /// Prefill the order fields from the form's `saved_order_info`
    /// (schema:4720) so returning buyers don't retype.
    fn prefill(&mut self, order: &OrderInfoData, window: &mut Window, cx: &mut Context<QuillApp>) {
        let mut set = |input: &Entity<TextareaState>, value: &str| {
            if !value.is_empty() {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        };
        set(&self.name_input, &order.name);
        set(&self.phone_input, &order.phone_number);
        set(&self.email_input, &order.email_address);
        let addr = &order.shipping_address;
        set(&self.street1_input, &addr.street_line1);
        set(&self.street2_input, &addr.street_line2);
        set(&self.city_input, &addr.city);
        set(&self.state_input, &addr.state);
        self.country_iso = addr.country_code.to_ascii_uppercase();
        set(&self.postal_input, &addr.postal_code);
    }

    pub(crate) fn card_form(&self) -> Option<&crate::ui::payments::card_form::CardForm> {
        match &self.page {
            PaymentPage::CardForm(form) => Some(form.as_ref()),
            PaymentPage::AwaitingForm | PaymentPage::Checkout | PaymentPage::Country(_) => None,
        }
    }

    pub(crate) fn card_form_mut(
        &mut self,
    ) -> Option<&mut crate::ui::payments::card_form::CardForm> {
        match &mut self.page {
            PaymentPage::CardForm(form) => Some(form.as_mut()),
            PaymentPage::AwaitingForm | PaymentPage::Checkout | PaymentPage::Country(_) => None,
        }
    }

    pub(crate) fn country_box(&self) -> Option<&crate::ui::country_box::CountryBox> {
        match &self.page {
            PaymentPage::Country(country_box) => Some(country_box),
            PaymentPage::AwaitingForm | PaymentPage::Checkout | PaymentPage::CardForm(_) => None,
        }
    }

    pub(crate) fn country_box_mut(&mut self) -> Option<&mut crate::ui::country_box::CountryBox> {
        match &mut self.page {
            PaymentPage::Country(country_box) => Some(country_box),
            PaymentPage::AwaitingForm | PaymentPage::Checkout | PaymentPage::CardForm(_) => None,
        }
    }

    /// Freeze the dialog inputs into an `orderInfo` (schema:4662).
    pub(crate) fn order(&self, cx: &App) -> OrderInfoData {
        let value = |input: &Entity<TextareaState>| input.read(cx).value().to_string();
        OrderInfoData {
            name: value(&self.name_input),
            phone_number: value(&self.phone_input),
            email_address: value(&self.email_input),
            shipping_address: AddressData {
                country_code: self.country_iso.clone(),
                state: value(&self.state_input),
                city: value(&self.city_input),
                street_line1: value(&self.street1_input),
                street_line2: value(&self.street2_input),
                postal_code: value(&self.postal_input),
            },
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{NewCard, PaymentCredentialChoice, PaymentMethod};
    use quill::payment_card::NewCredentials;

    fn card() -> NewCard {
        NewCard {
            credentials: NewCredentials::new("visa *4242".into(), "{}".into()),
            save: true,
        }
    }

    #[test]
    fn asking_for_a_password_keeps_the_card_and_every_exit_gives_it_back() {
        let mut method = PaymentMethod::NoCard;
        method.use_card(card(), true);
        assert!(method.needs_password() && method.card().is_some());
        // Dismissed, or another method chosen: the card stays, the ask goes.
        method.drop_password_prompt();
        assert!(!method.needs_password() && method.card().is_some());
        method.use_card(card(), true);
        method.choose(PaymentCredentialChoice::Saved("s".into()), true);
        assert!(!method.needs_password() && method.card().is_some());
        assert_eq!(method.choice(), PaymentCredentialChoice::Saved("s".into()));
        // The card chosen again asks again while it cannot be saved.
        method.choose(PaymentCredentialChoice::NewCard, true);
        assert!(method.needs_password());
        method.choose(PaymentCredentialChoice::NewCard, false);
        assert!(!method.needs_password() && method.card().is_some());
    }

    #[test]
    fn there_is_nothing_to_ask_a_password_for_without_a_card() {
        let mut method = PaymentMethod::NoCard;
        method.choose(PaymentCredentialChoice::NewCard, true);
        assert!(method.lacks_card() && !method.needs_password());
    }
}
