//! Paid messages (tdesktop `boxes/edit_privacy_box.cpp`,
//! `history/history_item_helpers.cpp`, `history/view/history_view_about_view.cpp`).
//!
//! Three sides of one feature:
//!
//! - What you charge: the Messages privacy box offers Everybody, Contacts
//!   and Premium, or "Charge for messages" with a price slider
//!   ([`MessagesPolicy`], [`price_steps`]). Users who may write for free
//!   are the exceptions list.
//! - What you pay: before writing to a user who restricts new chats or
//!   charges, the composer is replaced by a Premium prompt or shows the
//!   price, and the first send asks for a confirmation ([`send_gate`],
//!   [`PaidTrust`], [`PaidConfirm`]).
//! - Revenue: the Remove Fee flow reads the Stars a user already paid and
//!   can refund them ([`refund_label`]).
//!
//! The state sits in [`PaidMessages`] (`Session::payments.paid_messages`).
#![deny(clippy::indexing_slicing)]

use crate::premium_hub::group_digits;
use crate::privacy::NewChatPrivacy;
use crate::telegram::envelope::OptionValue;
use serde_json::Value;
use std::collections::HashMap;

/// tdesktop `kDefaultPrivateMessagesPrice`: where the slider starts when
/// the account does not charge yet.
pub const DEFAULT_PRIVATE_PRICE: i64 = 10;
/// tdesktop `paidMessageChannelStarsDefault`.
pub const DEFAULT_CHANNEL_PRICE: i64 = 10;
/// tdesktop `paidMessageStarsMax` default, used until the
/// `paid_message_star_count_max` option arrives.
pub const FALLBACK_MAX_STARS: i64 = 10_000;
/// tdesktop `paidMessageCommission` default (permille), used until the
/// `paid_message_earnings_per_mille` option arrives.
pub const FALLBACK_PER_MILLE: i64 = 850;
/// How long an answer of `canSendMessageToUser` is trusted before the
/// chat asks again (milliseconds).
pub const GATE_REFRESH_MS: u64 = 5 * 60 * 1000;
/// How long an unanswered `canSendMessageToUser` blocks a new one.
pub const ASK_TIMEOUT_MS: u64 = 30 * 1000;

/// The three choices of the Messages privacy box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagesPolicy {
    /// Anybody can write.
    Everybody,
    /// Non-contacts need Premium (`allow_new_chats_from_unknown_users`
    /// off).
    ContactsAndPremium,
    /// Non-contacts pay this many Stars per message.
    Charge(i64),
}

impl MessagesPolicy {
    /// The policy a `newChatPrivacySettings` answer stands for. A positive
    /// price wins: TDLib requires unknown users to be allowed then.
    pub fn from_settings(settings: &NewChatPrivacy) -> Self {
        if settings.incoming_paid_message_star_count > 0 {
            Self::Charge(settings.incoming_paid_message_star_count)
        } else if settings.allow_from_unknown {
            Self::Everybody
        } else {
            Self::ContactsAndPremium
        }
    }

    /// The `newChatPrivacySettings` to send for this policy.
    pub fn to_settings(self) -> NewChatPrivacy {
        match self {
            Self::Everybody => NewChatPrivacy {
                allow_from_unknown: true,
                incoming_paid_message_star_count: 0,
            },
            Self::ContactsAndPremium => NewChatPrivacy {
                allow_from_unknown: false,
                incoming_paid_message_star_count: 0,
            },
            Self::Charge(stars) => NewChatPrivacy {
                allow_from_unknown: true,
                incoming_paid_message_star_count: stars.max(1),
            },
        }
    }

