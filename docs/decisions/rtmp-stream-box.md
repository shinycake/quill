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
  join-as options, `ChooseJoinAsBox` (`:186`) instead.
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
  the schedule default, range and countdown.
- `src/connect/group_calls/rtmp_start.rs`: `begin_rtmp_start`,
  `revoke_rtmp_start_key`, `start_rtmp_stream` (`createVideoChat` with
  `is_rtmp_stream`), `close_rtmp_start`. `getVideoChatRtmpUrl` /
  `replaceVideoChatRtmpUrl` answers feed `calls.rtmp_start`; failures mark
  it, the UI shows "Internal server error." as a toast and a failed first
  request opens no box. `createVideoChat` is no longer refused while a
  one-to-one call is being hung up.
- `canManageGroupCall`: `updateSupergroup` / `getSupergroup` now carry
  `rights.can_manage_video_chats` of the viewer's own status into
  `groups.supergroup_manage_video_chats_right` (like the other supergroup
  rights), so promotions and demotions update it with no own-member probe;
  basic groups take it from `updateBasicGroup`.
  `Session::chat_can_manage_video_chats` is creator || right, false for a
  direct messages chat.
- `src/ui/dialogs/group_call_create.rs` (registered as `GroupCallStart`,
  replacing Quill's old title + presets start dialog): the leave-current
  confirmation, the create confirmation with the schedule link, and the
  schedule box on the kit `DatePicker` (date and time, minute precision)
  with the live countdown. The call button of a chat with no call (and the
  menu's Start) opens the create confirmation, Schedule opens the schedule
  box, Start Streaming goes through the leave check. The join is recorded
  only when the create request went out; a refused request is a toast.
- `src/ui/dialogs/rtmp_stream.rs`: the registered RTMP box, the chat call
  button's menu for managers and its visibility, and `rtmp_info_rows`, also
  used by the stream window's Settings with its own Revoke label. The start
  box's menu says "Revoke", the key warning's "I understand" is a danger
  button, failures to send are toasts, buttons sit right with the primary
  last.
- Strings: tdesktop's keys (`lng_group_call_create_sure*`,
  `lng_group_call_or_schedule`, `lng_group_call_schedule*`,
  `lng_schedule_button`, `lng_minutes`, `lng_call_leave_to_other_sure*`,
  `lng_call_bar_hangup`, `lng_group_call_leave*_to_other_sure*`) plus the
  earlier Quill ones; English only.
- Fixtures `ready-rtmp-start-box`, `ready-group-call-create`,
  `ready-group-call-schedule`.

## Not like tdesktop

- Join as: tdesktop asks `phone.getGroupCallJoinAs` before creating and,
  with more than one option (a channel admin can appear as the channel),
  shows `ChooseJoinAsBox` (its own "or schedule" link, Continue). Quill has
  no pre-create join-as box: it always shows the create confirmation (the
  "only me" path), and the identity is picked afterwards with its existing
  "Join as" row on the new call's pre-join screen
  (`group_call_extras.rs::group_call_join_as_row`, fed by
  `getVideoChatAvailableParticipants`). The channel text still says "as
  your personal account", which is the default there.
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
  leave-current decision, schedule default (including a half-hour zone),
  range, countdown units.
- Driver tests with the recording sender: URL request, answer, revoke,
  `createVideoChat` with `is_rtmp_stream`, failure, a late failure of an
  earlier process, `createVideoChat` while a one-to-one call hangs up, and
  the rights on the ingest path (`updateSupergroup` with an administrator
  status before `updateNewChat`, promotion, demotion, creator, member,
  direct messages).
- Envelope parse tests for the basic group and supergroup payloads.
- GPUI UI tests: the box after the answer, the key warning, Revoke, Start
  Streaming, Start Streaming while in another call (leave, then create),
  the call button's visibility, Start → confirmation → Create, Cancel, the
  schedule link and box (default time, countdown, Schedule), a past time
  refused, Start while in another call.
- Screenshots of the three fixtures on the Linux virtual display (light and
  dark).
- The gate on Linux: app name, file size, language, hotspot and format
  checks, clippy with `demo-capture` and with no default features, the UI
  test binary (818 passed) and the no-default-features suite (all passed
  except `settings::accounts::...::atomic_write_failure_keeps_old_file`,
  which fails only because the container runs as root).

Not verified: anything against a real account (nothing was sent), a real
stream, Windows and macOS, the countdown refresh over real time (the timer
only notifies; the text is computed per render), the kit picker's keyboard
entry in this box.
