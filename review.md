CHANGES

# Review of PR 1307, second pass (claude/sweep-b1, bundle B1)

Scope: the seven commits after b4b5a026e (c1c1e88 to acb5fa9, head acb5fa9),
checked against the findings in review.md on claude/review-b1 and the
"What stays open" list in PR.md on claude/prtext-b1-2. I read the full diff and
the code around it in `platform.rs`, `events.rs`, `direct_write.rs`, GPUI's
`app.rs` (window close and `QuitMode`), `main.rs`, `startup_failure/` and
`background_accounts/closing.rs`. CI on acb5fa9: `windows-check` passed,
`macos-metal` and `carrier-live-linux` passed, `licenses` **failed**, and
`linux-fmt-clippy-test` and `carrier-live-windows` were still running. I ran
nothing on Windows.

All six findings from the first review are fixed at the root. Three things
block the merge: one new red check, the branch is behind main, and two of the
three "stays open" items are real defects that need fixes.

## Findings from the first review

1. **windows-check red (N87): fixed at the root.** `probe` and
   `loader_installed` in `startup_failure/vulkan.rs` are now
   `#[cfg(target_os = "linux")]`. The pure manifest parsing stays under `test`,
   and `--all-targets` stays. Job 124918040099 on acb5fa9 passed.
2. **Unreachable "no adapter" error (N82): fixed at the root.** `get_adapter`
   now breaks on `DXGI_ERROR_NOT_FOUND`, passes any other `EnumAdapters` error
   up, skips an adapter whose `cast` or `get_device` fails, and returns the last
   error with context. That is the fix the first review asked for.
3. **Caught vsync panic filed as a crash (N84): fixed at the root.** The one
   counter now lives in the std-only `crates/panic-guard`. `thread_guard`
   re-exports it, and the vsync thread runs under `panic_guard::guarded`.
   Cargo.lock shows a single `quill-panic-guard` package used by both the app
   and `gpui-pre-windows`, so there is still one `thread_local`. The PR gives a
   sound reason for not putting it in `gpui-pre`: the `--no-default-features`
   core has no GPUI. The test covers both the hook and the count after.
4. **Silent exit after a lost device: fixed.** On Windows, `QuitMode::Explicit`
   plus `on_window_closed` covers this. GPUI removes the window from
   `cx.windows` before it calls the observers (`app.rs` ~3370 to 3388), so
   `windows().len() == 0` really means "last window". The steps are: close the
   rest, recover once through `request_windowing` and `open_main_window`, then
   give up with the lang-system alert. `report_gpu_gone` is no longer
   Linux-gated, and it runs after `run` returns (`interface_zoom` `run`). The
   decision is a tested pure function.
5. **Stale vsync thread (not blocking before): fixed at the root.** Every
   attach and detach advances a generation. `WM_GPUI_GPU_DEVICE_GONE` carries it
   in `lparam`. Recovered devices go to the platform as a `RecoveredDevices`
   that carries it too. The platform answers `HANDED_OVER` only when it took
   them, and a refused thread stops before it touches the windows. Every
   `WM_GPUI_GPU_DEVICE_LOST` sent to the platform window now points at a
   `RecoveredDevices`. The windows get a separate message with a bare
   `DirectXDevices`. I checked every send site.
6. **`open_window` on a removed device: fixed.** `replace_removed_devices`
   recreates the devices on the UI thread before it builds the window. It
   cannot wait for the vsync thread, because that thread's handover is a
   `SendMessageW` to this thread. The vsync thread's later recovery still
   reaches the new window: the window is in the list before the UI thread pumps
   the handover, and the N85 check covers the rest.
   `DirectWriteTextSystem::handle_gpu_lost` takes a write lock, so the two
   threads cannot race there.

## Blocking

### A. `licenses` fails on this head

