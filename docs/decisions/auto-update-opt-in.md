# Install updates automatically (opt-in)

Idan asked for this in the project thread on 2026-10-17: "build the opt in auto update toggle". He had chosen "Opt-in toggle" for "Should Quill install updates silently in the background, like Telegram Desktop?": "Adds the setting, off by default. Updates install on the next restart only once you turn it on."

## What tdesktop does

- `settings/sections/settings_advanced.cpp:1051,1176-1190`: the "Update automatically" switch (`lng_settings_update_automatically`) writes `cAutoUpdate` and starts or stops `Core::UpdateChecker`.
- `core/update_checker.cpp:1699`: `Updater::start` returns at once when `cAutoUpdate()` is off, so nothing is checked or downloaded.
- `core/settings.cpp:32`: the default is on. Quill's default is off, by Idan's choice.
- tdesktop applies a downloaded update on restart and shows "New version is ready" (`lng_settings_update_ready`) with a restart button.

## What Quill did

An update check found a release and the user had to click "Download, install and restart". The "Install beta versions" switch lived in the update section of the Appearance panel (where Quill keeps its update switches; the Advanced page has none).

## What changed

- `advanced/auto_update.rs`: the setting (JSON under the app root, off by default, honours `fast_buttons::persistence_enabled`) and `decide()`: stage only when opted in, the install can replace itself (`update_install::can_install_here`) and the release has a verified binary for this platform.
- Update section: "Install updates automatically" switch above "Install beta versions". Where self-update cannot run (app bundle, package manager, read-only folder, store install) it is disabled and shows the existing "Install the release package from GitHub" text.
- ON: after the regular check, the release is downloaded and verified with the existing path (`download_and_stage`) into a new `UpdateState::Staged`. The label reads "Quill X is ready. Restart to apply." with a "Restart to update" button and the update banner. Quill never restarts itself.
- When Quill quits with an update staged and the setting still on, the helper is started with `relaunch = false` (new plan field), installs once the process exits, and does not reopen the app. "Restart to update" turns relaunch on (`arm_relaunch`) and uses the existing handoff.
- OFF: unchanged.

## Verification

Unit tests for the setting and `decide`, plan relaunch flag, a GPUI test of the switch, captures in light and dark (see PR).
