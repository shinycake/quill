# Forward box

## What Telegram Desktop does

`ShowForwardMessagesBox` (window/window_peer_menu.cpp) opens a `PeerListBox`
over the window: a search field, the folder strip, Saved Messages first and
then the chat list, one row per chat with its userpic, name and status line
(presence, "bot", member count). Choosing a chat puts a round check on its
userpic. Once a chat is chosen, a comment field slides in under the list and
the box gets Send and Cancel. A right click on Send opens the send menu:
show or hide sender names and captions, send without sound, schedule.

## What Quill does now

- The forward box is a kit dialog (`DialogKind::ForwardBox`,
  `src/ui/forward_box.rs`) instead of a panel dropped into the chat under the
  selection bar. Its open flag is still `share.forward_picker_open`, so every
  place that opens or closes the picker is unchanged.
- Rows are 52 px with a 40 px userpic, the name in medium weight and a status
  line (`Session::share_row_status`: online in the accent color, last seen,
  "bot", member count; nothing for Saved Messages). A chosen chat gets an
  accent check on the userpic corner (it pops in unless reduced motion is on)
  and a light accent fill.
- Names keep their natural width inside a flex row, the way chat list titles
  do, so a Hebrew or Arabic name starts next to its userpic. Before, the name
  box stretched across the row and `bidi_line::one_line` rested right-to-left
  text against the far edge.
- Saved Messages is listed first and titled "Saved Messages" with its bookmark
  userpic (`Session::share_destinations`). Chat photos go through
  `sandboxed_display_path` like the chat list.
- The list is up to seven rows tall and never fewer than three, so the box
  fits short windows; the empty and searching states sit in the middle of the
  list instead of collapsing it.
- The comment field shows once a chat is chosen. The footer always shows the
  selection summary, a ⋯ menu, Cancel and Send (disabled until a chat is
  chosen), so its buttons never move.
- The ⋯ menu stands in for tdesktop's right click on Send: hide sender names,
  hide captions (only for copies), send without sound, schedule (Remind me
  for Saved Messages), open chat (one chat chosen) and copy link (one
  message, live session).
- Schedule is a step inside the box: the date picker replaces the list and
  Cancel there returns to it. The composer's own picker no longer opens
  behind the dialog for the share target.
- The search field takes focus once the dialog has mounted
  (`share.focus_search`), so typing filters right away.
- The silent "forward to…" and "forward picker closed" status notes are gone:
  the box itself is the feedback.

## Names in lists

`bidi_line::name_line` draws a right-to-left name from the left edge of its
box, next to the userpic, instead of resting it against the far end. Chat
header, search results, members, contacts, accounts, admins, folders,
statistics, story viewers, mention suggestions and the other name rows use
it; message previews keep `one_line_plain`, which rests right-to-left text at
the end as tdesktop does. Demo: `ready-rtl-names`.

"Reply in Another Chat" is the same kind of box (`reply_elsewhere_box.rs`):
the author first, then the chat list, one click takes the reply there. Agents
in the granted scope can still use it (`GRANTED_DIALOGS`); its rows are
tagged by chat, so only granted chats show.

## Differences from tdesktop

- Clicking a row ticks it (tdesktop: a click with nothing chosen opens that
  chat with the forward bar; multi-select needs the row's context menu). The
  "Open chat" menu item keeps the forward bar path.
- No folder strip yet.
- The send menu is a visible ⋯ button, not a right click on Send.

## Verified

- `--screenshot-demo ready-forward-box` (light and dark): chats with
  right-to-left names, online / last seen / bot / members lines, two chosen;
  `QUILL_DEMO_FORWARD_BOX=empty` for nothing chosen; scripted click on a row
  and on the ⋯ menu; typing into the search field right after opening.
- Unit tests: Saved Messages first, row status lines, list height bounds.
- Not verified against a live account.
