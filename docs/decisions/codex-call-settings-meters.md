# Call settings: accept calls here, system sound preferences

Parity item `parity:calls-settings-meters`.

## What tdesktop does

`settings/sections/settings_calls.cpp`:

- A microphone test with a live `Ui::LevelMeter` fed by `AudioInputTester`
  (already in Quill: `ui/call_audio_ui.rs`, `calls/level_tap.rs`; cpal
  stream on its own thread, frame-clock driven, stopped when the page is
  no longer rendered).
- Output and input device pickers.
- "Accept calls on this device" toggle, backed by
  `callsDisabledHere` / `toggleCallsDisabledHere` (the current session's
  `toggleSessionCanAcceptCalls`).
- "Open system sound preferences" via `OpenSystemSettings(Audio)`; on
  Linux without a known panel it shows an info box.
- There is no output "Test" playback button in tdesktop, so none was added.

## What changed

- `src/call_device_prefs.rs`: current-session lookup in the sessions list
  and the per-platform sound-settings target (macOS
  `x-apple.systempreferences:com.apple.Sound-Settings.extension`, Windows
  `ms-settings:sound`, Linux none, so the row is hidden).
- `src/ui/call_device_prefs_ui.rs`: the Switch and button in Settings >
  Calls. The Switch reuses `toggle_session_calls`; the sessions list is
  fetched with the guarded `maybe_fetch_active_sessions` when the section
  first renders. The Switch stays hidden until the list arrives.
- Request JSON for `toggleSessionCanAcceptCalls` is already covered in
  `telegram/requests/tests_auth.rs`.

## Verification

Unit tests for the session lookup and platform routing; gate. Not verified
by hand: clicking the toggle against a real account (the setting was not
changed), and the Windows/Linux paths (compiled by cfg only). Device
selection (pickers) is not part of this change.
