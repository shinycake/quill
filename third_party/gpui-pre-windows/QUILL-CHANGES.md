# Changes made by the Quill project

This directory is gpui-pre-windows 0.3.9 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Windows platform, used through `[patch.crates-io]`
in Quill's `Cargo.toml`. Commit `be35a01b` added the unmodified registry copy
(without the registry's `.cargo-ok`, `.cargo_vcs_info.json`,
`Cargo.toml.orig` and `Cargo.lock`), so
`git diff be35a01b -- third_party/gpui-pre-windows` shows every change.
(The patch was first made on 0.3.7, carried to 0.3.8 in
`docs/decisions/codex-gpui-kit-0.7.1.md` and to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`.)

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
  text_input }` registered under the notification's tag with
  `set_notification_extras` before `App::show_system_notification`, and
  `take_reply_text(tag)` for the text typed into the field, parked by the
  backend before the response is sent.
- `src/system_notifications.rs`: `toast_document` takes the extras; the
  image becomes `<image placement="appLogoOverride" hint-crop="circle"
  src="file:///...">`, the action named in `text_input` an `<input
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
