CHANGES

# Review of PR 1309 (`claude/sweep-b9`, bundle B9) against main

Reviewed at 2641b8c, against main at 948ad90. I read the rules in `sweep-briefs/rules.md` and the bundle in `sweep-briefs/bundles/B9.md`. I did not touch the PR branch.

## What I ran

- `cargo test --features demo-capture --bin quill` on the PR's 11 new and touched tests (render_decode, app_theme ui_tests, wake_wait, photo_editor_set, composer_keystroke, sticker_set_box, first_run_state). All 11 pass.
- A probe test I wrote and did not commit (finding 1). It fails on the PR branch, which confirms the defect.
- I did not build for Windows or macOS.

## Blocking

### 1. Adapt from a Sticker opens the photo editor after the set box is closed (confirmed)

`src/ui/sticker_set_box/creator.rs`, `pick_existing_sticker`, Webp/Unknown arm. The decode now runs through `run_off_ui_scoped_then`. That helper drops the result only when the window closes. `edit_pixels_for_set` then sets `viewer.photo_editor` and does not check that the set box is still open, or that this pick is still the one the user wants.

- Probe: pick a WebP sticker with `adapt: true`, call `close_sticker_set_dialog` in the same update, then call `frames`. Result: `viewer.photo_editor.is_some() == true`. The editor pops up for a set box the user dismissed, and Done then uploads to that set.
- The same gap lets an older pick win. If the user picks sticker A, then picks sticker B before A finishes decoding, whichever decode lands last replaces the editor. For example, a large A can land after B.
- Fix: do what the video path already does with `PendingVideo`. Record a pending still pick in `StickerSetBoxUi`, for example `pending_still: Option<(PickExisting, FileId)>`, when the decode starts. `sticker_box.reset()` (called by `close_sticker_set_dialog`) clears it, and a newer pick replaces it. On landing, open the editor only if the pending pick still matches `(pick, file_id)` and the open set's `set_id` is still `pick.set_id`, then clear it. Add the probe above as a regression test: close before the land means no editor; pick A then B with A landing last means the editor shows B.

### 2. Editing a custom theme opens the editor late, over whatever happened meanwhile

`src/ui/app_theme/editor.rs`, `open_theme_editor`, `EditorSource::Custom`. The load runs in a detached `spawn_in(window)`, and its result calls `show_theme_editor` with no checks. `this` is `QuillApp`, which outlives the chooser, and the editor dialog appears whenever `app_theme.editor.is_some()`. So a slow read (a large background picture, a slow disk) can land after any of these:

- the user closed Settings, and the editor dialog pops up on its own;
- the user clicked Create new theme, and the late read overwrites that editor, losing its unsaved edits;
- the user deleted the theme (`delete_custom_theme` calls `refresh_custom_themes`, which does not stop this task), and the editor opens on a theme that no longer exists, so Save brings it back;
- the user double-clicked Edit, which runs two reads and builds two editors.

Fix: store the read as `editor_read: Option<(String, Task<()>)>` in `AppThemeUi` instead of detaching it. Drop it in `refresh_custom_themes`, in `close_theme_editor`, when a `Current` editor opens, and when Settings closes. Ignore a second Edit while a read for the same name is running. On landing, also require that `editor` is still `None`. Test it on the demo app with a missing theme name: open the editor for Custom, then open Current before `run_until_parked`. After parking, the Current editor is still there and no load error note appeared.

### 3. Stays open: ui-thread-4 has no Windows timing

The PR's "What stays open" lists the bundle's own proof plan ("time `open_popup` with 20 toasts on a Windows runner, before and after") as not done. The shader cache is type-checked only. Its device-loss path has never run. Under the rules, that makes it a defect. To close it:

1. Run the bundle's `cursor/` Windows job, or any Windows machine, on two builds. Before: main with only the `trace_open` hunk of `notify_windows/mod.rs` cherry-picked, since main has no trace. After: this branch. Use `QUILL_TRACE_TOASTS=1` on both, trigger 20 toasts (a demo or a test account), and paste the 20 `toast window opened in` lines from each build into the PR text. Use release builds. A debug build adds `D3DCompileFromFile` and hides what release users see.
2. Device loss: run once with `dxcap -forcetdr`, or remove the device through the debug layer, while a toast is up. Then confirm that the next toast opens and draws. This exercises the `shared.device != *device` reset.
3. If toasts after the first still take more than about 10 ms, the swap chain and DirectComposition dominate. In that case build the hidden-popup pool the brief describes, in this PR, as its last step says. Do not leave it as a follow-up: the owner's rule is "hold nothing for later".

## Should fix

### 4. With custom day and night themes, the wrong theme flashes at each switch

`app_theme/reads.rs`, `custom_theme_palette`. There is still one cache slot (`custom_cache`). Suppose the day theme is custom A and the night theme is custom B. Each auto-night switch misses the cache and returns `Reading(applied)`, which shows the previous mode's palette (dark B during the day, or the reverse) until the off-thread read lands, then applies again. Before this PR the read was synchronous, so nothing flashed. Fix: key the cache by name with room for both themes, for example a 2-entry map that `refresh_custom_themes` clears. Then switching between two themes already read is a cache hit. Test it by flipping `night` twice with two custom themes and checking that `applied` never shows the other mode's palette.

### 5. A failed blurred preview is remembered per `row_id` and never retried

`message_media/surfaces.rs`. `OffRender` stores `None` under `row_id` alone. If a message's minithumbnail changes (an edit that replaces the spoiler media) after a failed or earlier decode, the old result stays until LRU eviction. The success case already had this problem on main, but this PR makes failures sticky as well. Fix: key by `(row_id, hash of mini.data)`. While you are there, `Arc::from(mini.data.as_slice())` copies the bytes on every render of every spoiler row, even on a cache hit. Build the `Arc` once in the envelope, or build it only on the `Start` branch, so render does not allocate.

## Non-blocking notes

- Replay intro while an earlier intro write is running: two `intro_file` writes race on one target. On Windows, the second rename can fail while the first player holds the file open. Whichever write lands first is applied. If the failed one lands first, the notice shows with no clip. This is rare. A fix is to keep the write `Task` in `FirstRun::Welcome` and reuse it instead of starting another.
- Theme import now takes `taken` from `store::list` instead of the cached readable themes. That is correct, and it is better than before.
- subsystems-12: `run_chrome_loop` and the macOS `due = Some(1s)` mapping match the old loop. The first poke after a rest still syncs at once (`gap_left`). The macOS branch is not compiled here, so a Mac `cargo clippy --all-targets -D warnings` must pass before merge.
- N94, N95 (reply pattern), N98, ui-thread-5 rest: no defects found.
- `shared_shaders`: `ShaderModule` is `Copy + Eq` and the comparison is by device pointer, so this is correct. Windows on two adapters would make the cache thrash, which costs speed but does not break anything.

## Verdict

CHANGES. Findings 1 and 2 are races where off-thread work returns to a closed view or overwrites a newer state. Finding 3 is the open Windows timing the bundle requires. Fix 4 and 5 in the same pass.
