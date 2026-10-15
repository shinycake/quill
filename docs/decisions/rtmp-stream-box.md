# Stream with other apps (admin RTMP box)

Design: `quill-tools/designs/group-call-streams.md`, build plan item 4.
tdesktop references are to `Telegram/SourceFiles` at `b16daff`.

## What tdesktop does

- `window/window_peer_menu.cpp:4369` `FillVideoChatMenu`: for a manager
  (`canManageGroupCall`) of a group or channel with no call, the entries
  Start video chat / Start live stream, Schedule ..., and "Stream with...".
  It is reached from the top bar's call button
  (`history/view/history_view_top_bar_widget.cpp:315` `groupCall`, `:80`
  `HasGroupCallMenu`: not a user, no call, manager) and the forum list menu.
- `data/data_peer.cpp:1880` `canManageGroupCall`: creator, or an
  administrator with the ManageCall right; never for a channel's direct
  messages chat (`isMonoforum`).
- `history_view_top_bar_widget.cpp:1429` `updateControlsVisibility`: the call
  button shows for a manager, or for anyone when the chat's call has nobody
  in it (`fullCount() == 0`); otherwise it is hidden (the Join bar covers a
  call with people in it).
- `calls/calls_instance.cpp:228` `startOrJoinGroupCall` and `:326`
  `confirmLeaveCurrent`: before anything is created, being in a one-to-one
  call asks `lng_call_leave_to_other_sure(_channel)` with "End call"; being
  in another chat's video chat asks `lng_group_call_leave*_to_other_sure*`
  with "Leave"; the same chat's call is brought up instead; a scheduled
  call is simply replaced. The current call ends only when the new one is
  created (`:494` `createGroupCall` → `destroyCurrentCall`). `:376`
  `showStartWithRtmp` runs the same check after "Start Streaming".
- `calls/group/calls_choose_join_as.cpp:268` `CreateOrJoinConfirmation` and
  `:452-500`: with only "me" to join as, "Start a video chat in this group?"
  (`lng_group_call_create_sure`, `_channel` for channels), a blank line and
  "You can also {link}." (`lng_group_call_or_schedule` with
  `lng_group_call_schedule(_channel)`) whose link opens the schedule box;
  buttons "Create" (`lng_create_group_create`) and Cancel. With several
  join-as options, `ChooseJoinAsBox` (`:186-266`) instead: "Start Video
  Chat as..." / "Start Live Stream as...", the about line, the identities
  ("personal account" or the member count) with the selected one checked,
  the schedule link for an administrator, Continue and Cancel. The list
  comes from `requestList` (`:361-394`): a failure creates as yourself at
  once, an empty list is the server error toast (`:419`).
- `calls/group/calls_group_call.cpp:1335` `initialJoin`: a call created to
  start now is joined at once; `:1318-1322` a failed `createGroupCall` is
  logged and the panel hangs up, with no toast.
- `calls_group_rtmp.cpp:188-197` `createBox`: "Start Streaming" joins as
  the chat when you created it, otherwise as yourself.
- `calls_choose_join_as.cpp:119` `ScheduleGroupCallBox` (`ChooseDateTimeBox`):
  title `lng_group_call_schedule_title(_channel)`, submit "Schedule"
  (`lng_schedule_button`), opens at the next whole hour at least half an
  hour away, range now + 12 s to one second before local midnight eight
  days ahead, and the description
  `lng_group_call_schedule_notified_group/_channel` with `{duration}` in
  days, hours or minutes (at least one), refreshed every 10 s. No "Now"
  preset.
- `calls/group/calls_group_rtmp.cpp` `StartRtmpProcess`: asks for the RTMP
  URL (`getGroupCallStreamRtmpUrl`), and only when the answer is in shows
  `StartWithBox` "Stream with other apps": Server URL and Stream Key rows
  (`FillRtmpRows`, shared with Settings), the key hidden behind a spoiler
  and a warning confirmed with "I understand" in the attention (danger)
  style (`:339-350`), the info text, "Start Streaming" and Cancel, and a top
  "..." whose item is "Revoke" (`:94`, `lng_group_invite_context_revoke`)
  behind a confirmation (`:198-207`). A failed request is a toast with the
  server error (`:171`). "Start Streaming" creates the call with the RTMP
  flag.

