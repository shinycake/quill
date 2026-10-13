//! Notification texts of `pushMessageContentTonWalletTransfer` and
//! `pushMessageContentTonConnectRequest` (td_api.tl:9036, 9040), worded
//! like the official iOS app's `MESSAGE_GRAM_TRANSFER*` and
//! `MESSAGE_WALLET_TONCONNECT_REQUEST*` push strings.
//!
//! TDLib hands these only to clients that feed it push payloads
//! (`processPushNotification`, `notificationTypeNewPushMessage`). Quill
//! gets its notifications from `updateNewMessage`, where
//! [`super::TonWalletTransfer::received_preview`] words them; this is the
//! wording for a push path, should one be added.

use serde_json::Value;

/// The body of a wallet push content, or `None` for any other content.
/// `user_name` resolves the sender of a transfer; an unknown sender gets
/// the "You received …" form.
pub fn push_text(content: &Value, user_name: impl Fn(i64) -> Option<String>) -> Option<String> {
    let kind = content.get("@type").and_then(Value::as_str)?;
    let field = |key: &str| {
        content
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    match kind {
        "pushMessageContentTonWalletTransfer" => {
            let amount = field("amount");
            let comment = field("comment");
            let sender = content
                .get("sender_user_id")
                .and_then(Value::as_i64)
                .filter(|id| *id != 0)
                .and_then(user_name)
                .filter(|name| !name.trim().is_empty());
            let line = match sender {
                Some(name) => format!("{name} sent you {amount}"),
                None => format!("You received {amount}"),
            };
            Some(if comment.is_empty() {
                line
            } else {
                format!("{line}: {comment}")
            })
        }
        "pushMessageContentTonConnectRequest" => {
            let dapp = field("dapp_name");
            Some(if dapp.is_empty() {
                "You have a TON Connect request".to_string()
            } else {
                format!("You have a TON Connect request from {dapp}")
            })
        }
        _ => None,
    }
}
