//! Invite-link and join-request admin builders (batch B8): paged and
//! searched join requests, bulk process, link members and counts,
//! deleting revoked links, and Stars subscription links.
use crate::ids::RequestId;
use serde_json::json;

/// The only subscription period TDLib accepts: 30 days in seconds
/// (`starSubscriptionPricing.period`, schema/td_api.tl:1252).
pub const SUBSCRIPTION_PERIOD_SECONDS: i32 = 2_592_000;

/// `getChatJoinRequests` (schema/td_api.tl:14568) with an explicit paging
/// offset. `offset` is the last row's `(user_id, date)`; `None` is the
/// first page (an empty `chatJoinRequest`, as official clients send).
pub fn get_chat_join_requests_page(
    extra: RequestId,
    chat_id: i64,
    invite_link: &str,
    query: &str,
    offset: Option<(i64, i32)>,
    limit: i32,
) -> String {
    let (user_id, date) = offset.unwrap_or((0, 0));
    json!({
        "@type": "getChatJoinRequests",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
        "query": query,
        "offset_request": {
            "@type": "chatJoinRequest",
            "user_id": user_id,
            "date": date,
        },
        "limit": limit,
    })
    .to_string()
}

/// `processChatJoinRequests` (schema/td_api.tl:14577): approve or dismiss
/// every pending request, or only those of `invite_link` when non-empty.
pub fn process_chat_join_requests(
    extra: RequestId,
    chat_id: i64,
    invite_link: &str,
    approve: bool,
) -> String {
    json!({
        "@type": "processChatJoinRequests",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
        "approve": approve,
    })
    .to_string()
}

