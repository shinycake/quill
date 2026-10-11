//! Driver tests: group members, member tags, group info, sticker sets and communities.
use super::*;

#[test]
fn driver_fetch_basic_group_members_shape() {
    // Slice G1: `getBasicGroupFullInfo` (schema 1.8.67, line 11507)
    // for a basic group chat; the answer populates
    // `basic_group_members`.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let extra = driver
        .fetch_basic_group_members(ChatId(9))
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "getBasicGroupFullInfo");
    assert_eq!(v["basic_group_id"], 3);
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"basicGroupFullInfo","@extra":"{}","members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":7}},"status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let fetch = driver.session.groups.basic_group_members.get(&9).unwrap();
    match fetch {
        SupergroupMembersFetch::Loaded { members, .. } => assert_eq!(members.len(), 1),
        other => panic!("unexpected {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_set_chat_member_tag_gates_and_shape() {
    // Slice G1: `setChatMemberTag` (schema 1.8.67, line 13598) —
    // owner of a group may retitle; channels are rejected; tags
    // over 16 characters are rejected client-side.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Not the owner → no request.
    assert!(
        driver
            .set_chat_member_tag(ChatId(9), 42, "boss")
            .unwrap()
            .is_none()
    );
    // Owner → sends `setChatMemberTag`.
    driver.session.my_user_id = Some(7);
    driver.session.chats.get_mut(&9).unwrap().my_member_status = Some(ChannelMemberStatus::Creator);
    let extra = driver
        .set_chat_member_tag(ChatId(9), 42, "boss")
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatMemberTag");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 9);
    assert_eq!(v["user_id"], 42);
    assert_eq!(v["tag"], "boss");
    // Over-long tag rejected without sending.
    assert!(
        driver
            .set_chat_member_tag(ChatId(9), 42, "this title is way too long")
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_group_info_edit_gates_and_shape() {
    // Slice: `setChatTitle` / `setChatDescription` / `setChatPhoto`
    // (schema 1.8.67, lines 13430/13533/13435) — basic groups,
    // supergroups and channels. Basic groups are democratic: any
    // member may edit (telegram.org/blog/supergroups), no right
    // needed. Supergroups and channels are gated on `can_change_info`
    // — creator, an admin with the right, or a plain member with
    // the default `permissions.can_change_info`. Client length
    // validation refuses invalid input before sending; a TDLib
    // `ok` is handled by the generic pending-request path — no
    // optimistic state.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":10,"title":"Supergroup","type":{"@type":"chatTypeSupergroup","supergroup_id":10,"is_channel":false},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Unknown chat id → refused without sending.
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .set_group_title(ChatId(404), "New")
            .unwrap()
            .is_none()
    );
    // Private chat (wrong kind) → refused without sending.
    assert!(
        driver
            .set_group_description(ChatId(7), "about")
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Plain member of a basic group → sends: basic groups are
    // democratic — no right needed.
    let extra = driver
        .set_group_photo(ChatId(9), None)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(v["photo"].is_null());
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetChatPhoto);
    // Supergroup plain member without `permissions.can_change_info`
    // → refused.
    let sent_before = recorder.snapshot().len();
    assert!(driver.set_group_photo(ChatId(10), None).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Supergroup plain member WITH the default
    // `permissions.can_change_info` → sends.
    driver
        .session
        .chats
        .get_mut(&10)
        .unwrap()
        .permissions
        .as_mut()
        .unwrap()
        .can_change_info = true;
    let extra = driver
        .set_group_photo(ChatId(10), None)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(v["photo"].is_null());
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetChatPhoto);
    // Creator → sends `setChatTitle`.
    driver.session.my_user_id = Some(7);
    driver.session.chats.get_mut(&9).unwrap().my_member_status = Some(ChannelMemberStatus::Creator);
    let extra = driver
        .set_group_title(ChatId(9), "New name")
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatTitle");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 9);
    assert_eq!(v["title"], "New name");
    // In-flight dedup: second title while one is pending → no-op.
    assert!(
        driver
            .set_group_title(ChatId(9), "Another")
            .unwrap()
            .is_none()
    );
    // Length validation before send: empty and 129 chars refused,
    // exactly 128 accepted by the builder (dedup keeps it unsent).
    assert!(driver.set_group_title(ChatId(9), "").is_err());
    assert!(driver.set_group_title(ChatId(9), &"x".repeat(129)).is_err());
    // 255-char description sends; 256 is refused.
    let extra = driver
        .set_group_description(ChatId(9), &"y".repeat(255))
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatDescription");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["description"], "y".repeat(255));
    assert!(
        driver
            .set_group_description(ChatId(9), &"y".repeat(256))
            .is_err()
    );
    // Photo set → `inputChatPhotoStatic` / `inputFileLocal`.
    let extra = driver
        .set_group_photo(ChatId(9), Some("/tmp/group.jpg"))
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["photo"]["@type"], "inputChatPhotoStatic");
    assert_eq!(v["photo"]["photo"]["@type"], "inputFileLocal");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/group.jpg");
    // Photo delete → null top-level `photo`.
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetChatPhoto);
    let extra = driver
        .set_group_photo(ChatId(9), None)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(v["photo"].is_null());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_group_sticker_set_gates_and_shape() {
    // Slice S11: `setSupergroupStickerSet` /
    // `setSupergroupCustomEmojiStickerSet` (schema 1.8.67, lines
    // 15154/15159). Gated on `supergroupFullInfo.can_set_sticker_set`
    // (fail closed while unfetched); unknown chats and non-supergroup
    // chats refused without sending; negative ids refused
    // client-side; 0 removes per the schema; in-flight dedup per chat.
    // Not optimistic — the server confirms via
    // `updateSupergroupFullInfo`.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":10,"title":"Supergroup","type":{"@type":"chatTypeSupergroup","supergroup_id":10,"is_channel":false},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Unknown chat → refused without sending.
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(404), 5)
            .unwrap()
            .is_none()
    );
    // Private chat (wrong kind) → refused.
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(7), 5)
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Full info unfetched → capability gate fails closed.
    assert!(driver.load_group_sticker_choices(ChatId(10)).is_err());
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(10), 5)
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Negative id → invalid request.
    assert!(driver.set_supergroup_sticker_set(ChatId(10), -1).is_err());
    // Seed the capability, then sends.
    driver.session.groups.supergroup_full_infos.insert(
        10,
        SupergroupFullInfoData {
            can_set_sticker_set: true,
            ..Default::default()
        },
    );
    driver.load_group_sticker_choices(ChatId(10)).unwrap();
    let requests: Vec<Value> = recorder
        .snapshot()
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .filter(|v: &Value| v["@type"] == "getInstalledStickerSets")
        .collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["sticker_type"]["@type"], "stickerTypeRegular");
    assert_eq!(
        requests[1]["sticker_type"]["@type"],
        "stickerTypeCustomEmoji"
    );
    let sent = recorder.snapshot().len();
    driver.load_group_sticker_choices(ChatId(10)).unwrap();
    assert_eq!(recorder.snapshot().len(), sent);
    assert!(!driver.session.stickers.open && !driver.session.emoji.open);
    let extra = driver
        .set_supergroup_sticker_set(ChatId(10), 1234567890123)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setSupergroupStickerSet");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["supergroup_id"], 10);
    assert_eq!(v["sticker_set_id"], "1234567890123");
    // In-flight dedup: second call while one is pending → no-op.
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(10), 6)
            .unwrap()
            .is_none()
    );
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetSupergroupStickerSet);
    // 0 removes the group sticker set per the schema.
    let extra = driver
        .set_supergroup_sticker_set(ChatId(10), 0)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["sticker_set_id"], "0");
    assert_eq!(v["@extra"], extra.0.to_string());
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetSupergroupStickerSet);
    // Custom-emoji variant: same gating, shape, and remove encoding.
    assert!(
        driver
            .set_supergroup_custom_emoji_sticker_set(ChatId(7), 5)
            .unwrap()
            .is_none()
    );
    let extra = driver
        .set_supergroup_custom_emoji_sticker_set(ChatId(10), 9876543210987)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setSupergroupCustomEmojiStickerSet");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["supergroup_id"], 10);
    assert_eq!(v["custom_emoji_sticker_set_id"], "9876543210987");
    assert_eq!(
        driver
            .session
            .supergroup_full_info(10)
            .unwrap()
            .custom_emoji_sticker_set_id,
        0
    );
    driver.ingest(copy_and_parse(&serde_json::json!({"@type":"error","@extra":extra.as_extra(),"code":403,"message":"private body"}).to_string(), &seq, &dyn_sink).unwrap()).unwrap();
    assert!(
        driver
            .session
            .chats_state
            .chat_action_error
            .as_deref()
            .is_some_and(|e| e.contains("Could not change") && !e.contains("private body"))
    );
    assert_eq!(
        driver
            .session
            .supergroup_full_info(10)
            .unwrap()
            .custom_emoji_sticker_set_id,
        0
    );
    assert!(
        driver
            .set_supergroup_custom_emoji_sticker_set(ChatId(10), 0)
            .unwrap()
            .is_some()
    );
    assert!(
        driver
            .set_supergroup_custom_emoji_sticker_set(ChatId(10), -1)
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_community_create_rename_refetch_chain() {
    // Slice (communities backend core): `createCommunity` /
    // `getCommunityFullInfo` (TDLib 1.8.68) / `setCommunityName`. Empty
    // names are refused client-side; unknown chats are refused without
    // sending; in-flight dedupe is per (purpose, chat) / (purpose,
    // community). The `communityId` answer chains into
    // `getCommunityFullInfo`,
    // and a confirmed `setCommunityName` refetches the dropped
    // full-info pack (the welcome-message-mutation pattern).
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Unknown chat → refused without sending.
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .create_community(ChatId(404), "Rustaceans", false)
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Empty / whitespace-only name → InvalidRequest.
    assert!(driver.create_community(ChatId(9), "", false).is_err());
    assert!(driver.create_community(ChatId(9), "   ", false).is_err());
    // Sends `createCommunity` with the right shape.
    let extra = driver
        .create_community(ChatId(9), "Rustaceans", true)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "createCommunity");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["name"], "Rustaceans");
    assert_eq!(v["chat_id"], 9);
    assert!(v["is_chat_hidden"].as_bool() == Some(true));
    // In-flight dedup: second create while one is pending → no-op.
    assert!(
        driver
            .create_community(ChatId(9), "Other", false)
            .unwrap()
            .is_none()
    );
    // `updateCommunity` (guaranteed before the `communityId` answer)
    // creates the community; the `communityId` then chains into
    // `getCommunityFullInfo`.
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1759000000}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.groups.communities.contains_key(&42));
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"communityId","id":42,"@extra":"{}"}}"#,
                    extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "getCommunityFullInfo");
    assert_eq!(v["community_id"], 42);
    // In flight → a second fetch is a no-op.
    let sent_before = recorder.snapshot().len();
    assert!(driver.get_community_full_info(42).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);
    // TDLib 1.8.68 answers the pack itself (no community id); the
    // pending request routes it to community 42.
    let load_extra = v["@extra"].as_str().expect("@extra").to_string();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"communityFullInfo","@extra":"{load_extra}","photo":null,"chats":[],"administrator_count":1,"banned_count":0,"add_chat_request_count":0}}"#
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        driver
            .session
            .groups
            .community_full_infos
            .get(&42)
            .map(|info| info.administrator_count),
        Some(1)
    );
    let sent_before = recorder.snapshot().len();
    assert!(driver.get_community_full_info(42).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);
    // `setCommunityName`: empty refused, shape right, deduped in
    // flight.
    assert!(driver.set_community_name(42, "").is_err());
    let extra = driver
        .set_community_name(42, "Rustaceans+")
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setCommunityName");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["community_id"], 42);
    assert_eq!(v["name"], "Rustaceans+");
    assert!(driver.set_community_name(42, "Again").unwrap().is_none());
    // `ok` drops the pack and the ingest refetches it.
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(!driver.session.groups.community_full_infos.contains_key(&42));
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "getCommunityFullInfo");
    assert_eq!(v["community_id"], 42);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_community_management_rights_and_delete() {
    // TDLib 1.8.68: `setCommunityPermissions` (can_ban_members),
    // `setCommunityPhoto` (can_change_info) and `deleteCommunity`
    // (owner). Missing rights are refused without sending; a confirmed
    // delete drops the community and its pack; errors surface in
    // `community_error`.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    // 42: owned. 43: plain member. 44: admin with can_ban_members only.
    ingest(
        &mut driver,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Mine","date":1,"status":{"@type":"communityMemberStatusCreator"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false}}}"#,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":43,"have_access":true,"name":"Theirs","date":1,"status":{"@type":"communityMemberStatusMember"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false}}}"#,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":44,"have_access":true,"name":"Moderated","date":1,"status":{"@type":"communityMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"communityAdministratorRights","can_manage_community":true,"can_change_info":false,"can_edit_chat_list":false,"can_promote_members":false,"can_ban_members":true}},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false}}}"#,
    );
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .set_community_permissions(43, true)
            .unwrap()
            .is_none()
    );
    assert!(driver.set_community_photo(43, None).unwrap().is_none());
    assert!(driver.set_community_photo(44, None).unwrap().is_none());
    assert!(driver.delete_community(43).unwrap().is_none());
    assert!(driver.delete_community(44).unwrap().is_none());
    assert!(driver.delete_community(404).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);

    // Admin with can_ban_members may change member permissions.
    let extra = driver
        .set_community_permissions(44, true)
        .unwrap()
        .expect("sent");
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["@type"], "setCommunityPermissions");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["permissions"]["can_edit_chat_list"], true);
    assert!(
        driver
            .set_community_permissions(44, false)
            .unwrap()
            .is_none()
    );
    // A refusal surfaces in the status line.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"Have not enough rights"}}"#,
            extra.0
        ),
    );
    let err = driver
        .session
        .groups
        .community_error
        .take()
        .expect("error surfaced");
    assert!(
        err.starts_with("Could not change the community permissions"),
        "{err}"
    );

    // Owner: photo (set + delete) and delete.
    driver
        .set_community_photo(42, Some("/tmp/photo.jpg"))
        .unwrap()
        .expect("sent");
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["@type"], "setCommunityPhoto");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/photo.jpg");
    let extra = driver.delete_community(42).unwrap().expect("sent");
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["@type"], "deleteCommunity");
    assert_eq!(v["community_id"], 42);
    driver.session.groups.community_full_infos.insert(
        42,
        crate::telegram::envelope::ParsedCommunityFullInfo {
            chats: Vec::new(),
            administrator_count: 1,
            banned_count: 0,
            add_chat_request_count: 0,
        },
    );
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!driver.session.groups.communities.contains_key(&42));
    assert!(!driver.session.groups.community_full_infos.contains_key(&42));
    assert!(driver.session.groups.communities.contains_key(&43));
    let _ = std::fs::remove_dir_all(&dir);
}
