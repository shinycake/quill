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
  community" with the joined communities. Next comes the Visible/Hidden
  choice. When the person can edit the community's chat list, the chat is
  added at once. Otherwise "Add to Community" asks first, then toasts "Your
  request was sent to the community admins". A
  `COMMUNITY_PEERS_TOO_MUCH` answer shows the limit toast, which reads
  `community_chat_count_max` / `community_bot_count_max` (default 100).
- **Add from the community.** The info panel's "Add a Chat to Community"
  lists the owned groups and channels without a community, from
  `getSuitableCommunityChats`, plus the owned bots. "Create a New Chat"
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
  - A hidden row opens "Message Group Owner". Any other row opens the chat.
- **Show as One Chat.**
  - The switch sends the toggle. It shows the value asked for until
    `updateCommunity` arrives.
  - The collapsed row stands in for the community's main-list chats, after
    the pinned chats, by its newest chat's date. Its text is the joined
    chats' names, unread ones in semibold, then "and N more chats" for the
    rest. The badge counts unread chats and is grey when they are all muted.
  - The picture is the rounded-square userpic with tdesktop's two cards
    peeking out on the left, drawn with tdesktop's constants.
  - The row menu has View community info, Ungroup (with tdesktop's confirm),
    Mute / Unmute, and Mark as read for every chat the row stands for.
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
  TDLib 1.8.70 has no chat-list position for a community, so neither can be
  done.
- **Open in new window** is missing from the row's menu. Quill's chat
  windows hold a chat, not a chat list.
- **The community photo** is not read yet (`community.photo`). The userpic
  shows the name's initials.
- **Mute** is Mute forever / Unmute. tdesktop opens the mute-duration
  submenu here.
- **The collapsed row shows no date.** It sorts by its newest chat's date
  (tdesktop `_chatsListDate`) but prints none.
- **Gaps between the cards** take the surface color, not transparent
  pixels. GPUI can't carve paths, and on a hovered row the gap shows the
  list color.
- **The requests list** loads its next page with a "Load more" button, not
  on scroll.
- **Owned bots** come from the known users with `is_bot && can_be_edited`.
  tdesktop asks `getOwnedBots`, which Quill does not call yet.
- **Create Community from the picker** reuses Quill's create dialog. Its
  hide option stands in for the visibility box, and it has no photo step.

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
  sequence, and how joined communities reload.
- State: `src/state/tests/communities.rs` (options, stray answers, paging,
  collapsed rows) and the `community_row` unit tests (grouping, names,
  badge).
- UI rules: `src/ui/communities/tests.rs` (the ban step, the undo countdown,
  the sections).
- Screens: the `ready-communities` screenshot demo, with
  `QUILL_DEMO_COMMUNITY=info|requests|pick|visibility|confirm-add|remove|choose|hidden-chat|ungroup|ban|manage|manage-linked`.
