# Group and channel Type box

## What tdesktop does

`boxes/peers/edit_peer_type_box.cpp` (`EditPeerTypeBox`) opens from the Edit box's
"Group type" / "Channel type" row. Only the owner sees that row. The box has:

- **Public / Private.**
- **The link (public).** A `t.me/` field. Local rules apply first: a-z, 0-9 and
  underscores, at least 5 characters. Then, 200 ms after the last keystroke,
  `checkChatUsername` runs. The line under the field says available, checking,
  invalid, occupied, too short, bad symbols, or "taken, but for sale" with a
  Fragment link. A too-many-public-chats answer sends the box back to private and
  opens `PublicLinksLimitBox`. That box lists "Your public communities" with
  Revoke, which runs `setSupergroupUsername("")` and
  `disableAllSupergroupUsernames`. A group that can't be public goes back to
  private.
- **"Link order" (public).** The `UsernamesList`. The editable username focuses the
  field. Other usernames ask before they are shown or hidden, and the toggle is
  sent at once. Active usernames drag to reorder. Copy is in the context menu.
- **The permanent link (private).** The primary invite link with its menu (Copy,
  Share, QR, Revoke), Copy Link and Share Link buttons, the joined count, and an
  about text.
- **"Who can send messages?"** "Only members" for discussion groups, plus "Approve
  new members/subscribers". There is none for a public broadcast channel.
- **Content protection.**

The box's Save does not send anything. It hands `EditPeerTypeData` back to the
Edit box, whose Save runs these stages in order:

1. username order (or deactivate all when going private)
2. username (a basic group migrates first)
3. no-forwards
4. join-to-send
5. join-by-request

When join approval changes and invite links exist, the Edit box asks "Apply to
existing invite links".

## What changed

- Core, `src/group_type.rs`: link rules, mapping of check answers, the save plan
  (only what changed, in tdesktop's stage order), the row value, and reorder.
  `src/state/domains/groups/group_type.rs`: check generations, the public links
  limit, the created public chats, the revoke, toggle errors, and saves waiting
  for an upgrade.
- Driver, `src/connect/groups/group_type.rs`:
  - `check_group_username` (chat id 0 for a basic group)
  - `check_public_chats_limit`
  - `load_created_public_chats`
  - `revoke_public_link`
  - `save_group_type`

  A basic group going public sends only `upgradeBasicGroupChatToSupergroupChat`.
  The rest of the save (info edits too) runs on the new supergroup once the
  upgrade answer arrives. `toggleSupergroupJoinByRequest` now carries
  `apply_to_invite_links` and works for channels.
- UI, `src/ui/group_type_box/`: the box. Its Save stores the choice in
  `GroupEditBox::type_choice`, and the Edit box's Save sends it with the info
  edits. A refused link shows under the type row. The PublicLinks limit box
  (`premium_ui/public_links.rs`) gains the "Your public communities" list with
  Revoke.
- Removed the old username prompt and row, and the Edit box's "Approve new
  members" row. Both now live in the Type box.
- The Group settings dialog loses its "Link order" page and its "Only members"
  and "Restrict saving content" switches. As in tdesktop, these live only in
  the Type box, which the owner opens from the same Edit box. The rights those
  switches used stay in `GroupAdminControls`, because the Type box and the
  driver still read them. They no longer count toward whether the settings
  dialog has anything to show.

## Deviations

- Errors from showing or hiding a username appear inline under the list, not in
  a toast.
- The limit box list has no chat avatars.
- After a basic group upgrades, errors from the follow-up edits land on the new
  chat. The Edit box has already closed by then.
- The Public / Private choice uses two standalone kit `Radio`s rather than a
  `RadioGroup`. A wrapped kit radio label runs past the dialog edge.

## How it was verified

- Unit tests: `src/group_type.rs`.
- Driver tests: `src/connect/tests/group_type.rs`.
  - newest check wins
  - basic group checked as chat 0
  - too-many opens the limit box
  - revoke frees the limit
  - going private
  - apply-to-invite-links flag
  - upgrade then link
- GPUI test: `src/ui/group_type_box/ui_tests.rs`. Switches to Private and back,
  types a taken link, the occupied state shows and Save stays blocked, then an
  available answer saves into the Edit box.
- Screenshot demos, captured light and dark:
  - `ready-group-type-box`
  - `ready-group-type-taken`
  - `ready-channel-type-private`
  - `ready-channel-edit-box` (type row)
- Not run against live TDLib: real check answers, the limit box with real public
  chats, revoke, upgrade, drag reorder by mouse.
