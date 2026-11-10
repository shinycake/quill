# Message widgets in a helper process

Status: design, nothing built. Background: `message-widgets.md`.

A widget's document is parsed, scripted and laid out by C++ that Quill did
not write: QuickJS, widget-sandbox's WidgetHost, litehtml with gumbo, and
`native/widget/quill_widget.cpp` over them. #1425 was an abort in that code
(a lock used before it existed), and it took the app down.
`docs/contributing/panic-safety.md` cannot reach C++. This design moves the
whole widget runtime into a helper process. A native crash or a hang then
ends the helper, the widgets it ran show the "Widget stopped" pill, and Quill
starts another helper.

## What Telegram Desktop does (in our words)

Telegram Desktop runs widgets inside its own process. `tgw/` and
`history_view_html_widget.*` start no child process and share no memory with
one. The sandbox engine has a thread of its own, the UI thread stands in for
WidgetHost's host thread, and the painter draws on the UI side. A native
crash in that code ends Telegram Desktop. The rule it does have, two fatal
stops in a row and the widget stays stopped, counts stops the runtime reports
about itself (CPU, memory, the validator). Quill keeps that rule. The helper
is Quill's own addition and has no counterpart there.

## Where Rust and C++ meet today

Three threads touch one live widget: Quill's runtime thread
(`widget_runtime/thread.rs`, one per widget), WidgetHost's host thread and
the sandbox's engine thread (both made by the C++). The UI thread never calls
the C ABI. It sends `Inbox` messages and takes an `Outbox`.