    /// Whether this is the same radio choice (the price may differ).
    pub fn same_choice(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
}

/// The slider values of `SetupChargeSlider`: every Star below 100, then
/// steps of 10 up to 1000, then steps of 100 up to `max`. A saved price
/// that falls between two steps is inserted so the slider can show it.
pub fn price_steps(current: i64, min: i64, max: i64) -> Vec<i64> {
    let max = max.max(min);
    let mut values = Vec::new();
    if current < min {
        values.push(current);
    }
    for i in min..max.min(100) {
        values.push(i);
    }
    let mut i = 100;
    while i < max.min(1000) {
        if i < current + 10 && current < i {
            values.push(current);
        }
        values.push(i);
        i += 10;
    }
    let mut i = 1000;
    while i <= max {
        if i < current + 100 && current < i {
            values.push(current);
        }
        values.push(i);
        i += 100;
    }
    values.dedup();
    values
}

/// The index of the step closest to `price`.
pub fn step_for_price(steps: &[i64], price: i64) -> usize {
    steps
        .iter()
        .enumerate()
        .min_by_key(|(_, step)| (**step - price).abs())
        .map_or(0, |(index, _)| index)
}

/// "85%" or "87.5%": the share of each fee the account receives
/// (tdesktop `QString::number(percent / 10.) + '%'`).
pub fn earnings_percent(per_mille: i64) -> String {
    let whole = per_mille / 10;
    match per_mille % 10 {
        0 => format!("{whole}%"),
        tenth => format!("{whole}.{tenth}%"),
    }
}

/// "1 Star" or "1,200 Stars".
pub fn stars_text(count: i64) -> String {
    if count == 1 {
        "1 Star".into()
    } else {
        format!("{} Stars", group_digits(count))
    }
}

/// The line under the price slider (`lng_messages_privacy_price_about`
/// without the dollar estimate: TDLib has no Stars rate option).
pub fn price_about(per_mille: i64) -> String {
    format!(
        "You will receive {} of the selected fee for each incoming message.",
        earnings_percent(per_mille)
    )
}

/// The same line for a channel's direct messages
/// (`lng_manage_monoforum_price_about`).
pub fn channel_price_about(per_mille: i64) -> String {
    format!(
        "Your channel will receive {} of the selected fee for each incoming message.",
        earnings_percent(per_mille)
    )
}

/// What the Messages row says about a price.
pub fn price_value_label(stars: i64) -> String {
    format!("{} per message", stars_text(stars))
}

/// `canSendMessageToUserResult*` (schema 1.8.68).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanSend {
    /// `canSendMessageToUserResultOk`.
    Ok,
    /// `canSendMessageToUserResultUserHasPaidMessages`.
    HasPaidMessages { stars: i64 },
    /// `canSendMessageToUserResultUserRestrictsNewChats`.
    RestrictsNewChats,
    /// `canSendMessageToUserResultUserIsDeleted`.
    UserIsDeleted,
}

impl CanSend {
    /// Reads one answer by its constructor; `None` for another type.
    pub fn parse(type_name: &str, value: &Value) -> Option<Self> {
        Some(match type_name {
            "canSendMessageToUserResultOk" => Self::Ok,
            "canSendMessageToUserResultUserHasPaidMessages" => Self::HasPaidMessages {
                stars: value
                    .get("outgoing_paid_message_star_count")
                    .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
                    .unwrap_or(0),
            },
            "canSendMessageToUserResultUserRestrictsNewChats" => Self::RestrictsNewChats,
            "canSendMessageToUserResultUserIsDeleted" => Self::UserIsDeleted,
            _ => return None,
        })
    }
}

/// What the composer shows for a private chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendGate {
    /// Nothing special.
    Free,
    /// The user accepts only contacts and Premium subscribers, and the
    /// viewer is neither.
    PremiumRequired,
    /// Every message costs Stars.
    Charges { stars: i64 },
}

/// What the gate decision reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct GateFacts {
    /// The chat is the viewer's own (Saved Messages).
    pub is_self: bool,
    pub is_bot: bool,
    /// The viewer has Telegram Premium.
    pub viewer_premium: bool,
    /// The latest `canSendMessageToUser` answer, if any.
    pub answer: Option<CanSend>,
}

/// tdesktop `requiresPremiumToWrite` and `starsPerMessageChecked`: bots,
/// Saved Messages and unknown answers never gate.
pub fn send_gate(facts: &GateFacts) -> SendGate {
    if facts.is_self || facts.is_bot {
        return SendGate::Free;
    }
    match facts.answer {
        Some(CanSend::RestrictsNewChats) if !facts.viewer_premium => SendGate::PremiumRequired,
        Some(CanSend::HasPaidMessages { stars }) if stars > 0 => SendGate::Charges { stars },
        _ => SendGate::Free,
    }
}

