//! Parses TDLib objects for groups and channels: members, admin rights, invite links, join requests, boosts, communities.
use crate::ids::ChatId;
use crate::telegram::envelope::*;
use serde_json::Value;

/// The groups domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_groups_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        // `canTransferOwnership` answer (schema 1.8.67, line 8568).
        "canTransferOwnershipResultOk"
        | "canTransferOwnershipResultPasswordNeeded"
        | "canTransferOwnershipResultPasswordTooFresh"
        | "canTransferOwnershipResultSessionTooFresh" => {
            let result =
                parse_can_transfer_ownership_result(value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Groups(
                GroupsPayload::CanTransferOwnershipResult { result },
            ))
        }
        // Phase 5.1: `updateSupergroup` (schema line 10738) and the
        // `getSupergroup` response both carry `supergroup.is_forum` (schema
        // line 2746). Parity slice: also keep the first active username
        // (`supergroup.usernames`, schema lines 2746/2372) for the
        // channel/supergroup header.
        "updateBasicGroup" => {
            let group = value.get("basic_group").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Groups(GroupsPayload::UpdateBasicGroup {
                basic_group_id: int53(group.get("id"))?,
                member_count: int53(group.get("member_count")).unwrap_or(0).sat_i32(),
                status: parse_channel_member_status(group.get("status"))
                    .map(|(status, _)| status)
                    .unwrap_or(ChannelMemberStatus::Unknown),
                can_restrict_members: parse_restrict_members_right(group.get("status"))
                    .unwrap_or(false),
                can_promote_members: parse_promote_members_right(group.get("status"))
                    .unwrap_or(false),
                can_manage_tags: parse_manage_tags_right(group.get("status")).unwrap_or(false),
                can_change_info: parse_change_info_right(group.get("status")),
                is_active: group
                    .get("is_active")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            }))
        }
        "updateSupergroup" => {
            let supergroup = value.get("supergroup").ok_or(ParseError::MissingField)?;
            let verification_flag = |name: &str| {
                supergroup
                    .get("verification_status")
                    .and_then(|v| v.get(name))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            };
            Ok(EnvelopePayload::Groups(GroupsPayload::UpdateSupergroup {
                supergroup_id: int53(supergroup.get("id"))?,
                verification: crate::peer_badge::VerificationStatus {
                    is_verified: verification_flag("is_verified"),
                    is_scam: verification_flag("is_scam"),
                    is_fake: verification_flag("is_fake"),
                },
                member_count: int53(supergroup.get("member_count")).unwrap_or(0).sat_i32(),
                is_forum: supergroup
                    .get("is_forum")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                has_forum_tabs: supergroup
                    .get("has_forum_tabs")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                has_automatic_translation: supergroup
                    .get("has_automatic_translation")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                username: parse_first_active_username(supergroup.get("usernames")),
                usernames: parse_supergroup_usernames(supergroup.get("usernames")),
                // Phase A1: own `chatMemberStatus*` (schema 1.8.67 line
                // 2746); unknown/missing → `Unknown` (gated, no bypass).
                // `can_restrict_members` gates the slow-mode admin control
                // (schema line 13551).
                status: parse_channel_member_status(supergroup.get("status"))
                    .map(|(status, _)| status)
                    .unwrap_or(ChannelMemberStatus::Unknown),
                can_restrict_members: parse_restrict_members_right(supergroup.get("status")),
                can_invite_users: parse_invite_users_right(supergroup.get("status")),
                can_promote_members: parse_promote_members_right(supergroup.get("status")),
                can_manage_tags: parse_manage_tags_right(supergroup.get("status")),
                // Slice G2: forum-topic / sign-messages / welcome-message
                // rights (schema 1.8.67, lines 1090/1092).
                can_manage_topics: parse_manage_topics_right(supergroup.get("status")),
                can_change_info: parse_change_info_right(supergroup.get("status")),
                can_send_welcome_messages: parse_send_welcome_messages_right(
                    supergroup.get("status"),
                ),
                // Slice G1: `supergroup.join_by_request` /
                // `supergroup.is_broadcast_group` (schema 1.8.67, lines
                // 2733/2736/2746).
                join_by_request: supergroup
                    .get("join_by_request")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_broadcast_group: supergroup
                    .get("is_broadcast_group")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                // Slice G2: `supergroup.sign_messages` /
                // `supergroup.show_message_sender` (schema 1.8.67, lines
                // 2731/2746).
                sign_messages: supergroup
                    .get("sign_messages")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                show_message_sender: supergroup
                    .get("show_message_sender")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                // B7: `supergroup.join_to_send_messages` (schema 1.8.67,
                // line 2746).
                join_to_send_messages: supergroup
                    .get("join_to_send_messages")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }))
        }
        "supergroup" => Ok(EnvelopePayload::Groups(GroupsPayload::Supergroup {
            supergroup_id: int53(value.get("id"))?,
            is_forum: value
                .get("is_forum")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_forum_tabs: value
                .get("has_forum_tabs")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_automatic_translation: value
                .get("has_automatic_translation")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            username: parse_first_active_username(value.get("usernames")),
            // Phase A1: own `chatMemberStatus*` (schema 1.8.67 line 2746).
            status: parse_channel_member_status(value.get("status"))
                .map(|(status, _)| status)
                .unwrap_or(ChannelMemberStatus::Unknown),
            can_restrict_members: parse_restrict_members_right(value.get("status")),
            can_invite_users: parse_invite_users_right(value.get("status")),
            can_promote_members: parse_promote_members_right(value.get("status")),
            can_manage_tags: parse_manage_tags_right(value.get("status")),
            // Slice G2: forum-topic / sign-messages / welcome-message
            // rights (schema 1.8.67, lines 1090/1092).
            can_manage_topics: parse_manage_topics_right(value.get("status")),
            can_change_info: parse_change_info_right(value.get("status")),
            can_send_welcome_messages: parse_send_welcome_messages_right(value.get("status")),
            // Slice G1: `supergroup.join_by_request` /
            // `supergroup.is_broadcast_group` (schema 1.8.67, lines
            // 2733/2736/2746).
            join_by_request: value
                .get("join_by_request")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_broadcast_group: value
                .get("is_broadcast_group")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice G2: `supergroup.sign_messages` /
            // `supergroup.show_message_sender` (schema 1.8.67, lines
            // 2731/2746).
            sign_messages: value
                .get("sign_messages")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            show_message_sender: value
                .get("show_message_sender")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            join_to_send_messages: value
                .get("join_to_send_messages")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })),
        "updateChatMember" => {
            let member = value
                .get("new_chat_member")
                .and_then(|m| parse_chat_member(Some(m)))
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Groups(GroupsPayload::UpdateChatMember {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                member,
            }))
        }
        "chatMember" => {
            let member = parse_chat_member(Some(value)).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Groups(GroupsPayload::ChatMember {
                member,
            }))
        }
        "supergroupFullInfo" => Ok(EnvelopePayload::Groups(GroupsPayload::SupergroupFullInfo {
            description: value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            member_count: value
                .get("member_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
            // Parity slice: `linked_chat_id` (schema 1.8.67, line 2792) —
            // the discussion-group chat id (0 = none).
            linked_chat_id: int53_or_zero(value.get("linked_chat_id")),
            // Phase A1: slow-mode fields (schema 1.8.67, lines 2758–2759)
            // plus the boost bypass counts (lines 2779–2780). The expiry is
            // `double` in the schema; `as_f64` accepts integer JSON too.
            slow_mode_delay: value
                .get("slow_mode_delay")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
            slow_mode_delay_expires_in: value
                .get("slow_mode_delay_expires_in")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            my_boost_count: value
                .get("my_boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
            unrestrict_boost_count: value
                .get("unrestrict_boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
            // Phase D2: `can_get_statistics` (schema 1.8.67, line 2792) —
            // gates the statistics entry point in the info panel.
            can_get_statistics: value
                .get("can_get_statistics")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice G2: anti-spam fields (schema 1.8.67, line 2792).
            has_aggressive_anti_spam_enabled: value
                .get("has_aggressive_anti_spam_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_toggle_aggressive_anti_spam: value
                .get("can_toggle_aggressive_anti_spam")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // Slice S11: sticker-set fields (schema 1.8.67, lines 2765 and
            // 2792); int64 ids arrive as JSON strings.
            can_set_sticker_set: value
                .get("can_set_sticker_set")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sticker_set_id: int64(value.get("sticker_set_id")).unwrap_or(0),
            custom_emoji_sticker_set_id: int64(value.get("custom_emoji_sticker_set_id"))
                .unwrap_or(0),
            admin: parse_supergroup_full_admin(Some(value)),
        })),
        // Parity slice: `updateSupergroupFullInfo` (schema 1.8.67, line
        // 10750) — same fields as the `supergroupFullInfo` response, with
        // an explicit `supergroup_id` so no pending-request correlation
        // is needed.
        "updateSupergroupFullInfo" => Ok(EnvelopePayload::Groups(
            GroupsPayload::UpdateSupergroupFullInfo {
                supergroup_id: int53(value.get("supergroup_id"))?,
                description: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("description"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                member_count: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("member_count"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                linked_chat_id: int53_or_zero(
                    value
                        .get("supergroup_full_info")
                        .and_then(|info| info.get("linked_chat_id")),
                ),
                // Phase A1: slow-mode + boost fields (schema 1.8.67,
                // lines 2758–2759 / 2779–2780), nested like the other fields.
                slow_mode_delay: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("slow_mode_delay"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                slow_mode_delay_expires_in: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("slow_mode_delay_expires_in"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                my_boost_count: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("my_boost_count"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                unrestrict_boost_count: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("unrestrict_boost_count"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                // Phase D2: `can_get_statistics` (schema 1.8.67, line 2792),
                // nested like the other fields.
                can_get_statistics: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("can_get_statistics"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                // Slice G2: anti-spam fields (schema 1.8.67, line 2792),
                // nested like the other fields.
                has_aggressive_anti_spam_enabled: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("has_aggressive_anti_spam_enabled"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                can_toggle_aggressive_anti_spam: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("can_toggle_aggressive_anti_spam"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                // Slice S11: sticker-set fields (schema 1.8.67, lines 2765 and
                // 2792), nested like the other fields.
                can_set_sticker_set: value
                    .get("supergroup_full_info")
                    .and_then(|info| info.get("can_set_sticker_set"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                sticker_set_id: int64(
                    value
                        .get("supergroup_full_info")
                        .and_then(|info| info.get("sticker_set_id")),
                )
                .unwrap_or(0),
                custom_emoji_sticker_set_id: int64(
                    value
                        .get("supergroup_full_info")
                        .and_then(|info| info.get("custom_emoji_sticker_set_id")),
                )
                .unwrap_or(0),
                admin: parse_supergroup_full_admin(value.get("supergroup_full_info")),
            },
        )),
        // Slice (communities backend core): `updateCommunity` (schema
        // 1.8.67, line 10726) — the update carries the full `community`
        // object; guaranteed to come before the community identifier is
        // returned, so no pending-request correlation is needed.
        "updateCommunity" => Ok(EnvelopePayload::Groups(GroupsPayload::UpdateCommunity {
            community: value
                .get("community")
                .and_then(parse_community)
                .ok_or(ParseError::MissingField)?,
        })),
        // Slice (communities backend core): `updateCommunityFullInfo`
        // (schema 1.8.68, line 11116) — sent whenever the pack changes.
        // Carries its own `community_id`.
        "updateCommunityFullInfo" => Ok(EnvelopePayload::Groups(
            GroupsPayload::UpdateCommunityFullInfo {
                community_id: int53(value.get("community_id"))?,
                full_info: value
                    .get("community_full_info")
                    .and_then(parse_community_full_info)
                    .ok_or(ParseError::MissingField)?,
            },
        )),
        // TDLib 1.8.68: `getCommunityFullInfo` answers the pack directly
        // (no community id — correlated through the pending request).
        "communityFullInfo" => Ok(EnvelopePayload::Groups(GroupsPayload::CommunityFullInfo {
            full_info: parse_community_full_info(value).ok_or(ParseError::MissingField)?,
        })),
        // Slice (communities backend core): `communityId` (schema 1.8.67,
        // line 2264) — the `createCommunity` response (line 11806).
        "communityId" => Ok(EnvelopePayload::Groups(GroupsPayload::CommunityId {
            id: int53(value.get("id"))?,
        })),
        // Phase D2: `getChatStatistics` response (schema 1.8.67, line
        // 15760) — `chatStatisticsChannel` / `chatStatisticsSupergroup`.
        // The response carries no chat id; `Session::apply` correlates it
        // via the pending `GetChatStatistics` request.
        "chatStatisticsChannel" | "chatStatisticsSupergroup" => {
            Ok(EnvelopePayload::Groups(GroupsPayload::ChatStatistics {
                statistics: parse_chat_statistics(value)?,
            }))
        }
        // Slice G2: welcome-message updates (schema 1.8.67, lines
        // 10599/10649) and boost responses (lines 6943/6968).
        "updateChatWelcomeMessages" => Ok(EnvelopePayload::Groups(
            GroupsPayload::UpdateChatWelcomeMessages {
                chat_id: int53(value.get("chat_id"))?,
                messages: value
                    .get("messages")
                    .and_then(Value::as_array)
                    .map(|messages| messages.iter().filter_map(parse_welcome_message).collect())
                    .unwrap_or_default(),
            },
        )),
        "updateChatHasWelcomeMessages" => Ok(EnvelopePayload::Groups(
            GroupsPayload::UpdateChatHasWelcomeMessages {
                chat_id: int53(value.get("chat_id"))?,
                has_welcome_messages: value
                    .get("has_welcome_messages")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "chatBoostStatus" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatBoostStatus {
            level: value
                .get("level")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
            boost_count: value
                .get("boost_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
        })),
        "chatBoostSlots" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatBoostSlots {
            slots: value
                .get("slots")
                .and_then(Value::as_array)
                .map(|slots| {
                    slots
                        .iter()
                        .filter_map(|slot| slot.get("slot_id").and_then(Value::as_i64))
                        .map(|id| id.sat_i32())
                        .collect()
                })
                .unwrap_or_default(),
        })),
        // Phase D3a: invite-link / join-request responses and updates
        // (schema 1.8.67, lines 2627/2630/2688/2691/10555/11210). The
        // responses carry no chat id; `Session::apply` correlates them via
        // the pending request. The updates carry their own `chat_id`.
        // Phase D3b: admin-list / member-list responses (schema 1.8.67,
        // lines 2485/2529) — same correlation, no chat id on the wire.
        "chatAdministrators" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatAdministrators {
            administrators: value
                .get("administrators")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| parse_chat_administrator(Some(entry)))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        // Phase D3c: `chatEvents` (schema 1.8.67, line 7938) — the
        // `getChatEventLog` response. Events whose actor fails to parse
        // are dropped in `parse_chat_event`, never misattributed.
        "chatEvents" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatEvents {
            events: value
                .get("events")
                .and_then(Value::as_array)
                .map(|events| events.iter().filter_map(parse_chat_event).collect())
                .unwrap_or_default(),
        })),
        "chatMembers" => Ok(EnvelopePayload::Groups(GroupsPayload::SupergroupMembers {
            total_count: int53(value.get("total_count")).map(|v| v.sat_i32())?,
            members: value
                .get("members")
                .and_then(Value::as_array)
                .map(|members| {
                    members
                        .iter()
                        .filter_map(|member| parse_chat_member(Some(member)))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        "chatInviteLink" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatInviteLink {
            link: parse_chat_invite_link(Some(value)).ok_or(ParseError::MissingField)?,
        })),
        // Checked invite preview; joining requires a separate confirmation.
        "chatInviteLinkInfo" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatInviteLinkInfo {
            title: json_field_str(value, "title"),
            member_count: int53(value.get("member_count")).unwrap_or(0).sat_i32(),
            creates_join_request: value
                .get("creates_join_request")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_channel: value
                .get("type")
                .and_then(|t| t.get("@type"))
                .and_then(Value::as_str)
                == Some("inviteLinkChatTypeChannel"),
        })),
        "chatInviteLinks" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatInviteLinks {
            total_count: int53(value.get("total_count")).map(|v| v.sat_i32())?,
            links: value
                .get("invite_links")
                .and_then(Value::as_array)
                .map(|links| {
                    links
                        .iter()
                        .filter_map(|link| parse_chat_invite_link(Some(link)))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        "foundChatBoosts" => Ok(EnvelopePayload::Groups(GroupsPayload::FoundChatBoosts {
            total_count: int53(value.get("total_count")).map(|v| v.sat_i32())?,
            boosts: value
                .get("boosts")
                .and_then(Value::as_array)
                .map(|boosts| {
                    boosts
                        .iter()
                        .filter_map(|boost| parse_chat_boost(Some(boost)))
                        .collect()
                })
                .unwrap_or_default(),
            next_offset: value
                .get("next_offset")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        })),
        "chatBoostLink" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatBoostLink {
            link: value
                .get("link")
                .and_then(Value::as_str)
                .ok_or(ParseError::MissingField)?
                .to_owned(),
            is_public: value
                .get("is_public")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })),
        "chatInviteLinkCounts" => Ok(EnvelopePayload::Groups(
            GroupsPayload::ChatInviteLinkCounts {
                counts: value
                    .get("invite_link_counts")
                    .and_then(Value::as_array)
                    .map(|counts| {
                        counts
                            .iter()
                            .filter_map(|count| parse_chat_invite_link_count(Some(count)))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        )),
        "chatInviteLinkMembers" => Ok(EnvelopePayload::Groups(
            GroupsPayload::ChatInviteLinkMembers {
                total_count: int53(value.get("total_count")).map(|v| v.sat_i32())?,
                members: value
                    .get("members")
                    .and_then(Value::as_array)
                    .map(|members| {
                        members
                            .iter()
                            .filter_map(|member| parse_chat_invite_link_member(Some(member)))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        )),
        "chatJoinRequests" => Ok(EnvelopePayload::Groups(GroupsPayload::ChatJoinRequests {
            total_count: int53(value.get("total_count")).map(|v| v.sat_i32())?,
            requests: value
                .get("requests")
                .and_then(Value::as_array)
                .map(|requests| {
                    requests
                        .iter()
                        .filter_map(|request| parse_chat_join_request(Some(request)))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        // Slice G1: `createdBasicGroupChat` (schema 1.8.67, line 3644).
        "createdBasicGroupChat" => Ok(EnvelopePayload::Groups(
            GroupsPayload::CreatedBasicGroupChat {
                chat_id: int53(value.get("chat_id"))?,
            },
        )),
        // Slice G1: `failedToAddMembers` (schema 1.8.67, line 3640).
        "failedToAddMembers" => Ok(EnvelopePayload::Groups(GroupsPayload::FailedToAddMembers {
            failed_count: value
                .get("failed_to_add_members")
                .and_then(Value::as_array)
                .map(|members| members.len() as i32)
                .unwrap_or(0),
        })),
        // Slice G1: `basicGroupFullInfo` (schema 1.8.67, line 2714).
        "basicGroupFullInfo" => Ok(EnvelopePayload::Groups(GroupsPayload::BasicGroupFullInfo {
            members: value
                .get("members")
                .and_then(Value::as_array)
                .map(|members| {
                    members
                        .iter()
                        .filter_map(|member| parse_chat_member(Some(member)))
                        .collect()
                })
                .unwrap_or_default(),
        })),
        "updateNewChatJoinRequest" => Ok(EnvelopePayload::Groups(
            GroupsPayload::UpdateNewChatJoinRequest {
                chat_id: int53(value.get("chat_id"))?,
                request: parse_chat_join_request(value.get("request"))
                    .ok_or(ParseError::MissingField)?,
                user_chat_id: int53(value.get("user_chat_id"))?,
                invite_link: parse_chat_invite_link(value.get("invite_link"))
                    .ok_or(ParseError::MissingField)?,
                query_id: int53(value.get("query_id"))?,
            },
        )),
        "updateChatPendingJoinRequests" => {
            let pending = value
                .get("pending_join_requests")
                .filter(|v| v.get("@type").and_then(Value::as_str) == Some("chatJoinRequestsInfo"));
            Ok(EnvelopePayload::Groups(
                GroupsPayload::UpdateChatPendingJoinRequests {
                    chat_id: int53(value.get("chat_id"))?,
                    total_count: int53(pending.and_then(|v| v.get("total_count")))
                        .map(|v| v.sat_i32())?,
                    user_ids: pending
                        .and_then(|v| v.get("user_ids"))
                        .and_then(Value::as_array)
                        .map(|ids| ids.iter().filter_map(|id| int53(Some(id)).ok()).collect())
                        .unwrap_or_default(),
                },
            ))
        }
        "chatJoinResultSuccess" => Ok(EnvelopePayload::Groups(GroupsPayload::JoinChatResult(
            ChatJoinResult::Success {
                chat_id: ChatId(int53(value.get("chat_id"))?),
            },
        ))),
        "chatJoinResultRequestSent" => Ok(EnvelopePayload::Groups(GroupsPayload::JoinChatResult(
            ChatJoinResult::RequestSent,
        ))),
        "chatJoinResultGuardBotApprovalRequired" => Ok(EnvelopePayload::Groups(
            GroupsPayload::JoinChatResult(ChatJoinResult::GuardBotApprovalRequired),
        )),
        "chatJoinResultDeclined" => Ok(EnvelopePayload::Groups(GroupsPayload::JoinChatResult(
            ChatJoinResult::Declined,
        ))),
        _ => return Ok(None),
    };
    payload.map(Some)
}
