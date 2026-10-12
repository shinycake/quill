//! The last stop before TDLib for anything that sends a message into a chat
//! that charges Stars (tdesktop confirms every paid send in
//! `SendPaymentHelper::check`).
//!
//! Text sends, stickers, GIFs, voice notes, media, polls, forwards and bot
//! results all end in one of four requests. The sender calls
//! [`PaidGuard::screen`] on each. A request to a chat with a price either
//! carries what the user agreed to pay, or is held until the user confirms
//! (or dropped when they cancel). Nothing is ever sent with a price the user
//! has not agreed to.
//!
//! `paid_message_star_count` is the TOTAL for the request: the schema says
//! "the number of Telegram Stars the user agreed to pay to send the
//! messages", and `sendMessageAlbum` and `forwardMessages` send several
//! messages with one set of options. So the guard writes price x messages.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Requests that create messages, and where their messages are counted.
const SENDING: [&str; 4] = [
    "sendMessage",
    "sendMessageAlbum",
    "forwardMessages",
    "sendInlineQueryResultMessage",
];

/// A request waiting for the user to confirm paying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldSend {
    pub chat_id: i64,
    pub per_message: i64,
    pub messages: i64,
    /// The request exactly as the driver built it.
    pub json: String,
}

impl HeldSend {
    pub fn total(&self) -> i64 {
        self.per_message * self.messages
    }

    /// The request that goes out after the user paid: the total in
    /// `options.paid_message_star_count`.
    pub fn with_price(&self) -> String {
        match serde_json::from_str::<Value>(&self.json) {
            Ok(mut value) => {
                set_paid(&mut value, self.total());
                value.to_string()
            }
            Err(_) => self.json.clone(),
        }
    }

    /// The `@extra` of the held request, to forget its pending entry.
    pub fn extra(&self) -> Option<String> {
        serde_json::from_str::<Value>(&self.json)
            .ok()
            .and_then(|value| value.get("@extra")?.as_str().map(str::to_string))
    }
}

#[derive(Debug, Default)]
struct State {
    /// Stars per message by chat id.
    prices: HashMap<i64, i64>,
    /// Prices the user agreed to for good, by chat id (`>=` price passes).
    trusted: HashMap<i64, i64>,
    held: Vec<HeldSend>,
}

/// Shared between the driver (which feeds prices and trust) and the sender
/// (which screens requests).
#[derive(Debug, Default)]
pub struct PaidGuard {
    state: Mutex<State>,
}

fn set_paid(request: &mut Value, total: i64) {
    if let Some(options) = request.get_mut("options").filter(|o| o.is_object()) {
        options["paid_message_star_count"] = total.into();
    }
}

/// The chat and the number of messages a sending request creates; `None`
/// for any other request.
fn sending(request: &Value) -> Option<(i64, i64)> {
    let kind = request.get("@type")?.as_str()?;
    if !SENDING.contains(&kind) {
        return None;
    }
    let chat_id = request.get("chat_id")?.as_i64()?;
    let count = match kind {
        "sendMessageAlbum" => request.get("input_message_contents")?.as_array()?.len(),
        "forwardMessages" => request.get("message_ids")?.as_array()?.len(),
        _ => 1,
    };
    Some((chat_id, count.max(1) as i64))
}

impl PaidGuard {
    /// Replaces the known prices and trusted prices.
    pub fn set_state(&self, prices: HashMap<i64, i64>, trusted: HashMap<i64, i64>) {
        if let Ok(mut state) = self.state.lock() {
            state.prices = prices;
            state.trusted = trusted;
        }
    }

