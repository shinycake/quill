# Communities on TDLib 1.8.70

TDLib 1.8.70 added the community methods tdesktop already uses:
`addCommunityChat`, `removeCommunityChat`, `getCommunityAddChatRequests`,
`processCommunityAddChatRequest(s)`, `toggleCommunityAreChatsCollapsed`,
`setCommunityNotificationSettings`, `toggleCommunityMemberIsBanned`,
`getCommunityMemberChats`, `getJoinedCommunities` and
`getSuitableCommunityChats`. This slice builds the screens tdesktop has for
them. tdesktop sources read: `boxes/peers/add_to_community_box.cpp`,
`boxes/peers/manage_community_box.cpp`, `boxes/peers/community_box.cpp`,
`info/community_requests/info_community_requests_widget.cpp`,
`boxes/moderate_messages_box.cpp`, `dialogs/dialogs_row.cpp`,
`ui/userpic_view.cpp` (`PaintCommunityUserpicEffect`).

## Where things live

- State: `src/state/domains/communities/` (`state.rs` holds the fetches,
  in-flight sends and outcomes; `query.rs` holds the derived values the UI
  reads: in-flight adds, the asked collapse value, notification settings;
  `chat_list.rs` builds the collapsed rows; `chat_link.rs` maps a chat to its
  community).
- Driver: `src/connect/communities.rs`; requests in
  `src/telegram/requests/communities.rs`.
- Grouping rule: `src/community_row.rs` (`group_main_list`), a pure function
  with unit tests.
- UI: `src/ui/communities/`. Each tdesktop box is one `CommunityBox` variant,
  and one box shows at a time. A box that leads to another replaces it, and
  Cancel goes back through `Back`. The dialog priorities are 5411–5416, below
  the bot (5420) and group (7700) manage boxes, so a community box shows over
  the manage screen and the manage screen comes back when it closes.
- Outcomes: every send ends in a `CommunityOutcome`. The poll drain turns it
  into tdesktop's toast and closes the box that asked. The box stays open on
  a failure, as tdesktop's does.

## Behavior, as in tdesktop

- **Add from a chat.** The manage screen shows "Add Group/Channel/Bot to a
  Community". It opens the picker: Create Community, then "Add to an existing
  community" with the joined communities.
  - Create Community is tdesktop's `CommunityIdentityBox`: a photo button
    with the two cards around it and the name, then the visibility box
    with "Create". `createCommunity` makes it, and the chosen photo goes
    up with `setCommunityPhoto` once its `communityId` arrives
    (`community_created` in the driver). The box waits for the answer:
    `CommunityOutcome::Created` closes it with "Community created.", and a
    refusal toasts the reason and leaves it open. Cancel on the visibility
    box comes back to the name and photo as they were. Next comes the Visible/Hidden
  choice. When the person can edit the community's chat list, the chat is
  added at once. Otherwise "Add to Community" asks first, then toasts "Your
  request was sent to the community admins". A
  `COMMUNITY_PEERS_TOO_MUCH` answer shows the limit toast, which reads
  `community_chat_count_max` / `community_bot_count_max` (default 100).
- **Add from the community.** The info panel's "Add a Chat to Community"
  lists the owned groups and channels without a community, from
  `getSuitableCommunityChats`, plus the owned bots from `getOwnedBots`
  (tdesktop `bots.getAdminedBots`). Picking a bot that has no chat yet
  makes it first (`createPrivateChat`), since TDLib adds chats. "Create a New Chat"
  offers New Group / New Channel, and the new chat goes to the visibility
  box.
- **Remove.** From the manage screen or the info panel row: "Remove {group}
  from the community?", then the "Removed from the community." toast.
- **Requests.** "Pending Requests" in the info panel shows for
  `can_edit_chat_list` with a nonzero count.
  - Add and Decline can be undone for 3 s. A new answer performs the previous
    one, and closing the list performs it too.
  - Add All / Decline All flush the pending answer first, then ask, then toast
    the count.
  - The list asks for its next page (`getCommunityAddChatRequests` from the
    last `next_offset`) when it is scrolled near its end or does not fill
    the box, with the "Loading…" row while it comes (`scroll_load`).
  - A hidden row opens "Message Group Owner". Any other row opens the chat.