Job 124918040063: `error: THIRD_PARTY_LICENSES.md is stale; run
scripts/third-party-licenses.sh`. The new `quill-panic-guard` crate is now one
of the users of a license entry. Fix: run `scripts/third-party-licenses.sh`
and commit the regenerated `THIRD_PARTY_LICENSES.md`. Then confirm that
`linux-fmt-clippy-test` and `carrier-live-windows` finish green.

### B. Branch is 25 commits behind main

origin/main is at 5a173f5 and is not in acb5fa9. rules.md says to merge
origin/main before the last push. `git merge-tree` reports no conflicts. Fix:
merge origin/main (no rebase), rerun the checks, and push.

### C. Open item 1 is a defect: the reopened window can open a TDLib database that is still closing

`open_main_window` builds a new `QuillApp`. The old one is dropped when its
window is destroyed. `QuillApp::drop` runs `close_all_for_quit`, which waits
at most `QUIT_CLOSE_DEADLINE` (1.5 s). Clients still closing after that stay in
the old app's `background.closing` set, and that set is dropped along with the
app. The new app's `open_wait` / `closed` therefore sees an empty set, returns
`Go`, and opens the same account database while the old client may still hold
it. `closing.rs` exists to prevent exactly this ("a new client never opens a
database that is still closing"). A driver reset is the moment a slow close is
most likely. The new window also loses the open chat and all UI state, and a
launch link that was not yet consumed is dropped (`pending_deep_link: None`).

Fix at the root: keep the `QuillApp` entity across the loss. Hold a strong
`Entity<QuillApp>` in the recovery state, and have `open_main_window` take an
optional existing entity. On reopen, wrap that entity in the new window's
`QuillShell` / `Root`, and redo only the per-window installs: crash notice,
tray, link inbox, the should-close handler and the `apply_taskbar_icon` /
`set_native_window` calls. `QuillApp::new` takes `window` only for setup, so
that setup has to move into a `rebind_window(window, cx)` step. Once the entity
survives, no client closes or reopens, and the pending deep link is still in
`app.pending_deep_link`.

If that is too large for this PR, the minimum fix: move the closing set out of
`QuillApp` into one main-thread registry per process (a `thread_local` next to
`early_connect::FIRST`), so a new app's `open_wait` sees the clients the old
one is still closing. Also carry the old app's still-unconsumed
`pending_deep_link` into the relaunch instead of `None`. Test: the
`open_wait` decision returns `Wait` for a key that a dropped app left closing.

### D. Open item 2 is a defect: a failed main-window renderer leaves Quill running with only the PiP

When `events.rs` `handle_device_lost` gives up on a window's renderer, it posts
`WM_GPUI_DESTROY_AFTER_DEVICE_LOST`. If that window is the main window and a
PiP is open, `after_window_closed(1, false, _)` returns `Nothing`. The device
still works, so `graphical_environment()` is `Some`. The user is left with a
video and no way back to their chats. Fix: record the main window's `WindowId`
in `open_main_window` and pass it to `recover_after_gpu_loss`. When that window
closes and Quill did not start the close, treat it like a lost GPU: reopen the
main window once (with the entity kept as in C), and leave the PiP open.
"Quill started it" is the `close_main_window` path. Set a flag there when it
returns `true` with `CloseOutcome::Quit`. On Windows, that path should also call
`cx.quit()` as it already does on macOS, so a user's quit with a PiP open
really quits, the way Telegram Desktop does. Add a `main_window_closed` input
to the pure `after_window_closed` and a test row for (PiP left, main closed
without a quit, GPU works) → `Recover`.

## Open item 3 is a defect, small fix

`WindowList::snapshot` allocates a `Vec` on every vsync tick and every gate
wake. The old code did not. "Likely noise" was not measured. Fix: return
`SmallVec<[T; 4]>`. `smallvec` is already a dependency of the fork, and this
gives back the old no-heap path for up to 4 windows with no other change.

## What I could not check

Nothing ran on Windows: no forced TDR, no vsync panic, no reopen after a loss,
no `replace_removed_devices` path. The `msvc` build of the head comes from the
passing `windows-check` job only. I did not run the test suites. This pass
read code and CI.
