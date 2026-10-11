use crate::composer::SendOptions;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::requests::*;
use serde_json::Value;

#[test]
fn get_chat_administrators_shape_matches_1_8_67() {
    // Phase D3b: `getChatAdministrators chat_id:int53 = ChatAdministrators;`
    // (schema 1.8.67, line 13632).
    let json = get_chat_administrators(RequestId(71), 13);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatAdministrators");
    assert_eq!(v["@extra"], "71");
    assert_eq!(v["chat_id"], 13);
    let schema = include_str!("../../../schema/td_api.tl");
    let line = schema
        .lines()
        .find(|l| l.starts_with("getChatAdministrators "))
        .expect("getChatAdministrators in schema");
    assert_eq!(
        line,
        "getChatAdministrators chat_id:int53 = ChatAdministrators;"
    );
}

#[test]
fn set_chat_member_status_promote_shape_matches_1_8_67() {
    // Phase D3b: promote shape — `setChatMemberStatus` (schema 1.8.67,
    // line 13592) with `messageSenderUser` (line 2831) and
    // `chatMemberStatusAdministrator` (line 2500) carrying all 18
    // `chatAdministratorRights` fields (line 1092).
    let member_id = MessageSenderRef::User(888).to_value();
    let rights = crate::telegram::envelope::ChatAdminRights {
        can_manage_chat: true,
        can_promote_members: true,
        ..Default::default()
    };
    let status = chat_member_status_administrator_json(true, &rights);
    let json = set_chat_member_status(RequestId(72), 13, &member_id, &status);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setChatMemberStatus");
    assert_eq!(v["@extra"], "72");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["member_id"]["@type"], "messageSenderUser");
    assert_eq!(v["member_id"]["user_id"], 888);
    assert_eq!(v["status"]["@type"], "chatMemberStatusAdministrator");
    assert_eq!(v["status"]["can_be_edited"], true);
    let schema = include_str!("../../../schema/td_api.tl");
    let rights_line = schema
        .lines()
        .find(|l| l.starts_with("chatAdministratorRights "))
        .expect("chatAdministratorRights in schema");
    // Every schema field of chatAdministratorRights must be present.
    for field in rights_line
        .split_whitespace()
        .skip(1)
        .take_while(|token| !token.starts_with('='))
    {
        let name = field.split(':').next().unwrap();
        assert!(
            v["status"]["rights"][name].is_boolean(),
            "missing rights field {name}"
        );
    }
    assert_eq!(v["status"]["rights"]["@type"], "chatAdministratorRights");
    assert_eq!(v["status"]["rights"]["can_manage_chat"], true);
    assert_eq!(v["status"]["rights"]["can_promote_members"], true);
    assert_eq!(v["status"]["rights"]["can_delete_messages"], false);
}

#[test]
fn set_chat_member_status_demote_shape_matches_1_8_67() {
    // Phase D3b: demote shape — `setChatMemberStatus` to
    // `chatMemberStatusMember` (schema 1.8.67, line 2504).
    let member_id = MessageSenderRef::User(888).to_value();
    let status = chat_member_status_member_json();
    let json = set_chat_member_status(RequestId(73), 13, &member_id, &status);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setChatMemberStatus");
    assert_eq!(v["status"]["@type"], "chatMemberStatusMember");
    assert_eq!(v["status"]["member_until_date"], 0);
}

#[test]
fn get_supergroup_members_shape_matches_1_8_67() {
    // Phase D3b: `getSupergroupMembers supergroup_id:int53
    // filter:SupergroupMembersFilter offset:int32 limit:int32 =
    // ChatMembers;` (schema 1.8.67, line 15238) with
    // `supergroupMembersFilterSearch` (line 2568).
    let filter = supergroup_members_filter_search_json("ada");
    let json = get_supergroup_members(RequestId(74), 25, &filter, 0, 200);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getSupergroupMembers");
    assert_eq!(v["@extra"], "74");
    assert_eq!(v["supergroup_id"], 25);
    assert_eq!(v["filter"]["@type"], "supergroupMembersFilterSearch");
    assert_eq!(v["filter"]["query"], "ada");
    assert_eq!(v["offset"], 0);
    assert_eq!(v["limit"], 200);

    let recent = supergroup_members_filter_recent_json();
    assert_eq!(recent["@type"], "supergroupMembersFilterRecent");
}

#[test]
fn get_supergroup_full_info_shape_matches_1_8_67() {
    // `getSupergroupFullInfo supergroup_id:int53 = SupergroupFullInfo;`
    // (schema 1.8.67, line 11513).
    let json = get_supergroup_full_info(RequestId(62), 77);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getSupergroupFullInfo");
    assert_eq!(v["@extra"], "62");
    assert_eq!(v["supergroup_id"], 77);
    assert!(!json.contains("CANARY"));
}

