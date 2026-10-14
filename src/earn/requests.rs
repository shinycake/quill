//! TDLib requests of the revenue and affiliate screens. None of them moves
//! funds: a withdrawal only asks for a Fragment URL the user opens in the
//! browser.

use super::{EarnPeer, ProgramParams, ProgramSort};
use crate::ids::RequestId;
use crate::premium_hub::TxFilter;
use serde_json::{Value, json};

/// TON history rows asked for first (tdesktop `EarnStatistics::kFirstSlice`).
pub const TON_FIRST_SLICE: i32 = 5;
/// TON history rows per "Show more" (`EarnStatistics::kLimit`).
pub const TON_MORE_SLICE: i32 = 10;
/// Stars history rows per page (tdesktop `kTransactionsLimit`).
pub const STAR_TX_LIMIT: i32 = 100;

fn request(extra: RequestId, mut body: Value) -> String {
    crate::json_access::set(&mut body, "@extra", json!(extra.as_extra()));
    body.to_string()
}

pub fn get_chat_revenue_statistics(extra: RequestId, chat_id: i64, is_dark: bool) -> String {
    request(
        extra,
        json!({"@type": "getChatRevenueStatistics", "chat_id": chat_id, "is_dark": is_dark}),
    )
}

pub fn get_chat_revenue_transactions(
    extra: RequestId,
    chat_id: i64,
    offset: &str,
    limit: i32,
) -> String {
    request(
        extra,
        json!({"@type": "getChatRevenueTransactions", "chat_id": chat_id,
               "offset": offset, "limit": limit}),
    )
}

/// `getChatRevenueWithdrawalUrl`; `password` is the user's 2FA password
/// as typed (empty on the first try), never stored.
pub fn get_chat_revenue_withdrawal_url(extra: RequestId, chat_id: i64, password: &str) -> String {
    request(
        extra,
        json!({"@type": "getChatRevenueWithdrawalUrl", "chat_id": chat_id, "password": password}),
    )
}

pub fn get_star_revenue_statistics(extra: RequestId, owner: EarnPeer, is_dark: bool) -> String {
    request(
        extra,
        json!({"@type": "getStarRevenueStatistics", "owner_id": owner.sender_json(),
               "is_dark": is_dark}),
    )
}

pub fn get_star_transactions(
    extra: RequestId,
    owner: EarnPeer,
    filter: TxFilter,
    offset: &str,
    limit: i32,
) -> String {
    let mut body = json!({"@type": "getStarTransactions", "owner_id": owner.sender_json(),
                          "subscription_id": "", "offset": offset, "limit": limit});
    if let Some(direction) = filter.direction_type() {
        crate::json_access::set(&mut body, "direction", json!({ "@type": direction }));
    }
    request(extra, body)
}

/// `getStarWithdrawalUrl`; same password rule as the TON one.
pub fn get_star_withdrawal_url(
    extra: RequestId,
    owner: EarnPeer,
    star_count: i64,
    password: &str,
) -> String {
    request(
        extra,
        json!({"@type": "getStarWithdrawalUrl", "owner_id": owner.sender_json(),
               "star_count": star_count, "password": password}),
    )
}

pub fn get_star_ad_account_url(extra: RequestId, owner: EarnPeer) -> String {
    request(
        extra,
        json!({"@type": "getStarAdAccountUrl", "owner_id": owner.sender_json()}),
    )
}

/// `setChatAffiliateProgram`; `None` ends the program.
pub fn set_chat_affiliate_program(
    extra: RequestId,
    bot_user_id: i64,
    params: Option<ProgramParams>,
) -> String {
    request(
        extra,
        json!({"@type": "setChatAffiliateProgram", "chat_id": bot_user_id,
               "parameters": params.map_or(Value::Null, ProgramParams::to_json)}),
    )
}

pub fn search_affiliate_programs(
    extra: RequestId,
    affiliate: Value,
    sort: ProgramSort,
    offset: &str,
    limit: i32,
) -> String {
    request(
        extra,
        json!({"@type": "searchAffiliatePrograms", "affiliate": affiliate,
               "sort_order": {"@type": sort.type_name()}, "offset": offset, "limit": limit}),
    )
}

pub fn connect_affiliate_program(extra: RequestId, affiliate: Value, bot_user_id: i64) -> String {
    request(
        extra,
        json!({"@type": "connectAffiliateProgram", "affiliate": affiliate,
               "bot_user_id": bot_user_id}),
    )
}

pub fn disconnect_affiliate_program(extra: RequestId, affiliate: Value, url: &str) -> String {
    request(
        extra,
        json!({"@type": "disconnectAffiliateProgram", "affiliate": affiliate, "url": url}),
    )
}

pub fn get_connected_affiliate_program(
    extra: RequestId,
    affiliate: Value,
    bot_user_id: i64,
) -> String {
    request(
        extra,
        json!({"@type": "getConnectedAffiliateProgram", "affiliate": affiliate,
               "bot_user_id": bot_user_id}),
    )
}

pub fn get_connected_affiliate_programs(
    extra: RequestId,
    affiliate: Value,
    offset: &str,
    limit: i32,
) -> String {
    request(
        extra,
        json!({"@type": "getConnectedAffiliatePrograms", "affiliate": affiliate,
               "offset": offset, "limit": limit}),
    )
}

/// Public chats the user owns (`channels.getAdminedPublicChannels`, what
/// tdesktop's `ResolveRecipients` asks for).
pub fn get_created_public_chats(extra: RequestId) -> String {
    request(
        extra,
        json!({"@type": "getCreatedPublicChats",
               "type": {"@type": "publicChatTypeHasUsername"}}),
    )
}

/// Bots the user owns (`bots.getAdminedBots`).
pub fn get_owned_bots(extra: RequestId) -> String {
    request(extra, json!({"@type": "getOwnedBots"}))
}
