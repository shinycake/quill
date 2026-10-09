use super::*;
use serde_json::Value;

/// Phase D3a: `starSubscriptionPricing` (TDLib 1.8.67,
/// `schema/td_api.tl:1252`): `starSubscriptionPricing period:int32
/// star_count:int53 = StarSubscriptionPricing;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarSubscriptionPricing {
    pub period: i32,
    pub star_count: i64,
}

/// Phase D3a: `chatInviteLink` (TDLib 1.8.67, `schema/td_api.tl:2627`).
/// `subscription_pricing` is `Option` because TDLib only attaches it to
/// subscription-priced links; everything else is required by the schema.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatInviteLink {
    pub invite_link: String,
    pub name: String,
    pub creator_user_id: i64,
    pub date: i32,
    pub edit_date: i32,
    pub expiration_date: i32,
    pub subscription_pricing: Option<StarSubscriptionPricing>,
    pub member_limit: i32,
    pub member_count: i32,
    pub expired_member_count: i32,
    pub pending_join_request_count: i32,
    pub creates_join_request: bool,
    pub is_primary: bool,
    pub is_revoked: bool,
}

/// Phase D3a: `chatJoinRequest` (TDLib 1.8.67, `schema/td_api.tl:2688`):
/// `chatJoinRequest user_id:int53 date:int32 bio:string = ChatJoinRequest;`
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatJoinRequest {
    pub user_id: i64,
    pub date: i32,
    pub bio: String,
}

pub(crate) fn parse_star_subscription_pricing(
    value: Option<&Value>,
) -> Option<StarSubscriptionPricing> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("starSubscriptionPricing") {
        return None;
    }
    Some(StarSubscriptionPricing {
        period: int53(value.get("period")).ok()?.sat_i32(),
        star_count: int53(value.get("star_count")).ok()?,
    })
}

pub(crate) fn parse_chat_invite_link(value: Option<&Value>) -> Option<ParsedChatInviteLink> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatInviteLink") {
        return None;
    }
    Some(ParsedChatInviteLink {
        invite_link: value.get("invite_link").and_then(Value::as_str)?.to_owned(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        creator_user_id: int53(value.get("creator_user_id")).ok()?,
        date: int53(value.get("date")).ok()?.sat_i32(),
        edit_date: int53(value.get("edit_date")).ok().unwrap_or(0).sat_i32(),
        expiration_date: int53(value.get("expiration_date"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
        subscription_pricing: parse_star_subscription_pricing(value.get("subscription_pricing")),
        member_limit: int53(value.get("member_limit")).ok().unwrap_or(0).sat_i32(),
        member_count: int53(value.get("member_count")).ok().unwrap_or(0).sat_i32(),
        expired_member_count: int53(value.get("expired_member_count"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
        pending_join_request_count: int53(value.get("pending_join_request_count"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
        creates_join_request: value
            .get("creates_join_request")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_primary: value
            .get("is_primary")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_revoked: value
            .get("is_revoked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn parse_chat_join_request(value: Option<&Value>) -> Option<ParsedChatJoinRequest> {
    let value = value?;
    if value.get("@type").and_then(Value::as_str) != Some("chatJoinRequest") {
        return None;
    }
    Some(ParsedChatJoinRequest {
        user_id: int53(value.get("user_id")).ok()?,
        date: int53(value.get("date")).ok()?.sat_i32(),
        bio: value
            .get("bio")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// B8: `chatInviteLinkCount` (TDLib 1.8.68, `schema/td_api.tl:2946`):
/// one administrator's active and revoked link counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChatInviteLinkCount {
    pub user_id: i64,
    pub invite_link_count: i32,
    pub revoked_invite_link_count: i32,
}

/// B8: `chatInviteLinkMember` (`schema/td_api.tl:2956`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChatInviteLinkMember {
    pub user_id: i64,
    pub joined_chat_date: i32,
    pub via_chat_folder_invite_link: bool,
    pub approver_user_id: i64,
}

pub(crate) fn parse_chat_invite_link_count(
    value: Option<&Value>,
) -> Option<ParsedChatInviteLinkCount> {
    let value = value?;
    Some(ParsedChatInviteLinkCount {
        user_id: int53(value.get("user_id")).ok()?,
        invite_link_count: int53(value.get("invite_link_count"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
        revoked_invite_link_count: int53(value.get("revoked_invite_link_count"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
    })
}

pub(crate) fn parse_chat_invite_link_member(
    value: Option<&Value>,
) -> Option<ParsedChatInviteLinkMember> {
    let value = value?;
    Some(ParsedChatInviteLinkMember {
        user_id: int53(value.get("user_id")).ok()?,
        joined_chat_date: int53(value.get("joined_chat_date"))
            .ok()
            .unwrap_or(0)
            .sat_i32(),
        via_chat_folder_invite_link: value
            .get("via_chat_folder_invite_link")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        approver_user_id: int53(value.get("approver_user_id")).ok().unwrap_or(0),
    })
}
