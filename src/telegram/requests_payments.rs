//! Payment request builders: saved order info and saved credentials
//! clearing (TDLib 1.8.67). New code lives here per the file-size
//! directive — `requests.rs` keeps only the irreducible core.

use crate::ids::RequestId;
use serde_json::json;

/// `deleteSavedOrderInfo` (TDLib 1.8.67, `schema/td_api.tl:15286`):
/// "Deletes saved order information". Response is `ok`. Both of these are
/// parameterless `= Ok` constructors; the saved info lives server-side, so
/// there is nothing client-side to clear.
pub fn delete_saved_order_info(extra: RequestId) -> String {
    json!({
        "@type": "deleteSavedOrderInfo",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `deleteSavedCredentials` (TDLib 1.8.67, `schema/td_api.tl:15289`):
/// "Deletes saved credentials for all payment provider bots". Response is `ok`.
pub fn delete_saved_credentials(extra: RequestId) -> String {
    json!({
        "@type": "deleteSavedCredentials",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_saved_order_info_shape_matches_1_8_67() {
        // `deleteSavedOrderInfo = Ok;` (schema 1.8.67, line 15286).
        assert!(include_str!("../../schema/td_api.tl").contains("deleteSavedOrderInfo = Ok;"));
        let v: serde_json::Value =
            serde_json::from_str(&delete_saved_order_info(RequestId(75))).unwrap();
        assert_eq!(v["@type"], "deleteSavedOrderInfo");
        assert_eq!(v["@extra"], "75");
    }

    #[test]
    fn delete_saved_credentials_shape_matches_1_8_67() {
        // `deleteSavedCredentials = Ok;` (schema 1.8.67, line 15289).
        assert!(include_str!("../../schema/td_api.tl").contains("deleteSavedCredentials = Ok;"));
        let v: serde_json::Value =
            serde_json::from_str(&delete_saved_credentials(RequestId(76))).unwrap();
        assert_eq!(v["@type"], "deleteSavedCredentials");
        assert_eq!(v["@extra"], "76");
    }
}
