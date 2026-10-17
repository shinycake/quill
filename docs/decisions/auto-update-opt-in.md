# Install updates automatically (opt-in)

Idan asked for this in the project thread on 2026-10-17: "build the opt in auto update toggle". He had chosen "Opt-in toggle" for "Should Quill install updates silently in the background, like Telegram Desktop?": "Adds the setting, off by default. Updates install on the next restart only once you turn it on."

## What tdesktop does

- `settings/sections/settings_advanced.cpp:1051,1176-1190`: the "Update automatically" switch (`lng_settings_update_automatically`) writes `cAutoUpdate` and starts or stops `Core::UpdateChecker`. Turning it off stops the checker and cancels a running download.
- `core/update_checker.cpp:1699`: `Updater::start` returns at once when `cAutoUpdate()` is off, so nothing is checked or downloaded.
- `core/sandbox.cpp:488-491`: a Ready update is applied by `checkReadyUpdate()` at the next start, before the app runs. The switch is not consulted again, so an update that is Ready installs even if the switch was turned off afterwards.
- `core/settings.cpp:32`: the default is on. Quill's default is off, by Idan's choice.

## What Quill does

- The setting (`advanced/auto_update.rs`): JSON under the app root, off by default, installation-wide, honours `fast_buttons::persistence_enabled`.
- ON: after the regular update check (only while "Check for updates on launch" is on, because the switch is only shown then), a release that passes the same check as the download (`update_install::valid_release`) is downloaded, verified and staged in the background (`UpdateState::Staged`). The card and the banner read "Quill X is ready. Restart to apply." with a "Restart to update" button. Quill never restarts itself.
- Install at the next start, like tdesktop (`update_install/staged.rs`, one call from `main.rs`): after the single-instance lock and before the UI or any account opens, a valid newer staged update is handed to the existing helper and the process exits; the helper installs it and starts the new version. This replaces an earlier design that started the helper at quit, which raced with a quick reopen and could be killed at logout.
- Setting turned off after staging: same as tdesktop, a Ready update still installs at the next start. The label stays true ("Restart to apply"); nothing is discarded. Turning the switch off does not cancel a download in progress (tdesktop does); the finished download is installed at the next start.
- Stale files: at startup every staged plan that is not newer than the running version, is for another install, or has lost its files is deleted with its staged package; of several valid ones the newest is kept. Staging a new update clears earlier staged files (now on every platform).
- OFF: unchanged; the user clicks "Download, install and restart".

## Platforms

- Linux and Windows self-update through the existing package install path. The switch is available there when the install can replace itself.
- macOS is not supported yet. The macOS release is `Quill.app`, which Quill does not replace, and no bare-binary asset is published. `self_update_available()` is false on macOS, so the switch is off and disabled with "On macOS, Quill updates through the release package from GitHub for now." The macOS branch of `installable()` now also refuses package-manager and read-only locations. Bundle self-update on macOS is a separate piece of work.
- Elsewhere the switch is disabled and says the installation cannot replace itself (app image, package manager, read-only folder).

## Verification

Unit tests for the setting, the decision, the check-result-to-staging step, the row state, the startup pick/discard rules and the staged label. GPUI tests for the switch (click flips the setting; a disabled switch ignores clicks), row visibility and the banner. Captures in the PR.