impl SendGate {
    /// The price of one message, 0 when free.
    pub fn stars(self) -> i64 {
        match self {
            Self::Charges { stars } => stars,
            _ => 0,
        }
    }

    /// The sentence that goes with the gate (`lng_send_non_premium_message_toast`,
    /// `lng_send_charges_stars_text`).
    pub fn sentence(self, name: &str) -> Option<String> {
        match self {
            Self::Free => None,
            Self::PremiumRequired => Some(format!(
                "{name} only accepts messages from contacts and Premium users."
            )),
            Self::Charges { stars } => Some(format!(
                "{name} charges {} for each message.",
                stars_text(stars)
            )),
        }
    }
}

/// Prices the user agreed to pay without being asked again, per chat
/// (tdesktop `markPeerTrustedPayForMessage`). A price up to the trusted one
/// goes out; a higher price asks again. Kept per account on disk.
#[derive(Debug, Clone, Default)]
pub struct PaidTrust {
    chats: HashMap<i64, i64>,
}

impl PaidTrust {
    pub fn trust(&mut self, chat_id: i64, stars: i64) {
        self.chats.insert(chat_id, stars);
    }

    /// tdesktop `isPeerTrustedPayForMessage`: free is always fine, a
    /// stored price covers prices up to it.
    pub fn is_trusted(&self, chat_id: i64, stars: i64) -> bool {
        stars <= 0 || self.chats.get(&chat_id).is_some_and(|t| *t >= stars)
    }

    /// tdesktop `clearPeerTrustedPayForMessage`: the chat is free again.
    /// True when an entry went away.
    pub fn forget(&mut self, chat_id: i64) -> bool {
        self.chats.remove(&chat_id).is_some()
    }

    pub fn entries(&self) -> HashMap<i64, i64> {
        self.chats.clone()
    }

    /// The file contents: `{"<chat id>": stars}`.
    pub fn to_json(&self) -> String {
        let map: std::collections::BTreeMap<String, i64> = self
            .chats
            .iter()
            .map(|(chat, stars)| (chat.to_string(), *stars))
            .collect();
        serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
    }

    /// Reads what [`PaidTrust::to_json`] wrote; anything unreadable is
    /// an empty list (the user is asked again).
    pub fn from_json(text: &str) -> Self {
        let chats = serde_json::from_str::<HashMap<String, i64>>(text)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(chat, stars)| Some((chat.parse().ok()?, stars)))
            .filter(|(_, stars): &(i64, i64)| *stars > 0)
            .collect();
        Self { chats }
    }
}

/// The confirmation before a paid send (`ShowSendPaidConfirm`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaidConfirm {
    pub chat_id: i64,
    pub name: String,
    pub per_message: i64,
    pub messages: i64,
    /// The text to send once confirmed (composer markup).
    pub text: String,
}

impl PaidConfirm {
    /// Stars the whole send costs.
    pub fn total(&self) -> i64 {
        self.per_message * self.messages
    }

    pub fn title(&self) -> &'static str {
        "Confirm payment"
    }

    pub fn body(&self) -> String {
        format!(
            "{} charges {} per message. Would you like to pay {} to send {}?",
            self.name,
            stars_text(self.per_message),
            stars_text(self.total()),
            if self.messages == 1 {
                "1 message".to_string()
            } else {
                format!("{} messages", self.messages)
            }
        )
    }

    pub fn button(&self) -> String {
        if self.messages == 1 {
            "Pay for 1 Message".into()
        } else {
            format!("Pay for {} Messages", self.messages)
        }
    }
}

/// Whether sending `messages` messages at `per_message` Stars needs the
/// confirmation box. Free sends and trusted prices go straight out.
pub fn needs_confirmation(trust: &PaidTrust, chat_id: i64, per_message: i64) -> bool {
    per_message > 0 && !trust.is_trusted(chat_id, per_message)
}

