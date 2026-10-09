# Idle CPU: redraw by urgency, and let the display link sleep

A release build of main (`0d4e799d`, M1 Pro, 2026-10-08) signed in to an
online account used 6.1% CPU frontmost 60 s after launch, 8.0% frontmost
and idle after opening a few chats, and 1.0% in the background. Goal: at
most 1% frontmost and 0.3% in the background with nothing animating, and
redraws only for what changes on screen.

## What kept Quill busy

Profiling first (`sample` on release builds with line tables, the
`QUILL_TRACE_TICKS` / `QUILL_TRACE_NOTIFY` traces, per-thread CPU from
`ps -M`):

- **The demo fixtures were already quiet.** Every static fixture
  (ready-chats, chat list, stories, media, reactions, avatars, unread…)
  idled at 0.3–1.0% with no frame-clock ticks and no notifies. The
  frontmost cost came from what only a signed-in account has: TDLib
  updates.
- **Every TDLib update redrew the whole window.** `poll_live` called
  `cx.notify()` after any batch that applied anything, and every
  `QuillApp` notify re-renders all slices (chat list, conversation,
  composer). A signed-in, *online* account receives a steady stream that
  mostly changes nothing on screen or only a chat-list row: contacts going
  online and offline, people typing in other chats, unread totals (only the
  tray reads them), messages and read receipts in chats that are not open,
  automatic avatar and thumbnail downloads, update types Quill does not
  parse. Being frontmost makes the account online, which is why the cost
  was a foreground cost. A full redraw costs 5–8 ms of CPU at idle clock
  speeds (forced 60 Hz full redraws of the stress fixture: 30% CPU), so
  10–15 updates a second account for the measured 6–8%.
- **The display link never stopped.** GPUI's macOS window keeps its
  `CVDisplayLink` subscription for as long as the window is visible, so the
  main thread (`gpui_macos::window::step`: thermal state query, ivar lookup,
  frame callback) and a CoreVideo thread wake at the display's refresh rate
  (120 times a second on a ProMotion panel) even when nothing is drawn.
  That was the whole 0.3–0.5% floor of an idle fixture, active or not. The
  platform hook to fix it exists in GPUI (`PlatformWindow::frame_waker`,
  which the invalidator calls when the window becomes dirty or a next-frame
  callback is queued) but the macOS backend doesn't implement it.
- Smaller: 1 s timers (tray, passcode), the 250 ms link inbox and the
  poll loop's 120 ms idle cadence cost nothing measurable (an idle fixture
  with the display link fixed reads 0.00%).

To measure the stream without an account, `QUILL_DEMO_UPDATE_STREAM=<n>`
(`src/ui/demo_stream.rs`) feeds a demo `n` synthetic TDLib updates a
second, through the real reducer and the poll loop's redraw policy. The
mix repeats every eight updates: a contact goes offline, someone starts
typing in another chat, an update Quill does not parse, the contact comes
back online, the typing stops, unread totals change, an automatic download
progresses, another chat's outbox is read. At 10 updates a second the old
policy sat at 5–7.5% with this fixture alone, the same range as the live
account, with `notify: 9.6/s` in the trace.

## Changes

### Redraw by urgency (all platforms)

`quill::state::redraw_need` (`src/state/redraw.rs`) grades each envelope
before it is applied, against the session as it is:

| Grade | Envelopes | Redraw |
| --- | --- | --- |
| Nothing | unparsed updates, unread totals, unchanged statuses, unknown users, `ok` for `viewMessages` / `setOnline` | none |
| ChatList | for chats that are not shown: typing, new and last messages, positions, read state, mention / reaction counts, drafts, notification settings, list membership; a contact's online dot flipping | the chat list slice (and `QuillApp` around it), at most every 400 ms (2 s behind another app) |
| Later | automatic downloads in flight, other users' and groups' records, `last seen` changes, options other than `my_id` / `is_premium`, online counts of other chats | a full redraw at most every 1 s (4 s behind another app) |
| Now | everything else: every answer or error for a request of ours, the open chat (or the comment thread shown in its place), its peer's status and record, our own user record, the open group's record and online count, user-started downloads, the open chat's media downloads, any download completing, uploads, auth, connection, calls, `my_id` / `is_premium`, any update type not graded | a full redraw at once (every 500 ms behind another app, as before) |

The poll loop keeps the most urgent grade of each batch. Anything the poll
itself changes outside the session is `Now` too: results the UI drains
(status notes, forward results, exports, links, failed sends, expired bot
messages, proxy switches), and a changed status line, login-review box or
folder tab, compared before and after the poll. A queued desktop
notification, its sound or a forced reply redraws at once even behind
another app, because only a render hands them out. `PolledRedraw`
(`src/ui/notifications.rs`) holds back what isn't due yet and never drops
it: the poll loop calls back at least every 120 ms, and the chat-list and
full-redraw batches are timed apart, so a pending full redraw never holds a
chat-list change past its own interval. A chat-list redraw notifies the
chat list slice, which re-renders `QuillApp` as its ancestor (chrome, side
and info panels, dialogs, render-time drains) while the conversation and
composer replay their last frame (`notify_chat_list`; outside the ready
layout everything redraws).

