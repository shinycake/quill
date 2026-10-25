//! Telegram page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Telegram;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("telegram.premium", P, keys::PREMIUM_SUMMARY_TITLE)
        .keywords(&["subscription", "upgrade"]),
    Entry::new("telegram.stars", P, keys::Q_MONEY_STARS)
        .keywords(&["balance", "top up", "credits"]),
    Entry::new("telegram.my_gifts", P, keys::GIFT_STARS_TABS_MY)
        .keywords(&["presents", "received gifts"]),
    Entry::new("telegram.grams", P, keys::SETTINGS_CURRENCY).keywords(&["balance", "fragment"]),
    Entry::new("telegram.money", P, keys::Q_MONEY_MONEY).keywords(&["ton", "wallet", "crypto"]),
    Entry::new("telegram.business", P, keys::BUSINESS_TITLE).keywords(&[
        "opening hours",
        "away message",
        "greeting",
        "chatbots",
    ]),
    Entry::new("telegram.send_gift", P, keys::SETTINGS_GIFT_PREMIUM).keywords(&[
        "present",
        "send",
        "gift premium",
        "birthday",
    ]),
    Entry::new(
        "telegram.star_subscriptions",
        P,
        keys::Q_NAVIGATION_STAR_SUBSCRIPTIONS,
    )
    .keywords(&["recurring", "cancel"]),
];
