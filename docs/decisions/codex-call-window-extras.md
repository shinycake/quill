# Call window extras

Items: `parity:calls-window-options`, `parity:calls-peer-indicators`.

## What tdesktop does

- `Calls::Panel::createPinOnTop`: a pin icon in the call window's title area
  toggles "always on top" and shows a toast. Hidden while maximized.
- `Calls::Panel::initMediaDeviceToggles` / `showDevicesMenu`: a small corner
  button on the camera button and on the mute button opens a popup menu of
  devices (camera; or playback and capture) over the button row. Picking one
  saves it as the call device setting.
- `Calls::Panel` shows `lng_call_microphone_off` under the status when the
  peer's audio state is Muted, and a battery pill (`lng_call_battery_level_low`
  tooltip) when `remoteBatteryLevelIsLowUpdated(true)` arrives from tgcalls.

## State in Quill before this change

- Stay on top: already in the call window (`window_control::pin_on_top_button`,
  macOS `setLevel`, Windows `HWND_TOPMOST`, X11 `_NET_WM_STATE_ABOVE`). No
  vendored GPUI patch is needed. Wayland cannot do it; the button simply
  does nothing there (`set_always_on_top` returns false).
- Peer microphone-off line: already wired from ntgcalls'
  `ntg_remote_source_change_callback` through `connect/call_audio.rs` to
  `ActiveCall::remote_audio_muted` and drawn in `call_panel.rs`.
- Device pickers existed only as radio groups in the main window's call card
  and Settings > Calls.

## What changed

- `src/ui/call_panel_devices.rs` (new): chevron corner buttons on the video
  and mute buttons of the call window (active phase only) and a device menu
  overlay (microphone and speaker for mute; camera for video). A pick calls
  the existing `QuillApp::select_call_device`, so the engine, driver
  selection state and error notes are shared with the other pickers. An
  outside click closes the menu; the corner button toggles it, and the other
  button's corner switches it.
- `src/ui/call_panel.rs`: a `devices_menu` field and small hooks. The backdrop and connecting-arc helpers moved to `call_panel_backdrop.rs` so the file stays under the size limit.

## Not done: the battery-low indicator

`vendor/ntgcalls/include/ntgcalls.h` has no battery or low-power callback or
setter. The only remote-state callback is `ntg_remote_source_change_callback`
(ssrc, stream status, device). tgcalls carries the battery flag in its own
signaling messages that ntgcalls consumes internally, and TDLib's
`updateNewCallSignalingData` payload is encrypted media-protocol data, so the
app cannot read it either. Without an engine signal there is nothing to show,
so `parity:calls-peer-indicators` is not declared. The microphone-off half is
already working.

## Also not done

tdesktop's pin toast ("Call window is pinned on top.") and unpinning on
maximize are not implemented; the pin icon swaps between pin and pin-off.

## Verification

Unit tests in `call_panel_devices.rs`: device grouping and selection marks,
empty-section behavior, and open/close/switch routing of corner clicks.
No real call was started and the menu was not exercised against a live
engine.
