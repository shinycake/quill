# Video Picture-in-Picture on the shared player

PR 4 of 6 of the video player overhaul (`quill-tools/designs/video-player.md`,
"PiP chrome", build plan item 4). PR 1 (`video-player-core.md`) made the old
macOS-only PiP window draw `video_frame` from the viewer's player. This PR
replaces that window with tdesktop's PiP on every platform and deletes
`src/ui/video_pip.rs`.

## Problem

Before this PR (`video-player-inline`, PR 3):

- PiP worked on macOS only. Elsewhere the button was hidden and the call
  site showed "Picture-in-Picture requires native floating-window support".
- The window was a titled 420 × 290 window with "Pause" / "Return to
  viewer" text buttons. It had none of tdesktop's chrome, snapping or
  aspect-locked resize.
- The viewer stayed open and hidden behind the window, still owning the
  player. Its panel kept watching the transport, so the hidden viewer
  slice re-rendered 7 to 9 times a second while PiP played (measured
  below).

## What tdesktop does

These references are to `Telegram/SourceFiles/media/view/media_view_pip.cpp`
unless a line says otherwise. Sizes are from `media_view.style:452–476`.
Colors are from lib_ui `colors.palette:544–547` and `radialBg` (`:71`).

| Behavior | tdesktop | Quill |
|---|---|---|
| Window | `Qt::Tool \| WindowStaysOnTopHint \| FramelessWindowHint \| WindowDoesNotAcceptFocus` (`PipPanel::init`, 351–366) | borderless, opened unfocused; macOS `NSPanel` at the floating level, all Spaces, no traffic lights; Windows/X11 kept on top and off the taskbar through `window_control` |
| Default place | 320 square (`pipDefaultSize`), snapped top-left on the main window's screen (`setPositionDefault`, 506–521) | `PipPosition::default_on` + `place_on_screen` |
| Size rules | aspect applied, max = screen − 3 × 20 (`MaxAllowedSizeForScreen`, 62–67), min 120 on the short side (`setPositionOnScreen`, 523–597) | `place_on_screen`, `size_limits` |
| Saved place | attached / snapped sides, geometry and screen, saved after each settle (`countPosition` 468–504, `updateDecorations` 881–909, `PipDelegate::pipSaveGeometry`, `media_view_overlay_widget.cpp:250–257`) | `settings::video_pip` (`video_pip.json`), matched by display uuid, else by screen bounds |
| Move | starts past `startDragDistance` (731–752); edges within 16 px of the line 20 px inside the screen snap (`ClampToEdges` 69–95) and slide there over 150 ms, `easeOutCirc` (`moveAnimated` 865–879); on release the window is pulled inside the screen under the pointer (`finishDrag` 818–847) | `drag_step`, `clamp_to_edges`, `slide_point`, `finish_drag` |
| Resize | 10 px bands on the free edges (`updateOverState` 670–729), aspect kept and the opposite corner or edge anchored (`Transformed` 97–145, `Constrained` 147–242) | `resize_part`, `transformed`, `constrained`; resize cursors per part |
| Wayland | system move or resize (`startSystemDrag` 754–768); the aspect is re-applied after a compositor resize (`handleWaylandResize` 607–635) | `start_window_move` / `start_window_resize`, `wayland_normalized` on bounds changes |
| Controls layout | close and enlarge 36 px (24 icon + 2 × 6) in the title-controls corner; volume toggle and 75 px line opposite, the line only when it fits; play 30 px centered; progress area 12 px at the bottom (`setupButtons` 1241–1327) | `ControlsLayout::new`; the side comes from GPUI's window button layout (always the left on macOS) |
| Hit states | `computeState` (1875–1891); the volume line highlights with its toggle (`ResolveShownOver` 1101–1105) | `ControlsLayout::state_at`, `OverState::shown` |
| Fade | controls show while the pointer is over the window, 200 ms linear (`setOverState` 1041–1060); each control brightens from 0xd9 to white over 200 ms (`updateActiveState` 1080–1099, raster `paintButton` 118–133); a `radialBg` wash over the picture (raster `paintFade` 97–113) | kit `transition` with linear easing, one per control |
| Progress line | 2 px growing to 4 px while active, played part 0xd9 / 0xda, rest white at 0x26; times in 11 px above it, "−" before the remaining time (`paintPlaybackProgress` 1536–1554, `paintProgressBar` 1556–1596, `paintPlaybackTexts` 1598–1614, `updatePlaybackTexts` 1712–1728) | composed from the kit's base slider parts (`chrome::line`) |
| Seek | a press on the line pauses a playing clip while dragging; the release seeks and resumes it (`seekProgress` / `seekFinish` 1179–1213) | `on_seek_event` |
| Volume | the toggle mutes or restores the last volume above zero (`volumeToggled` 1224–1228); the line sets the volume live (`volumeControllerUpdate` 1230–1239); icon tiers off / small (< 0.5) / on (`paintButtons` 1499–1515) | `toggle_volume`, `on_volume_event`, `controls::volume_icon` |
| Clicks | release over the same control acts; a moved press never does (`handleMouseRelease` 1125–1157); a click on the picture pauses or resumes, an ended clip starts over, a failed one closes (`playbackPauseResume` 1734–1748); a double click undoes the first click and goes back (`handleDoubleClick` 1159–1167) | `layout::Presses`, `VideoPip::toggle_pause` |
| Waiting | radial in a 50 px `radialBg` disc (`paintRadialLoadingContent` 1838–1863) | kit `Spinner` in the same disc after 500 ms (`WaitingGate`) |
| Errors | a streaming error closes the window (`handleStreamingError` 1730–1732) | `Failed` closes it |
| Ends | its account's session changing (948–951), the message removed (`itemRemoved`), the app locking (`switchToPip`, `media_view_overlay_widget.cpp:5806–5820`), a viewer clip with sound starting (`startStreamingPlayer`, `:5024–5031`) | `lock_by_passcode`, the update poll (`video_pip_account_ended`, `check_video_pip` on a tombstone) with the root render as a backstop, `sound_starts` |
| Handoff | the viewer gives PiP its shared streaming document and closes (`switchToPip`, `:5776–5830`); "enlarge" reopens the viewer on the document in its message with `continueStreaming`, which keeps the same player (`:4996–5013`), and so does opening that clip again | the `VideoPlayer` itself moves (`open_video_pip`; back through `pip_enlarge_requested` → `take_pip_player_for`) |

