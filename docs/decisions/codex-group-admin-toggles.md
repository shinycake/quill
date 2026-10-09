# Group and channel admin toggles (B7)

## What tdesktop does
- `edit_peer_info_box.cpp` `fillManageSection`: the Topics row (megagroup creator, or a basic group creator; locked below `forum_upgrade_participants_min` = 200 members and for a channel's discussion group), the Chat history for new members row (`canEditPreHistoryHidden`: megagroup creator or admin with ban rights, hidden for public groups, groups with a location, discussion groups and forums; basic group creator), the Reactions row (creator or `ChangeInfo`), and the discussion link row (channel with `ChangeInfo`, or any linked group). Saving topics or history on a basic group first calls `migrateChat`.
- `edit_peer_type_box.cpp` (creator only): "Only members" (join to write, shown for a megagroup with a discussion link), "Approve new members" (request to join), "Restrict saving content".
- `edit_members_visible.cpp`: "Hide Members" for a megagroup when the admin can ban and the group has at least `hidden_members_group_size_min` (100) members.
- `edit_peer_reactions.cpp`: All / Some (picker of emoji) / None for groups; channels add a per-post limit and "Enable Paid Reactions".
- `edit_discussion_link_box.cpp`: pick a suitable group, or unlink; history is made visible first (`togglePreHistoryHidden`) before linking.

## What changed in Quill
- Requests: `toggleSupergroupIsForum`, `toggleSupergroupIsAllHistoryAvailable`, `toggleSupergroupJoinToSendMessages`, `toggleSupergroupHasHiddenMembers`, `toggleChatHasProtectedContent`, `getSuitableDiscussionChats`, `setChatDiscussionGroup`, `setChatAvailableReactions`, `upgradeBasicGroupChatToSupergroupChat` (`src/telegram/requests/group_admin.rs`).
- Parsing: `supergroup.join_to_send_messages`, `supergroupFullInfo` flags (`can_hide_members`, `has_hidden_members`, `is_all_history_available`, `can_enable_paid_reaction`), `chat.available_reactions` + `updateChatAvailableReactions`, `updateActiveEmojiReactions`, and the viewer's own `basicGroup.status` / `is_active` (basic group owner and admin rights were unknown before).
- State (`session_group_admin.rs`): `Session::group_admin_controls` is the single gate for every control (deny by default). Toggles, protected content and reactions are optimistic with a rollback on error; topics, link/unlink and upgrade wait for the server's update. Chained steps (`AdminFollowup`): upgrade then enable topics / show history, and history visible then link a discussion group. A failed upgrade drops its step; the link still runs after a failed history step so the server gives the real error.
- Driver (`src/connect/group_admin.rs`) re-checks the gate before sending, dedupes in-flight requests, and runs the chained steps from `ingest`.
- UI (`src/ui/group_admin_settings.rs`): a "Group settings" / "Channel settings" row in the info panel opens one dialog with pages for Main, Reactions, Discussion and the upgrade/unlink confirmations. After an upgrade the old chat's dialog closes and the new supergroup opens.

## Gating summary
| Control | Who |
| --- | --- |
| Topics | supergroup owner (not broadcast group); basic group owner (upgrades first); locked under 200 members or when linked |
| History for new members | owner or admin with `can_restrict_members`; private, non-forum, non-discussion supergroups; basic group owner (upgrades first) |
| Only members (join to send) | admin with `can_restrict_members`, supergroup with a linked channel |
| Hide Members | server flag `can_hide_members` |
| Restrict saving content | owner (basic group, supergroup, channel) |
| Discussion group | channel admin with `can_change_info`; unlink from the group side for the owner |
| Reactions | `can_change_info` (basic group owner too) |
| Upgrade | basic group owner |

## Not done / differences
- The per-post reaction limit slider is not exposed (the current limit is kept when changing the setting).
- Topics layout (tabs/list) is not offered; tabs are used like tdesktop's default for a newly enabled forum.
- Group-side unlink is owner only (the pin right is not tracked).
- "Approve new members" already existed in the info panel and is unchanged; it is shown in the dialog only as an explanation next to "Only members".
- The 100-member floor for hiding members is left to the server's `can_hide_members`.

## Verification
- Unit tests: request shapes, envelope parsing, reducer gating matrix (owner, restrict admin, member, channel admin, basic group, discussion group), rollbacks, upgrade bookkeeping, and the driver (permission gating, optimistic update and rollback, upgrade then topics, failed upgrade, history then link, unlink both sides).
- Demo capture `ready-group-admin-settings` (`QUILL_DEMO_GROUP_ADMIN=group|channel|basic|reactions|discussion|linked|confirm`); group, reactions, discussion and basic group captures viewed.
- No live account: the real TDLib round trips (and which updates follow each toggle) are unverified.