## What changed

- `src/calls/rtmp_start.rs`: the process state, menu entries and facts;
  the process remembers the URL requests it sent and only their answers
  and failures count (a late failure of an earlier process for the same chat
  is ignored); `call_button_visible` (`updateControlsVisibility`).
- `src/calls/group_call_create.rs`: `before_create` (`confirmLeaveCurrent`),
  `join_as_step` (`processList`), `rtmp_join_as` (`createBox`), the schedule
  default, range (daylight saving changes included) and countdown.
- `src/connect/group_calls/rtmp_start.rs`: `begin_rtmp_start`,
  `revoke_rtmp_start_key`, `start_rtmp_stream` (`createVideoChat` with
  `is_rtmp_stream`), `close_rtmp_start`, and `request_create_join_as`
  (`getVideoChatAvailableParticipants` with the chat only, before any call
  exists; answer or failure in `calls.create_join_as`). `getVideoChatRtmpUrl`
  / `replaceVideoChatRtmpUrl` answers feed `calls.rtmp_start`; failures mark
  it, the UI shows the server error as a toast and a failed first request
  opens no box. `createVideoChat` is no longer refused while a one-to-one
  call is being hung up.
- A refused `createVideoChat` no longer sets `calls.group_call_error` (which
  only shows inside a tracked call's panel and lingered for the next call):
  its request id goes to `calls.failed_creates`, and the UI, which keeps the
  id of the create it sent, shows the server error toast and drops the join.
- `canManageGroupCall`: `updateSupergroup` / `getSupergroup` carry
  `rights.can_manage_video_chats` of the viewer's own status into
  `groups.supergroup_manage_video_chats_right` (like the other supergroup
  rights), so promotions and demotions update it with no own-member probe;
  basic groups take it from `updateBasicGroup`.
  `Session::chat_can_manage_video_chats` is creator || right, false for a
  direct messages chat.
- Being in a call (`Session::current_call_for_create`) counts a join still
  in flight, a pending `joinVideoChat` or the window's own join, as tdesktop's
  `inGroupCall()` does; the call you leave for the new one always goes
  through `leave_active_group_call` (`leaveGroupCall` and the transport torn
  down); only a scheduled call you never joined is dropped locally.
- `src/ui/dialogs/group_call_create.rs` (registered as `GroupCallStart`,
  replacing Quill's old title + presets start dialog): the leave-current
  confirmation; Start asks for the "join as" list, then shows the create
  confirmation (only yourself), `ChooseJoinAsBox`
  (`src/ui/dialogs/group_call_join_as.rs`: header, about line, rows with
  photo, name and "personal account" or the member count, a kit `Radio` for
  the selected row, the schedule link for an administrator, Continue) or the
  server error (empty list); a failed list creates as yourself at once, as
  tdesktop's `requestList` failure does. The schedule box is on the kit
  `DatePicker` (date and time, minute precision) with the live countdown.
  The join is recorded only when the create request went out.
- A call created to start now is joined as soon as it is tracked, whatever
  its kind (tdesktop `createGroupCall` joins at once), as the identity picked
  in `ChooseJoinAsBox`; a stream from "Stream with other apps" joins as the
  chat when you created it, otherwise as yourself
  (`StreamJoinIntent::Created { chat, join_as }`).
- `src/ui/dialogs/rtmp_stream.rs`: the registered RTMP box, the chat call
  button's menu for managers and its visibility, and `rtmp_info_rows`, also
  used by the stream window's Settings with its own Revoke label. The start
  box's menu says "Revoke"; the key warning is bold with a danger "I
  understand"; the Revoke question is plain text. The stream window's
  Settings confirmations now match (`group_call_stream_boxes.rs`: "I
  understand" in the danger style for the key warning, plain Revoke text).
- Strings: tdesktop's keys (`lng_group_call_create_sure*`,
  `lng_group_call_or_schedule`, `lng_group_call_schedule*`,
  `lng_schedule_button`, `lng_minutes`, `lng_call_leave_to_other_sure*`,
  `lng_call_bar_hangup`, `lng_group_call_leave*_to_other_sure*`,
  `lng_group_call_start_as_header*`, `lng_group_call_join_as_about`,
  `lng_group_call_join_as_personal`, `lng_chat_status_members`,
  `lng_chat_status_subscribers`), English only.
- Fixtures `ready-rtmp-start-box`, `ready-group-call-create` and
  `ready-group-call-schedule`; the last two are also what two UI tests open.

## Not like tdesktop

- Failure text: every failure toast here is "Internal server error."
  (`qll_ops_server_error`, the text of tdesktop's `Lang::Hard::ServerError`,
  which is hard-coded there and so has no language-pack key). It is shown
  where tdesktop shows it (RTMP URL requests, an empty join-as list) and
  also where Quill could not send the request at all (no connection), which
  tdesktop never meets because its requests queue. A refused
  `createVideoChat` is the same toast; tdesktop only logs it and hangs up
  the panel it opened, but Quill has no panel yet, so without the toast
  nothing would be shown. No other Quill-only failure strings are left in
  this flow.
- `ChooseJoinAsBox` uses Quill's list rows with a kit `Radio`, not
  tdesktop's peer list with round checkboxes over the photos.
- A scheduled video chat is created as yourself and not joined (Quill does
  not track the waiting state of a call you scheduled; tdesktop opens its
  panel in the Waiting state). For a channel tdesktop's Schedule entry also
  skips the join-as list; Quill skips it for groups too, as the identity
  only matters on joining.
- The Revoke and key-warning confirmations replace the RTMP box body instead
  of stacking a second box (the kit shows one dialog at a time); the leave
  confirmation for Start Streaming replaces the box and Cancel brings it
  back.
- The "..." Revoke is on the Server URL row (the shared rows), not the box's
  top-right corner.
- The schedule box uses one kit date and time picker instead of separate
  date and time fields, and an out-of-range time shows the existing
  "Pick a time in the future." line instead of shaking the field.
- Follow-ups, not done here: the animated RTMP icon at the top of the box
  (tdesktop's Lottie `CreateLottieIcon`), and the forum list menu entry
  (`FillVideoChatMenu` from the forum's dialogs row); the chat header
  button is the only entry for now.

## How verified

- Unit tests: menu facts and call button visibility, request matching,
  leave-current decision, join-as list outcomes and the RTMP identity,
  schedule default (including a half-hour zone), range (including clocks
  going forward and back within the eight days), countdown units.
- Driver tests with the recording sender: URL request, answer, revoke,
  `createVideoChat` with `is_rtmp_stream`, failure, a late failure of an
  earlier process, `createVideoChat` while a one-to-one call hangs up, a
  refused `createVideoChat` kept by request id with no call error, the
  pre-create `getVideoChatAvailableParticipants` answer and failure, a join
  in flight counting as being in the call and `leaveGroupCall` for it, and
  the rights on the ingest path (`updateSupergroup` with an administrator
  status before `updateNewChat`, promotion, demotion, creator, member,
  direct messages).
- Envelope parse tests for the basic group and supergroup payloads.
- GPUI UI tests: the box after the answer, the key warning, Revoke, a
  failed URL request's toast, Start Streaming (joined as yourself, or as the
  channel by its creator once the stream is tracked), Start Streaming while
  in another call, the call button's visibility, the create demo (question,
  Create, and the created video chat joined once tracked), Cancel, the
  join-as box (rows, schedule link, Continue as the channel, joined as it),
  only-yourself and empty lists, the schedule link and box (default time,
  countdown, Schedule), the schedule demo refusing a past time, Start while
  in another call and while a join is in flight, a refused create (toast,
  join dropped, another request's failure ignored).
- Screenshots of the create and schedule fixtures and the RTMP box on the
  Linux virtual display (light and dark), from the first round; the
  join-as box has no fixture and was not looked at.
- The gate on Linux: app name, file size, language, hotspot and format
  checks, clippy with `demo-capture` and with no default features, the UI
  test binary and the no-default-features suite (all passed except
  `settings::accounts::...::atomic_write_failure_keeps_old_file`, which
  fails only because the container runs as root).

Not verified: anything against a real account (nothing was sent), a real
stream, Windows and macOS, the countdown refresh over real time (the timer
only notifies; the text is computed per render), the kit picker's keyboard
entry in this box, the join-as box on screen.