## What changed

- `src/media_viewer/pip/` (core, pure, unit-tested):
  - `panel.rs` ports `PipPanel`'s geometry.
  - `layout.rs` ports `Pip`'s layout, hit states and press rules, plus the
    two keys.
  - `reopen.rs`: where back-to-viewer lands (the album item at its offset
    when it is still the same file, else the item with that file).
- `src/settings/video_pip.rs`: the saved position.
- `src/ui/video_player/controls/pip/` (new):
  - `mod.rs`: the view (`VideoPip`), its actions and the owner hooks.
  - `open.rs`: opening the window and the saved position.
  - `input.rs`: the pointer, drag, slide and system resizes.
  - `chrome.rs`: drawing.
  - `place.rs`: moving the native window.
- `src/ui/media_viewer/pip.rs`: the viewer's side:
  - `open_video_pip` takes the viewer's player, closes the viewer and opens
    the window.
  - Back to the viewer (`pip_enlarge_requested`) reopens the viewer on the
    clip: the album item at its offset if it is still the same file,
    else the item holding that file, else (the message no longer
    loaded) the saved item alone. `play_viewer_video` then takes the
    window's player (`take_pip_player_for`, `adopt_viewer_player`): no
    reopen, no seek, the same play state, rate and volume; the window
    detaches. If nothing takes it (another account is active, the file
    is gone), the viewer closes again and the window keeps playing.
  - Opening the clip again from the chat takes the window's player the
    same way instead of restarting.
  - It closes on a real deletion only: a tombstone, not the message
    leaving the loaded history window. The viewer's own
    `prune_deleted_viewer_items` follows the same rule now; it used to
    drop items whose message was paged out.
  - Lock (`lock_by_passcode`), its own account's logout or removal
    (`video_pip_account_ended` from the update poll and
    `remove_account_key`) and deletions (`check_video_pip` from the
    update poll) close it without a main-window render, so a minimized
    or tray-hidden Quill does not leave it playing. The root render
    check stays as a backstop. Switching to another account keeps it.
  - Every sound start goes through `sound_starts`, which closes it: a
    viewer clip with sound, a story with sound (opened or unmuted), a
    voice message or music (started, or resumed from the player bar or
    its row), a recording preview, a round video played with sound and
    the legacy video path.
- `ViewerUi.pip_window` became `pip: Option<PipLink>`; the viewer is closed
  while PiP plays, so the `pip_window.is_none()` checks in the root render,
  the viewer slice and chat windows are gone.