Tests: `src/state/tests/redraw.rs` covers each grade against an open chat,
another chat, no open chat, a comment thread, an open group, request
answers and errors, options, and downloads (in flight, completing, user
started, open chat media). `src/ui/notifications.rs` tests the throttle,
including a 20 000-poll pseudo-random run (needs of every grade, polls
10–120 ms apart, the window going to the background and back) that checks
every need is drawn within its interval plus one poll gap.

### macOS: the display link stops while idle

`third_party/gpui-pre-macos` is GPUI's macOS platform (0.3.7, vendored
like `third_party/gpui-base`; the commit that adds it is the pristine
registry copy, the changes are in `QUILL-CHANGES.md` and marked in each
file). A step that neither draws, presents nor asks for another frame is
idle; after 250 ms of those the window unsubscribes from the display link.
`frame_waker` requests one step on the window's dispatch source while the
link is idle (GPUI calls it when the window becomes dirty or queues a
next-frame callback); that step draws immediately and resubscribes. While
the link runs, or is stopped because the window is hidden, the waker does
nothing, so frames stay paced by the display. Animations at 4 fps and up
keep the link running; GPUI keeps presenting for a second after high-rate
input, which counts as activity. Window teardown cancels the dispatch
source, so a waker that outlives the window cannot run a step on a freed
view.

250 ms rather than 1 s: measured side by side, the shorter timeout was
equal or slightly better everywhere (stream, typing, stickers) and lets the
link sleep between batched background redraws.

## Verification on macOS

Release builds of this branch with a temporary trace in the vendored
`step`, `draw` and frame waker (not committed): steps, draws, stops and
wakes per second, the time from a wake to the frame reaching the renderer,
and the gaps between drawn frames. M1 Pro on a 60 Hz external display,
`QUILL_ASSUME_ACTIVE=1`, fixtures as in the bench below:

| Fixture | Display link | Frames drawn | Gap between frames |
| --- | --- | --- | --- |
| ready-chats, idle | stops 250 ms after the first frames; no steps after that | none | — |
| ready-typing (typing dots, 12 fps) | runs, 60 steps/s, never stops | 11–12/s | median 83 ms, max 100 ms |
| ready-sticker-playback (24 fps) | runs, never stops | 23–24/s | median 48–49 ms, max 50–51 ms |
| ready-gif-playback (24 fps) | runs, never stops | 23/s | median 49 ms, max 50 ms |
| ready-chats + stream, active | stops and restarts 1–2 times a second | 3–9/s (chat-list redraws, typing dots in a row) | 83 ms while dots play, ~400 ms between batches |
| ready-chats + stream, inactive | stops and restarts once every 2 s | one batch every 2 s | ~2 s |

Animations keep the link running and draw at their own rate with no
missed frames (the 24 fps content lands on every third 60 Hz frame, as
before). A frame demanded while the link is stopped reaches the renderer
1.3–2.4 ms after the wake, render time included, which is sooner than the
next vsync would have been. The stream fixture exercises the whole cycle
(stop, wake on a chat-list redraw, draw, resume, stop again) about once a
second for minutes without a stall. Not exercised by a script here:
resizing, moving between displays, minimizing and restoring; those paths
call `start_display_link` / `stop_display_link` as before, and both reset
the idle state.

## Linux and Windows

They get the redraw policy, which is platform independent. Their GPUI
frame sources (gpui-pre-linux and gpui-pre-windows 0.3.7) were read, not
changed:

- **Wayland** is demand driven already. The window's frame loop parks
  (`FrameLoop::Parked`) when a frame neither drew nor asked for another,
  and `schedule_frame`, which GPUI calls at the end of every effect flush
  for a dirty window, wakes it; frames are paced by compositor frame
  callbacks. An idle window does not wake.
- **X11 has the same always-on loop as macOS.** `X11Client::start_refresh_loop`
  inserts a calloop timer at the monitor's refresh rate for every visible
  window, and each tick calls the window's frame callback (and processes X
  events) whether or not anything changed: 60–144 main-thread wakeups a
  second for a visible, idle window. It only stops while the window is
  unmapped or fully obscured.
- **Windows has it too, for every window.** `begin_vsync_thread` runs a
  `VSyncProvider` thread that waits for each vblank (`DwmFlush`) and then
  `RedrawWindow(RDW_INVALIDATE)`s every GPUI window, so each window gets a
  `WM_PAINT` and a frame callback per refresh, forever. Neither backend
  implements `frame_waker`.

The fix for both is the macOS one: park the timer / vsync thread after a
few idle frames and resume it from `frame_waker` (X11: re-insert the timer;
Windows: signal a condition the vsync thread waits on). Neither is small
enough to land without running it: both mean vendoring another GPUI
platform crate, and this Mac can't verify either, so they are left for a
change tested on those systems. Expect each to remove a floor of roughly
the size the macOS change removed (0.3–0.5% here).

