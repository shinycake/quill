//! Parses TDLib objects for sign-in, registration and the TDLib session lifecycle.
use crate::telegram::envelope::*;
use serde_json::Value;

/// The auth domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_auth_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateAuthorizationState" => {
            let state = value
                .get("authorization_state")
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Auth(
                AuthPayload::UpdateAuthorizationState(parse_auth(state)),
            ))
        }
        "authorizationStateWaitTdlibParameters"
        | "authorizationStateWaitPhoneNumber"
        | "authorizationStateWaitPremiumPurchase"
        | "authorizationStateWaitEmailAddress"
        | "authorizationStateWaitEmailCode"
        | "authorizationStateWaitCode"
        | "authorizationStateWaitOtherDeviceConfirmation"
        | "authorizationStateWaitRegistration"
        | "authorizationStateWaitPassword"
        | "authorizationStateReady"
        | "authorizationStateLoggingOut"
        | "authorizationStateClosing"
        | "authorizationStateClosed" => Ok(EnvelopePayload::Auth(
            AuthPayload::UpdateAuthorizationState(parse_auth(value)),
        )),
        "updateTermsOfService" => parse_terms_of_service(value)
            .map(|terms| EnvelopePayload::Auth(AuthPayload::UpdateTermsOfService { terms }))
            .ok_or(ParseError::MissingField),
        "emailAddressAuthenticationCodeInfo" => {
            Ok(EnvelopePayload::Auth(AuthPayload::EmailCodeInfo {
                pattern: json_field_str(value, "email_address_pattern"),
                length: json_i32(value.get("length"), 0).max(0),
            }))
        }
        // `countries` — the `getCountries` answer for the sign-in picker.
        "countries" => Ok(EnvelopePayload::Auth(AuthPayload::Countries {
            countries: crate::phone::countries_from_json(value),
        })),
        // Slice A8: `authenticationCodeInfo` — the
        // `sendPhoneNumberCode` / `resendPhoneNumberCode` answer (schema
        // 1.8.67, line 78). A missing `phone_number` / `timeout` degrades
        // to empty / 0 rather than failing the parse; the reducer only
        // trusts it when it answers our own in-flight request.
        "authenticationCodeInfo" => {
            let phone_number = value
                .get("phone_number")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let timeout = value
                .get("timeout")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .clamp(0, i32::MAX as i64) as i32;
            Ok(EnvelopePayload::Auth(AuthPayload::AuthenticationCodeInfo {
                phone_number,
                timeout,
            }))
        }
        _ => return Ok(None),
    };
    payload.map(Some)
}
