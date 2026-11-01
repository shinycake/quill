# Changes made by the Quill project

This directory is gpui-pre-windows 0.3.10 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Windows platform, used through `[patch.crates-io]`
in Quill's `Cargo.toml`. Commit `6be84cd7` added the unmodified registry copy
(without the registry's `.cargo-ok`, `.cargo_vcs_info.json`,
`Cargo.toml.orig` and `Cargo.lock`), so
`git diff 6be84cd7 -- third_party/gpui-pre-windows` shows every change.
(The patch was first made on 0.3.7, carried to 0.3.8 in
`docs/decisions/codex-gpui-kit-0.7.1.md` and to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`, and to 0.3.10 in `docs/decisions/gpui-kit-0-8-1.md`.)

The Quill project changed it in 2026 so an idle window stops receiving a
`WM_PAINT` and a GPUI frame request on every vblank:

- `src/frame_idle.rs` (new, identical to
  `third_party/gpui-pre-linux/src/linux/x11/frame_idle.rs`): the idle
  decision. After 250 ms of frames that neither drew, presented nor asked for
  another frame, with no frame demand from GPUI in between, a window parks
  and gets no invalidations at all; a demand unparks it. `vsync_has_work`
  decides when the vsync thread may sleep, `contact_frames` what a touchpad
  contact needs. Unit tests run with `scripts/test-frame-idle.sh`.
  `QUILL_IDLE_FRAMES=0` (or `off`), read once, disables parking (upstream
  behavior).
- `src/vsync.rs`: `FrameGate`, the set of parked windows shared with the
  vsync thread, which sleeps on a condition variable, with no timeout, while
  every window is parked (or there is none), until an unpark or `wake` (the
  thread must stop or check its device). It is generic over the window
  list's entry type (0.3.8 tracks each window with its frame signal), and
  `parked_windows` snapshots the parked set before the vsync thread reads the
  window list.
- `src/platform.rs`: upstream's `begin_vsync_thread` invalidates every window
  on every vblank. It now waits on `FrameGate` first and skips parked windows
  (they get no frame signal either); the device-lost check and 0.3.8's stop
  flag still run on every iteration it is awake. Stopping vsync (headless
  mode, dropping the platform) wakes it. A new window is unparked (in case a
  closed one had the same handle), or parked if it was created hidden; a
  closed one is forgotten.
- `src/window.rs`: `draw` and `schedule_frame` mark activity; the new
  `frame_waker` unparks a parked window and invalidates it at once
  (`demand_frames`), except while the window is hidden or minimized;
  `resume_frames` does the same without that check; `park_frames` parks the
  window and its idle state together; `frames_after_draw` applies the idle
  decision.
- `src/events.rs`: `draw_window` applies the idle decision after each frame
  request, kept running while a forced render is pending (device-loss
  recovery) or a touchpad gesture may be in progress; a draw deferred because
  another draw was in progress, and a Direct Manipulation hit test, unpark
  the window. `WM_SHOWWINDOW` parks a window that was hidden (it gets no
  `WM_PAINT`, so the idle decision could not run) and resumes one that is
  shown again (Quill hides the main window to the tray with `ShowWindow`,
  `docs/decisions/codex-gpui-window-controls.md`); minimizing parks it the
  same way, and every other `WM_SIZE` demands frames. A renderer resize
  failure wakes the vsync thread for its device check. A touchpad contact
  arms a 250 ms window timer (`poll_touchpad_contact`): after the contact's
  5 s of frames it updates the viewport, resumes frames once a gesture
  starts or input is waiting, and stops 60 s later.
- `src/direct_manipulation.rs`: the viewport runs in manual-update mode and
  only advances when `draw_window` calls `update`, so it reports what the
  window owes it (`contact_frames`): frames while it is running or in
  inertia or a touchpad contact was handed to it in the last 5 s, a poll for
  the next 60 s.
- `src/gpui_windows.rs`: declares the new module.

Until `docs/decisions/sweep-idle-heartbeat.md` a parked window still got one
invalidation a second as a fail-safe; that heartbeat is gone. Those changes
are type-checked and linted with `cargo clippy --target
x86_64-pc-windows-msvc` on macOS and ran in CI's WARP launch smoke (idle
wakeups printed there); not run on a Windows machine with a GPU, a touchpad
or a second monitor.

The Quill project also changed it in 2026 so a toast can carry the sender's
picture and an inline reply field, which GPUI's `SystemNotification` (in the
unvendored `gpui` crate) has no fields for:

- `src/notification_extras.rs` (new, public, identical to
  `third_party/gpui-pre-macos/src/notification_extras.rs`; unit tests run
  with `scripts/test-notification-extras.sh`): `NotificationExtras { image,
  photo, text_input }` registered under the notification's tag with
  `set_notification_extras` before `App::show_system_notification`, and
  `take_reply_text(tag)` for the text typed into the field, parked by the
  backend before the response is sent.
- `src/system_notifications.rs`: `toast_document` takes the extras; the
  image becomes `<image placement="appLogoOverride" hint-crop="circle"
  src="file:///...">`, the photo an inline `<image src="file:///...">` under
  the text, the action named in `text_input` an `<input
  type="text">` with a send button (`hint-inputId`,
  `activationType="background"`, as Telegram Desktop's `fastReply`). The
  `Activated` handler reads that input from `UserInput()` into the side
  channel when the send button fired.
- `Cargo.toml`: adds the `Foundation_Collections` feature of `windows` for
  `ValueSet` (`UserInput`) to 0.3.9's trimmed feature list.
- `src/gpui_windows.rs`: declares the new module.

Each changed source file starts with a comment saying so. The changes are
licensed under Apache-2.0, like the rest of the crate. Background and what
still needs a live check: `docs/decisions/codex-idle-frames-x11-windows.md`;
notifications: `docs/decisions/codex-notify-native.md`.

The Quill project also changed it in 2026 so a window can switch between the
system frame and an application-drawn one while it is open (Telegram
Desktop's "Use system window frame"). GPUI fixes this at creation from
`TitlebarOptions::appears_transparent`, and `request_decorations` is a no-op on
Windows and is called with `Server` during every window's creation, so it
cannot carry an opt-in:

- `src/native_frame.rs` (new, public): `set_native_frame(hwnd, native)`
  flips the window's `hide_title_bar` and sends `SWP_FRAMECHANGED`, so
  `WM_NCCALCSIZE` and `WM_NCHITTEST` start or stop leaving the system title
  bar and caption buttons alone.
- `src/window.rs`, `src/events.rs`: `hide_title_bar` is a `Cell<bool>`;
  `notify_frame_changed` is `pub(crate)`.
- `src/gpui_windows.rs`: declares the new module.

Not built or run on Windows by the author (no Windows machine); see
`docs/decisions/codex-misc-bundle.md`.

The Quill project also changed it in 2026 so a lost GPU device or a failed
DirectX start no longer crashes the app (step 7 of
`docs/decisions/no-crash-audit.md`):

- `src/platform.rs`: when the vsync thread's device recreation fails,
  `recover_from_gpu_device_lost` retries it after 0.5 s, 2 s and 5 s (each
  round still makes upstream's five quick attempts). If every round fails it
  logs the error, posts `WM_GPUI_DESTROY_AFTER_DEVICE_LOST` to every window
  and stops the vsync thread, instead of `panic!("Device lost: ..")`. If
  DirectX can't be initialized at launch, `connect_initially` logs the error
  and stays headless instead of panicking, and `open_window` returns
  `failed to initialize DirectX: <cause>` so the app can report it.
  `hide_other_apps` and `unhide_other_apps` are no-ops instead of
  `unimplemented!()`.
- `src/events.rs`: a window whose renderer can't recover from a lost device
  retries after 0.2 s and 0.5 s, then logs and posts itself
  `WM_GPUI_DESTROY_AFTER_DEVICE_LOST` (new, `WM_USER + 11`; 0.3.9 took
  `WM_USER + 10` for `WM_GPUI_DISPLAYS_CHANGED`) instead of
  panicking; that message calls `DestroyWindow`, so the normal `WM_DESTROY`
  close path runs even if the app hides windows on `WM_CLOSE`.
- `src/directx_renderer.rs`: `pre_draw` and `present` return an error
  instead of `expect`/`unwrap` when the renderer has no resources or devices
  (what a failed recovery leaves), so a draw queued before the window is
  destroyed can't panic. The renderer keeps its creation-time
  `disable_direct_composition`; upstream re-derived it from
  `direct_composition`, which each recovery attempt clears, so every retry
  after a failed attempt lost DirectComposition. The `dxgi_device` unwraps
  on that path return errors.
- `src/directx_devices.rs`: `try_to_recover_from_device_lost` is a loop
  instead of `find_or_last(..).unwrap()`.
- `src/platform.rs`, `src/events.rs`: after an unrecoverable device loss
  the vsync thread posts `WM_GPUI_GPU_DEVICE_GONE` (`WM_USER + 12`) to the
  platform window, which drops the dead devices (so `is_windowed()` is false
  and `open_window` returns the reason; `request_windowing(Windowed)` can
  attach new ones). `close_one_window` logs an untracked window instead of
  unwrapping, and `handle_gpui_events` returns `None` instead of
  `unreachable!()`.

Type-checked with `cargo check --target x86_64-pc-windows-msvc` on macOS;
not built or run on Windows. See `docs/decisions/gpui-fork-panics.md`.

The Quill project changed it in 2026 to draw HDR stills (CPU pixels in a
`SurfaceFrame`, from the vendored gpui-pre core) in HDR on displays with
Windows HDR on (`docs/decisions/hdr-viewer-windows.md`):

- `src/hdr_output.rs` (new): `set_hdr_output(hwnd, on)` records that a
  window wants HDR output; `display_info(hwnd)` reads whether its display is
  in HDR mode (`IDXGIOutput6::GetDesc1`, colour space
  `RGB_FULL_G2084_NONE_P2020`), its peak (`MaxLuminance`) and the user's SDR
  white (`DisplayConfigGetDeviceInfo`, `DISPLAYCONFIG_SDR_WHITE_LEVEL`),
  cached for a second.
- `src/hdr_surfaces.rs` (new, a child module of `directx_renderer`): uploads
  each `SurfaceFrame` into an `R16G16B16A16_FLOAT` texture with a mip chain
  (cached by frame id, last two), draws it with the `hdr_surface` shaders,
  and switches the swap chain to `R16G16B16A16_FLOAT` +
  `DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709` (scRGB) while the window wants
  HDR output and its display is in HDR mode, and back to 8-bit sRGB
  otherwise. A failed switch is logged and not retried until the window
  releases HDR output.
- `src/directx_renderer.rs`: `draw_surfaces` draws through
  `hdr_surfaces`; `draw` syncs the output mode first; `GlobalParams` carries
  `hdr_output` and `output_scale` (SDR white in scRGB units); resizing keeps
  the current format; device loss resets to 8-bit; the opaque clear colour
  is SDR white in either mode; `render_to_image` refuses an HDR swap chain.
- `src/shaders.hlsl`: every pixel shader that writes the swap chain ends
  with `to_output` (exact sRGB decode times `output_scale` in scRGB, the
  colour unchanged otherwise), so the UI looks the same in both modes; path
  rasterization into the 8-bit intermediate is unchanged. New
  `hdr_surface_vertex` / `hdr_surface_fragment`: unpremultiply, tone-map to
  the frame's headroom (the curve of the Metal shader) and write scRGB, or
  clip to SDR white on an 8-bit swap chain.
- `build.rs` compiles the `hdr_surface` module; `Cargo.toml` adds the
  `Win32_Devices_Display` feature.

Type-checked and linted with `cargo clippy --target x86_64-pc-windows-msvc`
on macOS and the HLSL validated with glslang; not run on Windows.

The Quill project changed it in 2026 to draw video pictures as NV12
(`SurfaceFormat::Nv12` from the vendored gpui-pre core) and convert them on
the GPU, so the decoder skips its BGRA conversion and a 1080p picture
uploads 3 MB instead of 8 MB (`docs/decisions/video-player-nv12.md`):

- `src/yuv_surfaces.rs` (new, a child module of `directx_renderer`): each
  NV12 `PaintSurface` is uploaded with `UpdateSubresource` into an
  `R8_UNORM` luma texture and an `R8G8_UNORM` half-size chroma texture,
  only when its frame id is new. Texture pairs are slots reused for the
  next picture of the same size (up to 16, one per clip on screen; a slot
  not drawn for two frames is freed) and remade when the size changes. A
  slot whose frame is in the current scene is never handed to another
  frame (`begin_frame`), so two clips of one size don't trade slots and
  re-upload every frame. The per-surface instance data carries
  `YuvColor::rgb_rows` (the BT.601/BT.709 matrix and full/limited range,
  picked in gpui-pre). The chroma slot (`t2`) is unbound after each draw.
- Each NV12 frame is marked drawn or refused (`SurfaceFrame::mark_*`). A
  frame is refused when the NV12 pipeline couldn't be made for this
  renderer, its textures can't be made, its planes don't match its size,
  or it is larger than a texture. After a texture failure the renderer
  refuses NV12 for 120 frames, then tries again. The producer shows a
  refused frame as an image; nothing fails the frame, and one window's
  failure doesn't change what another window draws.
- `src/surface_support.rs` (new, crate-private): with `QUILL_TRACE_VIDEO`
  set, uploads print `VU,nv12,<bytes>,<us>`, and atlas uploads of 64 KiB
  and up print `VU,atlas,<bytes>,<us>` (`directx_atlas.rs`), so the two
  paths can be compared.
- `src/directx_renderer.rs`: `draw_surfaces` sends each surface, in order,
  to `hdr_surfaces` or `yuv_surfaces` by format; the NV12 pipeline is an
  `Option` (a failure to make it doesn't fail the renderer); slots age at
  the end of each frame and go with the HDR textures in a frame without
  surfaces.
- `src/shaders.hlsl`: `yuv_surface_vertex` / `yuv_surface_fragment`: sample
  luma (`t0`) and chroma (`t2`), `dot` each row with `(y, cb, cr, 1)`,
  saturate, and write through `to_output` (so HDR output mode keeps the
  picture at SDR white). `build.rs` compiles the module.
- `src/hdr_surfaces.rs`: uses its own 8 bytes a pixel (the core's
  `bytes_per_pixel` became `byte_len`) and ignores NV12 frames.

The HLSL was validated with glslang on macOS; the CI Windows release build
compiles it with fxc. Not run on Windows by the author.

## Touchpad inertia is momentum

`src/direct_manipulation.rs`: when the viewport goes from running to
inertia (the fingers lifted) during a scroll, the handler now emits the
gesture's `Ended`, and the inertia deltas that follow stay phaseless `Moved`
events, which is how a macOS trackpad reports momentum. Before, `Ended` came
only when the viewport was ready again, after the inertia, so consumers saw a
held finger throughout the fling. Quill's chat list takes a held edge past a
threshold as a pull that expands the stories strip, and a fling into the top
did that on Windows (`docs/decisions/elastic-chat-list.md`). The final
`Ended` at ready stays and is harmless. Not compiled or run on Windows by the
author of the patch.

## Images get atlas textures of their own

Added in 2026 to give back memory after browsing
(`docs/decisions/sweep-r3-browse-memory.md`):

- `src/directx_atlas.rs`: implements `AtlasBackend::insert_image` (see
  gpui-pre's `QUILL-CHANGES.md`). Each `DirectXAtlasTexture` records
  whether it holds images, and `allocate` only places a tile in a texture
  holding the same kind, so an image texture is freed once its images
  leave. Same change as gpui-pre-apple's `metal_atlas.rs`. Test:
  `test_glyphs_never_keep_an_image_texture_alive` (on WARP); not run on
  Windows by the author.

The Quill project also changed it in 2026 for Agent access's screenshots
(`docs/decisions/agent-access.md`, phase 5 step 3):

- `src/directx_renderer.rs`: `render_to_image` is built in every
  configuration (it was test-only). It copies the render target into a
  single-sample staging texture with `CopyResource`, which needs matching
  sample counts: the render target is buffer 0 of the flip-model swap
  chain, created with one sample (flip model allows no other; the 4x MSAA
  texture is path rasterization's own). It checks the count and refuses
  with an error rather than copy, so no `ResolveSubresource` is needed.
- `src/window.rs`: `WindowsWindow::capture_image` renders the scene through
  it, and answers with an error while the renderer is borrowed. Not run on
  Windows by the author. An HDR swap chain format refuses readback with an
  error.

Carried to 0.3.10, which validates GPUI's window messages with a per-process
number (`handle_gpui_events`): `WM_GPUI_DESTROY_AFTER_DEVICE_LOST` goes through
that check too and is posted with the validation number, and the dispatcher's
`unreachable!()` for an unknown id returns `None` instead.
- 0.3.10: `direct_manipulation.rs` inertia-start event uses `PlatformInput::Scroll` with `ScrollSource::Pointer` (the 0.3.10 rename).

The Quill project also changed it in 2026 so color emoji glyphs can be swept
(`docs/decisions/idle-wallpaper-and-emoji-trim.md`):

- `src/directx_atlas.rs`: `DirectXAtlas` forwards `PlatformAtlas::next_use_mark`,
  `emoji_glyph_count` and `remove_emoji_glyphs_unused_since` to its
  `AtlasState` (see gpui-pre's `QUILL-CHANGES.md`).

## Small pictures get small textures

Added in 2026 (`docs/decisions/images-left-behind.md`):

- `src/directx_atlas.rs`: tiles are sorted into `TileClass`es and small
  images get 512² textures, the same change as gpui-pre-apple's
  `metal_atlas.rs`. The tile-reuse test's keeper tile is 200². Not run on
  Windows by the author.

## A bad DWM refresh report can't stop the vsync thread

Added in 2026, listed in `docs/decisions/no-crash-audit.md` section 4:

- `src/refresh_interval.rs` (new, std-only; unit tests run with
  `scripts/test-refresh-interval.sh`, which CI's `linux-fmt-clippy-test` job
  runs): `refresh_interval` replaces upstream's `retrieve_duration`. Upstream
  divided by `ticks_per_second / 1_000_000`, and its low-period fallback passed
  the refresh rate through the same code, so any rate numerator under 1,000,000
  (60/1, 60000/1001) divided by zero and ended the thread with windows left on
  screen. It also took any large period as the sleep. The new function divides
  counts in `u128` with `checked_div` and returns the first of the two reports
  between 1 ms and 100 ms, or the 60 Hz default.
- `src/vsync.rs`: `get_dwm_interval` calls it. `QPC_TICKS_PER_SECOND` and its
  `unwrap` are gone: a failed `QueryPerformanceFrequency` takes the same `Err`
  path to the default as a failed `DwmGetCompositionTimingInfo`.
- `src/gpui_windows.rs`: declares the new module.

Type-checked with `cargo check -p gpui-pre-windows --target
x86_64-pc-windows-msvc` on macOS. Not run on Windows by the author.

## Device-loss recovery can't deadlock with a window opening or closing

Added in 2026:

- `src/window_list.rs` (new, std-only; unit tests run with
  `scripts/test-window-list.sh`, which CI's `linux-fmt-clippy-test` job runs):
  `for_each_after_unlock` copies a handle out of each window, drops the list's
  guard, then calls back with each handle.
- `src/platform.rs`: `handle_gpu_device_lost` sends through it. Upstream held
  `all_windows.read()` across `SendMessageW`, which waits for the UI thread,
  while `open_window` and `close_one_window` block the UI thread in
  `write()` without pumping messages, so neither thread could move. Every
  other holder of the list either runs on the UI thread or makes only calls
  that do not wait for it (`PostMessageW`, `IsWindowVisible`, `IsIconic`,
  `RedrawWindow` with `RDW_INVALIDATE` alone).
- `src/gpui_windows.rs`: declares the new module.

Type-checked with `cargo check -p gpui-pre-windows --target
x86_64-pc-windows-msvc` on macOS. Not run on Windows by the author.

## Async timers fire on time, not on the 15.6 ms system tick

Added in 2026. Upstream's `dispatch_after` armed a thread-pool timer per
call, and those land on the system timer tick (15.6 ms by default). Measured
on `windows-latest` VMs with no window: a 30 fps wait of 33.3 ms took about
47 ms (23 Hz), a 50 fps wait of 18.4 ms about 32 ms (34 Hz), and a 60 fps wait
of 15.3 ms fits one tick but slipped a whole tick on 1-17% of waits, or 18-32%
with 2 to 6 ms of main-thread work at 60 Hz (48-54 Hz). Through this timer
thread those loops ran at 29.6, 53.3 and 60.1 Hz.

- `src/timer_queue.rs` (new, std-only; unit tests run with
  `scripts/test-timer-queue.sh`, which CI's `linux-fmt-clippy-test` job runs):
  the deadline heap, earliest first, ties in the order added, and the wait
  until the next deadline (none when empty).
- `src/timer_thread.rs` (new): one "Timer" thread fires every
  `dispatch_after` runnable from that heap. It sleeps on a waitable timer made
  with `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION` (Windows 10 1803 and later),
  which needs no process-wide `timeBeginPeriod`, plus an event that a new
  earliest deadline sets. With nothing pending it waits on the event alone,
  with no timeout, so an idle app gains no wakeups. Where the flag is
  rejected it uses a plain waitable timer and holds `timeBeginPeriod(1)` only
  across waits shorter than 100 ms (animation frames), not idle ones.
- `src/dispatcher.rs`: `dispatch_after` schedules on that thread; the
  thread-pool timer remains only for when the thread could not start.
- `src/gpui_windows.rs`: declares the new modules.

Type-checked with `cargo check -p gpui-pre-windows --target
x86_64-pc-windows-msvc` on Linux. Not run on Windows by the author.

## The file pickers free each path and skip items without one

Added in 2026:

- `src/platform.rs`: `file_system_path` copies `GetDisplayName(SIGDN_FILESYSPATH)`
  into a `PathBuf` (as UTF-16, so a name that is not valid Unicode survives)
  and frees the string with `CoTaskMemFree`. Upstream's open dialog never
  freed it, and one item without a file system path (a phone, a library)
  failed the whole selection. Such items are now skipped, and a selection
  with none left returns the error as upstream did. The save dialog uses the
  same helper.

Type-checked with `cargo check -p gpui-pre-windows --target
x86_64-pc-windows-msvc` on Linux. Not run on Windows by the author.

## Giving up on a lost device closes the windows

Added in 2026:

- `src/gpui_message.rs` (new, std-only; unit tests run with
  `scripts/test-gpui-message.sh`, which CI's `linux-fmt-clippy-test` job runs):
  the `WM_GPUI_*` ids move here from `src/events.rs` as `GpuiMessageId`, whose
  value can't be read outside the module. `ValidationNumber::message` is the
  only way to build a `GpuiMessage`, so an id can't reach `PostMessageW` or
  `SendMessageW` without the number. Upstream's ids are plain `u32`s and every
  sender passes the number by hand.
- `src/wrapper.rs`: `GpuiMessage::post` and `send`, the only places that hand
  one to Win32.
- `src/platform.rs`: `destroy_all_windows_after_device_lost` posted
  `WPARAM::default()`, so the windows' `handle_gpui_events` dropped the message
  and every window stayed open and blank. It now gets the number.
- `src/events.rs`, `src/platform.rs`: `handle_msg` passes whatever it does not
  handle itself to `handle_gpui_events`, which parses the id with
  `GpuiMessageId::from_raw` and then checks the number. The window's check now
  calls `log::error!` on a wrong number, as the platform's always did, and
  Quill's `log` backend writes it to `logs/platform.log`.
- `src/dispatcher.rs`, `src/window.rs`: `validation_number` is a
  `ValidationNumber`.
- `src/gpui_windows.rs`: declares the new module.

Type-checked with `cargo check -p gpui-pre-windows --target
x86_64-pc-windows-msvc` on macOS. Not run on Windows by the author.

The Quill project also changed it in 2026 so a window can't survive a device
give-up:

- `src/platform.rs`: the vsync thread no longer posts
  `WM_GPUI_DESTROY_AFTER_DEVICE_LOST` itself. `handle_device_gone` posts it
  after dropping the devices, on the thread that owns the windows. The vsync
  thread read the list before `WM_GPUI_GPU_DEVICE_GONE` arrived, so a window
  opened in between missed the destroy. `open_window` fails from this point
  on, so no window can miss this one.
- `src/window.rs`, `src/events.rs`, `src/platform.rs`: `WM_DESTROY` sets a
  `destroyed` flag that the tracked window shares. A destroyed window stays
  listed until its `WM_GPUI_CLOSE_ONE_WINDOW` arrives, and a post to it fails.
- `src/window_list.rs`: `for_each_open_after_unlock` skips destroyed windows.
  It is std-only, tested by `scripts/test-window-list.sh`.

## The Windows platform never panics on its own threads

Added in 2026, after an audit of the earlier panic pass
(`docs/decisions/gpui-fork-panics.md`, "Windows, second pass"):

- `src/directx_devices.rs`: `get_device` returns an error for a device
  `D3D11CreateDevice` did not hand back, `get_adapter` skips an adapter
  without a device context and returns an error after the last adapter, and
  `DirectXDevices::new` logs an unexpected feature level. No `unwrap` or
  `unreachable!` is left in the file.
- `src/platform.rs`: `begin_vsync_thread` returns the thread's spawn error
  instead of unwrapping it. `attach_gpu` then detaches the GPU again and
  returns the error, so launch fails with the startup alert and
  `request_windowing(Windowed)` fails, instead of the app panicking.
- `src/platform.rs`: the vsync thread's body runs under `catch_unwind`. On a
  panic it logs and posts `WM_GPUI_GPU_DEVICE_GONE` to the platform window,
  which drops the devices and destroys every window, instead of leaving them
  without frames.
- `src/window_list.rs`: `WindowList` replaces the bare `RwLock` and the
  `for_each_after_unlock` / `for_each_open_after_unlock` helpers. Callers get
  copies only (`snapshot`, `find`) or change it under its own lock (`push`,
  `remove`), so none can hold the lock across `SendMessageW`. `src/platform.rs`
  and `src/vsync.rs` use it; `TrackedWindow` is `Clone`.
- `src/platform.rs`: after `open_window` lists a new window, it checks the
  window's device against the platform's current one and, when recovery
  replaced the devices meanwhile, sends the window `WM_GPUI_GPU_DEVICE_LOST`
  with the new ones (`hand_over_recovered_devices`).
