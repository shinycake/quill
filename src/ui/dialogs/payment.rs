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
    pub(crate) credential_choice: PaymentCredentialChoice,
    /// The card the provider tokenized, and whether to save it (tdesktop's
    /// `NewCredentials`). Kept only while the checkout is open.
    pub(crate) new_card: NewCardStage,
    /// What fills the dialog: the checkout, or the country box or "New
    /// Card" form open over it.
    pub(crate) page: PaymentPage,
    pub(crate) terms_accepted: bool,
    pub(crate) allow_save_order: bool,
    /// Slice P1 fix-up: the dialog opens on Buy press before the form
    /// arrives — the saved order info prefills once, when the form lands.
    pub(crate) prefill: Prefill,
}

/// Whether the form's saved order info went into the inputs yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Prefill {
    AwaitingForm,
    Filled,
}

/// The checkout's page: the form itself, or one picker over it. A page
/// leaves to the checkout: the pick, Cancel, or the dialog closing.
pub(crate) enum PaymentPage {
    Checkout,
    Country(crate::ui::country_box::CountryBox),
    CardForm(crate::ui::payments::card_form::CardForm),
}

/// The card entered in the "New Card" form. `NeedsPassword` is the same
/// card after asking to save it when the account cannot save yet: "Turn on
/// Two-Step Verification" (tdesktop's `askSetPassword`) waits on it and Pay
/// stays off; choosing another method, dismissing the ask or setting the
/// password leaves it for `Ready`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NewCardStage {
    None,
    Ready(NewCard),
    NeedsPassword(NewCard),
}

impl NewCardStage {
    pub(crate) fn card(&self) -> Option<&NewCard> {
        match self {
            NewCardStage::Ready(card) | NewCardStage::NeedsPassword(card) => Some(card),
            NewCardStage::None => None,
        }
    }

    pub(crate) fn is_none(&self) -> bool {
        matches!(self, NewCardStage::None)
    }

    pub(crate) fn needs_password(&self) -> bool {
        matches!(self, NewCardStage::NeedsPassword(_))
    }

    /// Let go of the password ask; the card stays.
    pub(crate) fn drop_password_prompt(&mut self) {
        if let NewCardStage::NeedsPassword(card) = std::mem::replace(self, NewCardStage::None) {
            *self = NewCardStage::Ready(card);
        }
    }

    /// Ask for a password (when `ask`) before the card can be saved.
    pub(crate) fn ask_password(&mut self, ask: bool) {
        self.drop_password_prompt();
        if ask && let NewCardStage::Ready(card) = std::mem::replace(self, NewCardStage::None) {
            *self = NewCardStage::NeedsPassword(card);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaymentCredentialChoice {
    Saved(String),
    /// The tokenized [`NewCard`], or nothing chosen yet when there is none.
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
            credential_choice: PaymentCredentialChoice::NewCard,
            new_card: NewCardStage::None,
            page: PaymentPage::Checkout,
            terms_accepted: false,
            allow_save_order: true,
            prefill: Prefill::AwaitingForm,
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
        if self.prefill == Prefill::Filled {
            return;
        }
        if let PaymentFormTypeData::Regular(regular) = &form.form_type {
            self.prefill(&regular.saved_order_info, window, cx);
            if let Some(first) = regular.saved_credentials.first() {
                self.credential_choice = PaymentCredentialChoice::Saved(first.id.clone());
            }
        }
        self.prefill = Prefill::Filled;
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
            PaymentPage::CardForm(form) => Some(form),
            PaymentPage::Checkout | PaymentPage::Country(_) => None,
        }
    }

    pub(crate) fn card_form_mut(
        &mut self,
    ) -> Option<&mut crate::ui::payments::card_form::CardForm> {
        match &mut self.page {
            PaymentPage::CardForm(form) => Some(form),
            PaymentPage::Checkout | PaymentPage::Country(_) => None,
        }
    }

    pub(crate) fn country_box(&self) -> Option<&crate::ui::country_box::CountryBox> {
        match &self.page {
            PaymentPage::Country(country_box) => Some(country_box),
            PaymentPage::Checkout | PaymentPage::CardForm(_) => None,
        }
    }

    pub(crate) fn country_box_mut(&mut self) -> Option<&mut crate::ui::country_box::CountryBox> {
        match &mut self.page {
            PaymentPage::Country(country_box) => Some(country_box),
            PaymentPage::Checkout | PaymentPage::CardForm(_) => None,
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
