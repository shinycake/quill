# Quality sweep (October 2026)

Owner ask (2026-10-16): a deep pass over performance, memory, UI/UX and
stability on macOS, Windows and Linux, fixing what comes up.

Audited at `72ff1ee5` (main after #856). Four audits ran side by side:
performance and memory (measured on an M1 Pro, 120 Hz, demo fixtures
only), stability (code reading), Linux and Windows (code reading plus the
vendored GPUI platforms and CI), and UI/UX (demo captures in light and dark
compared with tdesktop and the gpui-kit Design Guides). Numbers marked
"inferred" come from reading code, not from a measurement.

Measurement caveat: other builds were running during most CPU runs, so
absolute CPU figures read high. Comparisons within one run hold.

## Where Quill already stands well

- Idle CPU is 0.00% for every static fixture, frontmost and in the
  background (the #501 / #529 work holds). Background CPU with animated
  content stays at or under 0.06%.
- Scrolling 3000 chats or 2000 messages: draw p50 2.6–3.4 ms, p99 under
  4.1 ms against an 8.3 ms budget, no late frames.
- macOS memory over an 8-minute media session oscillates between 137 and
  180 MB with no upward trend and returns to 105 MB.
- Startup to first frame: 421 ms warm, about 1 s cold.
- Panic lints are on workspace-wide; threads and C callbacks go through
  `thread_guard`. No GPUI double-lease path was found (Touch Bar, OS menus,
  dialog builders and should-close handlers were checked).
- Crash reports on this Mac (`~/Library/Logs/DiagnosticReports`): the two
  real crashes (a Touch Bar double lease in a demo build, an overflow in
  `image_budget::pick_variant`) were already fixed on main.

## Findings, ranked by user impact

Status: **fixed** (merged PR), **in progress** (PR or builder running),
**open** (not started), **owner** (needs the owner's decision). Statuses
updated 2026-10-16 after release v2026.1016.2630.

### Crashes, hangs and data

| # | Finding | Platforms | Status |
|---|---|---|---|
| 1 | One panic on the TDLib receive thread ended Telegram for the whole run, and Retry reused the dead hub. | all | fixed #861 |
| 2 | Native crashes (TDLib, ntgcalls, FFmpeg, AppKit) left no trace and no "closed unexpectedly" notice. Now a `logs/running` sentinel like tdesktop's `tdata/working`, and a breadcrumb when TDLib logs a fatal error. Native signal and exception handlers followed in #880. | all | fixed #861, native handlers #880 |
| 3 | The UI thread waits up to 5 s per account for TDLib `close` (account signed out elsewhere, account switch, lock-screen log out, quit). | all | fixed #873 |
| 4 | Spell-check engine construction waited on another thread with no timeout during startup and language changes. | Windows, macOS | fixed #860 |
| 5 | Copy Image, Edit Photo, Edit Image and the sticker-set picker decoded full-size images on the UI thread, outside the bounded decoder (and could not open HEIC). | all | fixed #860 |
| 6 | A panic in the video decode thread leaves the player loading forever. | all | fixed #887 (merging) |
| 7 | Single-instance listener could spin on accept errors and grow threads; caught panics in hot FFI callbacks wrote one report per call; PipeWire lost a buffer per panic. | all / Linux | fixed #861 |

### Performance and memory

| # | Finding | Platforms | Status |
|---|---|---|---|
| 8 | **FFmpeg video path leaks about 47 MB/s** while only the video redraws (1.3 GB after 30 s of an inline video). Retired frames are freed only on a full window render. FFmpeg is the default on Linux and Windows. Measured on macOS with the FFmpeg backend. | Linux, Windows | fixed #870 |
| 9 | The synthetic update stream (10 updates/s) costs 5.5–9% CPU frontmost against a 1% target: the sidebar draws about 40 times a second with 120 Hz bursts while only a 12 fps tick is wanted. | all | fixed #876 |
| 10 | The Do Not Disturb probe forks `defaults` (macOS) or `gdbus` + `gsettings` (Linux) every 5 s on every render, even with nothing to notify. tdesktop asks in-process and only when it notifies. | macOS, Linux | fixed #866 |
| 11 | Chat-list scroll spends 5.8% of main-thread time in path canonicalize syscalls (verdicts expire after 1 s). | all, worse on Windows | fixed #866 |
| 12 | The tray badge count walks every chat and forum topic on every render (about 4.5% while scrolling 3000 chats). | all | fixed #866 |
| 13 | WebM stickers decode by spawning ffmpeg and round-tripping PNGs through disk, 95–145 ms per sticker, and need a system ffmpeg. | all | fixed #886 |
| 14 | Mirrored stickers are cached by count, outside the byte budget (inferred, up to about 94 MB). | all | fixed #867 |
| 15 | Minithumbs drawn as in-memory images are never evicted (inferred). | all | fixed #867 |
| 16 | Histories of chats that are not open, and `media.files`, grow without a cap on live updates (inferred). | all | fixed #867 |
| 17 | Startup deletes old cache directories on the main thread before the window opens (inferred). | all, worse on Windows | fixed #866 |
| 18 | FFmpeg decodes in software with `SWS_ACCURATE_RND`; no D3D11VA or VA-API (inferred). | Linux, Windows | open (video area) |
| 19 | X11 and Windows keep a 1 s heartbeat while parked, where the macOS display link stops (inferred). | Linux X11, Windows | fixed #882; Quill's own idle timers #884 |
| 43 | After browsing photos, the media viewer or a chat full of pictures, idle memory on macOS stayed far above launch: libmalloc kept freed large blocks charged to Quill, leaving a chat of photos never scheduled the image cache's idle trim, and colour emoji pinned image textures in the GPU atlas. Numbers in docs/decisions/sweep-r3-browse-memory.md. | macOS (atlas: all) | fixed #1093; open: reuse the screen-share `nv12_to_i420` buffer and measure CPU in a call, rate-limit `free_heap::release` on idle trim to once per 10 s (as tdesktop), a macOS CI job for `cargo test --lib large_alloc` + clippy, split `image_budget.rs` (12 lines of headroom) before its next change |

### Linux and Windows

| # | Finding | Status |
|---|---|---|
| 20 | CI never opens a window on Windows or Linux; every vendored Windows patch is marked "not run on Windows". An Xvfb + lavapipe demo-capture job on Linux and a WARP one on Windows would catch launch regressions. | fixed #869 |
| 21 | `quill.exe` had no application manifest (gpui is built with `windows-manifest-provided-by-host`): no per-monitor DPI awareness, long paths or UTF-8 code page. | not a bug: gpui-pre's default `windows-manifest` feature already embeds one (#863 found this) |
| 22 | The in-app updater is Unix-only; on Linux it swaps only the binary, so bundled TDLib/ntgcalls can go stale next to a new binary. | fixed #877 |
| 23 | The ffmpeg/ffprobe tools are not shipped on Windows or Linux, and 17 call sites depend on them (video editing, round video, profile camera, video stickers, downscaling). Windows capture takes the first camera and microphone instead of the chosen ones. | open |
| 24 | Linux window `app_id` and desktop file name did not match, so GNOME and KDE showed no icon grouping or pinning. | fixed #863 |
| 25 | Default emoji set "System" shows tofu on minimal Linux installs without a color emoji font. | fixed #874 |
| 26 | File names on save/export: Windows reserved names, trailing dots, extension lost on truncation, 255-byte limit. | fixed #863 |
| 27 | Wayland/Linux gaps: hide-to-tray only minimizes, PiP cannot stay on top, global push-to-talk is X11-only, file paste only through text heuristics, OS idle on GNOME only (idle now also works on KDE and X11 sessions, see sweep-linux-idle-backends.md; wlroots still has none); no Windows Hello / polkit unlock; dragging files out is unsupported everywhere. | open |
| 28 | No Vulkan on Linux exits with an alert that gives no hint how to fix it. | fixed #874 |
| 29 | Small: Linux autostart ignored `XDG_CONFIG_HOME`, `~` expansion used `HOME` (unset on Windows), media Save de-duplication could overwrite. | fixed #863 |

### UI and UX

| # | Finding | Status |
|---|---|---|
| 30 | The forward toast printed raw chat and message ids, and its text was English-only. | fixed #862 |
| 31 | Empty groups and channels showed developer text ("History arrives via getChatHistory…"). | fixed #862 |
| 32 | Selected chat row used a leading accent bar (the Design Guides forbid it) and its secondary text was 3.18:1 in light. | fixed #864 |
| 33 | Sidebar search chips and tabs were hard-coded English; labels sat off the chip baseline; the tab strip clipped with no affordance. | fixed #862 |
| 34 | Contrast misses: outgoing bubble time (3.3/3.1:1), "Terminate" danger text (3.8:1). | fixed #864 |
| 35 | Scrolling dialogs clipped content mid-line against the footer; the bio field showed 1.5 lines. | fixed #864 |
| 36 | "Terminate all other sessions" still uses the kit's old red on a pink fill (2.4:1); the forward panel's disabled Send/Schedule read as absent in dark. | fixed #875 |
| 37 | Edit profile is several mini-forms (per-field Save buttons, unlabeled name fields, a raw path box for the photo). tdesktop has one form with one Save. | fixed #878 |
| 38 | Affordances: "Experimental settings" is bare text, slow-mode selection is only a check glyph, invite-link loading has no spinner, session rows' buttons don't align. | fixed #872, #881 |
| 39 | Focus does not return to the trigger when hand-drawn layers close (sticker panel, forward panel, profile modal, earn layer, composer dialogs); chat rows have no tab stop and Quill has no focus ring of its own. | focus return fixed #872; chat-list keyboard navigation in progress |
| 40 | Copy: three-dot ellipses (fixed in lang files #862); still hard-coded "Forward to…" and call status strings. | fixed #862, #875 |
| 41 | 1545 literal `px()` values and 128 raw colours in `src/ui` keep interface zoom from scaling consistently. | not a zoom defect: zoom scales the logical pixel size, so fixed px scale with text (docs/decisions/sweep-interface-zoom-core.md); colours remain a theme concern |
| — | Selected row: tdesktop uses a solid accent fill with white text; Quill keeps a tint (now AA). Muted unread badge is 2.2:1, the same grey tdesktop uses. | owner |

### Test stability

| # | Finding | Status |
|---|---|---|
| 42 | GPUI UI tests (`rtmp_stream`, `ton_wallet_money`, `group_call_create`, `gift_value_ui`) fail intermittently under machine load and pass on rerun. Every builder in this sweep hit at least one. | fixed #871 |

## Results after the fixes

Measured by each fix PR (details in its decision doc):

- FFmpeg inline video: 1.3 GB after 30 s before, flat at about 110 MB after (#870).
- Update stream: sidebar draws went from bursts of 40/s to a steady 3/s, CPU from about 4.9% to 1.6% under load (#876).
- Parked windows on X11 and Windows: 10 main-thread wakeups in 10 s before, 0 after, measured in CI (#882). The Linux launch smoke now fails above 0.5 wakeups/s.
- Signed-in idle timers: about 15.5 scheduled wakeups/s before, about 0.75/s after, from the timer schedules (#884); not yet measured live.
- WebM stickers: 376 ms to 17 ms for a demo sticker, 604 ms to 77 ms for a 512 px one, with no ffmpeg process (#886).
- Flaky GPUI UI tests: 21 of 48 runs failing under load before, 0 of 20 after (#871).

Every risky fix (#873, #877, #880, #882, #884, #886) had an independent
read-only review before merge; the reviews found real blockers in four of
them, which were fixed first.

## Still open

- 18: hardware video decoding (D3D11VA, VA-API).
- 23: ffmpeg/ffprobe tools for video editing on Windows and Linux; Windows capture device choice.
- 27: Wayland and Linux desktop gaps (OS idle on wlroots needs ext-idle-notify-v1).
- 39: chat-list keyboard navigation (in progress).
- 41: literal `px()` and colour values.
- Owner decisions: solid-fill selected row; muted unread badge contrast.
- None of the Windows or Linux fixes has run on real hardware, only on CI (Xvfb with lavapipe, WARP).

## How each fix is checked

Every fix PR carries a test that fails before and passes after, or a
before/after measurement in its decision doc, and is reviewed before it is
merged through `quill-tools/queue.sh`. Platform code that cannot run here
(Windows, Linux) is pushed to a `cursor/<branch>` branch so the Windows and
Linux package jobs build it before merge. #863's first Windows build failed
with a duplicate manifest resource, which is how that job paid for itself.

## Measurement scripts

The scripts and raw numbers live outside the repo in
`/Volumes/Idan1TB/quill-work/scratch/quality-sweep/` (`sweep-perf/`:
fixtures, idle, scroll, memory, startup and the FFmpeg-on-macOS build).
