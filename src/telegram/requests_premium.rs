//! Premium / Stars / received-gift request builders (TDLib 1.8.68,
//! `schema/td_api.tl`). Read-only fetches plus the two own-gift mutations
//! (`toggleGiftIsSaved`, `sellGift`); nothing here pays, sends or
//! transfers.

use crate::ids::RequestId;
use crate::premium_hub::TxFilter;
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

fn owner(sender: MessageSender) -> Value {
    match sender {
        MessageSender::User { user_id } => json!({"@type":"messageSenderUser","user_id":user_id}),
        MessageSender::Chat { chat_id } => json!({"@type":"messageSenderChat","chat_id":chat_id}),
    }
}

/// `getStarTransactions` for the signed-in user. `filter` picks the
/// incoming/outgoing tab; the `direction` field is omitted (null) for All.
pub fn get_star_transactions(
    extra: RequestId,
    my_user_id: i64,
    filter: TxFilter,
    offset: &str,
    limit: i32,
) -> String {
    let mut request = json!({
        "@type": "getStarTransactions",
        "@extra": extra.as_extra(),
        "owner_id": owner(MessageSender::User { user_id: my_user_id }),
        "subscription_id": "",
        "offset": offset,
        "limit": limit,
    });
    if let Some(direction) = filter.direction_type() {
        request["direction"] = json!({ "@type": direction });
    }
    request.to_string()
}

/// `getReceivedGifts` (every gift the owner may be shown, no filters).
pub fn get_received_gifts(
    extra: RequestId,
    owner_id: MessageSender,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getReceivedGifts",
        "@extra": extra.as_extra(),
        "business_connection_id": "",
        "owner_id": owner(owner_id),
        "collection_id": 0,
        "exclude_unsaved": false,
        "exclude_saved": false,
        "exclude_unlimited": false,
        "exclude_upgradable": false,
        "exclude_non_upgradable": false,
        "exclude_upgraded": false,
        "exclude_without_colors": false,
        "exclude_hosted": false,
        "sort_by_price": false,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `toggleGiftIsSaved`: show (`true`) or hide a received gift on the
/// profile. Only valid for the user's own gifts.
pub fn toggle_gift_is_saved(extra: RequestId, received_gift_id: &str, is_saved: bool) -> String {
    json!({
        "@type": "toggleGiftIsSaved",
        "@extra": extra.as_extra(),
        "received_gift_id": received_gift_id,
        "is_saved": is_saved,
    })
    .to_string()
}

/// `sellGift`: convert a received gift to Stars (irreversible; the UI
/// confirms first). `business_connection_id` stays empty.
pub fn sell_gift(extra: RequestId, received_gift_id: &str) -> String {
    json!({
        "@type": "sellGift",
        "@extra": extra.as_extra(),
        "business_connection_id": "",
        "received_gift_id": received_gift_id,
    })
    .to_string()
}

/// `getPremiumFeatures` from the Settings entry (`premiumSourceSettings`).
pub fn get_premium_features(extra: RequestId) -> String {
    json!({
        "@type": "getPremiumFeatures",
        "@extra": extra.as_extra(),
        "source": {"@type": "premiumSourceSettings"},
    })
    .to_string()
}

/// `getPremiumState`.
pub fn get_premium_state(extra: RequestId) -> String {
    json!({"@type": "getPremiumState", "@extra": extra.as_extra()}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn transactions_request_shape() {
        let all = v(&get_star_transactions(
            RequestId(1),
            7,
            TxFilter::All,
            "",
            30,
        ));
        assert_eq!(all["@type"], "getStarTransactions");
        assert_eq!(all["owner_id"]["user_id"], 7);
        assert_eq!(all["limit"], 30);
        assert!(all.get("direction").is_none());
        let incoming = v(&get_star_transactions(
            RequestId(1),
            7,
            TxFilter::Incoming,
            "x",
            30,
        ));
        assert_eq!(
            incoming["direction"]["@type"],
            "transactionDirectionIncoming"
        );
        assert_eq!(incoming["offset"], "x");
        let outgoing = v(&get_star_transactions(
            RequestId(1),
            7,
            TxFilter::Outgoing,
            "",
            30,
        ));
        assert_eq!(
            outgoing["direction"]["@type"],
            "transactionDirectionOutgoing"
        );
    }

    #[test]
    fn received_gifts_request_shape() {
        let req = v(&get_received_gifts(
            RequestId(2),
            MessageSender::Chat { chat_id: -100 },
            "o",
            24,
        ));
        assert_eq!(req["@type"], "getReceivedGifts");
        assert_eq!(req["owner_id"]["@type"], "messageSenderChat");
        assert_eq!(req["owner_id"]["chat_id"], -100);
        assert_eq!(req["business_connection_id"], "");
        assert_eq!(req["collection_id"], 0);
        assert_eq!(req["offset"], "o");
        assert_eq!(req["exclude_unsaved"], false);
    }

    #[test]
    fn mutation_request_shapes() {
        let toggle = v(&toggle_gift_is_saved(RequestId(3), "g", false));
        assert_eq!(toggle["@type"], "toggleGiftIsSaved");
        assert_eq!(toggle["received_gift_id"], "g");
        assert_eq!(toggle["is_saved"], false);
        let sell = v(&sell_gift(RequestId(4), "g"));
        assert_eq!(sell["@type"], "sellGift");
        assert_eq!(sell["business_connection_id"], "");
    }

    #[test]
    fn premium_request_shapes() {
        let f = v(&get_premium_features(RequestId(5)));
        assert_eq!(f["source"]["@type"], "premiumSourceSettings");
        assert_eq!(
            v(&get_premium_state(RequestId(6)))["@type"],
            "getPremiumState"
        );
    }
}
