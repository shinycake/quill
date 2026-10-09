# Member moderation and ownership (batch B9)

## What tdesktop does
- `boxes/peers/edit_participants_box.cpp` `rowContextMenu`: View profile, member tag (add/edit, own or others'), Promote / Edit permissions, Restrict user (supergroups and a creator's basic group), Remove from group. Banned rows get "Add to group" and "Delete from removed". `kickParticipant` asks "Remove {user} from the group?".
- `window/window_peer_menu.cpp` `FillSenderUserpicMenu`: Mention (`@username`, else a text mention) and "Search messages" (in-chat search filtered to that sender).
- `boxes/moderate_messages_box.cpp`: from a message's Delete as admin: Report Spam, Delete all from {user}, Ban {user} with an expander that turns the ban into a restriction ("What can this user do?"); requests go out in order, the ban last.
- `edit_participant_box.cpp` restrict-until: Forever / 1 day / 1 week / custom date and time (`ChooseDateTimeBox`, max 366 days).
- `select_future_owner_box.cpp` / `channel_ownership_transfer.cpp`: an owner who leaves sees who inherits ("in 1 week", or immediately in a basic group) and may appoint someone else; a transfer runs the security check (2-step verification older than 7 days, session older than 24 hours) and then asks for the password.

## What changed in Quill
- `src/moderation.rs` (pure, unit tested): member menu gating (`member_menu_actions`), delete-box options (`moderate_options`, `ModerateChoice::clamp`, `plan_moderation`), restrict-until presets and bounds, mention text, new-owner candidates.
- Requests: `banChatMember`, `canTransferOwnership`, `transferChatOwnership`, `getChatOwnerAfterLeaving`, `deleteMessageReactionsFromSender`, `chatMemberStatusLeft`. `canTransferOwnershipResult*` is parsed. `updateBasicGroup` now keeps the viewer's own status and rights, so basic-group owners and admins are recognised (they were not).
- Member list: each row has a right-click menu and a "more" button (Mention, Search messages, tag, Promote / Edit admin rights, Restrict, Ban, Remove, Unban, Unrestrict), shown only where the rules allow. Mention appends to the composer; Search messages opens in-chat search with the sender filter.
- Remove (tdesktop `kickParticipant`, after "Remove {user} from the group?"): a basic group uses `banChatMember`; a supergroup or channel member is banned for good, shows under Banned and cannot rejoin until unbanned, with no automatic unban. Ban stays as a separate item with the until-date choices.
- Restrict / ban dialog: Forever, 1 day, 1 week, 1 month or a custom date and time from the same date picker as scheduled messages (30 seconds to 366 days ahead, errors shown in the dialog).
- Delete box: Report Spam, Delete all from, Ban, plus "Only restrict" (view-only, forever; the member stays). Choices are clamped to what the viewer may do. Channels offer Ban only.
- Who-reacted list: admins get "Delete" on each reactor when `messageProperties.can_delete_reactions` is set; the row leaves the list after TDLib confirms.
- Ownership: group info "Transfer group/channel ownership" for the owner (pick member, security check, masked password). Leaving as owner shows who inherits, "Appoint Another Owner" (then the same transfer, then leave) or Leave. The password is typed into a masked field, passed once to TDLib, and the field is cleared right after sending; it is never stored, put in a request purpose or logged. Errors never carry TDLib's message text.

## Not done
- tdesktop's per-permission expander inside the delete box (here "Only restrict" is view-only; use the member list for finer control) and "Delete all reactions of a user" (no TDLib method; only per-message deletion exists).
- Moderating from the reactor row (the box tdesktop opens for a reactor).
- "Add to group" on banned rows (Unban is offered).

## Verification
- Unit tests: menu and delete-box gating, plan order, restrict-until bounds, owner candidates, request shapes, envelope parsing, driver flows (kick sequence, basic-group removal, rights gating, reaction deletion gating, transfer gate and password handling, owner lookup), reducer.
- Demo capture `ready-member-moderation` (`QUILL_DEMO_MODERATION=members|restrict|ban|remove|delete|leave|pick|confirm|blocked`); members, restrict (custom date) and confirm viewed. `ready-message-menu` chat 31 now shows the reaction "Delete" buttons.
- No live account: every network round trip, the popup menus (static capture cannot open them) and the who-reacted "Delete" button click are unverified.
