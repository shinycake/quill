# launch-time-macos-appkit: move AppKit work off the path to the first frame

## Problem
Idan asked for a cold macOS launch that shows the window "in half a bounce" after a Dock click.
Traces of cold launches (after `sudo purge`) put the time to the first chat-list frame in three
places:

1. Before `main`: about 400 ms cold under LaunchServices, about 40 ms warm. A hello app that
   links the same 34 system frameworks takes about 30 ms cold, even padded to the same 96 MB,
   so most of this is Quill's own binary being paged in (`time -l quill --version`: about
   1600 to 2700 page faults cold, 16 warm). Static initializers are not the cause: the 608
   `__mod_init_func` entries sit on 10 pages.
2. AppKit and WindowServer setup on the main thread: the WindowServer connection and its TCC
   check, screen lookups, `NSWindow` creation, LaunchServices lookups. These are XPC waits
   that Quill does not control.
3. Work Quill or GPUI asked for that the first frame does not need. A System Trace of a cold
   direct launch showed the main thread blocked on:

| cause | blocked, cold |
| --- | --- |
| keyboard layout (`TISCopyCurrentKeyboardLayoutInputSource`) | ~87 ms |
| Writing Tools framework load (default main menu text input items) | ~100 ms |
| tray status item | ~33 ms |

TDLib is not on the path: early connect (`launch-time-early-connect.md`) has the chats ready
by the time the window opens, so the first frame already draws the chat list.

## Quill
- The keyboard layout is read after the first frame (`third_party/gpui-pre-macos`, see its
  QUILL-CHANGES.md). Before that, keys map as on a US layout, which only matters for a key
  typed before the window has drawn.
- GPUI sets an empty main menu before `[NSApp run]`, so `finishLaunching` does not build a
  default one. Quill installs its menus from `window.on_next_frame` (`install_app_menus`).
  `+[NSTextInputContext initialize]` still loads the Writing Tools framework during launch,
  so this moves only part of that cost.
- The tray icon and dock badge loop waits for the first frame (`install_main_window_tray`).
  A window that starts in the tray draws nothing, so there both go in at once.

## Tried and dropped
- Loading the Writing Tools framework on a worker thread at `main`. `dlopen` holds the
  dyld and Objective-C runtime locks, so the main thread waited on them instead and the
  platform setup took about 200 ms longer.
- Opening the window from `applicationWillFinishLaunching:` instead of
  `applicationDidFinishLaunching:`. Under a LaunchServices launch AppKit sits between the
  two for about 370 ms cold (62 ms warm) waiting for the open-application Apple Event; the
  window still appeared at the same time.

## Measuring
`QUILL_STARTUP_TRACE` now also writes `exec`, the process start time from the kernel, and the
marks `instance_checked`, `sweeps_done`, `prefs_loaded`, `platform_ready`, `kit_ready` and
`chat_list_shown` (the frame after `chat_list`, so the chat list is on screen).
`scripts/launch-bench.py` gained `--until`, `--data-root` and per-side environment
(`LABEL=BINARY@VAR=VALUE`), and quits only the bundle it launched.

Alternating runs against the real account on an M1 Pro with other builds running, ms after
`open` to `chat_list`:

| launch | before | after |
| --- | --- | --- |
| cold (median of 6) | 1537 | 1425 |
| warm (median of 8) | 384 | 367 |

Telegram Desktop took 3488 ms cold on the same machine in October.

## What is left
Half a Dock bounce (about 150 to 250 ms) is not reachable cold with these changes: about
400 ms goes before `main` and about 300 ms to AppKit and WindowServer. The levers left are
the size of the binary that is paged in (an order file that groups launch code, or fewer
pages touched before `main`) and anything that lets AppKit show the window sooner.

## Round 2: chats in the first frame
A System Trace of a warm launch showed three more waits the main thread did not need:

- `App::new` asks WindowServer for the display list and the first window asks Metal for its
  devices. Both are XPC round trips. `src/launch_prewarm.rs` makes the same two calls on a
  worker at the top of `ui_main`, so the main thread finds them answered. The first frame
  came about 30 ms sooner.
- The disk guard's open check read free space through `NSURL` resource values (the
  CacheDelete "important" capacity). That holds the file cache lock AppKit's window setup
  also takes. The open check now reads `statfs` first and only asks for the purgeable
  figure when `statfs` says the disk is short. The watcher's first read waits for the first
  frame.
- The first chat page was asked for by the main thread after it had built the window.
  TDLib answers a `loadChats` sent before authorization is restored with 401, so it cannot
  go with the open request. Instead the hub's receive thread sends it the moment
  `authorizationStateReady` arrives (`Routes::on_ready`), while the main thread is still in
  AppKit's launch. The page has no request id; the chats come in as updates.

`QUILL_STARTUP_TRACE` gained `presented` and `chat_list_presented`, written when the macOS
backend hands a frame to the compositor (`gpui_macos::on_frame_presented`).

Warm launches against the real account, median of 8, ms after `open`:

| mark | before | after |
| --- | --- | --- |
| first frame drawn | 372 | 358 |
| chat list on screen (`chat_list_presented`) | 472 | 437 |

With the change, the first frame that reaches the screen already holds the chat list.

Tried and dropped: opening TDLib before `application.run` (the launch got about 25 ms
slower) and sending `loadChats` with the open request (401 Unauthorized).

What is left on the warm path is drawing that first frame, about 80 ms: color emoji
rasterized on the main thread on atlas misses (about 20 ms), layout (about 20 ms), line
shaping (about 13 ms) and the Core Animation commit (about 18 ms).