#[test]
fn get_chat_statistics_shape_matches_1_8_67() {
    // `getChatStatistics chat_id:int53 is_dark:Bool = ChatStatistics;`
    // (schema 1.8.67, line 15760).
    let json = get_chat_statistics(RequestId(63), 13, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatStatistics");
    assert_eq!(v["@extra"], "63");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["is_dark"], true);
    assert!(!json.contains("CANARY"));
}

#[test]
fn get_chat_invite_links_shape_matches_1_8_67() {
    // `getChatInviteLinks chat_id:int53 creator_user_id:int53 is_revoked:Bool offset_date:int32 offset_invite_link:string limit:int32 = ChatInviteLinks;`
    // (schema 1.8.67, line 14138).
    let json = get_chat_invite_links(
        RequestId(64),
        101,
        202,
        true,
        1_700_000_000,
        "https://t.me/+offset",
        25,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatInviteLinks");
    assert_eq!(v["@extra"], "64");
    assert_eq!(v["chat_id"], 101);
    assert_eq!(v["creator_user_id"], 202);
    assert_eq!(v["is_revoked"], true);
    assert_eq!(v["offset_date"], 1_700_000_000);
    assert_eq!(v["offset_invite_link"], "https://t.me/+offset");
    assert_eq!(v["limit"], 25);
    assert!(!json.contains("CANARY"));
}

#[test]
fn create_chat_invite_link_shape_matches_1_8_67() {
    // `createChatInviteLink chat_id:int53 name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;`
    // (schema 1.8.67, line 14097).
    let json = create_chat_invite_link(
        RequestId(65),
        303,
        "Moderated access",
        1_800_000_000,
        50,
        true,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "createChatInviteLink");
    assert_eq!(v["@extra"], "65");
    assert_eq!(v["chat_id"], 303);
    assert_eq!(v["name"], "Moderated access");
    assert_eq!(v["expiration_date"], 1_800_000_000);
    assert_eq!(v["member_limit"], 50);
    assert_eq!(v["creates_join_request"], true);
    assert!(!json.contains("CANARY"));
}

#[test]
fn edit_chat_invite_link_shape_matches_1_8_67() {
    // `editChatInviteLink chat_id:int53 invite_link:string name:string expiration_date:int32 member_limit:int32 creates_join_request:Bool = ChatInviteLink;`
    // (schema 1.8.67, line 14115).
    let json = edit_chat_invite_link(
        RequestId(66),
        404,
        "https://t.me/+existing",
        "Updated access",
        1_900_000_000,
        75,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editChatInviteLink");
    assert_eq!(v["@extra"], "66");
    assert_eq!(v["chat_id"], 404);
    assert_eq!(v["invite_link"], "https://t.me/+existing");
    assert_eq!(v["name"], "Updated access");
    assert_eq!(v["expiration_date"], 1_900_000_000);
    assert_eq!(v["member_limit"], 75);
    assert_eq!(v["creates_join_request"], false);
    assert!(!json.contains("CANARY"));
}

#[test]
fn revoke_chat_invite_link_shape_matches_1_8_67() {
    // `revokeChatInviteLink chat_id:int53 invite_link:string = ChatInviteLinks;`
    // (schema 1.8.67, line 14152).
    let json = revoke_chat_invite_link(RequestId(67), 505, "https://t.me/+revoked");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "revokeChatInviteLink");
    assert_eq!(v["@extra"], "67");
    assert_eq!(v["chat_id"], 505);
    assert_eq!(v["invite_link"], "https://t.me/+revoked");
    assert!(!json.contains("CANARY"));
}

#[test]
fn get_chat_join_requests_shape_matches_1_8_67() {
    // `getChatJoinRequests chat_id:int53 invite_link:string query:string offset_request:chatJoinRequest limit:int32 = ChatJoinRequests;`
    // (schema 1.8.67, line 14174).
    let json = get_chat_join_requests(RequestId(68), 606, "https://t.me/+requests", "alice", 30);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatJoinRequests");
    assert_eq!(v["@extra"], "68");
    assert_eq!(v["chat_id"], 606);
    assert_eq!(v["invite_link"], "https://t.me/+requests");
    assert_eq!(v["query"], "alice");
    assert_eq!(
        v["offset_request"],
        serde_json::json!({
            "@type": "chatJoinRequest",
        })
    );
    assert_eq!(v["limit"], 30);
    assert!(!json.contains("CANARY"));
}

#[test]
fn process_chat_join_request_shape_matches_1_8_67() {
    // `processChatJoinRequest chat_id:int53 user_id:int53 approve:Bool = Ok;`
    // (schema 1.8.67, line 14177).
    let json = process_chat_join_request(RequestId(69), 707, 808, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "processChatJoinRequest");
    assert_eq!(v["@extra"], "69");
    assert_eq!(v["chat_id"], 707);
    assert_eq!(v["user_id"], 808);
    assert_eq!(v["approve"], true);
    assert!(!json.contains("CANARY"));
}

#[test]
fn get_supergroup_shape_matches_1_8_67() {
    let json = get_supergroup(RequestId(32), 77);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getSupergroup");
    assert_eq!(v["@extra"], "32");
    assert_eq!(v["supergroup_id"], 77);
    assert!(!json.contains("CANARY"));
}

#[test]
fn channel_request_shapes_match_1_8_67() {
    let me = get_me(RequestId(60));
    let v: serde_json::Value = serde_json::from_str(&me).unwrap();
    assert_eq!(v["@type"], "getMe");
    assert_eq!(v["@extra"], "60");

    let member = get_chat_member(RequestId(61), ChatId(13), 777);
    let v: serde_json::Value = serde_json::from_str(&member).unwrap();
    assert_eq!(v["@type"], "getChatMember");
    assert_eq!(v["@extra"], "61");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["member_id"]["@type"], "messageSenderUser");
    assert_eq!(v["member_id"]["user_id"], 777);

    let join = join_chat(RequestId(62), ChatId(13));
    let v: serde_json::Value = serde_json::from_str(&join).unwrap();
    assert_eq!(v["@type"], "joinChat");
    assert_eq!(v["@extra"], "62");
    assert_eq!(v["chat_id"], 13);

    let leave = leave_chat(RequestId(63), ChatId(13));
    let v: serde_json::Value = serde_json::from_str(&leave).unwrap();
    assert_eq!(v["@type"], "leaveChat");
    assert_eq!(v["@extra"], "63");
    assert_eq!(v["chat_id"], 13);
}

