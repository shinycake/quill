//! Sending gifts: the gifts `getAvailableGifts` lists, the Premium gift
//! options and the rules for when a gift can be sent (tdesktop
//! `boxes/star_gift_box.cpp`, `GiftBox` / `SendGiftBox`).

use super::{flag, int, text};
use crate::premium_hub::group_digits;
use crate::telegram::envelope::{MessageSender, ParsedFile, StickerContent, format_payment_price};
use serde_json::Value;

/// One `availableGift` (a gift that can be sent, or that is sold out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableGift {
    pub id: i64,
    pub sticker: Option<StickerContent>,
    pub files: Vec<ParsedFile>,
    pub star_count: i64,
    /// Extra Stars to let the receiver upgrade the gift to a collectible.
    pub upgrade_star_count: i64,
    pub is_premium: bool,
    /// Sold by auction (`gift.auction_info`).
    pub auction: Option<crate::gift_market::GiftAuctionInfo>,
    /// `availableGift.title`: the collection name ("Plush Pepe"), which a
    /// bare `gift` does not carry.
    pub title: String,
    /// `(remaining, total)` when the gift has a global supply.
    pub supply: Option<(i32, i32)>,
    /// How many more this account may buy, when limited per user.
    pub user_remaining: Option<i32>,
    pub next_send_date: i32,
    pub resale_count: i32,
    pub min_resale_star_count: i64,
}

/// Why a gift cannot be sent right now (or that it can).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendGate {
    Ready,
    SoldOut,
    /// Needs Telegram Premium.
    PremiumOnly,
    /// Sold by auction: the tile opens the auction box instead.
    Auction,
    /// Not before this Unix time.
    Later(i32),
    /// This account bought its share.
    UserLimit,
    /// Limited gifts cannot go to channels.
    NotForChannels,
}

impl AvailableGift {
    pub fn is_auction(&self) -> bool {
        self.auction.is_some()
    }

    pub fn is_limited(&self) -> bool {
        self.supply.is_some()
    }

    pub fn sold_out(&self) -> bool {
        self.supply
            .is_some_and(|(left, total)| total > 0 && left <= 0)
    }

    /// Whether the gift can be sent to `recipient` now. `premium` is whether
    /// this account has Telegram Premium.
    pub fn gate(&self, recipient: &GiftRecipient, premium: bool, now: i64) -> SendGate {
        // tdesktop opens the auction box for every auction tile, sold out
        // or not (`star_gift_box.cpp`, `auction` before `soldOut`).
        if self.auction.is_some() {
            SendGate::Auction
        } else if self.sold_out() {
            SendGate::SoldOut
        } else if self.is_premium && !premium {
            SendGate::PremiumOnly
        } else if self.user_remaining.is_some_and(|n| n <= 0) {
            SendGate::UserLimit
        } else if self.next_send_date > 0 && i64::from(self.next_send_date) > now {
            SendGate::Later(self.next_send_date)
        } else if recipient.is_channel && self.is_limited() {
            SendGate::NotForChannels
        } else {
            SendGate::Ready
        }
    }

    /// Stars the send costs, with the optional prepaid upgrade.
    pub fn total_cost(&self, pay_for_upgrade: bool) -> i64 {
        self.star_count
            + if pay_for_upgrade {
                self.upgrade_star_count
            } else {
                0
            }
    }

    /// "1,000 left of 5,000" for a limited gift.
    pub fn supply_label(&self) -> Option<String> {
        self.supply.map(|(left, total)| {
            if left <= 0 {
                "Sold out".to_string()
            } else {
                format!(
                    "{} of {} left",
                    group_digits(i64::from(left)),
                    group_digits(i64::from(total))
                )
            }
        })
    }
}

impl SendGate {
    /// Short reason shown on a gift tile and in the send box.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::SoldOut => Some("Sold out"),
            Self::PremiumOnly => Some("Needs Telegram Premium"),
            Self::Auction => None,
            Self::Later(_) => Some("Not available yet"),
            Self::UserLimit => Some("You bought all you can"),
            Self::NotForChannels => Some("Not for channels"),
        }
    }
}

