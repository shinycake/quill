# launch-warm-waits: what the main thread waited on before the first frame

## Problem
After `launch-cold-floor.md` a warm launch (Quill opened again after a quit, the file cache
full) put the window on screen about 350 ms after `open` (local release builds, medians of 8
to 10, 2026-11-06). The floor app in `scripts/launch-floor/`, which opens one Metal window
and nothing else, is on screen at about 225 ms. Time Profiler alone did not say where the
difference went: it samples only threads that run, and much of the gap was the main thread
waiting.

## How it was found
`scripts/launch-profile.py` records a Time Profiler trace of every process around each
launch, keeps the samples of Quill's main thread, lines them up with the
`QUILL_STARTUP_TRACE` marks and prints, for each gap between two marks, how many ms of
samples fell in it and which Quill function they were under. A gap much longer than its
samples is a wait. A System Trace of the same launch then gives the main thread's blocked
intervals and the thread that woke it. The waits longer than 1.5 ms, warm, in ms:

| wait | ms |
| --- | ---: |
| theme init on the installed-font list (the prefetch worker's `all_font_names` takes about 130 ms) | 27.6 |
| WindowServer (SkyLight) after `platform_ready` | 23.6 |
| `NSScreen` setup (`dispatch_once`) | about 20 |
| `-[NSApplication init]`, `_NXIsHiddenOnLaunch` | 16.3 |
| `UAZoomEnabled` when the window first sets its first responder | 6.3 |
| the first `poll_live` | 4.6 |
| `-[NSApplication init]`, appearance | 4.5 |

The same profile showed the menu bar being built inside the first frame: next-frame
callbacks run before that frame draws.

## Decision
- Theme init no longer reads the installed-font list when the default fonts load under their
  own names (`third_party/gpui-component/QUILL-CHANGES.md`). The list is still built on the
  prefetch worker for the font picker. On Linux and Windows the same wait goes.
- The launch prewarm worker calls `UAZoomEnabled` once, next to the display list and the GPU
  list. Its answer is cached for the process, so AppKit's call in `makeFirstResponder:`
  returns at once.
- The menu bar is built on the main queue's first turn after the first frame, instead of in
  a next-frame callback.
- On macOS, when TDLib is ready but the launch's first chat page is not in, the first frame
  waits up to 50 ms for the page. A page that misses the first frame on a cold launch waits
  behind the run loop's first pass, which loads Writing Tools (280 to 370 ms after `purge`),
  so its chats are drawn about 400 ms after the window. The receive thread moves
  `hub::LAUNCH_CHATS` along: to asked on `authorizationStateReady`, when it sends the page's
  `loadChats` (`@extra` `quill-launch-chats`), and to in on that request's answer or on an
  authorization state with no chats. The first chats came 9 to 15 ms after
  `authorizationStateReady` in all 20 launches of the second cold bench below. A TDLib still
  opening its database is further off (up to 150 ms in these runs), so the window does not
  wait for it. The wait happens once, and a main window opened later never waits. Linux and
  Windows lose only one poll tick when the page is late, so they do not wait.

The WindowServer, `NSScreen` and `_NXIsHiddenOnLaunch` waits are AppKit's own and stay.

## Results
Release builds, alternating, 10 runs each on 2026-11-06, load average 8 to 15 from other
work on the machine. Medians in ms after `open`.

Warm, `main` (with the ThorVG fix) against this branch:

| step | main | branch |
| --- | ---: | ---: |
| kit ready | 215 | 201 |
| window ordered front | 260 | 237 |
| chat list drawn | 271 | 248 |
| first frame presented, with the chats | 332 | 301 |
| window on screen | 344 | 314 |

Every branch launch put the window on screen sooner than every `main` launch (292 to 333 ms
against 337 to 372). The first frame's display call took 12.4 to 14.4 ms to present on `main`
and 6.3 to 6.6 ms on the branch. The difference is the menu bar, which has the same menus and
items on both (listed through System Events after launch).

Cold, after `purge`, the fixes above against the same plus an earlier form of the chat wait
(up to 100 ms whether or not TDLib was ready), 10 runs each, two benches:

| step | without | with |
| --- | ---: | ---: |
| window ordered front | 666, 653 | 672, 649 |
| chat list drawn | 691, 679 | 691, 669 |
| window on screen | 798, 827 | 805, 786 |

The window came no later. Without the wait, 4 of the 20 launches drew the chats behind the
Writing Tools load, at 1096 to 1177 ms. With it, none did, yet the second bench's per-launch
marks show that the wait never blocked: TDLib had answered before the first frame in all 10,
so the difference was luck. The two late launches in that bench without the wait were of two
kinds. In one, TDLib was ready 6 ms before the first frame's poll and its first chats came
9 ms after it, so a short wait would have drawn them with the window. In the other, TDLib
became ready 151 ms after the window was ordered front: no short wait helps, and a long one
would hold the window back. So the wait now starts only once TDLib is ready.

## Measuring
Release builds with `MACOSX_DEPLOYMENT_TARGET=14.0`, on an M1 Pro running macOS 27.2:

    QUILL_ENV_FILE=…/quill.local.env QUILL_CODESIGN_IDENTITY="Apple Development: …" \
      python3 scripts/launch-bench.py --onscreen --runs 10 --json runs.json \
      --template dist/Quill.app base=<main binary> new=<branch binary>
    QUILL_ENV_FILE=…/quill.local.env python3 scripts/launch-profile.py --runs 5 new

Add `--cold` to both for launches after `purge`, and `--until chat_list_presented` to the
bench for the first frame's marks. `--json` writes every run's marks. `launch-profile.py`
keeps its traces in the session's temporary directory (writing a trace to a removable volume
asks for consent), and `--from DIR` reports on them again. For the blocked intervals, record
the `System Trace` template the same way and export its `thread-state` table.
