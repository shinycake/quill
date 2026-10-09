# Idle CPU: redraw by urgency, and let the display link sleep

The 2026-10-08 benchmark against Telegram Desktop 7.2.9 (M1 Pro, same
account, release build of main `0d4e799d`, `quill-tools/bench/RESULTS.md`)
had Quill at 6.1% CPU frontmost 60 s after launch, 8.0% frontmost and idle
after opening a few chats, and 1.0% in the background. Telegram Desktop:
0.2%, 0.7% and 0.2%. Goal: at most 1% frontmost and 0.3% in the background
with nothing animating, and redraws only for animations that are visible.

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
  frame callback) and a CoreVideo thread wake 120 times a second on a
  ProMotion display even when nothing is drawn. That was the whole
  0.3–0.4% floor of an idle fixture, active or not, and above the 0.3%
  background goal by itself. The platform hook to fix it exists in GPUI
  (`PlatformWindow::frame_waker`, which the invalidator calls when the
  window becomes dirty; Wayland and the web use it) but the macOS backend
  doesn't implement it.
- Smaller: 1 s timers (tray, passcode), the 250 ms link inbox and the
  poll loop's 120 ms idle cadence cost nothing measurable (an idle fixture
  with the display link fixed reads 0.00%).

To measure the stream without an account, `QUILL_DEMO_UPDATE_STREAM=<n>`
(`src/ui/demo_stream.rs`) feeds a demo `n` synthetic TDLib updates a
second, through the real reducer and the poll loop's redraw policy. The
mix repeats every eight updates: a contact goes offline, someone starts
typing in another chat, an update Quill does not parse, the contact comes
back online, the typing stops, unread totals change, an automatic download
progresses, another chat's outbox is read. At 10 updates a second, main
with this fixture alone sat at 5.5–7.5%, the same range as the live
account, with `notify: 9.6/s` in the trace.

## Changes

### Redraw by urgency (all platforms)

`quill::state::redraw_need` (`src/state/redraw.rs`) grades each envelope
before it is applied, against the session as it is:

| Grade | Envelopes | Redraw |
| --- | --- | --- |
| Nothing | unparsed updates, unread totals, unchanged statuses, unknown users, `ok` for `viewMessages` / `setOnline` | none |
| ChatList | for chats that are not shown: typing, new and last messages, positions, read state, mention / reaction counts, drafts, notification settings, list membership; a contact's online dot flipping | the chat list slice only, at most every 400 ms (2 s behind another app) |
| Later | automatic downloads, user / group records, `last seen` changes, options, online counts of other chats | a full redraw at most every 1 s (4 s behind another app) |
| Now | everything else: answers to our requests, the open chat (or the comment thread shown in its place), its peer's status, user-started downloads, uploads, auth, connection, calls, any update type not graded | a full redraw at once (every 500 ms behind another app, as before) |

The poll loop keeps the most urgent grade of each batch. Results the UI
drains at render (status notes, forward results, exports, links, failed
sends, expired bot messages, proxy switches) and any queued desktop
notification or forced reply are `Now`, so a notification is never held
back. `PolledRedraw` (`src/ui/notifications.rs`) holds back what isn't due
yet and never drops it: the poll loop calls back at least every 120 ms.
A chat-list redraw notifies the chat list slice, which re-renders
`QuillApp` as its ancestor (chrome, side panels, render-time drains) while
the conversation and composer replay their last frame
(`notify_chat_list`; outside the ready layout everything redraws).

### macOS: the display link stops while idle

`third_party/gpui-pre-macos` is GPUI's macOS platform (0.3.7, vendored
like `third_party/gpui-base`; commit `e49b3724` is the pristine copy, the
changes are in `QUILL-CHANGES.md` and marked in each file). A step that
neither draws, presents nor asks for another frame is idle; after 250 ms
of those the window unsubscribes from the display link. `frame_waker`
requests one step on the window's dispatch source while the link is idle
(GPUI calls it when the window becomes dirty or queues a next-frame
callback); that step draws immediately and resubscribes. While the link
runs, or is stopped because the window is hidden, the waker does nothing,
so frames stay paced by the display. Animations at 4 fps and up keep the
link running; GPUI keeps presenting for a second after high-rate input,
which counts as activity.

250 ms rather than 1 s: measured side by side, the shorter timeout was
equal or slightly better everywhere (stream, typing, stickers) and lets the
link sleep between batched background redraws.

## Measurements

M1 Pro, release builds with line tables
(`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, same codegen as release),
`scripts/idle-cpu-bench.sh`: each fixture launched alone, ready marker plus
20 s, then the mean of 30 one-second `top` samples. "Before" is main
`0d4e799d` plus only the stream fixture (old redraw policy), "after" is
this branch. A terminal-launched window is never frontmost, so "active"
uses `QUILL_ASSUME_ACTIVE=1` and "inactive" `QUILL_DEMO_DEACTIVATE=1`.
Two alternating runs each:

RESULTS_TABLE

## Not measured here

The live account. These numbers are fixtures; the synthetic stream was
sized to reproduce the live 6–8%, but the real mix differs per account.
Re-run `quill-tools/bench` (relaunch, open the same chats, 30 s of `top`)
on a build of this branch to confirm the live numbers; `QUILL_TRACE_NOTIFY=1`
now shows far fewer `spawn_poll_loop` notifies, and `ingested:` lines name
whatever still arrives.

## What's left

- Visible animations cost 1.5–4% while they run (typing dots at 12 fps,
  stickers at 24–30 fps, GIFs). Each tick re-renders `QuillApp` as the
  animation layer's ancestor and re-encodes the whole scene; Telegram
  Desktop repaints only the animated rectangle. While an animation runs,
  the display link also keeps waking at the display rate.
- A full redraw is still 5–8 ms at idle clock speeds (history prepaint and
  layout dominate); urgent updates in a busy open group pay it per batch.
- Linux and Windows get the redraw policy; their GPUI frame sources were
  not changed (Wayland is already demand-driven through `frame_waker`;
  X11 and Windows were not profiled here).

## Risks

- A `ChatList` redraw leaves the conversation and composer slices as they
  were. Anything those slices draw from another chat's row data or from a
  contact's online state (not the open chat's peer) would show stale until
  the next full redraw. Nothing found does; the open chat, its comment
  thread and its peer are always `Now`.
- `Later` delays an automatic download's completion (an avatar or
  thumbnail appearing) by up to a second while idle; frames drawn for any
  other reason show it at once.
- The display-link change is in vendored GPUI code: a frame demand that
  neither dirties the window nor queues a next-frame callback would not
  wake a stopped link. GPUI routes `refresh`, `notify` and
  `request_animation_frame` through those two, and live resize, screen
  changes, activation and occlusion restart the link as before.
