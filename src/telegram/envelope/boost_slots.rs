//! `chatBoostSlots` (schema/td_api.tl:7303): the viewer's boost slots,
//! as `getAvailableChatBoostSlots` and `boostChat` answer them.
use super::*;
use serde_json::Value;

/// One `chatBoostSlot` (schema/td_api.tl:7300).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedBoostSlot {
    pub slot_id: i32,
    /// The chat the slot boosts now; 0 when it boosts none.
    pub chat_id: i64,
    pub start_date: i32,
    pub expiration_date: i32,
    /// The slot can move to another chat after this time.
    pub cooldown_until_date: i32,
}

pub(crate) fn parse_boost_slots(value: &Value) -> Vec<ParsedBoostSlot> {
    let num = |slot: &Value, name: &str| int53(slot.get(name)).unwrap_or(0);
    value
        .get("slots")
        .and_then(Value::as_array)
        .map(|slots| {
            slots
                .iter()
                .filter_map(|slot| {
                    Some(ParsedBoostSlot {
                        slot_id: int53(slot.get("slot_id")).ok()?.sat_i32(),
                        chat_id: num(slot, "currently_boosted_chat_id"),
                        start_date: num(slot, "start_date").sat_i32(),
                        expiration_date: num(slot, "expiration_date").sat_i32(),
                        cooldown_until_date: num(slot, "cooldown_until_date").sat_i32(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{ParsedBoostSlot, Value, parse_boost_slots};

    #[test]
    fn parses_every_slot_field() {
        let v: Value = serde_json::from_str(
            r#"{"@type":"chatBoostSlots","slots":[
              {"slot_id":1,"currently_boosted_chat_id":0,"start_date":0,"expiration_date":0,"cooldown_until_date":0},
              {"slot_id":2,"currently_boosted_chat_id":-100123,"start_date":10,"expiration_date":99,"cooldown_until_date":50},
              {"currently_boosted_chat_id":5}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_boost_slots(&v),
            vec![
                ParsedBoostSlot {
                    slot_id: 1,
                    chat_id: 0,
                    start_date: 0,
                    expiration_date: 0,
                    cooldown_until_date: 0
                },
                ParsedBoostSlot {
                    slot_id: 2,
                    chat_id: -100123,
                    start_date: 10,
                    expiration_date: 99,
                    cooldown_until_date: 50
                },
            ]
        );
    }
}