#[test]
fn g1_create_shapes_match_1_8_67() {
    // Slice G1: `createNewBasicGroupChat user_ids:vector<int53>
    // title:string message_auto_delete_time:int32 =
    // CreatedBasicGroupChat` (schema 1.8.67, line 13327).
    let v: serde_json::Value = serde_json::from_str(&create_new_basic_group_chat(
        RequestId(70),
        &[7, 8],
        "Study",
    ))
    .unwrap();
    assert_eq!(v["@type"], "createNewBasicGroupChat");
    assert_eq!(v["user_ids"], serde_json::json!([7, 8]));
    assert_eq!(v["title"], "Study");
    assert_eq!(v["message_auto_delete_time"], 0);

    // Slice G1: `createNewSupergroupChat title:string is_forum:Bool
    // is_channel:Bool description:string location:chatLocation
    // message_auto_delete_time:int32 for_import:Bool = Chat`
    // (schema 1.8.67, line 13337).
    let v: serde_json::Value = serde_json::from_str(&create_new_supergroup_chat(
        RequestId(71),
        "News",
        true,
        "desc",
    ))
    .unwrap();
    assert_eq!(v["@type"], "createNewSupergroupChat");
    assert_eq!(v["title"], "News");
    assert!(v["is_forum"].as_bool() == Some(false));
    assert!(v["is_channel"].as_bool() == Some(true));
    assert_eq!(v["description"], "desc");
    assert!(v["location"].is_null());
    assert!(v["for_import"].as_bool() == Some(false));
}

