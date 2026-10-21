//! The share theme's TDLib requests: the owned upgraded gifts the gifts
//! panel lists and `setProfileShareTheme`.

use super::theme::ThemeChoice;
use crate::ids::RequestId;
use serde_json::{Value, json};

/// tdesktop `kMyGiftsPerPage`.
pub const GIFTS_PAGE: i32 = 50;

/// Who owns the gifts: the current user, or a channel or supergroup chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftsOwner {
    User(i64),
    Chat(i64),
}

fn owner_json(owner: GiftsOwner) -> Value {
    match owner {
        GiftsOwner::User(user_id) => json!({"@type": "messageSenderUser", "user_id": user_id}),
        GiftsOwner::Chat(chat_id) => json!({"@type": "messageSenderChat", "chat_id": chat_id}),
    }
}

/// `getReceivedGifts` for the upgraded gifts `owner` owns, `offset` on
/// (tdesktop `PeerUniqueGiftsSlice` with `OnlyOwned`: no unlimited, no
/// upgradable or non-upgradable regular gifts, no hosted ones).
pub fn get_share_theme_gifts(extra: RequestId, owner: GiftsOwner, offset: &str) -> String {
    json!({
        "@type": "getReceivedGifts",
        "@extra": extra.as_extra(),
        "business_connection_id": "",
        "owner_id": owner_json(owner),
        "collection_id": 0,
        "exclude_unsaved": false,
        "exclude_saved": false,
        "exclude_unlimited": true,
        "exclude_upgradable": true,
        "exclude_non_upgradable": true,
        "exclude_upgraded": false,
        "exclude_without_colors": false,
        "exclude_hosted": true,
        "sort_by_price": false,
        "offset": offset,
        "limit": GIFTS_PAGE,
    })
    .to_string()
}

/// `setProfileShareTheme`; `chat_id` is the private chat with the current
/// user, or the supergroup or channel chat. `None` when `choice` cannot
/// be sent (a gift with no received id).
pub fn set_profile_share_theme(
    extra: RequestId,
    chat_id: i64,
    choice: &ThemeChoice,
) -> Option<String> {
    let theme = choice.input_json()?;
    Some(
        json!({
            "@type": "setProfileShareTheme",
            "@extra": extra.as_extra(),
            "chat_id": chat_id,
            "theme": theme,
        })
        .to_string(),
    )
}
