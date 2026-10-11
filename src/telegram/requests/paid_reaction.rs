//! Paid (Star) reaction requests (TDLib 1.8.68, `schema/td_api.tl`).
use crate::ids::{ChatId, MessageId, RequestId};
use crate::paid_reaction::PaidSender;
use serde_json::{Value, json};

/// `addPendingPaidMessageReaction chat_id message_id star_count type`:
/// adds Stars to the pending total of a message. `sender` is `None` when
/// the user did not choose a type (a tap on the chip).
pub fn add_pending_paid_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    star_count: i64,
    sender: Option<PaidSender>,
) -> String {
    json!({
        "@type": "addPendingPaidMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "star_count": star_count,
        "type": sender.map_or(Value::Null, PaidSender::to_tdlib_json),
    })
    .to_string()
}

/// `commitPendingPaidMessageReactions chat_id message_id`: spends the
/// pending Stars.
pub fn commit_pending_paid_message_reactions(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
) -> String {
    json!({
        "@type": "commitPendingPaidMessageReactions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `removePendingPaidMessageReactions chat_id message_id`: the Undo.
pub fn remove_pending_paid_message_reactions(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
) -> String {
    json!({
        "@type": "removePendingPaidMessageReactions",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `getChatAvailablePaidMessageReactionSenders chat_id`: the channels the
/// account may send Stars as.
pub fn get_chat_available_paid_message_reaction_senders(
    extra: RequestId,
    chat_id: ChatId,
) -> String {
    json!({
        "@type": "getChatAvailablePaidMessageReactionSenders",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `getStarPaymentOptions`: the Star packs for sale.
pub fn get_star_payment_options(extra: RequestId) -> String {
    json!({"@type": "getStarPaymentOptions", "@extra": extra.as_extra()}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn add_pending_carries_type_or_null() {
        let tap = parse(add_pending_paid_message_reaction(
            RequestId(7),
            ChatId(-100),
            MessageId(55),
            1,
            None,
        ));
        assert_eq!(tap["@type"], "addPendingPaidMessageReaction");
        assert_eq!(tap["star_count"], 1);
        assert_eq!(tap["chat_id"], -100);
        assert!(tap["type"].is_null());
        let box_send = parse(add_pending_paid_message_reaction(
            RequestId(8),
            ChatId(-100),
            MessageId(55),
            50,
            Some(PaidSender::Anonymous),
        ));
        assert_eq!(box_send["type"]["@type"], "paidReactionTypeAnonymous");
        let chat = parse(add_pending_paid_message_reaction(
            RequestId(9),
            ChatId(-100),
            MessageId(55),
            2,
            Some(PaidSender::Chat(-9)),
        ));
        assert_eq!(chat["type"]["chat_id"], -9);
    }

    #[test]
    fn commit_remove_and_options_requests() {
        let commit = parse(commit_pending_paid_message_reactions(
            RequestId(1),
            ChatId(5),
            MessageId(6),
        ));
        assert_eq!(commit["@type"], "commitPendingPaidMessageReactions");
        assert_eq!(commit["message_id"], 6);
        let undo = parse(remove_pending_paid_message_reactions(
            RequestId(2),
            ChatId(5),
            MessageId(6),
        ));
        assert_eq!(undo["@type"], "removePendingPaidMessageReactions");
        assert_eq!(
            parse(get_star_payment_options(RequestId(4)))["@type"],
            "getStarPaymentOptions"
        );
    }
}