/// `lng_payment_refund_text`.
pub fn remove_fee_text(name: &str) -> String {
    format!("Are you sure you want to allow {name} to message you for free?")
}

/// `lng_payment_refund_also`: the refund checkbox, only offered when the
/// user paid something already.
pub fn refund_label(paid: i64) -> Option<String> {
    (paid > 0).then(|| format!("Refund already paid {}", stars_text(paid)))
}

/// `lng_payment_bar_text`: the bar over a chat whose other side pays.
pub fn pays_bar_text(name: &str, stars: i64) -> String {
    format!(
        "{name} must pay {} for each message to you.",
        stars_text(stars)
    )
}

/// The channel's direct messages choice, as `setChatDirectMessagesGroup`
/// takes it: a disabled group has no price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectMessagesChoice {
    pub enabled: bool,
    pub stars: i64,
}

impl DirectMessagesChoice {
    /// Clamps the price to what TDLib accepts (0 up to the maximum).
    pub fn clamped(self, max: i64) -> Self {
        Self {
            enabled: self.enabled,
            stars: if self.enabled {
                self.stars.clamp(0, max.max(0))
            } else {
                0
            },
        }
    }

    /// "Off", "Free" or "N Stars" (the channel edit row).
    pub fn summary(self) -> String {
        match (self.enabled, self.stars) {
            (false, _) => "Off".into(),
            (true, 0) => "Free".into(),
            (true, stars) => stars_text(stars),
        }
    }
}

/// Paid-message state of one account.
#[derive(Debug, Clone, Default)]
pub struct PaidMessages {
    /// `paid_message_star_count_max`.
    pub max_stars: Option<i64>,
    /// `paid_message_earnings_per_mille`.
    pub per_mille: Option<i64>,
    /// `can_enable_paid_messages`: the account may charge at all.
    pub can_enable: Option<bool>,
    /// `can_set_new_chat_privacy_settings`: the account may restrict new
    /// chats (Premium, or free by the server's rule).
    pub can_set_privacy: Option<bool>,
    /// Latest `canSendMessageToUser` answer per user, with the time it
    /// arrived.
    gates: HashMap<i64, (CanSend, u64)>,
    /// Users a `canSendMessageToUser` is in flight for.
    asking: HashMap<i64, u64>,
    pub trust: PaidTrust,
    /// Stars a user paid so far (`getPaidMessageRevenue`).
    pub revenue: HashMap<i64, i64>,
    /// Channel direct messages price per supergroup id.
    pub dm_price: HashMap<i64, i64>,
    /// The trusted prices were read from disk.
    pub trust_loaded: bool,
    /// The trusted prices changed and are not written yet.
    pub trust_dirty: bool,
    /// A paid-message request is in flight.
    pub saving: bool,
    /// The last paid-message request failed with this line.
    pub error: Option<String>,
}

impl PaidMessages {
    /// Folds one `updateOption`.
    pub fn apply_option(&mut self, name: &str, value: &OptionValue) {
        match (name, value) {
            ("paid_message_star_count_max", OptionValue::Integer(max)) if *max > 0 => {
                self.max_stars = Some(*max);
            }
            ("paid_message_earnings_per_mille", OptionValue::Integer(rate)) if *rate > 0 => {
                self.per_mille = Some(*rate);
            }
            ("can_enable_paid_messages", OptionValue::Boolean(on)) => {
                self.can_enable = Some(*on);
            }
            ("can_set_new_chat_privacy_settings", OptionValue::Boolean(on)) => {
                self.can_set_privacy = Some(*on);
            }
            _ => {}
        }
    }

    pub fn max(&self) -> i64 {
        self.max_stars.unwrap_or(FALLBACK_MAX_STARS)
    }

    pub fn per_mille(&self) -> i64 {
        self.per_mille.unwrap_or(FALLBACK_PER_MILLE)
    }

    /// Whether the "Charge for messages" choice is offered at all.
    pub fn charging_available(&self) -> bool {
        self.can_enable.unwrap_or(false)
    }