pub fn parse_available_gift(value: &Value) -> Option<AvailableGift> {
    let gift = value.get("gift")?;
    let id = int(gift.get("id"));
    if id == 0 {
        return None;
    }
    let (sticker, files) = crate::premium_hub::parse_gift_sticker(gift.get("sticker"));
    let limits = |key: &str| {
        gift.get(key).filter(|v| !v.is_null()).map(|l| {
            (
                int(l.get("remaining_count")) as i32,
                int(l.get("total_count")) as i32,
            )
        })
    };
    Some(AvailableGift {
        id,
        sticker,
        files,
        star_count: int(gift.get("star_count")),
        upgrade_star_count: int(gift.get("upgrade_star_count")),
        is_premium: flag(gift, "is_premium"),
        auction: crate::gift_market::GiftAuctionInfo::parse(gift.get("auction_info")),
        title: text(value, "title"),
        supply: limits("overall_limits"),
        user_remaining: limits("user_limits").map(|(left, _)| left),
        next_send_date: int(gift.get("next_send_date")) as i32,
        resale_count: int(value.get("resale_count")) as i32,
        min_resale_star_count: int(value.get("min_resale_star_count")),
    })
}

pub fn parse_available_gifts(value: &Value) -> Vec<AvailableGift> {
    value
        .get("gifts")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(parse_available_gift).collect())
        .unwrap_or_default()
}

/// One `premiumGiftPaymentOption`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumGiftOption {
    pub month_count: i32,
    pub currency: String,
    pub amount: i64,
    pub star_count: i64,
    pub discount_percentage: i32,
}

impl PremiumGiftOption {
    pub fn title(&self) -> String {
        match self.month_count {
            12 => "1 year".to_string(),
            24 => "2 years".to_string(),
            1 => "1 month".to_string(),
            n if n % 12 == 0 => format!("{} years", n / 12),
            n => format!("{n} months"),
        }
    }

    pub fn price_label(&self) -> String {
        format_payment_price(&self.currency, self.amount)
    }
}

pub fn parse_premium_gift_options(value: &Value) -> Vec<PremiumGiftOption> {
    value
        .get("options")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|o| PremiumGiftOption {
                    month_count: int(o.get("month_count")) as i32,
                    currency: text(o, "currency"),
                    amount: int(o.get("amount")),
                    star_count: int(o.get("star_count")),
                    discount_percentage: int(o.get("discount_percentage")) as i32,
                })
                .filter(|o| o.month_count > 0)
                .collect()
        })
        .unwrap_or_default()
}

/// Who a gift goes to: the user or channel of the open chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftRecipient {
    pub sender: MessageSender,
    pub name: String,
    pub is_self: bool,
    pub is_channel: bool,
}

impl crate::state::Session {
    /// The user or channel behind `chat_id`, if gifts can go there: not a
    /// bot, not a group, not a secret chat.
    pub fn gift_recipient(&self, chat_id: crate::ids::ChatId) -> Option<GiftRecipient> {
        use crate::telegram::envelope::ChatKind;
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } if user_id.0 > 0 => {
                let is_self = Some(user_id.0) == self.my_user_id;
                if !is_self && self.is_bot_user(user_id.0) {
                    return None;
                }
                Some(GiftRecipient {
                    sender: MessageSender::User { user_id: user_id.0 },
                    name: chat.title.clone(),
                    is_self,
                    is_channel: false,
                })
            }
            ChatKind::Supergroup {
                is_channel: true, ..
            } => Some(GiftRecipient {
                sender: MessageSender::Chat { chat_id: chat_id.0 },
                name: chat.title.clone(),
                is_self: false,
                is_channel: true,
            }),
            _ => None,
        }
    }

    /// The same for a user known by id, with or without a chat (a contact
    /// picked from the gift recipient list).
    pub fn gift_recipient_user(&self, user_id: i64) -> Option<GiftRecipient> {
        let user = self.users.get(&user_id)?;
        let is_self = Some(user_id) == self.my_user_id;
        if !is_self && user.is_bot {
            return None;
        }
        Some(GiftRecipient {
            sender: MessageSender::User { user_id },
            name: user.display_name(),
            is_self,
            is_channel: false,
        })
    }
}
