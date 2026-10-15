# Stream with other apps (admin RTMP box)

Design: `quill-tools/designs/group-call-streams.md`, build plan item 4.

## What tdesktop does

- `window/window_peer_menu.cpp` `FillVideoChatMenu`: for a manager
  (`canManageGroupCall`) of a group or channel with no call, the entries
  Start video chat / Start live stream, Schedule ..., and "Stream with...".
  It is reached from the top bar's call button (`TopBarWidget::groupCall`,
  `HasGroupCallMenu`: not a user, no call, manager) and the forum list menu.
- `calls/group/calls_group_rtmp.cpp` `StartRtmpProcess`: asks for the RTMP
  URL (`getGroupCallStreamRtmpUrl`), and only when the answer is in shows
  `StartWithBox` "Stream with other apps": Server URL and Stream Key rows
  (`FillRtmpRows`, shared with Settings), the key hidden behind a spoiler
  and a warning, the info text, "Start Streaming" and Cancel, and a top "..."
  with Revoke behind a confirmation (the request again with `revoke`). A
  failed request is a toast with the server error. "Start Streaming" creates
  the call with the RTMP flag.

## What changed

- `src/calls/rtmp_start.rs`: the process state, menu entries and facts.
- `src/connect/group_calls/rtmp_start.rs`: `begin_rtmp_start`,
  `revoke_rtmp_start_key`, `start_rtmp_stream` (`createVideoChat` with
  `is_rtmp_stream`, join intent recorded by the UI), `close_rtmp_start`.
  `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl` answers feed
  `calls.rtmp_start` when no call is tracked; failures mark it, the UI shows
  "Internal server error." as a toast and a failed first request opens no box.
- `src/ui/dialogs/rtmp_stream.rs`: the registered dialog, the chat call
  button's menu for managers, and `rtmp_info_rows`, now also used by the
  stream window's Settings (rows not duplicated).
- `canManageGroupCall`: `ChatSummary::can_manage_video_chats` (own-member
  probe, `can_manage_video_chats` right) and the basic group right from
  `updateBasicGroup`; `Session::chat_can_manage_video_chats`.
- Strings: tdesktop's keys plus six Quill ones, English only.
- Fixture `ready-rtmp-start-box`.

## Not like tdesktop

- The start/schedule entries use Quill's existing flows: Start creates the
  chat at once (the title is editable in the call), Schedule opens the
  existing schedule dialog preset to one hour.
- The Revoke and key-warning confirmations replace the box body instead of
  stacking a second box (the kit shows one dialog at a time).
- The "..." Revoke is on the Server URL row (the shared rows), not the box's
  top-right corner. No animated RTMP icon.
- Not added to the forum list menu; the chat header button is the only entry.
- A non-manager without a call still gets the old start dialog.

## Verified

Unit tests (menu facts, process), driver tests with the recording sender
(URL request, answer, revoke, createVideoChat with `is_rtmp_stream`, failure,
rights), envelope parse test, GPUI UI tests (menu facts, box after the
answer, key warning, Start Streaming). Gate OK. Nothing was sent to a real
account. Not run: a real stream, Linux/Windows, a screenshot of the box.