/// `getChatInviteLinkCounts` (schema/td_api.tl:14523): admins with their
/// invite-link counts; owner only.
pub fn get_chat_invite_link_counts(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "getChatInviteLinkCounts",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// `getChatInviteLinkMembers` (schema/td_api.tl:14540). `offset` is the
/// last member's `(user_id, joined_chat_date)`; `None` is the first page
/// (TDLib expects a null offset member then).
pub fn get_chat_invite_link_members(
    extra: RequestId,
    chat_id: i64,
    invite_link: &str,
    only_with_expired_subscription: bool,
    offset: Option<(i64, i32)>,
    limit: i32,
) -> String {
    let offset_member = offset.map(|(user_id, date)| {
        json!({
            "@type": "chatInviteLinkMember",
            "user_id": user_id,
            "joined_chat_date": date,
            "via_chat_folder_invite_link": false,
            "approver_user_id": 0,
        })
    });
    json!({
        "@type": "getChatInviteLinkMembers",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
        "only_with_expired_subscription": only_with_expired_subscription,
        "offset_member": offset_member,
        "limit": limit,
    })
    .to_string()
}

/// `deleteRevokedChatInviteLink` (schema/td_api.tl:14549).
pub fn delete_revoked_chat_invite_link(
    extra: RequestId,
    chat_id: i64,
    invite_link: &str,
) -> String {
    json!({
        "@type": "deleteRevokedChatInviteLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
    })
    .to_string()
}

/// `deleteAllRevokedChatInviteLinks` (schema/td_api.tl:14554).
pub fn delete_all_revoked_chat_invite_links(
    extra: RequestId,
    chat_id: i64,
    creator_user_id: i64,
) -> String {
    json!({
        "@type": "deleteAllRevokedChatInviteLinks",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "creator_user_id": creator_user_id,
    })
    .to_string()
}

/// `createChatSubscriptionInviteLink` (schema/td_api.tl:14498): a channel
/// link charging `star_count` Stars per 30-day period.
pub fn create_chat_subscription_invite_link(
    extra: RequestId,
    chat_id: i64,
    name: &str,
    star_count: i64,
) -> String {
    json!({
        "@type": "createChatSubscriptionInviteLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "name": name,
        "subscription_pricing": {
            "@type": "starSubscriptionPricing",
            "period": SUBSCRIPTION_PERIOD_SECONDS,
            "star_count": star_count,
        },
    })
    .to_string()
}

/// `editChatSubscriptionInviteLink` (schema/td_api.tl:14515): only the
/// name of a subscription link can change.
pub fn edit_chat_subscription_invite_link(
    extra: RequestId,
    chat_id: i64,
    invite_link: &str,
    name: &str,
) -> String {
    json!({
        "@type": "editChatSubscriptionInviteLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "invite_link": invite_link,
        "name": name,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        SUBSCRIPTION_PERIOD_SECONDS, create_chat_subscription_invite_link,
        delete_all_revoked_chat_invite_links, delete_revoked_chat_invite_link,
        edit_chat_subscription_invite_link, get_chat_invite_link_counts,
        get_chat_invite_link_members, get_chat_join_requests_page, process_chat_join_requests,
    };
    use crate::ids::RequestId;
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn join_requests_page_carries_offset_and_query() {
        let v = parse(&get_chat_join_requests_page(
            RequestId(1),
            5,
            "",
            "ann",
            Some((77, 1234)),
            50,
        ));
        assert_eq!(v["@type"], "getChatJoinRequests");
        assert_eq!(v["query"], "ann");
        assert_eq!(v["offset_request"]["user_id"], 77);
        assert_eq!(v["offset_request"]["date"], 1234);
        assert_eq!(v["limit"], 50);
        let first = parse(&get_chat_join_requests_page(
            RequestId(1),
            5,
            "",
            "",
            None,
            50,
        ));
        assert_eq!(first["offset_request"]["user_id"], 0);
    }

    #[test]
    fn process_all_shape() {
        let v = parse(&process_chat_join_requests(RequestId(2), 5, "", false));
        assert_eq!(v["@type"], "processChatJoinRequests");
        assert_eq!(v["invite_link"], "");
        assert_eq!(v["approve"], false);
    }

    #[test]
    fn link_members_first_page_has_null_offset() {
        let v = parse(&get_chat_invite_link_members(
            RequestId(3),
            5,
            "https://t.me/+x",
            false,
            None,
            50,
        ));
        assert!(v["offset_member"].is_null());
        let next = parse(&get_chat_invite_link_members(
            RequestId(3),
            5,
            "https://t.me/+x",
            false,
            Some((9, 100)),
            50,
        ));
        assert_eq!(next["offset_member"]["user_id"], 9);
        assert_eq!(next["offset_member"]["joined_chat_date"], 100);
    }

    #[test]
    fn counts_and_delete_shapes() {
        assert_eq!(
            parse(&get_chat_invite_link_counts(RequestId(4), 5))["@type"],
            "getChatInviteLinkCounts"
        );
        let one = parse(&delete_revoked_chat_invite_link(RequestId(5), 5, "l"));
        assert_eq!(one["@type"], "deleteRevokedChatInviteLink");
        assert_eq!(one["invite_link"], "l");
        let all = parse(&delete_all_revoked_chat_invite_links(RequestId(6), 5, 777));
        assert_eq!(all["@type"], "deleteAllRevokedChatInviteLinks");
        assert_eq!(all["creator_user_id"], 777);
    }

    #[test]
    fn subscription_link_shapes() {
        let c = parse(&create_chat_subscription_invite_link(
            RequestId(7),
            5,
            "VIP",
            250,
        ));
        assert_eq!(
            c["subscription_pricing"]["period"],
            SUBSCRIPTION_PERIOD_SECONDS
        );
        assert_eq!(c["subscription_pricing"]["star_count"], 250);
        let e = parse(&edit_chat_subscription_invite_link(
            RequestId(8),
            5,
            "l",
            "New",
        ));
        assert_eq!(e["@type"], "editChatSubscriptionInviteLink");
        assert_eq!(e["name"], "New");
    }
}
