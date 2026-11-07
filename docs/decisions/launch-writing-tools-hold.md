# launch-writing-tools-hold: Writing Tools loads after launch, one image at a time

## Problem
After `launch-first-frame-text.md`, a cold launch put the window on screen in about 770 ms,
and then the main thread stopped for about 280 ms. An lldb backtrace and a System Trace of
cold launches (2026-11-07) showed why. On the first run-loop pass after the window becomes
key, AppKit's first-responder observer
(`+[_NSAutomaticFocusRing setActiveFirstResponderChanged]`) sends the first message to
`NSTextInputContext`. Its `+initialize` registers for Siri availability through
`+[NSWritingToolsCoordinator addObserverForEnhancedSiriAvailabilityDidChangeUsingBlock:]`,
and that loads WritingToolsUI and the 460 or so images behind it, on the main thread. Two
more things load it on first use: `setMainMenu:` with an Edit menu
(`_addTextInputMenuItems:` asks `isWritingToolsAvailable`) and a view's input context.

While it loaded, keys, clicks and redraws waited, and on launches where the chats missed the
first frame they were drawn only after it.

## What did not help
- Loading WritingToolsUI on a worker with one `dlopen`, from `main`
  (`launch-time-macos-appkit.md`) or from `applicationDidFinishLaunching:`. dyld holds its
  loader lock while it maps every image of the load, then the Objective-C runtime holds its
  lock while it registers them all. The main thread needs one of them for an
  `objc_msgSend` cache miss, a Swift metadata lookup, `NSVersionOfRunTimeLibrary` or a
  `dlopen` of its own, so it waited for the worker. A test program
  (`scripts/launch-floor/wtlock.m`) that loads WritingToolsUI on a worker after `purge` and
  probes both locks from a main-thread timer every millisecond:

  | load | load time | longest main-thread wait | main-thread time waiting |
  | --- | ---: | ---: | ---: |
  | one `dlopen` | 340 ms | 200 ms | 340 ms |
  | one image per `dlopen` | 340 ms | 35 ms | about 80 ms |

  In Quill's System Trace with one `dlopen`, main waited 115 ms on the first mouse
  enter/exit event (`_NSGetBoolAppConfig` calls `NSVersionOfRunTimeLibrary`) and 173 ms in
  `-[NSMenu dealloc]` (a Swift type lookup), between `did_finish` and the end of the load.
- Skipping AppKit's `updateWindows` pass: the trigger is the first-responder observer, not
  that pass.

## Decision
- `gpui-pre-macos/src/writing_tools.rs` swaps the implementation of the Siri availability
  method before the first window is made (`hold`). Until the framework is loaded it keeps
  each block AppKit registers; after, it puts the method back and registers them. The
  registration only listens for changes, so nothing is missed.
- The worker starts from `applicationDidFinishLaunching:` (`start_load`). `finishLaunching`
  loads images of its own, and the main thread would wait for those behind the worker.
- `gpui-pre-macos/src/staged_load.rs` loads the framework one image per `dlopen`,
  dependencies first, so each holds the two locks only for its own image. It reads the
  dependency graph from the images' load commands in the shared cache
  (`dyld_shared_cache_iterate_text`), and reads each level's headers on eight threads so
  their disk reads overlap (125 ms on one thread after `purge`, 35 ms on eight). Upward
  links are left out, since dyld loads them with the image that names them. Some system
  images link each other in a loop; dyld loads those together. Anything it cannot read
  falls back to one `dlopen` of the framework.
- Until the load is done, `set_main_menu` keeps the menu bar and installs it afterwards,
  and views answer `isEditable` NO, so AppKit makes no input context for them. A key makes
  them editable at once, without waiting for the worker (see Results for what that key
  costs). It also calls
  `-[NSApplication updateWindows]`, so AppKit makes and activates the key view's context
  before that key is read: `setWindowsNeedUpdate:` left it for the next pass, and an
  inactive context dropped a dead key (Option-E typed "cafe" for "café" on the first
  launch of a build).
- The tray icon and dock badge wait for the load too (`install_main_window_tray`):
  `NSStatusBar` loads images of its own.

