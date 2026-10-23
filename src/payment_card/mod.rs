//! Paying with a new card: the card form's checks and the provider
//! tokenization behind it, after Telegram Desktop's `payments/` and
//! `stripe/` / `smartglocal/` clients.
//!
//! The card goes to the payment provider, never to Telegram: the provider
//! answers with a one-time token, and `sendPaymentForm` carries that token
//! as `inputCredentialsNew`. Card data is never logged, stored or put in
//! an error.
//!
//! Wiped on drop: [`CardDetails`]; the request body
//! ([`transport::HttpRequest::body`]) and the Stripe and Smart Glocal
//! bodies it is built from; every copy the field formatter and the checks
//! make ([`field_edit::FieldText`], its char buffers, the validator's digit
//! strings) and the card form's reads of its fields. Each is built at its
//! final size, so no reallocation leaves an old buffer behind.
//!
//! Not wiped: the text inside the kit's `InputState` fields (the kit owns
//! it, with its undo history) and whatever `ureq` and rustls copy while
//! sending the request.
#![deny(clippy::indexing_slicing)]

pub mod field_edit;
pub mod smartglocal;
pub mod stripe;
pub mod transport;
pub mod validate;

use crate::telegram::envelope::PaymentProviderKind;
use transport::{HttpRequest, Transport};
use validate::{ValidationState, YearMonth};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// The card form's fields, in tdesktop's `CardField` order: the order
/// local checks report in and focus moves through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardField {
    Number,
    Expiry,
    Cvc,
    Name,
    Country,
    Zip,
}

/// A provider Quill can send a card to itself (tdesktop's native
/// methods). Any other provider shows its own web form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeProvider {
    Stripe {
        publishable_key: String,
        need_country: bool,
        need_zip: bool,
        need_cardholder_name: bool,
    },
    SmartGlocal {
        public_token: String,
        tokenize_url: String,
        is_test: bool,
    },
}

impl NativeProvider {
    /// The native method for a form's provider, or `None` when the form
    /// has none (tdesktop: a native provider without its key is not
    /// supported either).
    pub fn for_form(provider: &PaymentProviderKind, is_test: bool) -> Option<Self> {
        match provider {
            PaymentProviderKind::Stripe {
                publishable_key,
                need_country,
                need_postal_code,
                need_cardholder_name,
            } if !publishable_key.is_empty() => Some(Self::Stripe {
                publishable_key: publishable_key.clone(),
                need_country: *need_country,
                need_zip: *need_postal_code,
                need_cardholder_name: *need_cardholder_name,
            }),
            PaymentProviderKind::SmartGlocal {
                public_token,
                tokenize_url,
            } if !public_token.is_empty() => Some(Self::SmartGlocal {
                public_token: public_token.clone(),
                tokenize_url: tokenize_url.clone(),
                is_test,
            }),
            _ => None,
        }
    }

    /// Which optional fields the form shows. Smart Glocal asks for none.
    pub fn needs(&self, field: CardField) -> bool {
        match (self, field) {
            (_, CardField::Number | CardField::Expiry | CardField::Cvc) => true,
            (
                Self::Stripe {
                    need_cardholder_name,
                    ..
                },
                CardField::Name,
            ) => *need_cardholder_name,
            (Self::Stripe { need_country, .. }, CardField::Country) => *need_country,
            (Self::Stripe { need_zip, .. }, CardField::Zip) => *need_zip,
            (Self::SmartGlocal { .. }, _) => false,
        }
    }
}

/// What the person typed, as tdesktop's `UncheckedCardDetails`: number and
/// CVC as digits, expiry as month and four-digit year, the country as an
/// ISO code. Wiped when dropped; never printed.
#[derive(Default, Zeroize, ZeroizeOnDrop)]
pub struct CardDetails {
    number: String,
    cvc: String,
    expiry_month: u32,
    expiry_year: u32,
    cardholder_name: String,
    address_country: String,
    address_zip: String,
}

impl CardDetails {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        number: String,
        cvc: String,
        expiry_month: u32,
        expiry_year: u32,
        cardholder_name: String,
        address_country: String,
        address_zip: String,
    ) -> Self {
        Self {
            number,
            cvc,
            expiry_month,
            expiry_year,
            cardholder_name,
            address_country,
            address_zip,
        }
    }
}

impl std::fmt::Debug for CardDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CardDetails(..)")
    }
}

/// `cardErrorLocal`: the first field that fails its check before anything
/// is sent.
pub fn local_error(
    card: &CardDetails,
    provider: &NativeProvider,
    today: YearMonth,
) -> Option<CardField> {
    let valid = ValidationState::Valid;
    let checks = [
        (
            CardField::Number,
            validate::validate_card(&card.number).state == valid,
        ),
        (
            CardField::Expiry,
            validate::validate_parsed_expiry(card.expiry_month, card.expiry_year, today) == valid,
        ),
        (
            CardField::Cvc,
            validate::validate_cvc(&card.number, &card.cvc).state == valid,
        ),
        (CardField::Name, !card.cardholder_name.is_empty()),
        (CardField::Country, !card.address_country.is_empty()),
        (CardField::Zip, !card.address_zip.is_empty()),
    ];
    checks
        .into_iter()
        .find(|(field, ok)| provider.needs(*field) && !ok)
        .map(|(field, _)| field)
}

