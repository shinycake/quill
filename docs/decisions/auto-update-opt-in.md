# Install updates automatically (opt-in)

Idan asked for this in the project thread on 2026-10-17: "build the opt in auto update toggle". He had chosen "Opt-in toggle" for "Should Quill install updates silently in the background, like Telegram Desktop?": "Adds the setting, off by default. Updates install on the next restart only once you turn it on."

## What tdesktop does

- `settings/sections/settings_advanced.cpp:1051,1176-1190`: the "Update automatically" switch (`lng_settings_update_automatically`) writes `cAutoUpdate` and starts or stops `Core::UpdateChecker`. Turning it off stops the checker and cancels a running download.
- `core/update_checker.cpp:1699`: `Updater::start` returns at once when `cAutoUpdate()` is off, so nothing is checked or downloaded.
- `core/sandbox.cpp:488-491`: a Ready update is applied by `checkReadyUpdate()` at the next start, after the single-instance server is listening and before the app runs. The switch is not consulted again, so an update that is Ready installs even if the switch was turned off afterwards.
- `core/update_checker.cpp:1993-2165` (`checkReadyUpdate`): on Linux the downloaded package is kept and read and verified again (`ReadUpdatePackage`, `UnpackUpdateV2`) before it is unpacked. Every failure on the way calls `ClearAll()`, so a ready update is tried once.
- `platform/linux/launcher_linux.cpp:186-202`, `platform/win/launcher_win.cpp:164-184`: the updater relaunches the app with the launch mode it had (`-autostart`, `-startintray`, `-workdir`, `-key`).
- `core/settings.cpp:32`: the default is on. Quill's default is off, by Idan's choice.

## What Quill does

- The setting (`advanced/auto_update.rs`): JSON under the app root, off by default, installation-wide, honours `fast_buttons::persistence_enabled`.
- ON: after the regular update check (only while "Check for updates on launch" is on, because the switch is only shown then), a release that passes the same check as the download (`update_install::valid_release`) is downloaded, verified and staged in the background (`UpdateState::Staged`). The card and the banner read "Quill X is ready. Restart to apply." with a "Restart to update" button. Quill never restarts itself.
- Install at the next start, like tdesktop (`update_install/staged.rs`, called from `main.rs` once this process holds the single-instance lock and before the UI or any account opens): a valid newer staged update is claimed and handed to the existing helper, and the process exits. The helper installs it and starts the new version with the launch's arguments.
- Setting turned off after staging: same as tdesktop, a Ready update still installs at the next start. The label stays true ("Restart to apply"); nothing is discarded. Turning the switch off does not cancel a download in progress (tdesktop does); the finished download is installed at the next start.
- OFF: unchanged; the user clicks "Download, install and restart". A start with nothing staged only lists the executable's directory: no write probes, no temp cleanup.

## One helper per update, tried once

A start that applies an update exits with no window for the few seconds the helper needs, so a second click on the icon is likely. Two things must hold: a second launch never starts a second helper on the same plan, and no failure makes every launch exit.

Each staged update moves through three names beside the executable (`update_install/claim.rs`):

1. `<stamp>.json`: staged and ready.
2. `<stamp>.claim`: a launch claimed it. The claim is an exclusive create (`create_new`), so of two launches only one wins. The claim holds the launch's pid and the arguments to relaunch with, and the launch keeps a shared lock on it until it exits.
3. `<stamp>.applying`: the helper took it over. It locks the claim while its launch still holds its own lock, then renames it. Only one helper can win that rename.

On every path out, the helper deletes the claim right before it starts Quill: the new version after a swap, or the old one with a failed receipt. Validation failures, a parent that did not exit, a probe failure and a package that changed all take that path. So an update is tried once, like tdesktop's `ClearAll()`, and the Quill it starts never finds the claim.

A lock dies with its process. A start that finds a claim nobody has locked knows its launch and helper are gone. It discards the claim and its files, writes a failed receipt ("Update installation failed…", which also keeps the launch check from staging it again), and opens normally. The one exception is a `.claim` younger than 30 s, whose launch may have exited a moment before its helper locked it. A modification time in the future counts as old. On a file system without locks, any claim younger than 10 minutes counts as live, and an older one as stale.

