# Admin extras

Cluster "admin". Three items land in full, one lands in part, and the rest are skipped.

## What tdesktop does
- `boxes/peers/edit_tag_control.cpp`: the admin title field holds 16 characters, shows "Admin" (or "Owner") as its placeholder and strips emoji as you type. lib_ui `AddLengthLimitLabel` is the counter used on length-limited fields: it appears once fewer than `min(limit / 2, 9)` characters are left, and turns into a minus count when the text is over.
- `edit_peer_permissions_box.cpp`: "Convert to Broadcast Group" has an explainer under it. The conversion asks twice. The first box lists three facts (no member limit, only admins can send, cannot be undone). The second box warns that regular members permanently lose the right to send.
- `edit_peer_info_box.cpp` `deleteWithConfirmation`: one confirm box with separate group and channel text. tdesktop has no typed confirmation.
- `history/admin_log/`: the filter box picks admins and the choice goes to the server (`user_ids`). The log shows a short explainer of what it covers (last 48 hours).

## What changed in Quill
- `src/admin_extras.rs` holds the pure rules and copy, all unit tested: counter text, title limit by characters, placeholder and helper line, broadcast copy, delete copy, log explainer, and the admin selection toggle.
- Custom title dialog: live counter beside the field (muted, red when over), helper text "A title that members will see instead of 'Admin'", placeholder "Admin". The admin list already showed "Admin" for untitled admins.
- Broadcast conversion: explainer under the row, an intro box with the three facts, then the warning box, then `toggleSupergroupIsBroadcastGroup`.
- Delete group or channel now uses tdesktop's wording per kind, and the dialog title says "Delete channel" for channels.
- Recent actions: the admin chips now send `getChatEventLog.user_ids` (any number of admins, picked in order) instead of hiding rows on the client, so older pages are filtered too. Chips come from the administrator list, so they stay put when the filtered page is empty. The explainer sits under the heading. Session field `event_log_users`; driver methods `toggle_chat_event_log_user` and `clear_chat_event_log_users`.

## Not done, and why
- admin-delete-confirm-typing: tdesktop has no typed confirmation, so there is nothing to match. Not claimed.
- admin-log-extras: the server filter and explainer are done. There is no export in tdesktop's admin log that I could find, so the item is not claimed.
- admin-auto-translate: the channel toggle already existed. The sponsored-messages toggle would need new fields in `supergroupFullInfo` parsing across many exhaustive envelope matches, and tdesktop does not put it in the channel settings box. Not claimed.
- admin-multi-usernames: needs the username lists added to the `updateSupergroup` envelope variant (many exhaustive test matches), a new dialog, and reorder UI. Too large for this PR.
- admin-invite-link-admin, admin-restrict-until-custom (per-right exception lists), admin-appearance, admin-stats-messages, admin-boosts-list: each is a feature of its own (new panels and several TDLib flows). Restrict-until with a custom date already exists from the moderation PR.

## Verification
- Unit tests: counter thresholds, title limit by characters, copy by kind, selection toggle, and two driver tests that check `user_ids` in the sent `getChatEventLog` (empty, two admins in pick order, cleared).
- Demo capture `ready-admin-extras` (`QUILL_DEMO_ADMIN_EXTRAS=log|title|broadcast|warning|delete`); all five modes were viewed (the title capture shows the red minus 2).
- No live account: the real `getChatEventLog` filtering, typing into the counter field and the two-step conversion round trip are unverified.