Calls from Rust into C++ (`ffi.rs`) are all made on the runtime thread and
all block until the C++ returns. `qw_resources_init` runs once per process,
`qw_create`, `qw_start` and `qw_destroy` once per runtime. The setters
(`qw_set_visible`, `_theme`, `_data`, `_safe_area`, `_width`), `qw_respond`,
`qw_pointer`, `qw_hover`, `qw_tick` and `qw_trim` return nothing. Four return
data: `qw_get_state` (a struct, every turn of the thread's loop), `qw_paint`
(the display list as JSON, when the document is dirty and shown), `qw_cursor`
(after a hover) and `qw_selected_text` with `qw_clear_selection` (after a
press). `qw_set_max_height`, `qw_wheel` and `qw_key` are in the header and
never called from Rust.

Callbacks from C++ into Rust (`qw_callbacks`). The header says they arrive
"on the WidgetHost thread and on the calling thread", and the renderer ones
run with the renderer's lock held:

| Callback | Synchronous | Rate, read from the code |
| --- | --- | --- |
| `event` | no, it only queues `Inbox::Native` | a `kick` after every renderer call, so up to a few per display frame; the others are rare |
| `create_font` | yes, fills metrics | once per distinct face per widget, a handful to a few dozen |
| `delete_font` | yes | on face eviction and on destroy |
| `text_width` | yes | once per distinct text run per face on a layout (`Face::width` caches in C++, `Fonts::width` caches 4,096 more in Rust). A text-heavy first layout is hundreds of calls in a burst. Canvas `measureText` (`renderer.cpp:4076`) skips the C++ cache, so a chart that measures 20 labels a frame calls it 1,200 times a second |
| `load_image` | yes, returns the size | once per distinct `data:` URL per document |
| `canvas_run` | yes | once per canvas per flush, up to display rate, with the ops as JSON (the sandbox allows 10,000 ops a batch) |
| `canvas_read` | yes, fills a buffer | per `getImageData`, up to 4096 × 4096 × 4 bytes back |
| `canvas_release` | yes | on destroy |

These rates are read from the call sites. Nothing was counted in a running
widget.

## Where the process boundary goes

The boundary is the existing `Runtime` API in `widget_runtime/mod.rs`:
`Inbox` in, `Outbox` out. Everything under it moves into the helper: the C++,
`ffi.rs`, the runtime threads, fonts, images, canvases and the painter.

The owner was told fonts, images and text measuring could stay in Quill or
move, whichever the code supports. The code supports moving all of it:

- `Resources` (fonts, images, canvases) sits behind one mutex that both the
  renderer's callbacks and the painter use. The painter fills glyph outlines
  from the same font handles the renderer measured with, and draws the same
  decoded images and canvas bitmaps. Splitting them puts fonts on both sides.
- Every synchronous callback would become a blocking round trip made under
  the renderer's lock, in bursts of hundreds, and `canvas_read` would carry up
  to 64 MB back. `qw_paint` would send a display list as JSON every frame.
- The `Runtime` API is already one-way messages. `Selection` and
  `Request`/`Respond` look like round trips but are an event and a later
  command. With the cut here no synchronous call crosses the boundary.
- Image decoding (`image`, `resvg`) and font shaping parse bytes a bot chose.
  They are Rust, but they are better off outside Quill too.

So each crossing in the tables above stays an in-process call inside the
helper. What crosses is this:

| Today | Across the boundary |
| --- | --- |
| `Runtime::start(Launch)` | `Start { runtime, html, options, background }` plus a frame region |
| `Inbox::Visible`, `Theme`, `Data`, `SafeArea`, `Width`, `Respond`, `Input`, `Tick`, `Trim`, `Selection` | one message each, same fields, with the runtime's id |
| dropping the `Runtime` (`Inbox::Stop`) | `Stop { runtime }` |
| `Outbox::frame` | the pixels in the shared region, and `Frame { runtime, seq, w, h, dpr }` |
| `Outbox::events` | `Event { runtime, event }`, the `RuntimeEvent` enum |
| `Outbox::wants_frames`, `cursor` | `State { runtime, wants_frames, cursor }`, sent on change |

`widgets_ui.rs` keeps calling `Runtime` as it does now. That file is at 924
lines, so new behavior goes in new files.

## One binary

The helper is `quill --widget-helper`, one more early branch in `main.rs`
next to `--apply-update` and `--video-probe`. It returns before the crash
log, the single-instance check, GPUI and TDLib, so a helper crash writes no
Quill crash report and never touches `logs/clean-exit`.

"Nothing extra to sign or notarize" is true but a weak reason: the bundle
already ships and signs two helper executables (`quill-webview`,
`quill-qr-scanner`). The reasons that hold:

- The runtime lives in the binary crate's `ui` module. A separate executable
  means moving about 3,900 lines into a crate, plus a build step in three
  package workflows and the bundle checkers.
- Starting the big binary is cheap. `quill --version` on the 2026-10-21 local
  macOS arm64 build exits in 10 to 20 ms and peaks at 4.7 MB of footprint
  (`/usr/bin/time -l`, three runs). TDLib and the call library load on
  demand, and the helper never asks for them.
- Updates are applied after Quill exits (`update_install::hand_off`), so the
  file on disk normally is the running build. A package manager can replace
  it under a running Quill, which the handshake catches.

The cost: the helper maps every framework Quill links, which limits how tight
a sandbox can be, and Activity Monitor shows a second `quill`.

## Lifecycle

The supervisor is process-wide, like the `WIDGETS` signal and the font
database, and lives inside `widget_runtime` (its static goes in
`scripts/ui-statics.txt`). It is a pure state machine: an event and the
current time in, a new stage and a list of actions out. Threads and the child
process belong to a thin shell around it. Each helper is one value:

```
enum Helper {
    Absent,
    Spawning { child, deadline },            // waits for Ready
    Ready { link, runtimes },                // at least one runtime
    Lingering { link, until },               // no runtime left
    Closing { child, deadline },             // told to exit
    Backoff { until, failures },             // died, waiting to try again
    Unavailable { reason, retry_at },        // cannot run here
}
```

| Stage | Ways out |
| --- | --- |
| `Absent` | a widget starts: spawn, `Spawning`. A spawn error: `Backoff` |
| `Spawning` | `Ready` from the helper: `Ready`. Exit or end of stream: `Backoff`. Deadline: kill, `Backoff`. A different build or protocol: kill, `Unavailable`. Bundles refused: `Unavailable`. Quill quits: kill |
| `Ready` | last runtime stops: `Lingering`. Exit, end of stream, a malformed message, a flood, or a runtime that stopped answering: kill, tell every runtime, `Backoff` |
| `Lingering` | a widget starts: `Ready`. `until` passes: close the link, `Closing`. Exit: `Absent` |
| `Closing` | exit: `Absent`. Deadline: kill, `Absent`. A widget starts: it waits for `Absent` |
| `Backoff` | `until` passes with a widget waiting: spawn. No widget waiting: `Absent` |
| `Unavailable` | `retry_at` passes and a widget asks: spawn. Otherwise it lasts until Quill restarts |

Spawn: `current_exe()` with `--widget-helper`, no console window on Windows,
stderr kept as a bounded tail for the widget log, and only the environment
font lookup needs. On Linux the helper asks to be killed when its parent dies
(`PR_SET_PDEATHSIG`, as `crates/webview-host` does), on Windows it sits in a
kill-on-close job (as `proxy_carrier` does), and everywhere it exits when its
channel closes.

Handshake: Quill sends `Hello { protocol, build }`. The helper points its own
stdout at stderr, applies its sandbox, loads the JS bundles, starts the font
database on a worker and answers `Ready { protocol, build, bundles }`. Both
sides require equal values. A mismatch means the file on disk changed, and
widgets wait for the restart the update asks for anyway. `bundles: false` is
the #1425 CRLF case: every widget shows the stopped card with that reason, as
today.

Lazy start and idle exit hold, with one addition. A new helper pays for the
font scan and the bundle load again, so a helper lingers 30 s after its last
runtime ends before Quill closes it. The helper never exits by its own
choice.

Crash detection: the reader thread sees the end of the stream and the reaper
(`child_reap.rs`) reports the exit status. Any exit Quill did not ask for is
a death. The status and the stderr tail go to the widget log.

Hang detection: the sandbox's CPU limit already stops a widget's own script
loop. What is left is native code that loops or deadlocks holding the
renderer's lock, which blocks that widget's runtime thread in `qw_get_state`
or `qw_paint`. Each runtime thread reports the sequence number of the last
command it applied. A runtime with a command unanswered for 10 s on the
supervisor's clock is hung, and the helper is killed (a layout query may
block 3 s by design, hence 10). Nothing is sent while nothing is outstanding,
so idle CPU stays at zero.

