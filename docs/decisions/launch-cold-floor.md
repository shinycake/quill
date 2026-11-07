# launch-cold-floor: ThorVG's operator new cost a cold launch about 300 ms

## Problem
Idan set a new goal on 2026-11-06: a cold launch under 1 second. After
`launch-window-first.md`, the window was on screen about 1015 ms after `open` when the file
cache had been emptied with `purge` first (median of 8, local release build).

## What did not help
- Hello programs run straight after `sudo purge` reached `main` in about 165 ms when they
  linked AppKit alone, and in 230 to 275 ms when they also linked MediaPlayer,
  ScreenCaptureKit or CloudKit. Leaving those three out of Quill's link (a linker wrapper,
  `dlopen` on first use, and `objc2-app-kit` without its default features so CloudKit is not
  linked) moved `main` about 26 ms earlier and made the window no sooner.
- `DYLD_PRINT_LIBRARIES`, stamped against the startup trace, and lldb backtraces on every
  `dlopen` showed where the libraries went. `-[NSApplication init]` calls
  `_registerApplicationWithUIIntelligence`, which loads AppIntents and the 70 to 116
  libraries behind it on the main thread, then fails because Quill has no App Intents.
  Deferring that call only moved the loading into window creation: the window's close
  button resolves its style through SwiftUI, which needs the same Swift libraries.
- Building the renderer's shader library and pipelines on the launch prewarm worker left
  `app_run` to `window_ordered` unchanged (136 ms against 138).
- Opening TDLib's first client on the early-connect worker does not compile: `LiveConnect`
  holds the call engine and the call audio level sources, which are not `Send`. The window
  does not wait for TDLib, so this would not bring it sooner.

## The floor
FloorApp (`scripts/launch-floor/floor.m`) is about 60 lines of Objective-C: a shared
`NSApplication`, an empty main menu, one titled window and a `CAMetalLayer` view cleared
once. It orders the window front and flushes the Core Animation transaction before
`[NSApp run]`, as Quill does since `launch-window-first.md`. Without that flush the window
waits for `finishLaunching`, about 370 ms more after `purge`. The first version of this
write-up measured the floor without the flush and called Quill's 1015 ms the floor. An
outside review caught it.

With the flush, the floor's window is on screen 627 to 669 ms after `open` after `purge`
(medians of 8 to 10 in four benches), and 590 ms with a borderless window. Quill was about
400 ms above it.

## Where the 400 ms went
Most of it was before `main`: Quill reached `main` 517 to 609 ms after `open`, and a floor
app that links all 34 frameworks Quill links reached it at 229 ms. A floor app that also
binds every symbol Quill imports from the system (`floorbench.py binds`) reached it at
246 ms, so neither the frameworks nor the imports explain it. Reading Quill's own 96 MB
binary into the cache first did not change it either.

An Instruments trace of a cold launch put most of dyld's page-ins under weak-definition
patching (`JustInTimeLoader::cacheWeakDef` and `PatchTableV4::forEachPatchableCacheUseOfExport`)
and header reads for it. Quill's Mach-O header carries `MH_WEAK_DEFINES`, and its executable
exported C++ `operator new`, `operator new[]`, `operator delete` and `operator delete[]`.
They come from ThorVG's `tvgInitializer.cpp`, which replaces them for the whole process with
wrappers around `std::malloc` and `std::free`. A replaced `operator new` in the executable
means dyld has to point every system library's uses of it at the executable's copy before
`main` runs, and after `purge` that reads hundreds of pages of the shared cache from disk.

`floorbench.py opnew` adds those four functions, as ThorVG writes them, to FloorApp. After
`purge` it reached `main` at 432 ms against 135 ms, and its window was on screen at 898 ms
against 656 ms (medians of 10, alternating; every `opnew` launch was slower than every
plain one).

## Decision
Quill's ThorVG patch (`native/patches/thorvg-sticker-limits.patch`) removes the four
replacements. libc++'s own `operator new` and `delete` also call `malloc` and `free`, so
ThorVG allocates the same way. The one difference is a failed allocation. The replacement
returned null into code built with `-fno-exceptions`, which assumes `new` never does, so
ThorVG went on with a null pointer. libc++'s throws `std::bad_alloc`, which ends the
process at once. The sticker limits in the same patch keep allocations far from that.

`scripts/check-bundle-macho.sh`, which the macOS package smoke test and the release
workflow run, now fails when any executable in the bundle has `MH_WEAK_DEFINES`, and names
the `operator new` and `delete` symbols it exports.

Linux and Windows build the same ThorVG sources, so they lose the replacement too. Neither
has dyld's patching step, so neither is expected to start faster from it.

## Chats that miss the first frame
With the window about 240 ms sooner, TDLib's chats missed the first frame in 3 of 10 cold
launches. They were ready 15 to 20 ms after it, and were drawn about 1250 ms after `open`
instead of about 720. Sampling those launches showed why. The first run-loop pass after the
window becomes key runs AppKit's first-responder observer
(`___NSPostActiveFirstResponderChanged_block_invoke`), which calls
`+[NSTextInputContext initialize]`. Its `dispatch_once` registers for Siri availability
through `+[NSWritingToolsCoordinator addObserverForEnhancedSiriAvailabilityDidChangeUsingBlock:]`,
and that loads WritingToolsUI and about 420 libraries behind it on the main thread: 280 to
370 ms after `purge`. `finishLaunching` also asks whether Writing Tools is available when it
adds the text input items to the Edit menu.

A hold that ran the main run loop before `[NSApp run]` until the chats were drawn (capped
at 150 ms) did not help: the observer runs on that first pass too, before the poll loop
gets to draw. The hold was dropped. The window still comes sooner in those launches, and
only the chats wait.

## Results
Release builds, cold (`purge` before each launch), main against this branch alternating,
10 runs each on 2026-11-06 with a load average of 8 to 11 from other work on the machine.
Medians in ms after `open`:

| step | main | branch |
| --- | ---: | ---: |
| `main` | 534 | 293 |
| `window_ordered` | 927 | 699 |
| chat list drawn | 949 | 724 |
| window on screen | 1071 | 825 |

Seven of the ten branch launches drew the chats at 592 to 738 ms. The other three drew them
at 1229 to 1281 ms, behind the Writing Tools load above.

## Measuring
Release builds with `MACOSX_DEPLOYMENT_TARGET=14.0`, on an M1 Pro running macOS 27.2,
2026-11-06:

    QUILL_ENV_FILE=…/quill.local.env QUILL_CODESIGN_IDENTITY="Apple Development: …" \
      python3 scripts/launch-bench.py --cold --onscreen --runs 8 \
      --template dist/Quill.app base=<main binary> new=<branch binary>
    python3 -I scripts/launch-floor/floorbench.py --runs 10 --cold titled opnew
    python3 -I scripts/launch-floor/floorbench.py --runs 8 --cold --quill quill=<bundle> linked binds
    python3 -I scripts/launch-floor/dyld-timeline.py <bundle>/Contents/MacOS/quill out.txt --cold

The bench bundles have no hardened runtime, so `DYLD_PRINT_LIBRARIES` works on them and lldb
can attach. Quit Quill through `NSRunningApplication` (as `launch-bench.py` does), never with
a signal: a signal leaves a "closed unexpectedly" report for the account's next launch.

## What is left
- The Writing Tools load after the first frame. It holds the main thread for 280 to 370 ms
  after `purge`, delays input, and delays the chats when they miss the first frame.
- `main` is still about 160 ms after the floor app's: 293 against 135.
- `main` to the window on screen takes about 530 ms against the floor's about 520, so the
  window code itself is near the floor.