    /// What goes to TDLib for `request`: the request itself (maybe with the
    /// agreed total filled in), or `None` when it is held for a
    /// confirmation.
    pub fn screen(&self, request: &str) -> Option<String> {
        // Cheap test first: most requests are not sends.
        if !request.contains("\"@type\":\"send") && !request.contains("\"@type\":\"forwardM") {
            return Some(request.to_string());
        }
        let Ok(mut value) = serde_json::from_str::<Value>(request) else {
            return Some(request.to_string());
        };
        let Some((chat_id, messages)) = sending(&value) else {
            return Some(request.to_string());
        };
        let Ok(mut state) = self.state.lock() else {
            return Some(request.to_string());
        };
        let price = state.prices.get(&chat_id).copied().unwrap_or(0);
        if price <= 0 {
            return Some(request.to_string());
        }
        let total = price * messages;
        let carried = value
            .get("options")
            .and_then(|o| o.get("paid_message_star_count"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if carried == total || state.trusted.get(&chat_id).is_some_and(|t| *t >= price) {
            set_paid(&mut value, total);
            return Some(value.to_string());
        }
        state.held.push(HeldSend {
            chat_id,
            per_message: price,
            messages,
            json: request.to_string(),
        });
        None
    }

    /// The oldest held request.
    pub fn held(&self) -> Option<HeldSend> {
        self.state.lock().ok()?.held.first().cloned()
    }

    /// Takes the oldest held request out.
    pub fn take_held(&self) -> Option<HeldSend> {
        let mut state = self.state.lock().ok()?;
        (!state.held.is_empty()).then(|| state.held.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn guard(price: i64, trusted: Option<i64>) -> PaidGuard {
        let guard = PaidGuard::default();
        guard.set_state(
            HashMap::from([(7, price)]),
            trusted.map(|t| (7, t)).into_iter().collect(),
        );
        guard
    }

    fn text(chat: i64, paid: i64) -> String {
        json!({"@type":"sendMessage","@extra":"3","chat_id":chat,
            "options":{"@type":"messageSendOptions","paid_message_star_count":paid},
            "input_message_content":{"@type":"inputMessageText"}})
        .to_string()
    }

    fn album(chat: i64, n: usize) -> String {
        json!({"@type":"sendMessageAlbum","@extra":"4","chat_id":chat,
            "options":{"@type":"messageSendOptions","paid_message_star_count":0},
            "input_message_contents": vec![json!({}); n]})
        .to_string()
    }

    #[test]
    fn free_chats_and_other_requests_pass_untouched() {
        let g = guard(25, None);
        let free = text(8, 0);
        assert_eq!(g.screen(&free), Some(free));
        let other = r#"{"@type":"getChat","chat_id":7}"#;
        assert_eq!(g.screen(other).as_deref(), Some(other));
        assert!(g.held().is_none());
    }

    #[test]
    fn an_unconfirmed_paid_send_is_held_not_sent() {
        let g = guard(25, None);
        assert_eq!(g.screen(&text(7, 0)), None);
        let held = g.held().expect("held");
        assert_eq!((held.chat_id, held.per_message, held.messages), (7, 25, 1));
        assert_eq!(held.total(), 25);
        assert_eq!(held.extra().as_deref(), Some("3"));
        let sent: Value = serde_json::from_str(&held.with_price()).unwrap();
        assert_eq!(sent["options"]["paid_message_star_count"], 25);
        assert!(g.take_held().is_some());
        assert!(g.held().is_none());
    }

    #[test]
    fn albums_and_forwards_are_priced_by_their_message_count() {
        let g = guard(10, None);
        assert_eq!(g.screen(&album(7, 3)), None);
        assert_eq!(g.held().unwrap().total(), 30);
        let forward = json!({"@type":"forwardMessages","@extra":"5","chat_id":7,
            "message_ids":[1,2],"options":{"paid_message_star_count":0}})
        .to_string();
        assert_eq!(g.screen(&forward), None);
        let held = g.take_held().unwrap();
        let _ = g.take_held().map(|h| assert_eq!(h.messages, 2));
        assert_eq!(held.messages, 3);
    }

    #[test]
    fn an_agreed_total_passes_and_a_wrong_one_is_held() {
        let g = guard(10, None);
        let sent: Value = serde_json::from_str(&g.screen(&album(7, 1)).unwrap_or_default())
            .unwrap_or(Value::Null);
        assert!(sent.is_null(), "an album of one is held at 10");
        let ok = text(7, 10);
        assert!(g.screen(&ok).is_some());
        assert!(
            g.screen(&text(7, 5)).is_none(),
            "an old price is not agreement"
        );
    }

    #[test]
    fn a_trusted_price_fills_in_the_total() {
        let g = guard(10, Some(10));
        let sent: Value = serde_json::from_str(&g.screen(&album(7, 4)).unwrap()).unwrap();
        assert_eq!(sent["options"]["paid_message_star_count"], 40);
        // A higher price than the trusted one asks again.
        let g = guard(11, Some(10));
        assert!(g.screen(&text(7, 0)).is_none());
        // A lower one passes.
        let g = guard(5, Some(10));
        assert!(g.screen(&text(7, 0)).is_some());
    }
}