## Results
Cold, main (PR #1390) against this change, three alternating blocks of 6 launches each,
medians of 18 with a bootstrap 95% interval of the difference, ms after `open`:

| mark | main | this | difference |
| --- | ---: | ---: | ---: |
| onscreen | 807 | 769 | −38 [−83, +28] |
| `did_finish` | 1254 | 825 | −429 [−470, −355] |
| onscreen → `did_finish` | 435 | 56 | −379 [−404, −362] |
| window ordered → onscreen | 81 | 76 | −5 [−32, +6] |
| `chat_list` → first display | 35 | 34 | −1 [−29, +3] |

Warm, same method:

| mark | main | this | difference |
| --- | ---: | ---: | ---: |
| onscreen | 297 | 292 | −5 [−20, +6] |
| `did_finish` | 359 | 318 | −41 [−58, −31] |
| onscreen → `did_finish` | 59 | 26 | −33 [−38, −32] |

The load itself ends (`writing_tools`) about 120 ms after `did_finish` warm and 350 ms
cold; the menu bar's menus and the tray icon appear then.

A launch that starts in the tray (`--start-minimized`) still makes its tray icon before
`[NSApp run]`, as before, and the worker starts after it. Cold, two launches each:
`window_open` → `ns_run` 156 and 209 ms on main, 155 and 131 with this change;
`will_finish` → `did_finish` 332 and 76 on main, 38 and 56 with it.

Every launch on main reached `did_finish` 319 to 486 ms after the window showed (17 of
18 over 400), not only those whose chats missed the first frame; with this change, 47 to
63 ms. The window and its first frame do not
move. On main, 4 of the 18 launches drew the chats after `did_finish`, 1 to 1.3 s after
`open`; with this change the latest was 782 ms.

In a System Trace of one cold launch, the main thread's longest wait between
`did_finish` and the end of the load went from 173 ms (one `dlopen` on a worker, with the
hold) to 30 ms, an Objective-C runtime lock wait while the worker registered one image.

Typing during the load: `typelag.py` (scratch tool) launches cold, presses Cmd-K 100 ms
after the window shows and types "café" with Option-E, then screenshots the search box.
Six cold launches, the first of them the build's first launch, all typed "café". The
first key reached the view 79 to 86 ms after it was posted (150 ms on the build's first
launch); typed 3 s after launch, 69 to 75 ms, which is the tool's own delivery time.

## Measuring
Release builds with `MACOSX_DEPLOYMENT_TARGET=14.0`, on an M1 Pro running macOS 27.2,
2026-11-07. Blocks of 6 launches per build, alternating, so each build's line and glyph
caches stay its own (`launch-first-frame-text.md`):

    QUILL_ENV_FILE=…/quill.local.env QUILL_CODESIGN_IDENTITY="Apple Development: …" \
      python3 scripts/launch-bench.py --cold --onscreen --runs 6 --until did_finish \
      --template dist/Quill.app base=<main binary>
    clang -O2 -fobjc-arc -framework AppKit scripts/launch-floor/wtlock.m -o wtlock
    sudo purge; ./wtlock once; sudo purge; ./wtlock staged

The `writing_tools` startup mark is when the load finished and what waited for it ran.

## What is left
- A borderless first window would put it on screen sooner. In the same session, a floor
  app (`floorbench.py`) that orders a borderless window showed it 81 ms [58, 101] sooner
  from `main` than the titled one, and one that orders it borderless and then sets the
  titled style mask 67 ms sooner. Those frames show square corners and no traffic lights,
  then the title bar popping in: on every launch, so not taken.
- Time before `main`. In the same session Quill's process started (`exec`) 131 ms after
  the launch command, the floor's 74 ms. The floor padded to Quill's executable size
  (`linked-big`, 96 MB in `__TEXT`) started 51 ms [14, 76] later than the plain floor, and
  Quill 7 ms after that, so that part follows the executable's size, not the bundle: the
  floor in a copy of Quill's bundle, or with Quill's `Info.plist`, starts as soon as the
  plain one. `exec` is the process's start time, so this is before it exists:
  LaunchServices, syspolicyd, launchd. A System Trace of each floor shows no process
  reading the executable from disk or hashing its pages before then, so what the size
  costs is not named yet. A smaller executable is
  the lever. `exec` → `main` is 20 ms [6, 37] over the floor: binds and initializers, as
  in `launch-cold-floor.md`.
- WritingToolsUI loads about 60 ms longer warm one image at a time than in one `dlopen`
  (worker time only; the window and first frame do not wait for it).