    pub fn gate_answer(&self, user_id: i64) -> Option<CanSend> {
        self.gates.get(&user_id).map(|(answer, _)| *answer)
    }

    /// Stores an answer and ends the wait for it. A user who no longer
    /// charges loses the trusted price (tdesktop `checkTrustedPayForMessage`;
    /// a private chat has the user's id).
    pub fn record_answer(&mut self, user_id: i64, answer: CanSend, now_ms: u64) {
        self.gates.insert(user_id, (answer, now_ms));
        self.asking.remove(&user_id);
        if !matches!(answer, CanSend::HasPaidMessages { stars } if stars > 0)
            && self.trust.forget(user_id)
        {
            self.trust_dirty = true;
        }
    }

    /// Users that charge, with the price.
    pub fn charging_users(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        self.gates
            .iter()
            .filter_map(|(user, (answer, _))| match answer {
                CanSend::HasPaidMessages { stars } if *stars > 0 => Some((*user, *stars)),
                _ => None,
            })
    }

    /// Whether the chat with `user_id` should ask `canSendMessageToUser`
    /// now: nothing known yet, or the answer is old, and no ask in flight.
    pub fn should_ask(&self, user_id: i64, now_ms: u64) -> bool {
        // An ask that never came back (an error clears nothing) is
        // retried after the wait below.
        if self
            .asking
            .get(&user_id)
            .is_some_and(|at| now_ms.saturating_sub(*at) < ASK_TIMEOUT_MS)
        {
            return false;
        }
        match self.gates.get(&user_id) {
            None => true,
            Some((_, at)) => now_ms.saturating_sub(*at) >= GATE_REFRESH_MS,
        }
    }

    pub fn mark_asking(&mut self, user_id: i64, now_ms: u64) {
        self.asking.insert(user_id, now_ms);
    }

    /// The ask failed: forget it so a later open tries again.
    pub fn ask_failed(&mut self, user_id: i64) {
        self.asking.remove(&user_id);
    }

    /// After the viewer's Premium changes, old answers are wrong.
    pub fn forget_answers(&mut self) {
        self.gates.clear();
    }
}

impl crate::state::Session {
    /// What the composer of `chat_id` shows: the Premium prompt, the price
    /// or nothing. Only private chats with another user gate.
    pub fn send_gate(&self, chat_id: crate::ids::ChatId) -> SendGate {
        let Some(user_id) = self.private_chat_user_id(chat_id) else {
            // A supergroup with paid messages, or a channel's direct
            // messages group, charges everyone but its administrators; the
            // full info already holds the viewer's own price.
            let stars = self.supergroup_send_price(chat_id);
            return if stars > 0 {
                SendGate::Charges { stars }
            } else {
                SendGate::Free
            };
        };
        send_gate(&GateFacts {
            is_self: Some(user_id) == self.my_user_id,
            is_bot: self.is_bot_user(user_id),
            viewer_premium: self.my_is_premium(),
            answer: self.payments.paid_messages.gate_answer(user_id),
        })
    }

    /// `supergroupFullInfo.outgoing_paid_message_star_count` of a chat.
    fn supergroup_send_price(&self, chat_id: crate::ids::ChatId) -> i64 {
        self.chat_supergroup(chat_id)
            .and_then(|id| self.supergroup_full_info(id))
            .map_or(0, |info| info.admin.outgoing_paid_message_star_count)
    }

    /// Stars one message to `chat_id` costs the account: a user who
    /// charges (never a bot or yourself) or a supergroup with a price;
    /// 0 when free or unknown (`PeerData::starsPerMessageChecked`).
    pub fn paid_send_price(&self, chat_id: crate::ids::ChatId) -> i64 {
        match self.private_chat_user_id(chat_id) {
            Some(user) if self.is_bot_user(user) || Some(user) == self.my_user_id => 0,
            Some(user) => self
                .payments
                .paid_messages
                .charging_users()
                .find_map(|(charging, stars)| (charging == user).then_some(stars))
                .unwrap_or(0),
            None => self.supergroup_send_price(chat_id),
        }
    }

