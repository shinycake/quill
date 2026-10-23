//! The "New Card" form over the checkout: Telegram Desktop's
//! `Payments::Ui::EditCard` with `Form::validateCard`. The fields format
//! as they are typed, Done checks them and sends the card to the provider
//! off the UI thread, and the provider's token becomes the checkout's
//! payment method.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::country_box::{CountryBox, CountryHost};
use crate::ui::dialogs::{NewCard, PaymentCredentialChoice};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use quill::country_select::CountryMode;
use quill::lang::{keys, tr, tr_with};
use quill::payment_card::field_edit::{
    EditSnapshot, FieldResult, FieldText, digits_of, edit_card_number, edit_cvc, edit_expiry,
    edit_holder_name, edit_required, parse_expiry,
};
use quill::payment_card::transport::Transport;
use quill::payment_card::validate::YearMonth;
use quill::payment_card::{
    CardDetails, CardErrorNotice, CardField, NativeProvider, NewCredentials, TokenizeError,
    local_error, tokenize,
};
use quill::telegram::envelope::PaymentFormTypeData;
use std::sync::Arc;
use zeroize::Zeroizing;

/// One text field of the form.
pub(crate) struct CardInput {
    pub(crate) field: CardField,
    pub(crate) state: Entity<InputState>,
    /// Shown as an error until the next edit says otherwise.
    pub(crate) invalid: bool,
    /// The text just before a Backspace or Delete (see `field_edit`).
    before_delete: Option<EditSnapshot>,
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
impl CardInput {
    pub(crate) fn has_delete_snapshot(&self) -> bool {
        self.before_delete.is_some()
    }
}

/// Editing, or waiting for the provider's token (Done is busy then and
/// a second press does nothing, as `if (_stripe) return;` in tdesktop).
pub(crate) enum CardPhase {
    Editing,
    Tokenizing { _task: Task<()> },
}

pub(crate) struct CardForm {
    pub(crate) provider: NativeProvider,
    pub(crate) inputs: Vec<CardInput>,
    /// The billing country's ISO code (Stripe's `need_country`).
    pub(crate) country_iso: String,
    pub(crate) country_invalid: bool,
    pub(crate) country_box: Option<CountryBox>,
    /// Focus on the country field itself, without opening its box.
    pub(crate) country_focus: FocusHandle,
    /// The country the box starts on: the account's phone country.
    default_country: String,
    /// "Save Information for future use", when the form allows saving.
    pub(crate) save: Option<bool>,
    pub(crate) phase: CardPhase,
    /// The provider's last refusal ("Your card was declined."), shown in
    /// the form until Done is pressed again or it is closed. Error ids
    /// only, never card data.
    pub(crate) notice: Option<SharedString>,
}

impl CardForm {
    /// Fields in focus order: the text fields, with the country between
    /// the name and the ZIP code.
    pub(crate) fn order(&self) -> Vec<CardField> {
        [
            CardField::Number,
            CardField::Expiry,
            CardField::Cvc,
            CardField::Name,
            CardField::Country,
            CardField::Zip,
        ]
        .into_iter()
        .filter(|field| self.provider.needs(*field))
        .collect()
    }

    pub(crate) fn input(&self, field: CardField) -> Option<&CardInput> {
        self.inputs.iter().find(|input| input.field == field)
    }

    fn input_mut(&mut self, field: CardField) -> Option<&mut CardInput> {
        self.inputs.iter_mut().find(|input| input.field == field)
    }

    pub(crate) fn is_tokenizing(&self) -> bool {
        matches!(self.phase, CardPhase::Tokenizing { .. })
    }

    /// A copy of the field's text, wiped when dropped.
    fn value(&self, field: CardField, cx: &App) -> Zeroizing<String> {
        Zeroizing::new(
            self.input(field)
                .map(|input| input.state.read(cx).value().to_string())
                .unwrap_or_default(),
        )
    }