#[test]
fn group_info_edit_request_shapes() {
    use super::{set_chat_description, set_chat_photo, set_chat_title};

    // `setChatTitle chat_id:int53 title:string = Ok` (schema 1.8.67,
    // line 13430).
    let v: serde_json::Value =
        serde_json::from_str(&set_chat_title(RequestId(81), ChatId(9), "Rustaceans")).unwrap();
    assert_eq!(v["@type"], "setChatTitle");
    assert_eq!(v["chat_id"], 9);
    assert_eq!(v["title"], "Rustaceans");
    // Boundary lengths encode fine; the driver enforces 1-128.
    let title_128 = "x".repeat(128);
    let v: serde_json::Value =
        serde_json::from_str(&set_chat_title(RequestId(82), ChatId(9), &title_128)).unwrap();
    assert_eq!(v["title"], title_128);

    // `setChatDescription chat_id:int53 description:string = Ok`
    // (schema 1.8.67, line 13533); empty clears, 255 is the max.
    let v: serde_json::Value =
        serde_json::from_str(&set_chat_description(RequestId(83), ChatId(9), "about")).unwrap();
    assert_eq!(v["@type"], "setChatDescription");
    assert_eq!(v["chat_id"], 9);
    assert_eq!(v["description"], "about");
    let desc_255 = "y".repeat(255);
    let v: serde_json::Value =
        serde_json::from_str(&set_chat_description(RequestId(84), ChatId(9), &desc_255)).unwrap();
    assert_eq!(v["description"], desc_255);
    let v: serde_json::Value =
        serde_json::from_str(&set_chat_description(RequestId(85), ChatId(9), "")).unwrap();
    assert_eq!(v["description"], "");

    // `setChatPhoto chat_id:int53 photo:InputChatPhoto = Ok` (schema
    // 1.8.67, line 13435) — `inputChatPhotoStatic` / `inputFileLocal`.
    let photo_json = serde_json::json!({
        "@type": "inputChatPhotoStatic",
        "photo": { "@type": "inputFileLocal", "path": "/tmp/pic.jpg" },
    });
    let v: serde_json::Value =
        serde_json::from_str(&set_chat_photo(RequestId(86), ChatId(9), photo_json)).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["chat_id"], 9);
    assert_eq!(v["photo"]["@type"], "inputChatPhotoStatic");
    assert_eq!(v["photo"]["photo"]["@type"], "inputFileLocal");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/pic.jpg");
    // Delete = null top-level `photo` ("pass null to delete the chat
    // photo", schema line 13435) — not `inputChatPhotoPrevious`,
    // which is only a reused *user profile* photo.
    let v: serde_json::Value = serde_json::from_str(&set_chat_photo(
        RequestId(87),
        ChatId(9),
        serde_json::Value::Null,
    ))
    .unwrap();
    assert!(v["photo"].is_null());
}

/// The vendored schema line for `name`, and its parameter names.
fn schema_params(name: &str) -> Vec<&'static str> {
    let schema = include_str!("../../../schema/td_api.tl");
    let line = schema
        .lines()
        .find(|l| l.starts_with(&format!("{name} ")))
        .unwrap_or_else(|| panic!("{name} in schema"));
    line.split_whitespace()
        .skip(1)
        .take_while(|token| !token.starts_with('='))
        .map(|field| field.split(':').next().unwrap())
        .collect()
}

/// Every schema parameter of `name` is present in `v`, and nothing else
/// (besides `@type` / `@extra`).
fn assert_matches_schema(v: &Value, name: &str) {
    assert_eq!(v["@type"], name);
    let params = schema_params(name);
    let object = v.as_object().unwrap();
    for param in &params {
        assert!(object.contains_key(*param), "{name}: missing {param}");
    }
    for key in object.keys() {
        assert!(
            key.starts_with('@') || params.contains(&key.as_str()),
            "{name}: unknown field {key}"
        );
    }
}

#[test]
fn community_management_shapes_match_1_8_68() {
    use super::{
        delete_community, get_community_full_info, set_community_permissions, set_community_photo,
    };
    // TDLib 1.8.68 replaced `loadCommunityFullInfo` (answered `ok`) with
    // `getCommunityFullInfo` (answers `communityFullInfo`).
    assert!(
        !include_str!("../../../schema/td_api.tl").contains("\nloadCommunityFullInfo "),
        "loadCommunityFullInfo is gone in 1.8.68"
    );
    let v: Value = serde_json::from_str(&get_community_full_info(RequestId(92), 42)).unwrap();
    assert_matches_schema(&v, "getCommunityFullInfo");
    assert_eq!(v["community_id"], 42);
    assert_eq!(v["@extra"], "92");

    let photo = serde_json::json!({
        "@type": "inputChatPhotoStatic",
        "photo": { "@type": "inputFileLocal", "path": "/tmp/a.jpg" },
    });
    let v: Value = serde_json::from_str(&set_community_photo(RequestId(94), 42, photo)).unwrap();
    assert_matches_schema(&v, "setCommunityPhoto");
    assert_eq!(v["photo"]["@type"], "inputChatPhotoStatic");
    let v: Value =
        serde_json::from_str(&set_community_photo(RequestId(95), 42, Value::Null)).unwrap();
    assert!(v["photo"].is_null(), "null deletes the photo");

    let v: Value =
        serde_json::from_str(&set_community_permissions(RequestId(96), 42, true)).unwrap();
    assert_matches_schema(&v, "setCommunityPermissions");
    assert_eq!(v["permissions"]["@type"], "communityPermissions");
    for field in schema_params("communityPermissions") {
        assert!(
            v["permissions"][field].is_boolean(),
            "communityPermissions.{field}"
        );
    }
    assert_eq!(v["permissions"]["can_edit_chat_list"], true);

    let v: Value = serde_json::from_str(&delete_community(RequestId(97), 42)).unwrap();
    assert_matches_schema(&v, "deleteCommunity");
    assert_eq!(v["community_id"], 42);
}

