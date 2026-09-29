use super::super::*;

/// Slice P1: the payment checkout dialog. The text inputs mirror the
/// `orderInfo` fields the invoice needs (`need_name` / `need_phone_number`
/// / `need_email_address` / `need_shipping_address`, schema:4655); only
/// the needed ones render. The credential choice is either a saved
/// credential (`inputCredentialsSaved`, schema:4677) or a provider token
/// (`inputCredentialsNew`, schema:4680).
pub struct PaymentDialog {
    pub(crate) name_input: Entity<TextareaState>,
    pub(crate) phone_input: Entity<TextareaState>,
    pub(crate) email_input: Entity<TextareaState>,
    pub(crate) street1_input: Entity<TextareaState>,
    pub(crate) street2_input: Entity<TextareaState>,
    pub(crate) city_input: Entity<TextareaState>,
    pub(crate) state_input: Entity<TextareaState>,
    pub(crate) country_input: Entity<TextareaState>,
    pub(crate) postal_input: Entity<TextareaState>,
    pub(crate) token_input: Entity<TextareaState>,
    pub(crate) credential_choice: PaymentCredentialChoice,
    pub(crate) terms_accepted: bool,
    pub(crate) allow_save_order: bool,
    pub(crate) allow_save_credentials: bool,
    /// Slice P1 fix-up: the dialog opens on Buy press before the form
    /// arrives — the saved order info prefills once, when the form lands.
    pub(crate) prefilled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaymentCredentialChoice {
    Saved(String),
    NewToken,
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
            name_input: input(cx, "Full name"),
            phone_input: input(cx, "Phone number"),
            email_input: input(cx, "Email address"),
            street1_input: input(cx, "Street address"),
            street2_input: input(cx, "Street address (line 2, optional)"),
            city_input: input(cx, "City"),
            state_input: input(cx, "State"),
            country_input: input(cx, "Country code (e.g. US)"),
            postal_input: input(cx, "Postal code"),
            token_input: input(cx, "Provider credential token"),
            credential_choice: PaymentCredentialChoice::NewToken,
            terms_accepted: false,
            allow_save_order: true,
            allow_save_credentials: true,
            prefilled: false,
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
        if self.prefilled {
            return;
        }
        if let PaymentFormTypeData::Regular(regular) = &form.form_type {
            self.prefill(&regular.saved_order_info, window, cx);
            if let Some(first) = regular.saved_credentials.first() {
                self.credential_choice = PaymentCredentialChoice::Saved(first.id.clone());
            }
        }
        self.prefilled = true;
    }

    /// Prefill the order fields from the form's `saved_order_info`
    /// (schema:4720) so returning buyers don't retype.
    fn prefill(&self, order: &OrderInfoData, window: &mut Window, cx: &mut Context<QuillApp>) {
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
        set(&self.country_input, &addr.country_code);
        set(&self.postal_input, &addr.postal_code);
    }

    /// Freeze the dialog inputs into an `orderInfo` (schema:4662).
    pub(crate) fn order(&self, cx: &App) -> OrderInfoData {
        let value = |input: &Entity<TextareaState>| input.read(cx).value().to_string();
        OrderInfoData {
            name: value(&self.name_input),
            phone_number: value(&self.phone_input),
            email_address: value(&self.email_input),
            shipping_address: AddressData {
                country_code: value(&self.country_input),
                state: value(&self.state_input),
                city: value(&self.city_input),
                street_line1: value(&self.street1_input),
                street_line2: value(&self.street2_input),
                postal_code: value(&self.postal_input),
            },
        }
    }

    pub(crate) fn token(&self, cx: &App) -> String {
        self.token_input.read(cx).value().to_string()
    }
}