- **Show as One Chat.**
  - The switch sends the toggle. It shows the value asked for until
    `updateCommunity` arrives.
  - The collapsed row stands in for the community's main-list chats, after
    the pinned chats, by its newest chat's date, which it prints as chat
    rows do. Its text is the joined
    chats' names, unread ones in semibold, then "and N more chats" for the
    rest. The badge counts unread chats and is grey when they are all muted.
  - The picture is the rounded-square userpic with tdesktop's two cards
    peeking out on the left, drawn with tdesktop's constants. The userpic
    is `community.photo.small`, claimed for download when drawn, as user
    avatars are (`Session::community_photo_path`); the initials show until
    it is on disk. The info panel draws the same file: `communityFullInfo.photo`
    is the full `chatPhoto` of the same picture.
  - The row menu has View community info, Ungroup (with tdesktop's confirm),
    the Mute submenu, and Mark as read for every chat the row stands for.
    Mute is the chat rows' submenu (`FillMuteMenu`): sound on or off, 1
    hour, 8 hours, 2 days, "Mute for..." (the days, hours and minutes box)
    and forever; a muted community shows Unmute. Each applies with
    `setCommunityNotificationSettings`. The submenu opens by hover or
    Right/Enter, like the chat rows'.
- **Notifications.** A chat that keeps its default notification settings
  follows its community's before the scope defaults (`effective_muted`,
  `effective_story_muted`).
- **Ban.** The delete box of a moderated group shows "Ban from the
  Community" when the person may ban from the group's community, with
  "User will be removed from N chats" under it. The ban first asks
  `getCommunityMemberChats`.
  - When the member owns chats there, the "Warning" box lists them before
    the ban.
  - When they own none, or the ask fails, the ban goes at once, as
    tdesktop's `requestParticipantJoinedChats` does.

## Where Quill differs, and why

- **Pin and Add to folder** are missing from the collapsed row's menu.
  TDLib 1.8.70 has no chat-list position for a community: `chatPosition`
  belongs to a chat, `toggleChatIsPinned` and `setPinnedChats` take chat
  ids, and `chatFolder` lists chat ids only. tdesktop pins and files the
  community's own channel peer, which TDLib does not expose.
- **Open in new window** is missing from the row's menu. TDLib is not the
  limit here: tdesktop opens the community's chat list in its own window
  (`SeparateType::Community`), and Quill's chat windows hold one chat,
  not a chat list.
- **Select tone** is missing from the community's Mute submenu. TDLib takes
  a `sound_id` in `setCommunityNotificationSettings`; Quill's tone picker
  lives in the per-chat notification panel, which is keyed to a chat.
- **Gaps between the cards** take the surface color, not transparent
  pixels. GPUI can't carve paths, and on a hovered row the gap shows the
  list color.

## Still blocked

- `communities-admin-rights`: no 1.8.70 method sets
  `communityMemberStatusAdministrator` (checked in `schema/td_api.tl`; the
  community methods are listed above plus create, name, photo, permissions
  and delete).
- `communities-chat-visibility`: `is_hidden` is chosen once, in
  `addCommunityChat` ("You won't be able to change this later"). Nothing
  changes it afterwards, in TDLib or in tdesktop.

## Tests

- Driver: `src/connect/tests/communities.rs`. It covers the send-once rules,
  the answers, the optimistic collapse and notification values, the ban
  sequence, how joined communities reload, `getOwnedBots`, and the photo
  that follows a created community.
- State: `src/state/tests/communities.rs` (options, stray answers, paging,
  collapsed rows, the community photo and its download claim, owned bots) and the `community_row` unit tests (grouping, names,
  badge).
- UI rules: `src/ui/communities/tests.rs` (the ban step, the undo countdown,
  the sections) and `flow_tests.rs` (Create Community's name and back, the
  row menu's Mute submenu and "Mute for...").
- Screens: the `ready-communities` screenshot demo, with
  `QUILL_DEMO_COMMUNITY=info|requests|pick|visibility|confirm-add|remove|choose|hidden-chat|ungroup|ban|manage|manage-linked|menu|mute-for|create`.
  Both demo communities have a photo.
