# launch-time-fonts: list the installed fonts once, off the main thread

## Problem
A warm launch of the macOS release build took a median of 551 ms from `open` to the first frame
that draws the chat list. A Time Profiler trace of the launch put about 100 ms of that on one
call made twice on the main thread: `TextSystem::all_font_names`, which lists every installed
family through CoreText's font server.

- The kit's theme `init` lists them to check the family `.SystemUIFont` resolves to
  (`gpui-component` `theme/system_font.rs`), about 50 ms.
- `QuillApp::new` lists them again for the font picker in Settings → Chat
  (`ui/appearance_power.rs` `installed_fonts`), about 46 ms.

## Quill
- The kit keeps one list per process (`installed_font_names`) and now exports it with
  `prefetch_installed_font_names`, which fills the same list on a background task.
- `quill_application` starts the prefetch as soon as the platform exists, about 100 ms before
  the theme's `init` asks for the list. `init` then finds it ready (or waits for the rest).
- The font picker builds its choices from the kit's list instead of listing the fonts again.

The list itself is unchanged: the same `TextSystem::all_font_names` call, sorted and
deduplicated, read by the same two callers.

## Measuring
`QUILL_STARTUP_TRACE=<file>` makes the app append `<milestone> <unix µs>` the first time each
launch milestone is reached (`src/startup_trace.rs`): `main`, `app_run`, `window_open`,
`app_new`, `first_render`, `connect_start`, `prepare_done`, `client_open`, `auth_ready`,
`chats_loaded`, and `chat_list`, the first sidebar frame that has chats. Unset, it costs one
atomic load per call. `scripts/launch-bench.py` builds a signed bundle per binary, launches
them alternately through LaunchServices against the real account, and prints each milestone's
median, min and max in ms after `open`.

Seven alternating warm runs per side on an M1 Pro (load average 5 to 8 from other builds):

| milestone | before | after |
| --- | --- | --- |
| `main` | 103 | 104 |
| theme `init` done | 326 | 258 |
| `app_new` (QuillApp built) | 463 | 319 |
| `chat_list` | 551 (491–770) | 399 (384–414) |

The remaining time to `chat_list` is AppKit and GPUI startup (about 60 ms to reach `main` beyond
what a trivial signed app needs, NSApplication and window creation), and the TDLib chain, which
only starts once `QuillApp` exists: keychain read, client open, `authorizationStateReady` and
the chat list, about 70 ms after the first frame.

A cold launch (after a reboot, with nothing in the file cache) was not measured: dropping the
cache needs `sudo purge`.