#[test]
fn community_request_shapes() {
    use super::{create_community, set_community_name};

    // `createCommunity name:string chat_id:int53 is_chat_hidden:Bool
    // = CommunityId` (schema 1.8.67, line 11806).
    let v: serde_json::Value =
        serde_json::from_str(&create_community(RequestId(91), "Rustaceans", 9, true)).unwrap();
    assert_eq!(v["@type"], "createCommunity");
    assert_eq!(v["name"], "Rustaceans");
    assert_eq!(v["chat_id"], 9);
    assert!(v["is_chat_hidden"].as_bool() == Some(true));

    // `setCommunityName community_id:int53 name:string = Ok` (schema
    // 1.8.67, line 11811).
    let v: serde_json::Value =
        serde_json::from_str(&set_community_name(RequestId(93), 42, "Rustaceans+")).unwrap();
    assert_eq!(v["@type"], "setCommunityName");
    assert_eq!(v["community_id"], 42);
    assert_eq!(v["name"], "Rustaceans+");
}

#[test]
fn g1_group_admin_shapes_match_1_8_67() {
    // Slice G1: `toggleSupergroupIsBroadcastGroup supergroup_id:int53
    // = Ok` (schema 1.8.67, line 15221).
    let v: serde_json::Value =
        serde_json::from_str(&toggle_supergroup_is_broadcast_group(RequestId(72), 13)).unwrap();
    assert_eq!(v["@type"], "toggleSupergroupIsBroadcastGroup");
    assert_eq!(v["supergroup_id"], 13);

    // Slice G1: `addChatMembers chat_id:int53 user_ids:vector<int53>
    // = FailedToAddMembers` (schema 1.8.67, line 13584).
    let v: serde_json::Value =
        serde_json::from_str(&add_chat_members(RequestId(73), 11, &[7])).unwrap();
    assert_eq!(v["@type"], "addChatMembers");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["user_ids"], serde_json::json!([7]));

    // Slice G1: `setChatPermissions chat_id:int53
    // permissions:chatPermissions = Ok` (schema 1.8.67, line 13464).
    let perms = serde_json::json!({"@type": "chatPermissions", "can_send_basic_messages": true});
    let v: serde_json::Value =
        serde_json::from_str(&set_chat_permissions(RequestId(74), 11, &perms)).unwrap();
    assert_eq!(v["@type"], "setChatPermissions");
    assert_eq!(v["permissions"]["can_send_basic_messages"], true);

    // Slice G1: `replacePrimaryChatInviteLink chat_id:int53 =
    // ChatInviteLink` (schema 1.8.67, line 14089).
    let v: serde_json::Value =
        serde_json::from_str(&replace_primary_chat_invite_link(RequestId(75), 11)).unwrap();
    assert_eq!(v["@type"], "replacePrimaryChatInviteLink");
    assert_eq!(v["chat_id"], 11);

    // Slice G1: `toggleSupergroupJoinByRequest supergroup_id:int53
    // join_by_request:Bool guard_bot_user_id:int53
    // apply_to_invite_links:Bool = Ok` (schema 1.8.67, line 15188).
    let v: serde_json::Value =
        serde_json::from_str(&toggle_supergroup_join_by_request(RequestId(76), 13, true)).unwrap();
    assert_eq!(v["@type"], "toggleSupergroupJoinByRequest");
    assert_eq!(v["supergroup_id"], 13);
    assert!(v["join_by_request"].as_bool() == Some(true));
    assert_eq!(v["guard_bot_user_id"], 0);
    assert!(v["apply_to_invite_links"].as_bool() == Some(false));

    // Slice G1: `setSupergroupUsername supergroup_id:int53
    // username:string = Ok` (schema 1.8.67, line 15136).
    let v: serde_json::Value =
        serde_json::from_str(&set_supergroup_username(RequestId(77), 13, "news")).unwrap();
    assert_eq!(v["@type"], "setSupergroupUsername");
    assert_eq!(v["username"], "news");

    // Slice G1: `deleteChat chat_id:int53 = Ok` (schema 1.8.67, line
    // 11850).
    let v: serde_json::Value = serde_json::from_str(&delete_chat(RequestId(78), 11)).unwrap();
    assert_eq!(v["@type"], "deleteChat");
    assert_eq!(v["chat_id"], 11);

    // Slice CL1: `toggleChatIsPinned chat_list:ChatList chat_id:int53
    // is_pinned:Bool = Ok` (schema 1.8.67, line 13678).
    let v: serde_json::Value =
        serde_json::from_str(&toggle_chat_is_pinned(RequestId(79), 12, false, true)).unwrap();
    assert_eq!(v["@type"], "toggleChatIsPinned");
    assert_eq!(v["chat_id"], 12);
    assert_eq!(v["chat_list"]["@type"], "chatListMain");
    assert_eq!(v["is_pinned"], true);
    let v: serde_json::Value =
        serde_json::from_str(&toggle_chat_is_pinned(RequestId(80), 12, true, false)).unwrap();
    assert_eq!(v["chat_list"]["@type"], "chatListArchive");
    assert_eq!(v["is_pinned"], false);

    // Slice CL1: `toggleChatIsMarkedAsUnread chat_id:int53
    // is_marked_as_unread:Bool = Ok` (schema 1.8.67, line 13519).
    let v: serde_json::Value =
        serde_json::from_str(&toggle_chat_is_marked_as_unread(RequestId(81), 13, true)).unwrap();
    assert_eq!(v["@type"], "toggleChatIsMarkedAsUnread");
    assert_eq!(v["chat_id"], 13);
    assert_eq!(v["is_marked_as_unread"], true);

    // Slice CL1: `deleteChatHistory chat_id:int53
    // remove_from_chat_list:Bool revoke:Bool = Ok` (schema 1.8.67,
    // line 11845). Clear-history keeps the chat in the list;
    // remove-from-list drops it (Telegram X `Tdlib.deleteChat`).
    let v: serde_json::Value =
        serde_json::from_str(&delete_chat_history(RequestId(82), 14, false, true)).unwrap();
    assert_eq!(v["@type"], "deleteChatHistory");
    assert_eq!(v["chat_id"], 14);
    assert_eq!(v["remove_from_chat_list"], false);
    assert_eq!(v["revoke"], true);
    let v: serde_json::Value =
        serde_json::from_str(&delete_chat_history(RequestId(83), 15, true, false)).unwrap();
    assert_eq!(v["remove_from_chat_list"], true);
    assert_eq!(v["revoke"], false);
}

