# Group permissions box, exceptions and member paging

Branch `claude/group-permissions-box`. Parity: `admin-permissions-groups`,
`admin-exceptions-add`, `groups-member-paging`.

## What tdesktop does

`boxes/peers/edit_peer_permissions_box.cpp` (`ShowEditPeerPermissionsBox`):

- "What can members of this group do?" then "Send messages", a "Send
  media N/10" row that expands to ten checkboxes and has its own switch
  (on while any row is on, a click flips the group), then Add members,
  Create topics (forums only), Pin messages, Edit own tags and Change
  group info.
- Dependencies: embed links needs send messages; the changed row wins,
  a stored set is read strictest (`ApplyDependencies`,
  `FixDependentRestrictions`).
- Locks: rows the admin cannot grant start off with "You cannot change
  this permission." (`DisabledByAdminRights`); a public group forces
  info and pin off ("not available in public groups"); a discussion
  group locks them with its own reason. Clicking a locked row shows the
  reason as a toast.
- "Do not restrict boosters" with a 1–5 slider, shown only while some
  sending row is off or slow mode is on; the slow mode slider with its
  stops and sentence; the broadcast group offer for the owner near
  `megagroup_size_max`; "Exceptions" and "Removed users" rows with counts
  that open the participant lists.
- Save sends what changed; a basic group with slow mode or boosters is
  migrated first.

`add_participants_box.cpp` (`AddSpecialBoxController`) and
`edit_participants_box.cpp`:

- "Add exception" / "Remove user" pickers over the members list with
  search. An admin the viewer promoted asks "This user is an admin…";
  the owner and other admins get "You can't ban this user…". Removing a
  plain member asks "Remove {user} from the group?".
- Exceptions rows say "Restricted by {user}", removed rows "Removed by
  {user}"; the restrict box says "Restricted by {user} on {date}.".
- The removed users menu has "Add to group" and "Delete" (the user leaves
  the list without being added back).
- Lists page through `channels.getParticipants` past the first page.

## What changed

- `src/permissions_box.rs`: the rows, media group, dependency and lock
  rules, stops and what Save sends, all pure and unit tested.
- `src/ui/group_moderation/permissions_box.rs`: the box, built from kit
  `Switch`, `Checkbox`, `Collapsible`, `Slider` (through
  `stops_slider.rs`) and `Separator`. The lock reason shows under the
  clicked row instead of a toast.
- `src/connect/moderation/permissions.rs`: `save_permissions_box` sends
  `setChatPermissions`, `setChatSlowModeDelay` and
  `setSupergroupUnrestrictBoostCount` only when they differ; a basic
  group upgrades first through the `AdminFollowup` chain.
- `supergroupFullInfo.restricted_count` / `banned_count`, `chatMember`
  inviter and date, and the `supergroup_size_max` option are parsed.
- `src/add_special.rs` + `src/ui/group_moderation/add_special.rs`: the
  pickers and their questions. The restrict box now sits above the
  member dialog (priority 4290; the picker is 4295), so restricting from
  the member list is no longer hidden behind it.
- Member dialog: the Add exception / Remove user rows, the "by" status
  lines, and paging with `getSupergroupMembers` offsets (the 200 row cap
  is gone).
- Removed users: "Delete" (was "Unban") sends `chatMemberStatusLeft`,
  tdesktop's `removeKicked`, instead of `chatMemberStatusMember`;
  "Add to group" (`addChatMembers`) is new. Unrestrict still sends
  `chatMemberStatusMember`.
- Slow mode moved from the group info panel into the box: tdesktop has
  no slow mode control in the info panel.

## Not done (follow-ups)

- The restrict box keeps its flat checkbox list; grouping it needs
  changes in `src/ui/groups/mod.rs`, owned by another PR.
- Basic groups show no Exceptions row: TDLib keeps restricted members
  for supergroups only (tdesktop shows it to the owner and migrates).
- "Charge Stars for messages" in the box is not built.
- The manage panel still has its own "Convert to Broadcast Group" row
  and the boosts overview its own unrestrict buttons; tdesktop has them
  only in this box. Those files belong to other work.

## How it was verified

- Unit tests: `src/permissions_box/tests.rs`, `src/add_special.rs`,
  `src/moderation.rs` (Add to group).
- Driver tests with the recording sender:
  `src/connect/tests/permissions_box.rs` (only changed settings are sent,
  basic group upgrade then slow mode, Delete sends Left).
- GPUI tests: `src/ui/demos/group_permissions_tests.rs` toggles a media
  sub-permission and saves; picks an admin (question) and a member
  (restrict box) in the Add exception picker.
- Screenshot demos `group-permissions`, `group-permissions-forum`,
  `group-exceptions`, `group-add-exception`, in light and dark, with
  scripted clicks for the media group, the text/links dependency and a
  locked row. Not covered: real TDLib answers, the upgrade round trip
  with a live server, paging against a group with more than 200 members.