- `VideoPlayer` gained:
  - `clear_watchers`: each surface watches the player it holds, so the
    closed viewer's panel is no longer redrawn.
  - `picture_size`: the aspect.
  - `same_as`: whether two handles are the same player.
- The PiP button shows on every platform when the viewer has a player. The
  `cfg!(target_os = "macos")` gate is gone. The Touch Bar and the
  `ready-video-pip` demo call the same `open_video_pip`.
- `src/ui/video_pip.rs` is deleted.
- `Maximize2` joins the bundled icons (the back-to-viewer button).
- Strings: `qll_media_pip_back_to_viewer`, `_failed`, `_seek` and `_volume`.
  Close, Mute/Unmute and the window's name reuse existing keys.

### Moving the native window

GPUI places a window when it opens but cannot move or resize it later.
`place.rs` does it on the native handle, as `notify_windows::position`
already does:

- macOS: `setFrame:display:`.
- Windows: `SetWindowPos` with the client frame grown by the window's
  own borders. A top-level window keeps a left, right and bottom frame
  even with GPUI's titlebar hidden (`WM_NCCALCSIZE`), so the borders
  are read from `GetWindowInfo` (window and client rectangles) on each
  move; `frame()` reads the client rectangle the same way.
- X11: `ConfigureWindow`. The frame and the pointer are also read from the
  server, because GPUI's X11 bounds follow `ConfigureNotify`, which is
  relative to the window manager's frame.
- Wayland: there is no client placement, so moves and resizes go to the
  compositor.

A native resize calls back into GPUI to re-lay the window out. GPUI drops
that callback while the app is borrowed, so a frame is set from a
foreground task after the event that asked for it has returned. GPUI's
own `Window::resize` does the same on macOS.

Resizes the window system makes (a Wayland compositor, an X11 window
manager) are brought back to the aspect and its limits
(`fit_system_resize`). GPUI sets a window's minimum size only when it
opens, so it is 120 × 120 (the smallest minimum of any aspect) and the
aspect's own minimum is kept this way. A pixel of rounding is left alone
so the window's own frames and this never fight.

macOS: GPUI gave a floating window only `acceptsMouseMovedEvents`, which
reports moves to the key window alone; PiP opens unfocused, so it saw
no hover, cursor or exit events. The vendored `gpui-pre-macos` now gives
floating windows the always-active `NSTrackingArea` normal windows have
(`QUILL-CHANGES.md`).

### Why the lines are composed

tdesktop's PiP lines are a 2 px bar with no thumb that grows to 4 px. The
kit `Slider` has a fixed 6 px track and a 16 px thumb. So the lines are
built from the base slider parts (`Slider`, `SliderTrack`,
`SliderIndicator`), like the viewer's `VideoScrubber`, and keep the kit's
drag. The base slider's keyboard and screen-reader steps set its value
without an event, so the window observes the value: a step that did not
come from the window's own sync seeks or sets the volume. A step on the
progress line is 0.1 % of the clip, the drag resolution.

The buttons are kit `Button`s in a custom variant with no background or
wash. tdesktop only brightens the icon, and the foreground follows the
per-control fade.

## Measurements

Same scenario before (PR 3, `video-player-inline`) and after:

- **Setup:** `ready-video-pip` (the 12 s, 15 fps demo clip handed to PiP),
  12 s per run.
- **Environment:** debug build on this container's Xvfb with Mesa
  lavapipe (software Vulkan). The FFmpeg shim was built against the
  system's FFmpeg 6.1 for the run.
- **Flags:** `QUILL_TRACE_VIDEO=1 QUILL_TRACE_TICKS=1 QUILL_ASSUME_ACTIVE=1`.
- **Analysis:** per-player counts of the `VT` lines.
- **Caveat:** absolute numbers are a software renderer's. The comparison
  is what counts.

| | Before | After |
|---|---|---|
| Viewer slice renders/s while PiP plays (steady state) | 7–9 | 0 |
| PiP pulls / new pictures | 101 / 100 in 11.4 s (8.8 new/s) | 153 / 152 in 11.5 s (13.2 new/s) |
| Share of the clip's 15 pictures/s shown | 59 % | 88 % |

Before, the hidden viewer's panel was still a watcher and redrew 10 times a
second on the same thread as the picture. Now only the PiP view is asked
for frames; the UI test asserts this with `take_video_targets`.

