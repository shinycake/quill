# B8: join requests and invite-link admin

## What tdesktop does

- `boxes/peers/edit_peer_requests_box.cpp` (`RequestsBoxController`): a
  list of pending requests, each row "requested to join today at 14:05"
  with "Add to Group/Channel" and "Dismiss". The box has a server-side
  search (`RequestsBoxSearchController`, 1 s delay) and pages with
  `offset_date` / `offset_user`. It has no bulk buttons for groups (the
  "Add All" strings in `lang.strings` belong to communities).
- `boxes/peers/edit_peer_invite_links.cpp`: the admin's active links,
  a "Links created by other admins" list with `lng_group_invite_other_count`
  ("N invite links"), and a "Revoked links" section whose menu has
  "Delete Link" per row and "Delete all revoked links" with the
  `lng_group_invite_delete_all_sure` confirmation.
- `boxes/peers/edit_peer_invite_link.cpp`: link details with the joined
  members ("N joined", "joined <date>", folder-link variant), the pending
  request count, and for subscription links the price
  (`lng_group_invite_subscription_price`: "~cost / month"). Creating a link
  in a channel offers "Require Monthly Fee" with a Stars amount; the fee
  cannot be edited afterwards (only the name).

## What Quill had

Per-request Approve/Dismiss (`processChatJoinRequest`), a single unsearched
50-row page of requests, and list/create/edit/revoke of links. Revoke
replaced the whole cached link list with the answer of
`revokeChatInviteLink`, which only holds the revoked link (and the new
primary), so revoking emptied the panel until a refresh.

## What changed

Requests (`src/telegram/requests/invite_admin.rs`), all verified against
`schema/td_api.tl`: `getChatJoinRequests` with an offset and query,
`processChatJoinRequests`, `getChatInviteLinkCounts`,
`getChatInviteLinkMembers` (null offset on the first page),
`deleteRevokedChatInviteLink`, `deleteAllRevokedChatInviteLinks`,
`createChatSubscriptionInviteLink` (period fixed at 30 days),
`editChatSubscriptionInviteLink`.

State and driver (`src/connect/invite_admin.rs`, reducer in `src/state`):

- Join requests: the search query and the newest page request id are kept
  per chat; a reply for an older search is dropped. "Show more" appends the
  next page using the last row's `(user_id, date)`. Approve all / Dismiss all
  send `processChatJoinRequests` with an empty link; on `ok` the cached
  list and the badge count go to zero. A failure keeps the list and reports
  through `invite_link_error`.
- Link members: one link's members per chat, paged; the request id is stored
  in the state so a late reply for a previously opened link is ignored.
- Counts: fetched only for the owner (TDLib: "requires owner privileges").
- Revoked links: loaded lazily with `getChatInviteLinks(is_revoked = true)`.
  Revoking now moves the link from the active list to the revoked list and
  upserts the replacement primary instead of replacing the list.
  `RequestPurpose` is `Copy`, so the link of an in-flight single delete rides
  in `Session::revoked_link_deletions` keyed by request id.
- Subscription links reuse the create/edit purposes, so the existing answer
  handling upserts them. Creation is offered only in channels the viewer can
  invite in; the price shows as "N Stars / month" on the row.

UI: the join-requests box gets a search field, Add all / Dismiss all (hidden
while searching, with a confirmation), and Show more. The info panel's
invite-link rows get "Joined (N)" (expands the paged member list), the price
line, and "Rename" for subscription links; below the list sit "Links created
by other admins" (owner) and a collapsible "Revoked links" with Delete and a
confirmed "Delete all revoked links". The create dialog gains a Stars-per-month
field in channels. Everything is gated by `chat_can_invite_users`; counts and
the per-admin list need ownership.

## Skipped

- Opening another admin's links from the counts list (the counts are shown,
  not drilled into) and the link QR code: `admin-invite-link-admin` is not
  declared complete.
- Per-link pending-request list (`getChatJoinRequests` with `invite_link`).
- Approve all / Dismiss all inside the info panel's request section (only in
  the box, as in tdesktop where the requests live).
- Editing a subscription link's price (not possible in Telegram either).

## Verified

- Unit tests: request builder shapes, envelope parsing of counts/members,
  driver tests for search + stale replies + paging, bulk process and its
  failure path, owner-only counts, member paging with a stale reply,
  revoke/delete/delete-all, subscription gating and period, and non-admin
  gating of every call.
- Demo capture: `ReadyInviteLinks` (links with a Stars link, joined members,
  other admins, revoked links) and `ReadyTopBars` `requests-box` (search,
  Add all, Dismiss all).
- Not verified against a live account.
