//! Requests of the paid-messages feature (TDLib 1.8.68 `td_api.tl`:
//! `canSendMessageToUser`, `getPaidMessageRevenue`,
//! `allowUnpaidMessagesFromUser`, `setChatDirectMessagesGroup`). The price
//! itself rides `setNewChatPrivacySettings` and `messageSendOptions`.

use crate::ids::{ChatId, RequestId};
use serde_json::json;

/// `canSendMessageToUser`: asks the server too (`only_local: false`), so a
/// stale cache cannot hide a price.
pub fn can_send_message_to_user(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "canSendMessageToUser",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "only_local": false,
    })
    .to_string()
}

/// `getPaidMessageRevenue`: Stars received from one user for paid messages.
pub fn get_paid_message_revenue(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "getPaidMessageRevenue",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// `allowUnpaidMessagesFromUser`: adds the user to the free-messages
/// exceptions, optionally refunding what they paid before.
pub fn allow_unpaid_messages_from_user(
    extra: RequestId,
    user_id: i64,
    refund_payments: bool,
) -> String {
    json!({
        "@type": "allowUnpaidMessagesFromUser",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "refund_payments": refund_payments,
    })
    .to_string()
}

/// `setChatDirectMessagesGroup` for a channel chat; owner only.
pub fn set_chat_direct_messages_group(
    extra: RequestId,
    chat_id: ChatId,
    is_enabled: bool,
    paid_message_star_count: i64,
) -> String {
    json!({
        "@type": "setChatDirectMessagesGroup",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "is_enabled": is_enabled,
        "paid_message_star_count": paid_message_star_count,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn sends_carry_the_agreed_price_and_default_to_free() {
        use crate::composer::SendOptions;
        use crate::telegram::requests::send_text;
        let paid = SendOptions {
            paid_message_star_count: 25,
            ..SendOptions::default()
        };
        let v = parse(send_text(RequestId(1), ChatId(5), None, "hi", None, &paid));
        assert_eq!(v["options"]["paid_message_star_count"], 25);
        let free = parse(send_text(
            RequestId(2),
            ChatId(5),
            None,
            "hi",
            None,
            &SendOptions::default(),
        ));
        assert_eq!(free["options"]["paid_message_star_count"], 0);
    }

    #[test]
    fn can_send_asks_the_server() {
        let v = parse(can_send_message_to_user(RequestId(3), 77));
        assert_eq!(v["@type"], "canSendMessageToUser");
        assert_eq!(v["user_id"], 77);
        assert_eq!(v["only_local"], false);
    }

    #[test]
    fn revenue_and_exception_requests() {
        let v = parse(get_paid_message_revenue(RequestId(1), 9));
        assert_eq!(v["@type"], "getPaidMessageRevenue");
        assert_eq!(v["user_id"], 9);
        let v = parse(allow_unpaid_messages_from_user(RequestId(2), 9, true));
        assert_eq!(v["@type"], "allowUnpaidMessagesFromUser");
        assert_eq!(v["refund_payments"], true);
    }

    #[test]
    fn direct_messages_group_carries_the_price() {
        let v = parse(set_chat_direct_messages_group(
            RequestId(4),
            ChatId(-100),
            true,
            25,
        ));
        assert_eq!(v["@type"], "setChatDirectMessagesGroup");
        assert_eq!(v["chat_id"], -100);
        assert_eq!(v["is_enabled"], true);
        assert_eq!(v["paid_message_star_count"], 25);
    }
}