#[test]
fn g1_member_status_shapes_match_1_8_67() {
    // Slice G1: `chatMemberStatusRestricted is_member:Bool
    // restricted_until_date:int32 permissions:chatPermissions =
    // ChatMemberStatus` (schema 1.8.67, line 2510).
    let perms = serde_json::json!({"@type": "chatPermissions"});
    let v = chat_member_status_restricted_json(true, 1700000000, &perms);
    assert_eq!(v["@type"], "chatMemberStatusRestricted");
    assert!(v["is_member"].as_bool() == Some(true));
    assert_eq!(v["restricted_until_date"], 1700000000);
    assert_eq!(v["permissions"]["@type"], "chatPermissions");

    // Slice G1: `chatMemberStatusBanned banned_until_date:int32 =
    // ChatMemberStatus` (schema 1.8.67, line 2517).
    let v = chat_member_status_banned_json(0);
    assert_eq!(v["@type"], "chatMemberStatusBanned");
    assert_eq!(v["banned_until_date"], 0);

    // Slice G1: member-list filters (schema 1.8.67, lines 2563/2571/2574).
    assert_eq!(
        supergroup_members_filter_administrators_json()["@type"],
        "supergroupMembersFilterAdministrators"
    );
    let v = supergroup_members_filter_restricted_json("");
    assert_eq!(v["@type"], "supergroupMembersFilterRestricted");
    assert_eq!(v["query"], "");
    let v = supergroup_members_filter_banned_json("x");
    assert_eq!(v["@type"], "supergroupMembersFilterBanned");
    assert_eq!(v["query"], "x");
}

#[test]
fn g1_basic_group_full_info_shape_matches_1_8_67() {
    // Slice G1: `getBasicGroupFullInfo basic_group_id:int53 =
    // BasicGroupFullInfo` (schema 1.8.67, line 11507).
    let v: Value = serde_json::from_str(&get_basic_group_full_info(RequestId(9), 42)).unwrap();
    assert_eq!(v["@type"], "getBasicGroupFullInfo");
    assert_eq!(v["basic_group_id"], 42);
    // `addChatMember` (schema 1.8.67, line 13578).
    let v: Value = serde_json::from_str(&add_chat_member(RequestId(9), 7, 11)).unwrap();
    assert_eq!(v["@type"], "addChatMember");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["user_id"], 11);
}

