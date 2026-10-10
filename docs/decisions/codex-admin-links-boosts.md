# Admin links, boosts and usernames

Two README items land in full. The boosts item lands in part and is not claimed.

## What tdesktop does

- `boxes/peers/edit_peer_invite_links.cpp`: the owner sees "Links created by other admins". Opening an admin lists that admin's active and revoked links, with "Delete all revoked links".
- `boxes/peers/edit_peer_invite_link.cpp`: a link with pending requests lists them with Add and Dismiss. The link also has a QR code.
- `boxes/peers/edit_peer_usernames_list.cpp`: a "Link order" list of the active usernames followed by the inactive ones. Rows reorder, and a tap on a collectible username asks to show or hide it. The editable username cannot be hidden.
- `info/boosts/`: a boosts list with an "All" and a "Gifts" tab, "boost expires on <date>" per row, "Unclaimed / To be distributed" for giveaway prizes, "Show N more boosts", and a "Link for boosting" with copy.

## What changed

Requests (`src/telegram/requests/boosts_usernames.rs`, plus the existing `getChatInviteLinks` and `getChatJoinRequests` builders): `getChatBoosts`, `getChatBoostLink`, `toggleSupergroupUsernameIsActive`, `reorderSupergroupActiveUsernames`. All are in `schema/td_api.tl`.

Envelope: `foundChatBoosts`, `chatBoostLink`, and a `usernames` field on `updateSupergroup` (active, disabled, editable, collectible), kept in `Session::supergroup_username_lists`. The older single-username map is unchanged.

State and driver (`src/state/session_links_boosts.rs`, `src/connect/links_boosts.rs`):

- Another admin's links (owner only): two `getChatInviteLinks` calls with `creator_user_id`, one active and one revoked. The state keeps both request ids, so a late reply for the admin you left is dropped. "Delete all" sends `deleteAllRevokedChatInviteLinks` with that admin's id and clears only that admin's list.
- Per-link requests: `getChatJoinRequests` with `invite_link`, paged by the last row. A single Add or Dismiss removes the row on `ok`. Add all and Dismiss all send `processChatJoinRequests` with the link.
- Boosts: paged with the server's `next_offset`. Switching tab resets the list and stale pages are dropped. The boost link is fetched once per chat. Administrators only.
- Usernames: owner only. The editable username cannot be turned off. Reorder moves one slot and sends the whole order. One change at a time. A refused change reports through the status note, and the lists refresh from the next `updateSupergroup`.

UI: the group settings dialog gets "Link order" (shown to the owner when there is more than one username to manage) and "Boosts" (any administrator). Hide and Show ask first. In the info panel, the other-admin rows have a View button, and link rows get "Requests (N)" and "QR code". The QR uses the same renderer as the login QR (`render_qr_image`).

## Not done

- The boost features table (levels and what each unlocks) and the unrestrict-by-boosts setting, so `admin-boosts-list` is not claimed.
- Drag to reorder. Reordering uses up and down buttons.
- Revoking another admin's link from the drill-in.

## Verified

- Unit tests: request shapes, envelope parsing, driver tests for each flow above (stale replies, owner gating, paging offsets, one change at a time, error path), and the copy helpers.
- Demo captures, all viewed: `ready-links-boosts` with `QUILL_DEMO_LINKS_BOOSTS=usernames|boosts|gifts|admin-links|link-requests|qr`.
- No live account: real server answers and the reorder round trip are unverified.