/// The credentials a tokenization produced (tdesktop's `NewCredentials`):
/// a title like "visa *4242" for the payment method row and the JSON the
/// provider token travels in.
#[derive(Clone, PartialEq, Eq)]
pub struct NewCredentials {
    title: String,
    data: Zeroizing<String>,
}

impl NewCredentials {
    pub fn new(title: String, data: String) -> Self {
        Self {
            title,
            data: Zeroizing::new(data),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The `inputCredentialsNew.data` JSON.
    pub fn data(&self) -> &str {
        &self.data
    }
}

impl std::fmt::Debug for NewCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewCredentials")
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

/// tdesktop's `CardTitle`: "<brand> *<last four>".
fn card_title(kind: &str, last4: &str) -> String {
    format!("{kind} *{last4}")
}

/// A provider error by tdesktop's error id ("CardDeclined",
/// "InvalidNumber", "RequestError203", …). Ids never carry card data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    id: String,
}

impl ProviderError {
    fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    /// "RequestError<n>" for an HTTP failure, with the
    /// `QNetworkReply::NetworkError` number Qt gives that status.
    fn request(status: u16) -> Self {
        let code = match status {
            400 | 418 => 302,
            401 => 204,
            403 => 201,
            404 => 203,
            405 => 202,
            407 => 105,
            409 => 206,
            410 => 207,
            500 => 401,
            501 => 402,
            503 => 403,
            400..=499 => 299,
            _ => 499,
        };
        Self::new(format!("RequestError{code}"))
    }

    fn transport(error: transport::TransportError) -> Self {
        Self::new(format!("RequestError{}", error.qt_code()))
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Reads a provider answer: an empty body is no document, anything else
/// must be a JSON object.
fn parse_body(body: &[u8]) -> Result<Option<serde_json::Value>, ProviderError> {
    if body.is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| ProviderError::new("InvalidJson"))?;
    if !value.is_object() {
        return Err(ProviderError::new("InvalidJsonRoot"));
    }
    Ok(Some(value))
}

/// A failed tokenization, by provider, as tdesktop's `Error::Type::Stripe`
/// and `Error::Type::SmartGlocal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenizeError {
    Stripe(ProviderError),
    SmartGlocal(ProviderError),
}

/// How the checkout shows a [`TokenizeError`]
/// (`CheckoutProcess::handleError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardErrorNotice {
    /// Mark this field and focus it.
    Field(CardField),
    /// "Your card was declined."
    Declined,
    /// "Sorry, a processing error occurred."
    ProcessingError,
    /// "Stripe Error: <id>".
    Stripe(String),
    /// "SmartGlocal Error: <id>".
    SmartGlocal(String),
}

impl TokenizeError {
    pub fn notice(&self) -> CardErrorNotice {
        match self {
            Self::Stripe(error) => match error.id() {
                "InvalidNumber" | "IncorrectNumber" => CardErrorNotice::Field(CardField::Number),
                "InvalidCVC" | "IncorrectCVC" => CardErrorNotice::Field(CardField::Cvc),
                "InvalidExpiryMonth" | "InvalidExpiryYear" | "ExpiredCard" => {
                    CardErrorNotice::Field(CardField::Expiry)
                }
                "CardDeclined" => CardErrorNotice::Declined,
                "ProcessingError" => CardErrorNotice::ProcessingError,
                id => CardErrorNotice::Stripe(id.to_string()),
            },
            Self::SmartGlocal(error) => CardErrorNotice::SmartGlocal(error.id().to_string()),
        }
    }
}

/// Sends the card to the provider and returns its token as credentials.
/// Blocks on the network: run it off the UI thread.
pub fn tokenize(
    provider: &NativeProvider,
    card: &CardDetails,
    transport: &dyn Transport,
) -> Result<NewCredentials, TokenizeError> {
    let post = |request: HttpRequest| transport.post(&request).map_err(ProviderError::transport);
    match provider {
        NativeProvider::Stripe {
            publishable_key, ..
        } => post(stripe::token_request(publishable_key, card))
            .and_then(|response| stripe::parse_token_response(&response))
            .map_err(TokenizeError::Stripe),
        NativeProvider::SmartGlocal {
            public_token,
            tokenize_url,
            is_test,
        } => post(smartglocal::token_request(
            public_token,
            tokenize_url,
            *is_test,
            card,
        ))
        .and_then(|response| smartglocal::parse_token_response(&response))
        .map_err(TokenizeError::SmartGlocal),
    }
}

#[cfg(test)]
mod field_edit_tests;
#[cfg(test)]
mod tokenize_tests;
#[cfg(test)]
mod validate_tests;