    /// tdesktop's `EditCard::collect`.
    fn collect(&self, cx: &App) -> CardDetails {
        let (month, year) = parse_expiry(&self.value(CardField::Expiry, cx));
        CardDetails::new(
            digits_of(&self.value(CardField::Number, cx)),
            digits_of(&self.value(CardField::Cvc, cx)),
            month,
            year,
            std::mem::take(&mut *self.value(CardField::Name, cx)),
            self.country_iso.clone(),
            std::mem::take(&mut *self.value(CardField::Zip, cx)),
        )
    }
}

/// The field placeholder, as tdesktop's `EditCard::setupContent`.
pub(crate) fn placeholder(field: CardField) -> &'static str {
    match field {
        CardField::Number => tr(keys::PAYMENTS_CARD_NUMBER),
        CardField::Expiry => tr(keys::PAYMENTS_CARD_EXPIRE_DATE),
        CardField::Cvc => tr(keys::PAYMENTS_CARD_CVC),
        CardField::Name => tr(keys::PAYMENTS_CARD_HOLDER),
        CardField::Country => tr(keys::PAYMENTS_BILLING_COUNTRY),
        CardField::Zip => tr(keys::PAYMENTS_BILLING_ZIP_CODE),
    }
}

fn today() -> YearMonth {
    let now = quill::local_time::civil_local(quill::local_time::now_unix());
    YearMonth {
        year: u32::try_from(now.year).unwrap_or(0),
        month: u32::from(now.month),
    }
}

fn char_index(value: &str, byte: usize) -> usize {
    value
        .get(..byte)
        .map_or(value.chars().count(), |head| head.chars().count())
}

fn byte_index(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map_or(value.len(), |(byte, _)| byte)
}

/// The text and caret of an input, in characters.
fn field_text(state: &InputState) -> FieldText {
    let value = state.value().to_string();
    let cursor = char_index(&value, state.cursor());
    FieldText::new(value, cursor)
}