#[test]
fn g1_reply_quote_shape_matches_1_8_67() {
    // Slice G1: `inputMessageReplyToMessage message_id:int53
    // quote:inputTextQuote checklist_task_id:int32 poll_option_id:string
    // = InputMessageReplyTo` (schema 1.8.67, line 3086) with
    // `inputTextQuote text:formattedText position:int32 =
    // InputTextQuote` (line 3056).
    let v = input_message_reply_to_with_quote(Some(MessageId(101)), Some(("sel", 7)));
    assert_eq!(v["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["message_id"], 101);
    assert_eq!(v["quote"]["@type"], "inputTextQuote");
    assert_eq!(v["quote"]["text"]["text"], "sel");
    assert_eq!(v["quote"]["position"], 7);

    // Whole-message reply keeps `quote: null` (existing behavior).
    let v = input_message_reply_to_with_quote(Some(MessageId(101)), None);
    assert!(v["quote"].is_null());
    assert!(input_message_reply_to_with_quote(None, Some(("sel", 7))).is_null());
}

#[test]
fn g1_send_reply_quote_rides_send_text() {
    // Slice G1: `SendReply` with a quote produces
    // `inputMessageReplyToMessage` with a populated `inputTextQuote`
    // through the `sendMessage` builder.
    let reply = SendReply {
        message_id: MessageId(101),
        quote: Some(("sel".to_string(), 7)),
        source_chat: None,
    };
    let json = send_text(
        RequestId(1),
        ChatId(7),
        None,
        "hi",
        Some(reply),
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["reply_to"]["message_id"], 101);
    assert_eq!(v["reply_to"]["quote"]["@type"], "inputTextQuote");
    assert_eq!(v["reply_to"]["quote"]["text"]["text"], "sel");
    assert_eq!(v["reply_to"]["quote"]["position"], 7);
    // Plain replies keep `quote: null`.
    let json = send_text(
        RequestId(1),
        ChatId(7),
        None,
        "hi",
        Some(SendReply::plain(MessageId(101))),
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["reply_to"]["quote"].is_null());
}

#[test]
fn external_reply_rides_send_text_with_its_quote() {
    // "Reply in Another Chat": `inputMessageReplyToExternalMessage`
    // (schema `td_api.tl:3404`) carries the source chat and the quote.
    let reply = SendReply::external(ChatId(12), MessageId(40), Some(("sel".to_string(), 7)));
    let json = send_text(
        RequestId(1),
        ChatId(7),
        None,
        "hi",
        Some(reply),
        &SendOptions::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToExternalMessage");
    assert_eq!(v["reply_to"]["chat_id"], 12);
    assert_eq!(v["reply_to"]["message_id"], 40);
    assert_eq!(v["reply_to"]["quote"]["@type"], "inputTextQuote");
    assert_eq!(v["reply_to"]["quote"]["text"]["text"], "sel");
    assert_eq!(v["reply_to"]["quote"]["position"], 7);
    assert_eq!(v["reply_to"]["checklist_task_id"], 0);
    assert_eq!(v["reply_to"]["poll_option_id"], "");
    // Without a quote the field is null, like the same-chat variant.
    let plain = input_message_reply_to_external(ChatId(12), MessageId(40), None);
    assert!(plain["quote"].is_null());
    // The constructors are in the bundled schema.
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(schema.contains(
        "inputMessageReplyToExternalMessage chat_id:int53 message_id:int53 quote:inputTextQuote checklist_task_id:int32 poll_option_id:string = InputMessageReplyTo;"
    ));
    assert!(schema.contains(
        "inputMessageReplyToMessage message_id:int53 quote:inputTextQuote checklist_task_id:int32 poll_option_id:string = InputMessageReplyTo;"
    ));
}

#[test]
fn external_reply_value_is_shared_by_every_send_builder() {
    let reply = SendReply::external(ChatId(12), MessageId(40), None);
    let value = send_reply_value(Some(&reply));
    assert_eq!(value["@type"], "inputMessageReplyToExternalMessage");
    assert!(value["quote"].is_null());
    assert!(send_reply_value(None).is_null());
    assert_eq!(
        send_reply_value(Some(&SendReply::plain(MessageId(40))))["@type"],
        "inputMessageReplyToMessage"
    );
}

#[test]
fn g1_set_chat_member_tag_shape_matches_1_8_67() {
    // Slice G1: `setChatMemberTag chat_id:int53 user_id:int53
    // tag:string = Ok` (schema 1.8.67, line 13598) — the admin
    // custom-title setter.
    let json = set_chat_member_tag(RequestId(3), ChatId(7), 42, "boss");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setChatMemberTag");
    assert_eq!(v["@extra"], "3");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["user_id"], 42);
    assert_eq!(v["tag"], "boss");
    // Empty tag clears the title.
    let json = set_chat_member_tag(RequestId(3), ChatId(7), 42, "");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["tag"], "");
}

#[test]
fn g2_toggle_sign_messages_shape_matches_1_8_67() {
    // Slice G2: `toggleSupergroupSignMessages supergroup_id:int53
    // sign_messages:Bool show_message_sender:Bool = Ok` (schema 1.8.67,
    // line 15175). `show_message_sender` is forced false when
    // `sign_messages` is false (Telegram X behavior).
    let json = toggle_supergroup_sign_messages(RequestId(3), 42, true, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleSupergroupSignMessages");
    assert_eq!(v["supergroup_id"], 42);
    assert_eq!(v["sign_messages"], true);
    assert_eq!(v["show_message_sender"], true);
    let json = toggle_supergroup_sign_messages(RequestId(3), 42, false, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["sign_messages"], false);
    assert_eq!(v["show_message_sender"], false);
}

#[test]
fn g2_toggle_anti_spam_shape_matches_1_8_67() {
    // Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled
    // supergroup_id:int53 has_aggressive_anti_spam_enabled:Bool = Ok`
    // (schema 1.8.67, line 15212).
    let json = toggle_supergroup_aggressive_anti_spam(RequestId(3), 42, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleSupergroupHasAggressiveAntiSpamEnabled");
    assert_eq!(v["supergroup_id"], 42);
    assert_eq!(v["has_aggressive_anti_spam_enabled"], true);
}

#[test]
fn g2_forum_topic_request_shapes_match_1_8_67() {
    // createForumTopic (schema 1.8.67, line 12665).
    let json = create_forum_topic(RequestId(3), ChatId(7), "Announcements");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "createForumTopic");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["name"], "Announcements");
    assert_eq!(v["is_name_implicit"], false);
    assert_eq!(v["icon"]["@type"], "forumTopicIcon");
    // editForumTopic (line 12674) — name only.
    let json = edit_forum_topic(RequestId(3), ChatId(7), 2, "News");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editForumTopic");
    assert_eq!(v["forum_topic_id"], 2);
    assert_eq!(v["name"], "News");
    assert_eq!(v["edit_icon_custom_emoji"], false);
    // toggleForumTopicIsClosed (line 12713).
    let json = toggle_forum_topic_closed(RequestId(3), ChatId(7), 2, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleForumTopicIsClosed");
    assert_eq!(v["is_closed"], true);
    // toggleForumTopicIsPinned (line 12725).
    let json = toggle_forum_topic_pinned(RequestId(3), ChatId(7), 2, true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleForumTopicIsPinned");
    assert_eq!(v["is_pinned"], true);
    // deleteForumTopic (line 12736).
    let json = delete_forum_topic(RequestId(3), ChatId(7), 2);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteForumTopic");
    assert_eq!(v["forum_topic_id"], 2);
    // toggleGeneralForumTopicIsHidden (line 12718) — General only.
    let json = toggle_general_forum_topic_hidden(RequestId(3), ChatId(7), true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleGeneralForumTopicIsHidden");
    assert_eq!(v["is_hidden"], true);
}

#[test]
fn g2_thread_boost_welcome_request_shapes_match_1_8_67() {
    // getMessageThreadHistory (schema 1.8.67, line 11839).
    let json =
        get_message_thread_history(RequestId(3), ChatId(7), MessageId(101), MessageId(0), 0, 50);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getMessageThreadHistory");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 101);
    assert_eq!(v["from_message_id"], 0);
    assert_eq!(v["offset"], 0);
    assert_eq!(v["limit"], 50);
    // Around the read position / the newer page: a negative offset.
    let json = get_message_thread_history(
        RequestId(3),
        ChatId(7),
        MessageId(101),
        MessageId(44),
        -25,
        50,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["from_message_id"], 44);
    assert_eq!(v["offset"], -25);
    // getChatBoostStatus (line 13917) / getAvailableChatBoostSlots
    // (line 13914) / boostChat (line 13922).
    let json = get_chat_boost_status(RequestId(3), ChatId(7));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatBoostStatus");
    let json = get_chat_boost_link_info(RequestId(3), "https://t.me/c/1/?boost");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatBoostLinkInfo");
    assert_eq!(v["url"], "https://t.me/c/1/?boost");
    let json = get_available_chat_boost_slots(RequestId(3));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getAvailableChatBoostSlots");
    let json = boost_chat(RequestId(3), ChatId(7), &[1, 2]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "boostChat");
    assert_eq!(v["slot_ids"], serde_json::json!([1, 2]));
    // loadChatWelcomeMessages (line 12630) /
    // addChatWelcomeMessage (line 12639) /
    // editChatWelcomeMessage (line 12646) /
    // deleteChatWelcomeMessage (line 12651).
    let json = load_chat_welcome_messages(RequestId(3), ChatId(7));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "loadChatWelcomeMessages");
    let json = add_chat_welcome_message(RequestId(3), ChatId(7), "Welcome!");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "addChatWelcomeMessage");
    assert_eq!(v["input_message_content"]["text"]["text"], "Welcome!");
    let json = edit_chat_welcome_message(RequestId(3), ChatId(7), 5, "Hello!");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editChatWelcomeMessage");
    assert_eq!(v["welcome_message_id"], 5);
    let json = delete_chat_welcome_message(RequestId(3), ChatId(7), 5);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteChatWelcomeMessage");
    assert_eq!(v["welcome_message_id"], 5);
}