After, the animation layer renders 28–30 times a second where it rendered
18–20 before. With the viewer closed, the chat behind PiP draws its own
animations again (the demo's 30 fps history tick), as it does whenever no
viewer covers it.

A second fix landed while measuring. The progress slider's value followed
the transport on every render, and its notify redrew the window every
display frame. It now follows in 1 % steps, while the drawn line reads
the transport itself.

## Deviations

**From the design** (each follows tdesktop):

- **Button corners.** The design puts close top-right and enlarge top-left.
  Here both sit in the title-controls corner, close outermost: top-left on
  macOS, otherwise the side the system's close button is on (GPUI's
  `button_layout`; right when unknown).
- **Volume toggle.** It sits in the opposite top corner with its line, not
  bottom-left.
- **Button size.** Buttons are 36 px, not 32: a 24 px icon plus 6 px each
  side.
- **No buffered tint.** tdesktop's progress line draws only the played and
  unplayed parts.
- **No quarter-of-the-screen clamp.** The default is 320 on the longer
  side; the limit is the screen less 60 px.

**From tdesktop:**

- **Keys.** tdesktop's window never takes focus, so it has none. As the
  design asks, Escape closes and Space pauses or resumes once the window
  has been clicked. It still opens unfocused.
- **No shadow margin or translucent rounded corners.** Quill draws
  tdesktop's opaque path (`useTransparency` false) on every platform: the
  window is the picture. On macOS the panel keeps the system's own shadow
  and rounded corners.
- **Window kind on Linux and Windows.** It is a plain window kept on top.
  GPUI makes a floating window transient for the focused one, which
  tdesktop explicitly undoes (`ClearTransientParent`).
- **Other sounds close PiP.** tdesktop drops PiP for another viewer or
  story clip with sound (`startStreamingPlayer`) and pauses video when
  audio plays. Quill closes it for every sound (`sound_starts`), its
  existing one-sound rule, which also closed the old PiP.
- **Back to the viewer after an account switch.** The window keeps
  playing, but going back does nothing until its account is active
  again: Quill runs one account's session at a time, where tdesktop can
  open the viewer in the other session.
- **A slide finishes after the release.** tdesktop's `finishDrag` stops
  a running snap slide where it is; Quill lets it reach the snap line.
- **Not in Quill's PiP:**
  - Automatic quality switching (`applyVideoQuality`). PiP plays the
    rendition the viewer chose.
  - The display-sleep blocker (`updatePowerSaveBlocker`). The viewer has
    none in Quill either; this is a follow-up for both.
- **Media keys and Now Playing.** These end when the viewer closes into
  PiP. tdesktop has no system media controls for video.
- **Progress line gutters.** A press in the line's 4 px side gutters does
  nothing, where tdesktop clamps it to the start or end.
- **Pointer start under a slide.** Moves read the pointer from the window
  on macOS and Windows, and from the X server on X11. tdesktop reads
  `QCursor::pos()`.

## Supersedes

- **`codex-parity-macos-pip.md`, all three paragraphs:**
  - The window no longer shares frames with a hidden viewer: it owns the
    player, and the viewer closes.
  - Its controls are tdesktop's chrome, not "Pause/Play and Return to
    viewer".
  - "Platform scope: macOS implements this item … The control is
    unavailable on other hosts" no longer holds: PiP works on macOS,
    Windows and Linux. Wayland has no always-on-top.
- **`codex-cross-platform-gaps.md`, the Picture-in-picture row:** the
  follow-up it names (an always-on-top borderless GPUI window) is this PR.
- **`codex-native-video.md`, "Not yet", "PiP on the native player":** done,
  on the shared player.
- **`video-player-core.md`, Approach item 4:** "PiP draws `video_frame`
  from the same shared player" still holds, but the window now owns that
  player rather than borrowing the viewer's.