impl QuillApp {
    /// "New Card..." on a Stripe or Smart Glocal checkout: the card form,
    /// focused on the number (tdesktop's `showEditCard`).
    pub(in crate::ui) fn open_card_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.session().and_then(|s| s.payments.form.clone()) else {
            return;
        };
        let PaymentFormTypeData::Regular(regular) = &form.form_type else {
            return;
        };
        let Some(provider) = NativeProvider::for_form(&regular.provider, regular.invoice.is_test)
        else {
            return;
        };
        let can_save = regular.can_save_credentials || regular.need_password;
        let default_country = self.phone_country();
        let mut inputs = Vec::new();
        for field in [
            CardField::Number,
            CardField::Expiry,
            CardField::Cvc,
            CardField::Name,
            CardField::Zip,
        ] {
            if !provider.needs(field) {
                continue;
            }
            let state = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder(field)));
            // window-scope: the checkout and its card form are the app's, no chat window's.
            cx.subscribe_in(
                &state,
                window,
                move |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.card_field_changed(field, window, cx),
                    InputEvent::PressEnter { .. } => this.card_field_submitted(field, window, cx),
                    InputEvent::Focus | InputEvent::Blur => {}
                },
            )
            .detach();
            inputs.push(CardInput {
                field,
                state,
                invalid: false,
                before_delete: None,
            });
        }
        let card_form = CardForm {
            provider,
            inputs,
            country_iso: String::new(),
            country_invalid: false,
            country_box: None,
            country_focus: cx.focus_handle(),
            default_country,
            save: can_save.then_some(false),
            phase: CardPhase::Editing,
            notice: None,
        };
        if let Some(dialog) = self.payments.dialog.as_mut() {
            dialog.card_form = Some(card_form);
        }
        self.focus_card_field(CardField::Number, window, cx);
        cx.notify();
    }

    /// The account's country by its phone number (tdesktop's
    /// `defaultCountry`), for the country box to start on.
    fn phone_country(&self) -> String {
        let Some(session) = self.session() else {
            return String::new();
        };
        let phone = session
            .my_user_id
            .and_then(|id| session.users.get(&id))
            .map(|me| quill::phone::digits_only(&me.phone_number))
            .unwrap_or_default();
        let countries = session.auth_state.countries.clone().unwrap_or_default();
        quill::phone::country_for_digits(&phone, &countries)
            .map(|country| country.iso.clone())
            .unwrap_or_default()
    }

    /// Cancel: back to the checkout, unless the card is being sent.
    pub(in crate::ui) fn close_card_form(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.payments.dialog.as_mut()
            && dialog
                .card_form
                .as_ref()
                .is_none_or(|form| !form.is_tokenizing())
        {
            dialog.card_form = None;
        }
        cx.notify();
    }

    pub(in crate::ui) fn card_form(&self) -> Option<&CardForm> {
        self.payments.dialog.as_ref()?.card_form.as_ref()
    }

    fn card_form_mut(&mut self) -> Option<&mut CardForm> {
        self.payments.dialog.as_mut()?.card_form.as_mut()
    }

    /// Focus `field` (tdesktop's `Field::setFocus`): the country field
    /// takes focus without opening its box.
    pub(in crate::ui) fn focus_card_field(
        &mut self,
        field: CardField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.card_form() else {
            return;
        };
        if field == CardField::Country {
            let focus = form.country_focus.clone();
            window.focus(&focus, cx);
        } else if let Some(input) = form.input(field) {
            let state = input.state.clone();
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    /// Activate `field` (tdesktop's `Field::activate`): the country field
    /// opens its box.
    pub(in crate::ui) fn activate_card_field(
        &mut self,
        field: CardField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if field == CardField::Country {
            self.open_country_box(CountryHost::PaymentCard, window, cx);
        } else {
            self.focus_card_field(field, window, cx);
        }
    }

    fn neighbour_field(&self, field: CardField, step: isize) -> Option<CardField> {
        let order = self.card_form()?.order();
        let at = order.iter().position(|f| *f == field)?;
        order.get(at.checked_add_signed(step)?).copied()
    }

    /// Enter in a valid field, or a picked country: activate the next
    /// field, or press Done after the last.
    fn advance_card_field(
        &mut self,
        field: CardField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.neighbour_field(field, 1) {
            Some(next) => self.activate_card_field(next, window, cx),
            None => self.submit_card_form(window, cx),
        }
    }

    /// Backspace or Delete is about to edit `field`: remember the text so
    /// the edit can step over a separator. Backspace at the very start
    /// moves to the previous field.
    pub(in crate::ui) fn card_field_before_delete(
        &mut self,
        field: CardField,
        backspace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.card_form().and_then(|form| form.input(field)) else {
            return;
        };
        let state = input.state.read(cx);
        let value = state.value().to_string();
        let selected = state.selected_range();
        let cursor_byte = state.cursor();
        let anchor_byte = if selected.start == cursor_byte {
            selected.end
        } else {
            selected.start
        };
        let cursor = char_index(&value, cursor_byte);
        let anchor = char_index(&value, anchor_byte);
        let at_front = backspace && cursor == 0 && anchor == 0;
        let snapshot = EditSnapshot::new(value, cursor, anchor);
        if let Some(input) = self.card_form_mut().and_then(|form| form.input_mut(field)) {
            input.before_delete = Some(snapshot);
        }
        // The input handles the key after this capture handler and emits
        // its Change in this same effect cycle. A defer queued from a defer
        // runs after that Change, so a press that changed nothing (at the
        // start or end of the text) leaves no snapshot for later typing.
        cx.defer_in(window, move |_, window, cx| {
            cx.defer_in(window, move |this, _, _| {
                if let Some(input) = this.card_form_mut().and_then(|form| form.input_mut(field)) {
                    input.before_delete = None;
                }
            });
        });
        if at_front && let Some(previous) = self.neighbour_field(field, -1) {
            self.focus_card_field(previous, window, cx);
        }
    }

    /// The field's text changed: reformat it, mark it invalid or move on
    /// when it is complete (tdesktop's `Field::setupValidator`).
    fn card_field_changed(
        &mut self,
        field: CardField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.card_form_mut() else {
            return;
        };
        let number = Zeroizing::new(digits_of(&form.value(CardField::Number, cx)));
        let Some(input) = form.input_mut(field) else {
            return;
        };
        let now = field_text(input.state.read(cx));
        let was = input.before_delete.take();
        let result: FieldResult = match field {
            CardField::Number => edit_card_number(was.as_ref(), &now),
            CardField::Expiry => edit_expiry(was.as_ref(), &now, today()),
            CardField::Cvc => edit_cvc(&number, &now),
            CardField::Name => edit_holder_name(&now),
            CardField::Zip | CardField::Country => edit_required(&now),
        };
        input.invalid = result.invalid() && !result.finished();
        if result.value() != now.value() || result.cursor() != now.cursor() {
            let value = Zeroizing::new(result.value().to_string());
            let cursor = byte_index(&value, result.cursor());
            input.state.update(cx, |state, cx| {
                if state.value().as_ref() != value.as_str() {
                    state.set_value(value.as_str(), window, cx);
                }
                state.set_selected_range(cursor..cursor, cx);
            });
        }
        if result.finished()
            && let Some(next) = self.neighbour_field(field, 1)
        {
            self.focus_card_field(next, window, cx);
        }
        cx.notify();
    }

    /// Enter in a field: an invalid field shows its error, a valid one
    /// moves on (tdesktop's `Field::setupSubmit`).
    fn card_field_submitted(
        &mut self,
        field: CardField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let invalid = self
            .card_form()
            .and_then(|form| form.input(field))
            .is_some_and(|input| input.invalid);
        if invalid {
            self.show_card_error(field, window, cx);
        } else {
            self.advance_card_field(field, window, cx);
        }
    }

    /// Mark `field` and focus it (tdesktop's `EditCard::showError`).
    pub(in crate::ui) fn show_card_error(
        &mut self,
        field: CardField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.card_form_mut() {
            if field == CardField::Country {
                form.country_invalid = true;
            } else if let Some(input) = form.input_mut(field) {
                input.invalid = true;
            }
        }
        self.focus_card_field(field, window, cx);
        cx.notify();
    }

    pub(in crate::ui) fn toggle_card_save(&mut self, save: bool, cx: &mut Context<Self>) {
        if let Some(form) = self.card_form_mut()
            && form.save.is_some()
        {
            form.save = Some(save);
        }
        cx.notify();
    }

    /// A billing country was picked in the box: keep it and move on to
    /// the next field.
    pub(in crate::ui) fn card_country_picked(
        &mut self,
        iso: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.card_form_mut() {
            form.country_iso = iso;
            form.country_invalid = false;
        }
        self.advance_card_field(CardField::Country, window, cx);
    }

    /// The country box's starting choice: the picked country, else the
    /// phone country.
    pub(in crate::ui) fn card_country_box(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<CountryBox> {
        let form = self.card_form()?;
        let start = if form.country_iso.is_empty() {
            form.default_country.clone()
        } else {
            form.country_iso.clone()
        };
        let chosen: Vec<String> = Some(start)
            .filter(|iso| !iso.is_empty())
            .into_iter()
            .collect();
        Some(CountryBox::new(CountryMode::Single, &chosen, window, cx))
    }

    /// Done: check the fields, then send the card to the provider
    /// (tdesktop's `Form::validateCard`).
    pub(in crate::ui) fn submit_card_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.card_form() else {
            return;
        };
        if form.is_tokenizing() {
            return;
        }
        let card = form.collect(cx);
        if let Some(field) = local_error(&card, &form.provider, today()) {
            self.show_card_error(field, window, cx);
            return;
        }
        let provider = form.provider.clone();
        let transport = self.card_transport();
        if let Some(form) = self.card_form_mut() {
            form.notice = None;
        }
        let tokenizing = cx
            .background_executor()
            .spawn(async move { tokenize(&provider, &card, transport.as_ref()) });
        let task = Self::spawn_main_in(window, cx, async move |this, cx| {
            let result = tokenizing.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_card_tokenize(result, window, cx);
            });
        });
        if let Some(form) = self.card_form_mut() {
            form.phase = CardPhase::Tokenizing { _task: task };
        }
        cx.notify();
    }

    /// The live transport, a test's replacement, or in an offline demo a
    /// recorded answer (demos never reach the network).
    fn card_transport(&self) -> Arc<dyn Transport> {
        if let Some(transport) = self.payments.card_transport.clone() {
            return transport;
        }
        if self.demo_session.is_some() {
            return Arc::new(super::card_form_demo::RecordedStripe);
        }
        Arc::new(quill::payment_card::transport::UreqTransport::default())
    }

    /// The provider answered (tdesktop's `setPaymentCredentials` or the
    /// Stripe / SmartGlocal branch of `handleError`).
    pub(in crate::ui) fn finish_card_tokenize(
        &mut self,
        result: Result<NewCredentials, TokenizeError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.card_form_mut() {
            form.phase = CardPhase::Editing;
        }
        let notice = match result {
            Ok(credentials) => {
                self.use_new_card(credentials, cx);
                return;
            }
            Err(error) => error.notice(),
        };
        let text = match notice {
            CardErrorNotice::Field(field) => {
                self.show_card_error(field, window, cx);
                return;
            }
            CardErrorNotice::Declined => tr(keys::PAYMENTS_CARD_DECLINED).to_string(),
            CardErrorNotice::ProcessingError => {
                tr(keys::Q_PAYMENT_CARD_PROCESSING_ERROR).to_string()
            }
            CardErrorNotice::Stripe(id) => {
                tr_with(keys::Q_PAYMENT_CARD_STRIPE_ERROR, &[("id", &id)])
            }
            CardErrorNotice::SmartGlocal(id) => {
                tr_with(keys::Q_PAYMENT_CARD_SMARTGLOCAL_ERROR, &[("id", &id)])
            }
        };
        if let Some(form) = self.card_form_mut() {
            form.notice = Some(text.into());
        }
        cx.notify();
    }

    pub(in crate::ui) fn dismiss_card_notice(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.card_form_mut() {
            form.notice = None;
        }
        cx.notify();
    }

    /// The tokenized card becomes the chosen payment method; saving it
    /// without a password first asks to set one (tdesktop's
    /// `Form::setPaymentCredentials`).
    fn use_new_card(&mut self, credentials: NewCredentials, cx: &mut Context<Self>) {
        let ask_password = self.needs_password_to_save();
        let Some(dialog) = self.payments.dialog.as_mut() else {
            return;
        };
        let save = dialog
            .card_form
            .as_ref()
            .and_then(|form| form.save)
            .unwrap_or(false);
        dialog.new_card = Some(NewCard { credentials, save });
        dialog.credential_choice = PaymentCredentialChoice::NewCard;
        dialog.card_form = None;
        dialog.password_prompt = save && ask_password;
        cx.notify();
    }

    /// The checkout can save a card on the server: it says so, or it only
    /// lacked a password and the account has one now (tdesktop's
    /// `Form::setHasPassword`, fed by the cloud password state).
    pub(in crate::ui) fn checkout_can_save_credentials(&self) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let Some(PaymentFormTypeData::Regular(regular)) =
            session.payments.form.as_ref().map(|form| &form.form_type)
        else {
            return false;
        };
        let has_password = session
            .auth_state
            .password_state
            .as_ref()
            .is_some_and(|state| state.has_password);
        regular.can_save_credentials || (regular.need_password && has_password)
    }

    /// Saving the new card needs a password the account does not have yet
    /// (tdesktop's `requestNewPassword`).
    pub(in crate::ui) fn needs_password_to_save(&self) -> bool {
        let need_password = self
            .session()
            .and_then(|s| s.payments.form.as_ref())
            .is_some_and(|form| match &form.form_type {
                PaymentFormTypeData::Regular(regular) => regular.need_password,
                _ => false,
            });
        need_password && !self.checkout_can_save_credentials()
    }
}
