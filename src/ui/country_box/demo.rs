//! Screenshot demos: the country box over the checkout and the poll.
//! `QUILL_DEMO_COUNTRY` is `closed` (the field row only) or `open`.

use super::CountryHost;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::telegram::envelope::{
    InvoiceForm, LabeledPrice, OrderInfoData, PaymentFormData, PaymentFormRegular,
    PaymentFormTypeData, PaymentProviderKind,
};

register_demos![
    DemoSpec::chat_list("ready-country-poll").setup(QuillApp::demo_country_poll),
    DemoSpec::chat_list("ready-country-payment").setup(QuillApp::demo_country_payment),
];

fn open_requested() -> bool {
    std::env::var("QUILL_DEMO_COUNTRY").is_ok_and(|mode| mode == "open")
}

pub(super) fn shipping_form() -> PaymentFormData {
    PaymentFormData {
        id: 1,
        form_type: PaymentFormTypeData::Regular(Box::new(PaymentFormRegular {
            invoice: InvoiceForm {
                currency: "USD".into(),
                price_parts: vec![LabeledPrice {
                    label: "Mug".into(),
                    amount: 1200,
                }],
                max_tip_amount: 0,
                suggested_tip_amounts: Vec::new(),
                recurring_terms_url: String::new(),
                terms_url: String::new(),
                is_test: false,
                need_name: true,
                need_phone_number: false,
                need_email_address: false,
                need_shipping_address: true,
                is_flexible: false,
            },
            provider: PaymentProviderKind::Token {
                name: "Stripe".into(),
            },
            additional_options: Vec::new(),
            saved_order_info: OrderInfoData::default(),
            saved_credentials: Vec::new(),
            can_save_credentials: false,
            need_password: false,
        })),
        seller_bot_user_id: 0,
        product_title: "Ceramic mug".into(),
        product_description: "Dishwasher safe.".into(),
    }
}

impl QuillApp {
    fn demo_country_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_poll_dialog(window, cx);
        if let Some(dialog) = self.composer_ui.poll_dialog.as_mut() {
            dialog.country_codes = vec!["US".into(), "GB".into()];
        }
        if open_requested() {
            self.open_country_box(CountryHost::Poll, window, cx);
        }
    }

    fn demo_country_payment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.payments.form = Some(shipping_form());
        }
        self.open_payment_dialog(window, cx);
        if let Some(dialog) = self.payments.dialog.as_mut() {
            dialog.country_iso = "DE".into();
        }
        if open_requested() {
            self.open_country_box(CountryHost::Payment, window, cx);
        }
    }
}