A start that finds a live claim exits: the helper starts Quill when it is done. `main.rs` checks this before the single-instance lock (`update_in_progress`), so the waiting launch never holds the lock that the relaunched version needs. A start that could not check the single-instance lock ("unavailable; continuing") never applies an update, because another Quill could be running from the same install. The manual "Download, install and restart" and "Restart to update" go through the same claim (`update_install::hand_off`).

Why a claim file and not the single-instance lock held by the helper:

- The parent holds the single-instance endpoint until it exits, and the helper can only take it after that. A second launch fits into that gap.
- The endpoint can be unavailable, and Quill then starts without it.
- The relaunched version needs the endpoint. The helper would have to let go of it right as the install finishes, which opens the gap again. While the helper held it, a launch's links would be forwarded to a process with no window.
- The claim sits beside the plan. It covers a helper that never started and a helper that died, and the next start can tell both from a live one. It also covers the manual install with no UI change.

## Arguments through the relaunch

The launch's `--start-minimized`, `tg:`/`t.me` links and files go into the claim (`start_files::relaunch_args`): the flag first, then `--`, then links and absolute file paths. The new version (or the old one, after a failure) is started with them, so a launch at login stays in the tray and a link click opens its link. The data directory needs no flag: it comes from the environment, which the helper and the new process inherit. "Restart to update" relaunches with no arguments, because the user is looking at a window.

## The staged package is checked again

The archive's SHA-256 is checked at download. A staged tree can then wait for days, and another install sharing the parent folder, an interrupted delete, antivirus or a disk error can damage it. A tree with a missing library still answers `--version`.

At staging, after the version probe, the plan records the size and SHA-256 of every file in the unpacked package (`update_install/manifest.rs`). The helper checks the tree against it before the probe and the swap. A file missing, added or changed fails the install with a failed receipt and the old version restarted. A package plan without a manifest is discarded at start.

tdesktop keeps the package and re-verifies it instead. A manifest keeps one copy on disk and needs no second unpack at the start. The cost is one more read of the unpacked package, in the helper. That was not measured.

## Stale files

At start, every ready plan that is not newer than the running version, is for another install, has lost its files or has no manifest is deleted with its staged package; of several valid ones the newest is kept. Unrelated temp entries are no longer swept at start, so one install's start cannot delete another install's staged tree in a shared parent folder. Staging a new update still clears earlier staged files.

## Platforms

- Linux and Windows self-update through the existing package install path. The switch is available there when the install can replace itself.
- The claim's locks are `std::fs::File::lock_shared` / `try_lock` (`flock` on Unix, `LockFileEx` on Windows), so no new platform code was needed. On Windows a lock also blocks reads from other handles, so every read of a locked claim goes through the handle that holds the lock.
- macOS is not supported yet. The macOS release is `Quill.app`, which Quill does not replace, and no bare-binary asset is published. `self_update_available()` is false on macOS, so the switch is off and disabled with "On macOS, Quill updates through the release package from GitHub for now." The macOS branch of `installable()` also refuses package-manager and read-only locations. Bundle self-update on macOS is a separate piece of work.
- Elsewhere the switch is disabled and says the installation cannot replace itself (app image, package manager, read-only folder).

## Verification

- Unit tests:
  - the setting, the decision and the check-result-to-staging step;
  - the row state and the staged label;
  - claims: one launch, one helper, live and stale, the clock going back, a helper that cannot start;
  - the startup scan (`on_start`): a hand-off with the relaunch arguments, a second start while the first is applying, a dead claim and a dead helper discarded with a failed receipt, a fresh claim left alone, nothing staged means no probes;
  - the manifest;
  - the helper: install with the arguments passed through, a failed probe, a missing or corrupted package file, a refused handoff, a second helper on the same plan;
  - the relaunch arguments.
- GPUI tests:
  - the switch: a click flips it; a disabled switch ignores clicks, and the test now fails if the switch never rendered;
  - row visibility and the banner;
  - a check result staged only when opted in.
- Not run: the helper on a real Linux or Windows install. Windows code is not compiled on PRs.
