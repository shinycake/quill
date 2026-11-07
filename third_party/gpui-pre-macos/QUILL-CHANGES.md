# Changes made by the Quill project

This directory is gpui-pre-macos 0.3.11 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's macOS platform, used through `[patch.crates-io]` in
Quill's `Cargo.toml`. Commit `1398c875` added the unmodified registry copy
(without the registry's `.cargo-ok`, `.cargo_vcs_info.json`,
`Cargo.toml.orig` and `Cargo.lock`), so
`git diff 1398c875 -- third_party/gpui-pre-macos` shows every change. (The
patch was first made on 0.3.7, carried to 0.3.8 in
`docs/decisions/codex-gpui-kit-0.7.1.md`, where `step` kept 0.3.8's frame
signal timestamps, and to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`, to 0.3.10 in `docs/decisions/gpui-kit-0-8-1.md`, and to 0.3.11 in `docs/decisions/gpui-kit-0-9-0.md`.)

The Quill project changed it in 2026 so an idle window stops costing CPU:

- `src/window.rs`: upstream keeps a window's `CVDisplayLink` subscription
  running whenever the window is visible, so the main thread and a CoreVideo
  thread wake 60–120 times a second even when nothing changes. Now a step
  that neither draws, presents nor asks for another frame counts as idle;
  after 250 ms of those the window unsubscribes from the display link.
  `MacWindow` implements `frame_waker` (GPUI calls it when a window becomes
  dirty or queues a next-frame callback) to request one step on the window's
  dispatch source while the window is visible and the link is not running.
  That step draws, and the link stays stopped after a lone draw: it comes
  back when GPUI asks for another frame after the draw, or for a second
  woken draw within 100 ms, so a caret blink wakes the app once per blink
  and scrolling is still paced by the display. `schedule_frame` before a
  draw does not count, because GPUI's effect flushes call it for any dirty
  window. The decision is in `src/link_pace.rs` (new, standard library only;
  `scripts/test-frame-idle.sh` runs its tests on any host). Spare drawables
  go 3 s after the last activity, one timer at a time.
  The waker keys on whether the link runs, not on why it stopped:
  a restart that finds no screen yet, or misses the occlusion change that
  would retry it, no longer leaves a visible window that never repaints
  (`docs/decisions/notification-click-repaint.md`). The waker holds the window's
  dispatch source; window teardown still cancels it, so a waker that
  outlives the window never runs a step.
- `src/display_link.rs`: `WindowFrameSource::is_running` and
  `WindowFrameSource::requests`.
- `Cargo.toml`: allows the `deprecated` lint, which the registry build caps
  anyway (the crate still uses the `cocoa` crate).

The Quill project also changed it in 2026 so an OS notification can carry
the sender's picture and an inline reply field, which GPUI's
`SystemNotification` (in the unvendored `gpui` crate) has no fields for:

- `src/notification_extras.rs` (new, public, identical to
  `third_party/gpui-pre-windows/src/notification_extras.rs`; unit tests run
  with `scripts/test-notification-extras.sh`): `NotificationExtras { image,
  photo, text_input }` registered under the notification's tag with
  `set_notification_extras` before `App::show_system_notification`, and
  `take_reply_text(tag)` for the text typed into the field, parked by the
  backend before the response is sent.
- `src/system_notifications.rs`: `show` takes the extras for the tag; the
  action named in `text_input` becomes a `UNTextInputNotificationAction`
  (categories are keyed by actions plus text input); the photo, or else the
  image, is copied to a temporary file (keeping its extension) and attached as a `UNNotificationAttachment` (the center
  moves attached files into its own store). The delegate reads
  `UNTextInputNotificationResponse.userText` into the side channel.
- `src/system_notifications.rs`: the notification center lives on one
  worker thread (`gpui-notifications`). Creating it (about 130 ms on the
  first post), copying and validating the picture, registering categories
  and every post or dismiss are round trips to `usernoted` that used to run
  on the main thread; the main thread now only queues the job. A single
  thread keeps posts and dismissals in order.
- `src/gpui_macos.rs`: declares the new module.

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and
measurements: `docs/decisions/codex-idle-cpu.md`; notifications:
`docs/decisions/codex-notify-native.md`.

The Quill project also changed it in 2026 to remove panics a user can hit
(step 7 of `docs/decisions/no-crash-audit.md`):

- `src/keyboard.rs`: the input source id and name are read with
  `to_string_lossy` (and as empty for a null pointer) instead of
  `to_str().unwrap()`.

See `docs/decisions/gpui-fork-panics.md`. Two more fixes from that step
are no longer here: 0.3.9 fixed `MacDisplay::all` upstream (it logs and
returns no displays when `CGGetActiveDisplayList` fails), and it moved
`text_system.rs` to gpui-pre-apple, which now carries the CoreText fix.

The Quill project also changed it in 2026 so video can be paced on the
display's real present time (`docs/decisions/video-player-core.md`):

- `src/display_link.rs`: the output callback converts its `output_time`
  (the vsync the frame being requested is shown at) to an `Instant`, sampled
  against the current host time like `frame_request_instant` (both through
  `objc2_core_video::CVGetCurrentHostTime` since 0.3.9), and keeps it
  with its host ticks on the display's registry entry, together with the
  output time's refresh period (`videoRefreshPeriod / videoTimeScale`).
  `present_timing(display_id)` reads both under one registry lock.
- `src/gpui_macos.rs`: `pub fn present_timing(display_id: u64)`, taking a
  `DisplayId` as `u64`. Quill's video player passes the host ticks to
  `AVPlayerItemVideoOutput::itemTimeForMachAbsoluteTime` and paces from the
  refresh period: a player that sleeps between pictures draws at its own
  rate, so draw spacing alone would mistake that rate for the display's.
  The time is per display (the link is shared by the windows on it), not
  per window.

Picture-in-Picture (`docs/decisions/video-pip.md`):

- `src/window.rs`: a `WindowKind::Floating` window gets the same
  always-active `NSTrackingArea` (`add_mouse_tracking_area`) and
  `acceptsMouseMovedEvents = NO` as a normal window. Upstream gave floating
  windows only `acceptsMouseMovedEvents = YES`, which delivers mouse moves
  to the key window alone, so a panel opened without focus never saw
  hover, cursor or mouse-exit events.

The Quill project also changed it in 2026 to give back idle window memory
(`docs/decisions/idle-drawables.md`):

- `src/window.rs`: when `step` stops the display link for idling, it
  schedules `MetalRenderer::release_idle_drawables` 3 s later on the main
  queue (`DRAWABLE_RELEASE_AFTER`); `FrameIdle::idle_stops` skips the
  release if the window drew in between. An occluded window releases at
  once.

The Quill project also changed it in 2026 for Agent access's screenshots
(`docs/decisions/agent-access.md`, phase 5 step 3):

- `src/window.rs`: `MacWindow::capture_image` renders the scene through
  `MetalRenderer::capture_scene`. No screen-recording permission is
  involved: the window renders its own frame.

The Quill project also changed it in 2026 so a failed Metal setup costs one
window, not the app:

- `src/window.rs`: `MacWindow::open` returns `Result` and builds the renderer
  before any AppKit object, so a renderer error leaves no half-made window.
  gpui-pre-apple's `new_renderer` no longer takes the window and view.
  `src/platform.rs` passes the error on from `open_window`.

The Quill project also changed it in 2026 so a main window hidden by its
close button comes back the way Telegram Desktop's does:

- `src/platform.rs`: `should_handle_reopen` runs the reopen callback with
  windows showing too (upstream skips it then). A call or
  Picture-in-Picture window counts as showing, and the Dock click must still
  bring back a hidden main window.
- `src/platform.rs`: `applicationDidBecomeActive:` reaches
  `Platform::on_app_lifecycle` as `AppLifecyclePhase::Active`, deferred to
  the main queue. The other phases stay unused on macOS.
- `src/window.rs`: public `is_exiting_fullscreen`, so Quill can wait out the
  animation before it hides a full screen window.

The Quill project also changed it in 2026 so a Dock Quit or a log out can be
refused, as Telegram Desktop refuses them through Qt while an export runs:

- `src/platform.rs`: `applicationShouldTerminate:` asks the new
  `Platform::on_quit_request` callback and answers `NSTerminateCancel` when it
  refuses. Upstream has no such method, so AppKit terminated at once. When the
  app cannot be asked (it is borrowed, or the callback is running further up
  the stack), it answers `NSTerminateLater` and asks again from the main
  queue, then calls `replyToApplicationShouldTerminate:` with the answer.
  It asks again at most `QUIT_ASKS_AGAIN` times (`QuitAnswer::step`), then
  replies with a cancel, so a modal panel left up does not keep the main
  queue spinning.
  `Platform::quit` sets `quit_decided`, so the app's own quit, `restart`
  included, reaches `terminate:` without asking.

0.3.11 note (`docs/decisions/gpui-kit-0-9-0.md`): upstream paces windows with a
`CAMetalDisplayLink` on macOS 14 and later, and Quill moved to it without a
fallback (`docs/decisions/gpui-kit-0-9-0.md`). The `CVDisplayLink` registry is
gone; `display_link.rs` keeps a window's link and the frame waker's dispatch
source, and the display's latest frame time for `present_timing`. Mentions of
`CVDisplayLink` above describe why the rules exist. `link_pace` still decides when
the link runs, in `run_step`, and upstream's `MetalFrames` pausing is not used.
`MacWindowState` now lives in an `Rc` as upstream's does, so the
delayed drawable release carries its weak handle in `dispatch2::MainThreadBound`.

The Quill project also changed it in 2026 so a cold launch draws its first
frame sooner (`docs/decisions/launch-time-macos-appkit.md`):

- `src/platform.rs`: `MacPlatform::new` no longer reads the keyboard layout.
  `TISCopyCurrentKeyboardLayoutInputSource` starts the Text Services Manager,
  about 90 ms on the main thread on a cold launch. The mapper starts with an
  empty layout id (keys map as on a US layout) and `keyboard_layout` answers
  `MacKeyboardLayout::unknown()` until `read_keyboard_layout_after_first_frame`
  has run the delegate's `onKeyboardLayoutChange:` on a later run loop turn.
- `src/window.rs`: `draw` calls `read_keyboard_layout_after_first_frame` once
  the scene is presented. Only the first call does anything.
- `src/platform.rs`: `run` sets an empty `NSMenu` as the main menu when there
  is none. Without one, `finishLaunching` builds a default menu and adds the
  text input items, which loads the Writing Tools framework before the app
  delegate hears `didFinishLaunching`. Quill installs its real menu after the
  first frame.
- `src/gpui_macos.rs`: `on_frame_presented` sets a hook that `draw` in
  `src/window.rs` calls after each presented frame, so Quill's startup trace
  can mark when the first frames reach the screen.
- `src/gpui_macos.rs`: re-exports `gpui_apple::glyph_cache` (with the
  `font-kit` feature) so the app can load and save the launch glyph cache.
- `src/platform.rs`: `run` calls the app's launch callback before
  `[NSApp run]` instead of from `applicationDidFinishLaunching:`, then
  flushes the Core Animation transaction, so the windows the app opens
  there reach the screen before `finishLaunching` (which waits for the
  open-application Apple Event and loads AppKit's text input, hundreds of
  ms on a cold launch). `did_finish_launching` finds no callback left
  (`docs/decisions/launch-window-first.md`).
  The `NSAutoFillHeuristicControllerEnabled` default is written at the
  start of that block (`disable_autofill_heuristics`) instead of in
  `applicationWillFinishLaunching:`, so it is set before any window exists.
- `src/writing_tools.rs` (new): keeps the Writing Tools framework (WritingToolsUI
  and about 460 libraries, ~280 ms after `purge`) off the main thread at launch
  (`docs/decisions/launch-writing-tools-hold.md`). `run` calls `hold`, which
  replaces `+[NSWritingToolsCoordinator
  addObserverForEnhancedSiriAvailabilityDidChangeUsingBlock:]` (called from
  `+[NSTextInputContext initialize]`, which AppKit's first-responder observer
  runs once the window is key) with one that keeps the block.
  `applicationDidFinishLaunching:` calls `start_load`: a worker loads the
  framework one image per `dlopen` (`src/staged_load.rs`, new: the
  dependency graph comes from the images' load commands in the shared
  cache, so no `dlopen` holds dyld's and the Objective-C runtime's locks
  for long), then the main thread puts the method back, registers the kept
  blocks with it, lets views make input contexts and installs the menu bar
  that `set_menus` kept (`set_main_menu`; an Edit menu loads the framework
  too). `after_writing_tools_load` (public) runs app callbacks then.
- `src/window.rs`: the view answers `isEditable` with NO until that load is
  done, or until a key arrives. `-[NSView _inputContext]` makes no
  `NSTextInputContext` for a view that says it is not editable; ordering the
  first window front asked for one. `text_input_ready` then runs
  `updateWindows`, which makes and activates the context at once, so the
  key is read by an active context (an inactive one drops dead keys).
  `update_ime_position` does nothing before then.
- `src/gpui_macos.rs`: `on_launch_mark` sets a hook that hears AppKit's
  launch steps (`ns_run`, `will_finish`, `did_finish`, `window_ordered`,
  `first_display`, `writing_tools`, and `first_key` and `first_text` from
  `window.rs`) for Quill's startup trace.