    /// Every chat the account pays to write in, with the price per
    /// message: what the send guard screens against.
    pub fn paid_send_prices(&self) -> HashMap<i64, i64> {
        // A private chat has the id of its user.
        let mut prices: HashMap<i64, i64> = self
            .payments
            .paid_messages
            .charging_users()
            .filter(|(user, _)| !self.is_bot_user(*user) && Some(*user) != self.my_user_id)
            .collect();
        for chat in self.chats.values() {
            let stars = self.supergroup_send_price(chat.id);
            if stars > 0 {
                prices.insert(chat.id.0, stars);
            }
        }
        prices
    }

    /// Stars the user of a private chat pays per message to the account
    /// (`userFullInfo.incoming_paid_message_star_count`), 0 when none or
    /// unknown.
    pub fn user_pays_me(&self, chat_id: crate::ids::ChatId) -> i64 {
        self.private_chat_user_id(chat_id)
            .and_then(|user_id| self.user_full_info(user_id))
            .map_or(0, |info| info.extras.incoming_paid_message_star_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn facts(answer: Option<CanSend>) -> GateFacts {
        GateFacts {
            answer,
            ..GateFacts::default()
        }
    }

    #[test]
    fn restricted_users_ask_for_premium() {
        assert_eq!(
            send_gate(&facts(Some(CanSend::RestrictsNewChats))),
            SendGate::PremiumRequired
        );
        let mut premium = facts(Some(CanSend::RestrictsNewChats));
        premium.viewer_premium = true;
        assert_eq!(send_gate(&premium), SendGate::Free);
    }

    #[test]
    fn paid_users_charge_and_zero_is_free() {
        assert_eq!(
            send_gate(&facts(Some(CanSend::HasPaidMessages { stars: 25 }))),
            SendGate::Charges { stars: 25 }
        );
        assert_eq!(
            send_gate(&facts(Some(CanSend::HasPaidMessages { stars: 0 }))),
            SendGate::Free
        );
    }

    #[test]
    fn bots_self_and_unknown_never_gate() {
        let answer = Some(CanSend::HasPaidMessages { stars: 5 });
        for mut f in [facts(answer), facts(answer)] {
            f.is_bot = true;
            assert_eq!(send_gate(&f), SendGate::Free);
        }
        let mut own = facts(Some(CanSend::RestrictsNewChats));
        own.is_self = true;
        assert_eq!(send_gate(&own), SendGate::Free);
        assert_eq!(send_gate(&facts(None)), SendGate::Free);
        assert_eq!(send_gate(&facts(Some(CanSend::Ok))), SendGate::Free);
        assert_eq!(
            send_gate(&facts(Some(CanSend::UserIsDeleted))),
            SendGate::Free
        );
    }

    #[test]
    fn gate_sentences_name_the_user() {
        assert_eq!(SendGate::Free.sentence("Ann"), None);
        assert_eq!(
            SendGate::PremiumRequired.sentence("Ann").as_deref(),
            Some("Ann only accepts messages from contacts and Premium users.")
        );
        assert_eq!(
            SendGate::Charges { stars: 1 }.sentence("Ann").as_deref(),
            Some("Ann charges 1 Star for each message.")
        );
        assert_eq!(
            SendGate::Charges { stars: 1500 }.sentence("Ann").as_deref(),
            Some("Ann charges 1,500 Stars for each message.")
        );
    }

    #[test]
    fn answers_parse_by_constructor() {
        let paid = json!({"outgoing_paid_message_star_count": "40"});
        assert_eq!(
            CanSend::parse("canSendMessageToUserResultUserHasPaidMessages", &paid),
            Some(CanSend::HasPaidMessages { stars: 40 })
        );
        assert_eq!(
            CanSend::parse(
                "canSendMessageToUserResultUserRestrictsNewChats",
                &json!({})
            ),
            Some(CanSend::RestrictsNewChats)
        );
        assert_eq!(
            CanSend::parse("canSendMessageToUserResultOk", &json!({})),
            Some(CanSend::Ok)
        );
        assert_eq!(CanSend::parse("ok", &json!({})), None);
    }

    #[test]
    fn trust_is_per_chat_and_per_price() {
        let mut trust = PaidTrust::default();
        assert!(needs_confirmation(&trust, 7, 10));
        assert!(!needs_confirmation(&trust, 7, 0));
        trust.trust(7, 10);
        assert!(!needs_confirmation(&trust, 7, 10));
        assert!(
            !needs_confirmation(&trust, 7, 4),
            "a lower price is covered"
        );
        assert!(
            needs_confirmation(&trust, 7, 11),
            "a higher price asks again"
        );
        assert!(needs_confirmation(&trust, 8, 10), "another chat asks");
        assert!(trust.forget(7));
        assert!(!trust.forget(7));
        assert!(needs_confirmation(&trust, 7, 10));
    }

    #[test]
    fn trust_survives_a_round_trip_through_the_file() {
        let mut trust = PaidTrust::default();
        trust.trust(7, 10);
        trust.trust(-1001, 250);
        let back = PaidTrust::from_json(&trust.to_json());
        assert_eq!(back.entries(), trust.entries());
        assert!(PaidTrust::from_json("not json").entries().is_empty());
        assert_eq!(
            PaidTrust::from_json(r#"{"x":5,"3":0,"4":9}"#).entries(),
            HashMap::from([(4, 9)]),
            "bad ids and non-positive prices are dropped"
        );
    }

    #[test]
    fn a_user_who_stops_charging_loses_the_trusted_price() {
        let mut state = PaidMessages::default();
        state.trust.trust(5, 10);
        state.record_answer(5, CanSend::HasPaidMessages { stars: 10 }, 1);
        assert!(state.trust.is_trusted(5, 10));
        assert!(!state.trust_dirty);
        state.record_answer(5, CanSend::Ok, 2);
        assert!(!state.trust.is_trusted(5, 10));
        assert!(state.trust_dirty);
    }

    #[test]
    fn confirmation_texts_count_messages() {
        let one = PaidConfirm {
            chat_id: 1,
            name: "Ann".into(),
            per_message: 10,
            messages: 1,
            text: "hi".into(),
        };
        assert_eq!(one.total(), 10);
        assert_eq!(
            one.body(),
            "Ann charges 10 Stars per message. Would you like to pay 10 Stars to send 1 message?"
        );
        assert_eq!(one.button(), "Pay for 1 Message");
        let three = PaidConfirm { messages: 3, ..one };
        assert_eq!(three.total(), 30);
        assert!(three.body().ends_with("pay 30 Stars to send 3 messages?"));
        assert_eq!(three.button(), "Pay for 3 Messages");
    }

    #[test]
    fn slider_steps_follow_tdesktop() {
        let steps = price_steps(10, 1, 10_000);
        assert_eq!(steps[0], 1);
        assert_eq!(steps[98], 99);
        assert_eq!(steps[99], 100);
        assert_eq!(steps[100], 110);
        assert!(steps.contains(&1000) && steps.contains(&1100));
        assert_eq!(*steps.last().unwrap(), 10_000);
        assert!(steps.windows(2).all(|w| w[0] < w[1]));
        // A saved price between two steps stays selectable.
        let odd = price_steps(105, 1, 10_000);
        assert!(odd.contains(&105));
        assert!(odd.windows(2).all(|w| w[0] < w[1]));
        let low = price_steps(0, 1, 50);
        assert_eq!(low[0], 0, "a saved zero stays on the slider");
        assert_eq!(*low.last().unwrap(), 49);
    }

    #[test]
    fn zero_allowed_for_channels() {
        let steps = price_steps(0, 0, 200);
        assert_eq!(steps[0], 0);
        assert_eq!(step_for_price(&steps, 0), 0);
        assert_eq!(step_for_price(&steps, 14), 14);
        assert_eq!(step_for_price(&steps, 104), step_for_price(&steps, 100));
    }

    #[test]
    fn earnings_read_as_percent() {
        assert_eq!(earnings_percent(850), "85%");
        assert_eq!(earnings_percent(875), "87.5%");
        assert_eq!(
            price_about(850),
            "You will receive 85% of the selected fee for each incoming message."
        );
        assert!(channel_price_about(900).contains("90%"));
    }

    #[test]
    fn policy_round_trips_through_settings() {
        for policy in [
            MessagesPolicy::Everybody,
            MessagesPolicy::ContactsAndPremium,
            MessagesPolicy::Charge(15),
        ] {
            assert_eq!(MessagesPolicy::from_settings(&policy.to_settings()), policy);
        }
        // A price wins over the unknown-users flag, like TDLib requires.
        let odd = NewChatPrivacy {
            allow_from_unknown: false,
            incoming_paid_message_star_count: 9,
        };
        assert_eq!(
            MessagesPolicy::from_settings(&odd),
            MessagesPolicy::Charge(9)
        );
        assert!(MessagesPolicy::Charge(3).to_settings().allow_from_unknown);
        assert_eq!(
            MessagesPolicy::Charge(0)
                .to_settings()
                .incoming_paid_message_star_count,
            1
        );
        assert!(MessagesPolicy::Charge(3).same_choice(MessagesPolicy::Charge(8)));
        assert!(!MessagesPolicy::Charge(3).same_choice(MessagesPolicy::Everybody));
    }

    #[test]
    fn options_fill_the_limits() {
        let mut state = PaidMessages::default();
        assert_eq!(state.max(), FALLBACK_MAX_STARS);
        assert_eq!(state.per_mille(), FALLBACK_PER_MILLE);
        assert!(!state.charging_available());
        state.apply_option("paid_message_star_count_max", &OptionValue::Integer(2500));
        state.apply_option(
            "paid_message_earnings_per_mille",
            &OptionValue::Integer(900),
        );
        state.apply_option("can_enable_paid_messages", &OptionValue::Boolean(true));
        state.apply_option("paid_message_star_count_max", &OptionValue::Integer(0));
        assert_eq!(state.max(), 2500);
        assert_eq!(state.per_mille(), 900);
        assert!(state.charging_available());
    }

    #[test]
    fn answers_refresh_after_five_minutes() {
        let mut state = PaidMessages::default();
        assert!(state.should_ask(5, 0));
        state.mark_asking(5, 0);
        assert!(!state.should_ask(5, 1), "one ask at a time");
        state.record_answer(5, CanSend::RestrictsNewChats, 1_000);
        assert_eq!(state.gate_answer(5), Some(CanSend::RestrictsNewChats));
        assert!(!state.should_ask(5, 2_000));
        assert!(state.should_ask(5, 1_000 + GATE_REFRESH_MS));
        state.mark_asking(6, 0);
        state.ask_failed(6);
        assert!(state.should_ask(6, 1));
        state.forget_answers();
        assert_eq!(state.gate_answer(5), None);
    }

    #[test]
    fn remove_fee_copy() {
        assert_eq!(
            remove_fee_text("Ann"),
            "Are you sure you want to allow Ann to message you for free?"
        );
        assert_eq!(refund_label(0), None);
        assert_eq!(
            refund_label(1).as_deref(),
            Some("Refund already paid 1 Star")
        );
        assert_eq!(
            refund_label(120).as_deref(),
            Some("Refund already paid 120 Stars")
        );
        assert_eq!(
            pays_bar_text("Ann", 5),
            "Ann must pay 5 Stars for each message to you."
        );
    }

    #[test]
    fn direct_messages_choice_clamps_and_summarises() {
        let off = DirectMessagesChoice {
            enabled: false,
            stars: 40,
        }
        .clamped(100);
        assert_eq!(off.stars, 0);
        assert_eq!(off.summary(), "Off");
        let capped = DirectMessagesChoice {
            enabled: true,
            stars: 400,
        }
        .clamped(100);
        assert_eq!(capped.stars, 100);
        assert_eq!(capped.summary(), "100 Stars");
        assert_eq!(
            DirectMessagesChoice {
                enabled: true,
                stars: 0
            }
            .summary(),
            "Free"
        );
        assert_eq!(
            DirectMessagesChoice {
                enabled: true,
                stars: -4
            }
            .clamped(100)
            .stars,
            0
        );
    }
}