- **`codex-gpui-window-controls.md`, the Picture-in-picture bullet:** still
  holds (`set_always_on_top` after the window opens, `Floating` on macOS).
  PiP also leaves the taskbar (tdesktop's `Qt::Tool`): `WS_EX_TOOLWINDOW`
  without `WS_EX_APPWINDOW` on Windows (`ITaskbarList::DeleteTab` can
  return on activation), `_NET_WM_STATE_SKIP_TASKBAR` on X11.

## How verified

- **Unit tests** (`cargo test --no-default-features --lib pip`):
  - `panel_tests.rs`:
    - Qt's `scaled`.
    - The default placement, including tdesktop's pull-up to a lower usable
      area such as the macOS menu bar.
    - The minimal and screen-size clamps.
    - Restoring attached and snapped sides on another screen size.
    - A move sliding onto the snap line, and the exact 16 px boundary.
    - Resizes from all eight parts keeping 16:9 and the opposite anchor.
    - Resizes stopping at the minimal and maximal sizes.
    - The resize bands skipping snapped sides.
    - Counting positions, and a move or Wayland counting none.
    - `finishDrag`, the easing and the Wayland aspect fix.
    - System resizes kept to the aspect and its limits.
  - `reopen.rs`: the album item at its offset, a changed album matched by
    file, a moved file, a missing one.
  - `layout.rs`:
    - Both control layouts at 320 × 180.
    - The volume line dropped when narrow.
    - Hit states.
    - Line fractions and heights.
    - The press rules: same control, drag, double click.
    - Escape and Space.
  - `settings::video_pip`: the saved file round-trips, and a broken file
    reads as none.
- **GPUI UI tests** (`video_player/tests/pip.rs`, scripted backends):
  - **The viewer hands its player over:**
    - The viewer closes, and the window's player is the same handle.
    - No seek, no pause, no new backend.
    - Every frame request targets the PiP view.
    - A click on the picture pauses, Space resumes.
    - Escape closes the window, drops the player (the backend is
      released) and clears the link.
  - **Back to the viewer:** the enlarge button reopens the viewer on
    message 204 with the same player, still playing where it was, with
    nothing restarted.
  - **Snap:**
    - The window opens 320 × 320 (square test picture) at (20, 20).
    - A drag far from the edges sets the frame.
    - A move to within 16 px of the left snap line slides onto it (test
      clock, `background_executor().now()`).
    - The release neither pauses nor moves again.
- **GPUI UI tests** (`video_player/tests/pip_lifetime.rs`):
  - The chat's history window is unloaded (paging): PiP keeps playing;
    then a real deletion of the (not loaded) message closes it and drops
    the player.
  - With the message unloaded, back to the viewer still reopens it, alone,
    with the same player and no seek.
  - Locking closes PiP and drops the player with no main-window refresh.
  - Switching the session to another account keeps PiP; another
    account's logout does nothing; its own closes it.
  - A two-video paid album opened on its second video comes back to that
    item (offset 1, file 99) with the same player.
  - Opening the clip again from the chat takes the window's player: the
    window closes, nothing restarts.
- **Real window on X11** (Xvfb, no window manager; the shim built against
  system FFmpeg, driven with XTest):
  - The window opens over the chat and plays.
  - Hovering shows the chrome: volume and line top-left, back-to-viewer
    and close top-right, pause in the middle, times and line at the bottom.
  - A drag moves the native window.
  - A move near the left edge slides it onto the 20 px line.
  - The enlarge button reopens the viewer on the same clip, still playing
    to its end.
  - The screenshots were reviewed and not committed. Drag, snap and
    back-to-viewer were checked again after the review fixes.
- **Gate:** the full gate command in the brief.

## Not verified

- macOS and Windows: this container compiles Linux only. The macOS code
  (`setFrame:display:`, hidden traffic lights, collection behavior, the
  vendored tracking-area change) and the Windows code (`GetWindowInfo`
  border math, `WS_EX_TOOLWINDOW`, `SetWindowPos`) was checked against
  the crates' signatures and is modelled on the already-shipped
  `notify_windows::position` and `window_control` calls, but first
  compiles and runs in CI and the packaging builds. Hover on an unfocused
  macOS panel, the client frame staying put through Windows moves and
  the taskbar staying clear after activation are the things to check
  there.
- Real always-on-top behavior: Xvfb has no window manager, so
  `_NET_WM_STATE_ABOVE`, `HWND_TOPMOST` and the macOS floating level were
  not observed. Wayland was not run; there the window is a normal one the
  compositor may cover.
- Multiple screens, HiDPI scaling on X11 and Windows, and the saved
  position across runs on a real desktop.
- Frame pacing on real hardware: the before/after numbers above are from a
  software renderer.
- A live Telegram account: logout through `LoggingOut` and deletions
  arriving through the update poll were exercised only through the
  demo session's apply path and the hook the poll calls.
- The story, recording-preview, player-bar resume and round-video sound
  routes to `sound_starts` are covered by reading the code, not by a
  test of each.

## Parity

No parity fragment. `parity:media-video-pip` is already checked in the
README (from the macOS-only window). This PR makes it cross-platform,
but it is not declared again: macOS and Windows have not been run, the
display-sleep blocker and automatic quality switching are missing, and
an adversarial check of those would fail.
