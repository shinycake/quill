# launch-window-first: show the window before AppKit finishes launching

## Problem
Idan wants a cold macOS launch to show Quill within half a Dock bounce. After the earlier
launch work (`launch-time-macos-appkit.md`), the window of a cold launch reached the screen
about 1.36 s after `open`, and a warm one about 0.38 s after.

The biggest single wait was between `[NSApp run]` and `applicationDidFinishLaunching:`,
about 370 ms cold and 50 ms warm, and GPUI opened the app's windows only after it, from the
did-finish callback. The earlier round tried opening the window from
`applicationWillFinishLaunching:`, which comes before that wait, and the window still
appeared at the same time.

## What the traces showed
- A bare AppKit app that orders its window front and flushes the Core Animation transaction
  before `[NSApp run]` is on screen before `finishLaunching` returns: about 690 ms cold
  instead of 1130, about 200 ms warm instead of 270.
- Doing the same in Quill gained only about 60 ms cold. Ordering the window front made AppKit
  create the view's `NSTextInputContext` (`-[NSWindow _setFrameCommon:display:fromServer:]`
  calls `-[NSView _inputContext]`). The class's `+initialize` loads the Writing Tools
  framework, about 280 ms cold: 90 ms of main-thread work in dyld and the Objective-C runtime,
  and the rest paging the framework in. In the old order the same load ran inside
  `finishLaunching`, from `-[NSApplication updateWindows]`, so it was always on the path to the
  window.
- `-[NSView _inputContext]` (macOS 27.2) asks a view that responds to `isEditable` first and
  returns nil, without remembering the answer, when it says NO. Otherwise it creates the
  context for any `NSTextInputClient`.

## Quill
- `third_party/gpui-pre-macos/src/platform.rs`: `run` calls the app's launch callback before
  `[NSApp run]` and flushes the Core Animation transaction, so the windows opened there reach
  the screen while `finishLaunching` waits for the open-application Apple Event and loads text
  input. `did_finish_launching` finds no callback left. GPUI's
  `NSAutoFillHeuristicControllerEnabled` default, written in `applicationWillFinishLaunching:`
  before, is now written in `run` ahead of the callback, so it is still set before any window
  or text input exists.
- `third_party/gpui-pre-macos/src/window.rs`: `GPUIView` answers `isEditable` with NO until a
  window has drawn a frame. The Writing Tools load moves to `finishLaunching`, after the window
  is on screen. AppKit makes and activates the view's input context on its next window update;
  right after launch, cold and warm, the key view's context was AppKit's current one and
  active (checked with `lldb` on the launched bundle). Synthetic key events typed "café" into
  chat search, the é through the Option-E dead key, a few hundred ms after the window
  appeared, and again after revealing a window started in the tray (`--start-minimized`).
  The view used not to implement `isEditable`, which AppKit treats as editable; the Edit menu,
  AppKit's AutoFill, Start Dictation and Emoji & Symbols items included, has the same items
  enabled with nothing focused as before the change.
- `src/main.rs`: on macOS the main window opens straight from the launch callback. It used to
  be spawned onto the next main-queue turn, which only runs inside `[NSApp run]`. Linux and
  Windows still spawn it.
- `src/spellcheck/mod.rs`: the embedded word list's rank table is built on the first check.
  The launch's provisional checker built it on the main thread (about 4 ms); the platform
  engine usually replaces that checker before any text is checked.
- Startup trace marks: `app_built`, and AppKit's launch steps through
  `gpui_macos::on_launch_mark` (`ns_run`, `will_finish`, `did_finish`, `window_ordered`,
  `first_display`), and `td_ready` and `td_chat_position` from the TDLib receive thread, to see
  whether the chats are ready before the first frame.

The window now shows before the app has activated. Over eleven launches with another app
frontmost, Quill came to the front in ten. In the other its window stayed behind the browser;
macOS keeps a launching app behind when the user clicks another app meanwhile, which is the
likely cause, but that is a guess. It did not happen again in ten more tries. Quill does not
force activation when launching finishes: the old order did not either, and it would override
a user who switched to another app while Quill was starting.

## Measuring
`scripts/launch-bench.py --onscreen` adds `onscreen`, when WindowServer first lists Quill's
window, seen from outside (`scripts/winwatch.swift`). The bench now quits each launch with
`NSRunningApplication.terminate`, the way the Dock does. It used to send SIGTERM, which ends
Quill without its clean-exit marker, so the real account's next launch showed a false
"Quill closed unexpectedly" notice.

Release builds target macOS 14 (`package-macos.yml`), so their binary uses chained fixups.
A local `cargo build` targets rustc's default, 11.0, and its binary carries classic rebase
opcodes that dyld applies before `main`. Build with `MACOSX_DEPLOYMENT_TARGET=14.0` to bench
what ships.

All numbers: Apple M1 Pro, macOS 27.2, 2026-11-06, the real account, other builds running,
alternating runs, medians of 8, ms after `open`. One build, with the new launch order switched
off (`old order`) or the `isEditable` gate switched off for the comparison:

| launch | mark | old order | window first | window first, no gate |
| --- | --- | --- | --- | --- |
| cold | window on screen (`onscreen`) | 1342 | 1041 | 1282 |
| cold | chat list presented | 1334 | 1022 | 1261 |
| warm | window on screen (`onscreen`) | 371 | 332 | 320 |
| warm | chat list presented | 357 | 318 | 308 |

Warm, the first presented frame holds the chat list in every launch: TDLib has delivered the
chat positions (`td_chat_position`) before the window opens. Cold, in some launches of either
order the first frame came before the chats and the list followed in a later frame. Warm, the
gate looks about 10 ms slower than no gate, inside the spread of the runs (chat list presented
between 306 and 329 ms against 296 and 357). Cold, it saves about 240 ms.

The final build of this change against the old order, same conditions, another 8 launches
each: cold, window on screen 1299 → 1015 ms and chat list presented 1321 → 1026; warm,
346 → 326 and 333 → 315.

## What is left
In a cold launch with these changes:

- About 120 ms from `open` until the process starts (LaunchServices).
- About 375 ms before `main`, paging in Quill's binary and the frameworks it links.
- About 210 ms from `main` to a built `Application`: the WindowServer connection and its TCC
  check, `NSApplication` creation, platform setup.
- About 130 ms creating and ordering the window, a third of it AppKit building the title bar
  buttons.
- About 110 ms for the first frame (layout, text shaping, Core Animation commit).

Warm, the window is on screen about 330 ms after `open`.

Tried and dropped: opening the window from `applicationWillFinishLaunching:` (previous round);
opening it before `[NSApp run]` without the `isEditable` gate (about 60 ms cold, as above).