Backoff: 1 s, doubling to 60 s, reset when a helper closes cleanly or stays
`Ready` for 60 s. Five failures in a row give `Unavailable`, retried after
five minutes.

### What a death does to each widget

- Every runtime of that helper gets `RuntimeEvent::Lost`. Its entry becomes
  `Lifecycle::Stopped` with a typed reason (`StopReason::Limit(text)` or
  `HelperDied`, in place of today's string), so the bubble draws the last
  frame with the pill, or the card when there was no frame.
- Local storage is safe. Every commit already reached Quill as a `Storage`
  event and is written from there. Session storage lives in `WidgetsUi`. The
  next start hands both back.
- The script's in-memory state is lost, as when a widget is released today.
- Queries in flight are dropped. `entry.queries` is cleared on start, so a
  late answer never reaches the new runtime. Runtime ids carry the helper's
  generation, so a late message from a dead helper matches nothing.
- Poll timers, height and the last frame stay with the entry.

Restart and blame. Today a stopped widget starts again only when it comes
back on screen, and two stops in a row keep it down. A helper death is
different: with several widgets in one helper, Quill cannot tell which one
caused it, and counting a strike for each would let one bad widget switch off
its neighbors after two crashes. The rule:

- A widget stopped with `HelperDied` starts again by itself once a helper is
  ready, if it is still on screen and under `MAX_CRASHES`.
- If it was the only live widget in the helper that died, it takes a strike.
- If it shared the helper, it takes no strike and becomes a suspect. A
  suspect's next run is in a helper of its own (`Placement::Solo`). If that
  helper dies, it takes the strike. If its run ends any other way, it is
  cleared and uses the shared helper next time.

So there is one shared helper, plus one solo helper per suspect for as long
as that suspect runs. Solo helpers use the same `Helper` state machine and
close with their runtime, with no linger.

## Transport

Control channel, one per helper:

- macOS and Linux: a Unix socket pair. The helper's end is its stdin, which
  `std::process::Command` can set without a pre-exec hook. One socket carries
  both directions and can pass descriptors.
- Windows: two anonymous pipes as stdin and stdout.

So "a pipe" is right on Windows only. On Unix it has to be a socket, because
the frame memory is passed over it.

Framing: one JSON object per line, with the bounded line reader the mini-app
helper already uses (`crates/webview-protocol`, `lines`). Lines are capped at
1 MiB in both directions. Nearly every payload is JSON already (theme, data,
options, storage), and the bulky parts (display lists, canvas ops, pixels)
never go through the channel.

Frame memory, one region per runtime, with no names in any namespace:

- Linux: `memfd_create`, sealed against shrinking and growing, sent with
  `SCM_RIGHTS`.
- macOS: `shm_open` under a random name, unlinked at once, sized once, sent
  with `SCM_RIGHTS`.
- Windows: an unnamed pagefile section, duplicated into the helper with
  `DuplicateHandle`. Quill holds the child's handle, and the handle value
  goes in the `Start` message.

Quill creates each region, sized for the widget's width and `MAX_HEIGHT` at
its scale, and maps it read-only. The helper maps it writable. A wider widget
gets a new region with its `Width` message. A hidden widget's region is
dropped with `Trim`.

A hostile helper must not be able to fault Quill by shrinking a region it
shares. The Linux seals and Windows sections prevent that. On macOS I expect
a second `ftruncate` on a shared memory object to fail, and PR 2 needs a test
that proves it before the design is final.

Frames are not zero copy, and the owner was told they would be. GPUI's
`RenderImage` owns its pixels (`third_party/gpui-pre/src/assets.rs`), so
Quill copies each frame out of the region once. Today's path also copies once
(`pixmap.data().to_vec()`, then the BGRA swap). In the helper the swap writes
straight into the region, so the total is one CPU copy more than today. Real
zero copy needs a shared GPU surface per OS and a GPUI patch (see Rejected).

Frame handoff: the helper writes a frame, sends `Frame { seq, … }` and leaves
the region alone until Quill answers `FrameTaken { seq }`. Meanwhile it keeps
painting into its private pixmap, and the newest picture wins, which is what
`Outbox::frame` does today. Quill's reader thread makes the copy, so the UI
thread still never waits on a widget.

Backpressure:

- Quill to helper: a bounded queue on a writer thread. `Tick`, hover and
  pointer moves, `Width` and `Theme` replace an older one still queued for
  the same runtime. A queue that reaches its cap means the helper stopped
  reading, and it is treated as hung.
- Helper to Quill: at most one frame in flight per runtime. Events are
  counted against a budget of messages and bytes not yet landed by the UI,
  and going over it is a death. Counts decide this, never a clock.

Versioning: `protocol` is a number, bumped on any change to a message, and
the handshake also compares the build id. Unknown message types and unknown
fields are errors on both sides.

## Security

The helper trusts Quill: it validates nothing beyond what the C++ already
does. Quill trusts nothing from the helper, since a widget that breaks out of
QuickJS controls it. Today Quill takes several things on the sandbox's word.
Each becomes a check in Quill:

- A message for a runtime that is not live in this helper's generation is
  dropped.
- `Frame`: `w × h × 4` must fit the region Quill made, and `w` and `h` must
  be within what Quill asked for. A helper that writes during the handoff can
  only tear its own picture.
- `Height` is checked finite and clamped to 50 to 600 again.
- `Link` and `Copy` need a press. Quill grants one activation per pointer
  release it forwarded to that runtime, and one `Link` or `Copy` spends it.
  The sandbox enforces this today and Quill takes its word. Without the
  check a compromised helper could replace the clipboard at will. Links still
  go through the confirmation box.
- `Request`: Quill checks `can_send_queries` from the message itself, caps
  the data size and allows one query in flight per runtime, the rule the
  sandbox applies today.
- `Storage`: an area over `QUOTA_BYTES` is refused. Today the quota is only
  checked when a file is read back.
- `Selection`, `Copy` and cursor names are capped in size, and an unknown
  cursor is the default one.
- A line that is too long, is not JSON or has an unknown type kills the
  helper.

One limit stays. The shared helper holds widgets from different bots and
accounts in one address space, as Quill's own process does today. A widget
that takes over the helper can read another widget's data and storage and
send it to its own bot through a query. One helper per widget would close
that at a memory and start-up cost (see Rejected). The OS sandbox does not.

## Sandboxing

Optional, and last. After `Ready` the helper needs its channel, the regions
it is handed, memory, threads, clocks and read access to font files.
cosmic-text opens a font file when a face is first used, so the font folders
stay readable, which rules out the strictest profiles. It needs no network,
no writes, no child process and no window.

- macOS: a custom default-deny profile applied with `sandbox_init` at helper
  start, allowing reads of the system and user font folders. The call is
  deprecated, but Chromium and Firefox still use it and notarization accepts
  it.
- Linux: `PR_SET_NO_NEW_PRIVS`, a seccomp filter (no `socket`, `execve` or
  `ptrace`) and Landlock read rules for the font folders. Landlock needs
  kernel 5.13; without it seccomp still applies. Both nest inside Flatpak.
  New dependencies: `seccompiler` and `landlock`.
- Windows: the job object gains limits (one process, a memory cap, the UI
  restrictions) and the helper sets `SetProcessMitigationPolicy` for child
  processes and extension points. AppContainer would hide per-user fonts
  under `%LOCALAPPDATA%`, so it is out. A low-integrity token needs
  `CreateProcessAsUser`, which `std::process::Command` cannot do. Win32k
  lockdown is not practical while the helper is the GUI binary.

A sandbox that fails to apply is logged and the helper runs without it.
Crash isolation does not depend on it.

## Memory and latency

Nothing here was measured on a running helper. PR 2 measures it with
`footprint` and the frame clock. Derivation, macOS arm64:

- Process baseline: 4.7 MB (measured, above).
- JS bundles copied once by `qw_resources_init`: 1.9 MB (file sizes).
- Font database: unmeasured, a few MB.
- Per live widget: the sandbox's QuickJS heap, capped at 8 MB
  (`memoryLimitBytes`), the validator, the document, three or more thread
  stacks, and a private pixmap plus a region of the same size (1.9 MB each
  at 800 × 600 px).

One or two simple widgets land in the 15 to 25 MB the owner was told. Eight
heavy ones could pass 100 MB because of the 8 MB heaps. Most of this is
memory Quill holds itself today and would stop holding. What the design adds
is the baseline plus one frame buffer per live widget, about 5 + 2n MB for n
widgets.

Latency: an input goes through a writer thread, a socket and the helper's
reader before the runtime thread sees it, and the frame comes back through a
message and one copy. Four thread wakeups cost tens of microseconds each, and
the copy 0.2 to 1 ms for a 2 to 5 MB frame. That is about 0.3 to 1.2 ms on
top of today's path, 2 to 7% of a 60 Hz frame, so a press shows one display
frame later about that often. "A frame of input latency" is the worst case.

Start-up: 10 to 20 ms to reach `main` (measured), then the font scan and the
bundle load, which the first widget already pays today inside Quill.

## Files and state

All under `src/ui/widget_runtime/`, each well under 1,000 lines: `engine.rs`
(today's `thread.rs`, run by the helper), `protocol.rs`, `helper_main.rs`,
`supervisor.rs` (the pure state machine), `supervisor_io.rs` (spawn, reader,
writer, reaper), `transport/{unix,windows}.rs` (the only new `unsafe`) and
`sandbox/{macos,linux,windows}.rs`. `Lifecycle` and `StopReason` stay in
`src/message_widget`. No stage is a bool, and the supervisor's clock is
injected.

## Tests

No test sleeps or waits on wall time. The supervisor takes its clock as an
argument, and tests with a real process block on the runtime's own events, as
the smoke tests do now.

- Supervisor, with a fake link and a manual clock: every stage and way out
  in the table, backoff steps and reset, linger, build mismatch, refused
  bundles, a flood, a full outgoing queue, an unanswered command with the
  clock moved past 10 s, a late message from an old generation, a shared
  death with one widget and with three, a suspect's solo death and clearing.
- Protocol: round trips and each check under Security failing as it should.
- Transport, per OS, both ends in one test process: a region made, passed,
  written and read, and a shrink attempt.
- Fake helper: a scripted link that plays the helper's side from a list. It
  drives the `widgets_ui` tests (pill after `Lost`, auto restart, strikes).
- Real helper: the test binary runs itself as the helper through an ignored
  child test, as `a_refused_bundle_stops_the_widget` does, boots `tabs.html`
  and finds drawn pixels through the region.
- Crash injection: the child, under `cfg(test)`, reads
  `QUILL_WIDGET_HELPER_FAULT` and calls `abort()` when the widget starts, or
  stops reading. The parent sees `Lost`, moves the clock through the backoff,
  and the widget paints again from a new helper. The CRLF test keeps passing,
  with the refusal now inside the helper.
- CI: `windows-widgets` runs all of it under cdb, and the same tests run on
  Linux and macOS. Linux widgets are untested today.
- `/verify`: `ready-widgets` through the real helper, and a demo step that
  kills the helper and captures the pill, then the redrawn widget.
- By hand, once per OS: abort the helper and confirm that no crash dialog,
  Dock icon or console window appears and Quill's own crash log stays empty.

## Pull requests

Four, one more than the owner was told. The fourth is optional.

1. The boundary in process: split `thread.rs` into the engine and the
   `Runtime` front, add the protocol, the pure supervisor and the fake link,
   run the engine over an in-memory link, add `StopReason` and the checks
   under Security. Nothing changes for users.
2. The process: `--widget-helper`, transports and regions on three OSes,
   spawn, handshake, linger, crash detection, backoff, pill and auto restart,
   the real-helper and crash-injection tests on three OSes, and measured
   memory and latency. Production stops running the engine in Quill.
3. Hangs and blame: the unanswered-command watchdog, suspects and solo
   helpers, the outgoing queue cap.
4. Sandboxing per OS.

## Rejected

- Only the C++ in the helper, with fonts, images and the painter in Quill:
  the synchronous-callback design, see "Where the process boundary goes".
- A separate `quill-widget` executable. It allows a tighter sandbox and takes
  about 9 MiB out of the main binary, at the cost of a crate move, packaging
  in three workflows and a second build to keep in step. Worth a second look
  if PR 4's sandbox turns out too loose.
- One helper per widget, always. Exact blame and no shared address space,
  but each helper repeats the 4.7 MB baseline, the bundles and the font scan.
  Suspects get this treatment anyway.
- Frames through the channel. A 1.9 MB frame at 60 Hz is 115 MB a second
  through 64 KB pipe buffers, and a large frame delays the input behind it.
- Named shared memory. Names collide and leak after a crash, and a sandbox
  would have to allow opening them.
- A shared GPU surface (IOSurface, dmabuf, a DXGI handle). The only real
  zero copy, for three backends and a GPUI patch, to save under a
  millisecond per frame.
- The C++ compiled to WebAssembly inside Quill. A crash becomes a trap, but
  it needs a wasm toolchain on three CI systems and a runtime dependency,
  loses the system ICU on Windows, and still needs a watchdog.
- Catching the signal or the abort in process and carrying on. The crashed
  thread's heap and locks are in an unknown state.
- Running widgets inside Quill when the helper cannot start. That brings
  #1425 back in exactly the broken setups.

## Open questions for the owner

1. One binary or a separate small one? This note picks one binary.
2. After a helper death, widgets on screen restart by themselves. The other
   choice is today's rule: only after they leave the screen and come back.
3. Is the shared address space acceptable (as it is today), or should
   helpers be split per account, or per bot?
4. May PR 4 add `seccompiler` and `landlock`, and use the deprecated
   `sandbox_init` on macOS?
5. The numbers: 30 s linger, 10 s hang limit, backoff 1 to 60 s, five
   failures. And should the number of solo helpers have a cap?