## Measurements

M1 Pro, 60 Hz external display, release builds (the release profile),
`scripts/idle-cpu-bench.sh` with a 15 s settle: each fixture launched
alone, then the mean of 30 one-second `top` samples. A terminal-launched
window is never frontmost, so "active" uses `QUILL_ASSUME_ACTIVE=1` and
"inactive" `QUILL_DEMO_DEACTIVATE=1`; "stream10" adds
`QUILL_DEMO_UPDATE_STREAM=10`, "big" the 60-chat / 400-message stress
fixture. Three builds of the same tree, alternating order between the two
runs:

- **before**: the old policy (every streamed update redraws the window,
  as `poll_live` did) and upstream gpui-pre-macos;
- **A**: redraw by urgency, upstream gpui-pre-macos;
- **A+B**: this branch.

Other builds were running on the machine (load average 6–12, 27 during
the first "before" run), so single numbers move by about ±0.5 points.
%CPU, run 1 / run 2:

| Scenario | before | A | A+B |
| --- | --- | --- | --- |
| active, idle | 0.42 / 0.41 | 0.58 / 0.47 | **0.00 / 0.00** |
| inactive, idle | 0.47 / 0.43 | 0.59 / 0.33 | **0.00 / 0.00** |
| active, stream10 | 4.20 / 4.49 | 2.78 / 2.17 | **1.57 / 2.21** |
| active, stream10, big | 8.01 / 5.87 | 2.50 / 1.73 | **2.07 / 1.71** |
| inactive, stream10 | 1.22 / 1.16 | 1.06 / 0.80 | **0.45 / 0.38** |
| active, typing dots | 2.74 / 1.98 | 2.31 / 1.96 | 1.77 / 1.86 |
| active, stickers | 5.40 / 3.57 | 3.46 / 3.18 | 3.30 / 3.07 |
| active, GIF | 7.14 / 4.30 | 4.98 / 4.02 | 5.32 / 6.34 |

The GIF spread looked like a regression, so it was re-run four more times,
alternating: means 4.59 (before), 5.10 (A), 4.94 (A+B), single runs
4.0–6.1 for every build. The display link runs throughout GIF playback
(table above), so B cannot change it, and A doesn't touch it; it is noise.

Reading it:

- Idle, active or not: the display link was the whole floor (0.4–0.6%);
  with it stopped, an idle window reads 0.00%.
- The update stream: redraw by urgency takes the active cost from 4–8% to
  2–2.8% (the stress fixture gains most, as a full redraw of it costs
  most), and stopping the link between batches takes the rest down to
  1.6–2.2% active and 0.4% inactive. What remains is real drawing: the
  stream has someone typing in another chat about a third of the time,
  and those typing dots animate in the chat list at 12 fps, plus one
  batched full redraw a second for the download progress.
- Animations cost what they did: they keep the link running, and neither
  change touches how they draw (see What's left).

## Not measured here

The live account. These numbers are fixtures; the synthetic stream was
sized to reproduce the live 6–8%, but the real mix differs per account.
Re-run `quill-tools/bench` (relaunch, open the same chats, 30 s of `top`)
on a build of this branch to confirm the live numbers; `QUILL_TRACE_NOTIFY=1`
shows far fewer `spawn_poll_loop` notifies, and `ingested:` lines name
whatever still arrives.

## What's left

- Visible animations cost 1.5–4% while they run (typing dots at 12 fps,
  stickers at 24–30 fps, GIFs). Each tick re-renders `QuillApp` as the
  animation layer's ancestor and re-encodes the whole scene, where only the
  animated rectangle needs repainting. While an animation runs, the display
  link also keeps waking at the display rate.
- A full redraw is still 5–8 ms at idle clock speeds (history prepaint and
  layout dominate); urgent updates in a busy open group pay it per batch.
- X11 and Windows frame loops (above): addressed in `codex-idle-frames-x11-windows.md`.

## Risks

- A `ChatList` redraw leaves the conversation and composer slices as they
  were. Anything those slices draw from another chat's row data or from a
  contact's online state (not the open chat's peer) would show stale until
  the next full redraw. Nothing found does: the conversation header reads
  only the open chat, its peer and its group record (all `Now`); the info
  panel, member and contact lists, dialogs and the chat list are drawn by
  `QuillApp` or the chat list slice, which a chat-list redraw re-renders.
- `Later` shows a background change up to a second late while idle (a
  contact's "last seen" text, another user's new name, an option); frames
  drawn for any other reason show it at once. Anything the open chat, a
  request or a download completion changes is `Now`.
- The display-link change is in vendored GPUI code: a frame demand that
  neither dirties the window nor queues a next-frame callback would not
  wake a stopped link. GPUI routes `refresh`, `notify`, `on_next_frame` and
  `request_animation_frame` through those two, and live resize, screen
  changes, activation and occlusion restart the link as before.
