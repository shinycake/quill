# Handoff: video player overhaul and other open work (2026-10-15)

Written by the cloud session that took over the owner's local threads
after they hit their usage limit. Any agent (local Mac, cloud container,
Windows) can continue from here. Read this file first, then the design.

The owner (Idan) asked for the **video player overhaul** to be finished
first; everything else is secondary. The owner gave standing permission
to review PRs carefully and merge them on GitHub once green and reviewed.

## Where things are

- Designs and rules (copied here because `quill-tools` lives only on the
  owner's Mac, not on GitHub):
  - `docs/handoff/quill-tools/designs/video-player.md`: the full
    overhaul design, including the 6-PR build plan. **Read it all.**
  - `docs/handoff/quill-tools/rules.txt`: the worker rules every brief
    points to (tdesktop fidelity, kit components first, never crash,
    tests, decision doc, parity fragments). The Mac-only parts (SSD,
    `ssd-target.sh`, `/Volumes`, `queue.sh`, hearthbot) don't apply in
    the cloud.
  - `docs/handoff/quill-tools/gate.sh`: the owner's local gate (Mac).
  - `call-media-capture.md`, `group-call-streams.md`: designs for the
    non-video open work below.
- Decision docs of the merged overhaul PRs: `docs/decisions/video-player-core.md`
  (PR 1), `video-player-controls.md` (PR 2), `video-player-inline.md` (PR 3).
- tdesktop reference: `git clone --depth 1 --filter=blob:none --sparse
  https://github.com/telegramdesktop/tdesktop.git` then
  `git sparse-checkout set Telegram/SourceFiles/media Telegram/SourceFiles/calls`
  (add other paths as needed). On the owner's Mac it is at
  `~/Developer/Reference/tdesktop`.

## Video player overhaul: status

| PR | What | State |
|---|---|---|
| 1 | Core player, macOS pacing, viewer | merged (#804) |
| 2 | Controller and scrubber | merged (#817) |
| — | Call tile atlas leak fix | merged (#828) |
| 3 | Inline tiles, round videos, GIF panel, stories | merged (#830) |
| 4 | Picture-in-Picture on the shared player, all platforms | **open: shinycake/quill#849**, branch `video-pip` — fixing its review (below) |
| 5 | Legacy removal | not started |
| 6 | Windows NV12 surfaces | not started |

### PR 4 (#849): what is left

The PR is open with 4 commits (head 68878295 at the time of writing; a
builder was adding fix commits on top — check `git log origin/video-pip`).
Its independent review said **do not merge** until these are fixed. If
the branch does not already have fix commits for them, do them:

Blocking:
- **B1** PiP closes when its message leaves the *loaded* history window
  (`src/ui/media_viewer/pip.rs` checks `history.messages`, which
  `trim_window_end` and `reset_window` shrink on paging/jumps). Close only
  on a real deletion (`history.is_tombstone(id)`), like tdesktop's
  `itemRemoved`. `pip_enlarged` must reopen the viewer by chat+message even
  when the message isn't loaded, never silently dropping the player.
- **B2** Close on lock / account logout / deletion only runs in the root
  render, which doesn't run while the main window is minimized, occluded or
  hidden to the tray. Call `close_video_pip` from `lock_by_passcode`
  (`src/passcode/mod.rs`, next to the notification-window close) and from
  the account logout path. UI tests for close-on-lock/delete/account.

Should-fix:
- **S1 (Windows)** `controls/pip/place.rs` passes the client rect to
  `SetWindowPos` as the outer rect; with the hidden titlebar the frame is
  not zero, so every move/resize shrinks the window. Inflate by the
  `GetWindowRect`/`GetClientRect` delta like GPUI's `calculate_window_rect`.
- **S2 (macOS)** Floating windows get no `NSTrackingArea` in GPUI-mac
  (`third_party/gpui-pre-macos/src/window.rs` ~1245), so the unfocused PiP
  gets no hover: no control fade, no resize cursors, no mouse-exit. Add a
  tracking area (ActiveAlways | MouseMoved | EnteredAndExited |
  InVisibleRect).
- **S3** Handoff keyed by message id only: albums reopen the wrong item.
  Carry the item offset and file id.
- **S4** One sound at a time: a story with sound, the recording preview
  and resuming a player-bar track must close PiP.
- **S5** Close only on that account's own logout, not on account switch.
- **S6** Keep `parity:media-video-pip` in the fragment only if fully done.
- Nits: seeking an ended clip plays it; screen-reader adjust on the lines;
  min size follows the real aspect; X11 aspect normalize; drag start
  geometry at drag start; saved position across all screens; serialize
  `save_position` writes; Windows taskbar tool-window style; opening the
  same clip from the chat continues the PiP player instead of restarting.

Merge checklist for #849: Linux CI green, **and** the Windows and macOS
packaging builds green on the same commit (see "Platform CI" below),
independent re-review clean. Real hover/drag/always-on-top on macOS and
Windows can only be tried on a real machine; say so in the PR.

### PR 5: legacy removal (next)

Design "Migration plan" and build plan item 5: delete `src/video/mod.rs`
(ffmpeg-binary PNG extraction), `src/ui/inline_playback.rs`, the video
paths of `src/ui/playback_state.rs` (`playing_video` /
`playing_animation`), `animation.rs` video frame cycling; move the
editor (`video_editor_tools.rs`), premium promo (`premium_ui/promo.rs`) and
premium preview (`premium_preview_state.rs`) onto `VideoPlayer`
(`Purpose::Editor` / `Purpose::Inline`); re-base the demos `ready-video`,
`ready-video-note`, `ready-video-pip` on the player with the
`QUILL_DEMO_CAPTURE` first-frame freeze (`ready-video-note` still runs the
legacy `spawn_video_tick`); then delete `src/ui/native_video.rs` and
`src/ui/ffmpeg_video.rs`. `image/gif` documents play through the GIF
decoder used for stickers. Also fold in this follow-up from PR 3's review:
rows rendered just outside the visible area reopen their decoder every
2 s after release (`inline_players.rs` open/release) — avoid the churn
(e.g. don't open a player for a row whose picture was never painted in
view, or keep released keys cooled down until painted). Decision doc:
`docs/decisions/video-player-legacy-removal.md`, naming the superseded
decision-doc paragraphs (`codex-native-video`, `codex-inline-video`,
`codex-video-cross-platform`, `codex-parity-macos-pip`,
`codex-viewer-controls`, `codex-viewer-video-extras`).

After PR 5 the language-packs thread can finish: it left the media
viewer, video player and group-call strings untranslated until "the video
paths settle" (see that thread's handoff in the owner's memory, and
`scripts/lang-baseline.txt`).

### PR 6: Windows NV12 surfaces

Design "Platform notes → Windows" and build plan item 6:
`SurfaceFormat::Nv12` in `third_party/gpui-pre` (`SurfaceFrame::new`
validates `w*h*3/2`), the FFmpeg shim's `quillvideo_set_output(NV12)` to
skip `sws_scale` for yuv420p/nv12, and a `yuv_surface` pipeline in the
DirectX renderer next to `hdr_surfaces.rs` (R8 luma + R8G8 chroma,
BT.601/709 from the stream colorspace, full/limited range). Must be
measured before/after on a real Windows machine (the design's trace
lines); it cannot be verified from a Linux container. Linux stays on BGRA
`RenderImage` (documented).

## Working in a cloud container (what this session learned)

- Gate (CI's Linux job; run from the checkout root):
  ```
  bash scripts/check-app-name.sh && bash scripts/check-file-size.sh && python3 scripts/check-lang.py && bash scripts/check-hotspots.sh origin/main && cargo fmt --all -- --check && ! grep -rn 'cfg(all(test' src crates --include='*.rs' && cargo clippy --features demo-capture --all-targets --locked -p quill -p ntgcalls-sys -p quill-webview-protocol -- -D warnings && cargo test --features demo-capture --locked --bin quill && cargo clippy --no-default-features --all-targets --locked -- -D warnings && cargo test --no-default-features --locked
  ```
  `settings::accounts::...::atomic_write_failure_keeps_old_file` fails
  only when running as root (containers); ignore that one.
- **Platform CI:** PRs only run the Linux job. Windows and macOS packaging
  builds run on pushes to `main` and to `cursor/**`. To check macOS/Windows
  code before merge, push the same commit to `cursor/<branch>` and watch
  the `ci` workflow run for that branch (jobs `windows`, `macos-package`).
  Never dispatch `quill-ui-build.yml` (it targets the owner's own Mac).
- Disk: a debug target is ~11 GB and the incremental cache grows fast.
  Share one `CARGO_TARGET_DIR` between worktrees and prune old sessions in
  `target/debug/incremental/*/s-*` (keep the newest per crate).
- Force-push and remote branch deletion were refused in this environment:
  add commits instead of rebasing a pushed branch (merge main in if
  needed); leave merged branches for the owner to delete.
- UI tests that scroll must not assume one platform's text metrics (PR 3's
  Linux-only failure); step the scroll until the condition holds.
- Merge with a merge commit (the owner's `mq.sh` does
  `gh pr merge --merge --delete-branch`), then apply pending parity
  fragments to the README the way `parity-apply.sh` does
  (`bash scripts/apply-parity-fragment.sh parity-fragments/*.txt` on main,
  commit "Parity: check off merged items", push to main).

## Other open work (secondary)

- **shinycake/quill#837** "Calls: Stream with other apps box" (live
  streams PR 4 of 4). Two review rounds fixed; branch `rtmp-stream-box`
  head 0b5a26cf; a re-review was running. Merge once the re-review is
  clean and CI is green. Its "Start as…" join-as box has no screenshot yet.
- **Linux screen sharing for calls** (call capture PR 4 of 4, design
  `call-media-capture.md`): unfinished WIP saved on branch
  `wip/call-capture-screen-linux` (not compiled as a whole). Remaining:
  chooser module doc, demo backend option + GPUI test, decision doc, CI
  apt deps (libpipewire/libspa), THIRD_PARTY_LICENSES regeneration, gate.
  Open it as a normal branch/PR once finished.
- **Live story viewer** (live streams PR 3 of 4, design
  `group-call-streams.md`): was being built on the owner's Mac when the
  limit hit; never pushed. Check the Mac worktrees before rebuilding.
- **Language packs**: waits on video PR 5 (above).
