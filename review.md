CHANGES

# Review of PR 1313 (claude/sweep-b14) against main, bundle B14

Reviewed at PR head c8fe13f. Merge base 948ad90; it merges cleanly with main 5a173f5. Briefs read: sweep-briefs/rules.md and sweep-briefs/bundles/B14.md on claude/sweep-briefs, plus the PR text on claude/prtext-b14.

The bundle is mostly fixed well. The writer, copy map, copy callback, audio test, paste and document-album changes are small and mostly hit the cause. Five things block approval:

1. The writer change brings back a deleted account's folder (a new bug).
2. robustness-8 is not fixed on every surface.
3. The quit flush has no deadline.
4. Received file albums are not grouped.
5. L06 does not split a mixed tray the way tdesktop does.

The PR text lists 3, 4 and the window-title gap as "stays open". The owner's rule is that 100% of defects get fixed, so those count as defects. Each one has a fix named below.

## Blocking findings

### B1. A failed write that stays queued brings a deleted account's folder back (new in this PR, N55)

`delete_account_folder` (`src/settings/accounts/deletion.rs:54-56`) calls `flush_deferred_writes()` first, because "a queued settings write would recreate the folder after it is gone". With this PR, a write that fails during that flush stays in `pending` with `failed: true`. The folder is then removed. On the next `write_json_deferred` for any path, `submit` calls `retry_failed()` (`deferred.rs`). The writer then runs `write_bytes_atomic`, whose `create_dir_all(parent)` (`settings/mod.rs:192`) recreates `accounts/<key>/` with the stale file inside. Until that happens, `read_with_pending` also keeps returning the deleted account's bytes.

Trigger: the disk fills, or one file in the account folder cannot be written. The user removes the account, which frees space. The next settings change in any account puts the removed account's folder back.

Proof: a test added in a scratch worktree, not pushed, is at the end of this file. It puts a regular file where the write needs a directory, so `create_dir_all` fails, then deletes the account and saves one other setting. Result: it fails on the PR head. The folder comes back, and the stale bytes can still be read.

Fix: give `DeferredWriter` a `discard_under(&Path)` that removes every pending entry under a folder. Call it in `delete_account_folder` after the flush and before `remove_dir_all`. Turn the test above into a regression test. Stop on the same lock window, too: a write in flight for that folder must not land after the discard. The writer's `ptr_eq` check after the sink already drops a stale entry, but the file is already on disk by then. So the discard should also wait for `writer_running` to clear, the way `flush` does.

### B2. robustness-8: many shaped text paths still get raw names

The bundle's default is "Filter every text surface", and the Fix line says "run the same filter on every shaped text path". The PR filters per surface. It covers the chat row, the preview line, bubble sender names, reply strips, `one_line_plain`, `emoji_sprites::one_line` and notifications. Many name surfaces still shape raw text, for example:

- `src/ui/bubble_header.rs:236`: the "Forwarded from" name on every forwarded message.
- `src/ui/group_call_ui.rs:234`, `src/ui/group_call_tiles.rs:383,496`: participant names in calls, including the video tiles.
- `src/ui/group_admin_panels/invite_links_section.rs:174,395,580`, `src/ui/invite_admin_more_ui.rs:141`, `src/ui/pull_next_ui.rs:551`: member and creator names.
- OS window titles: `set_window_title` gets only `window_title::single_line`. The PR text keeps this open because "the OS shapes those". But the reason tdesktop drops tag characters is that they crash some Linux text stacks, and that includes the window manager drawing the title.

Root fix: filter where names enter the state, not at each place they are drawn. Apply `display_text::filtered` once when a user's first and last name, a chat title and a forward origin name are parsed or built: `telegram/envelope/users.rs:178` `display_name`, the chat title parse, and `state/session_reply.rs:105`. Then every current and future surface gets it. Also apply it in `window_title::single_line`. The per-surface calls this PR adds for names can then go. Keep the per-run filtering for message previews and message text, since those carry entities and copy maps. Test: one fixture user with a Zalgo name and a tag sequence, asserted through `display_name()` and the forward header.

### B3. The quit flush now has no deadline (N60)

`flush_settings_on_quit` (`src/ui/navigation.rs:563-569`) calls `flush_deferred_writes()` synchronously inside the `on_app_quit` callback, on the main thread, while the windows are still up. `DeferredWriter::flush` now waits on the condvar with no bound. If the filesystem stops answering (a network home folder that dropped, a failing USB disk, an `fsync` stuck in the kernel), quit never finishes. The window freezes and the user has to force-quit, which loses the queue anyway. That is the outcome the old 5 s deadline was there to prevent, so this PR swaps one defect for another. `delete_account_folder` also calls the same unbounded flush.

Fix: bound how long the flush waits without progress, not how long it waits in total. Wait while writes keep finishing. Give up only after a stall budget (say 10 s) passes with no write completing, and reset it after each completed write. A slow disk then still writes the whole queue (N60's point), and a hung one cannot hold quit forever. Take the budget as a parameter so a test can pass `Duration::ZERO` with a held gate. The test then shows that flush returns while the write is held and that the bytes are still pending, with no sleep. Optionally hide the windows before flushing, so a long flush is not a frozen window. I have not checked what tdesktop does on quit. As far as I know it writes settings synchronously when they change, so it has no queue to flush at quit and offers no model here.

### B4. Received file albums show as separate messages (L06, now produced by Quill itself)

`album::is_album_media` (`src/album.rs:36`) accepts only photos and videos, so `group_media_albums` (`ui/conversation/row_build.rs:97`) leaves a file album as N separate bubbles. This PR now sends file albums, so Quill makes them itself, and the sender sees each file in its own bubble. Other code already treats them as one group: `forward_drag_source.rs:62-72` drags the whole album when you drag one file, and the edit and menu code asks `in_album(media_album_id != 0)`. So one file's bubble acts on files the user does not see as grouped with it. tdesktop draws a file album as one bubble with the files stacked (`HistoryView::GroupedMedia` with documents), caption under the last one.

Fix: add a file-album variant to `HistoryGroup`, grouping consecutive `MessageContent::Document` items (and audio, as its own kind) that share a `media_album_id`. Render it as one bubble that stacks the existing file rows, with chrome on the group as photo albums have, and selection and delete on the group. Add a rows test for a received two-file album and a demo capture.

### B5. L06: a mixed tray goes out one message per file, not split by type as tdesktop does

`ComposerAttachment::albumable` (`composer/attachments.rs`) only allows a tray that is all media or all files. Any other tray takes the "one per message" path in `ui/composer/mod.rs:206-220`. A mixed tray is easy to make: two photos, then drop a `.txt` (`append_dropped_files` adds it as a Document next to the photos). tdesktop's `DivideByGroups` (`attach_prepare.cpp`, the lines the bundle cites) starts a new group whenever the album type changes (PhotoVideo, File, Music) or a group reaches 10. So it sends a photo album and then the file. Quill sends three messages.

Fix: make `split_for_send` walk the tray in order, start a new group when the album type changes or a group reaches `ALBUM_MAX_ITEMS`, and send groups of one as singles. Then remove the all-or-nothing `albumable` gate from both send paths. Test: `[photo, photo, txt]` with grouping on gives `[[photo, photo], [txt]]`.

Related, plausible but not checked against a live TDLib: album items are built with `disable_content_type_detection: false` (`telegram/requests/media.rs:360`). TDLib allows only one content type per album ("Documents and audio files can be only grouped in an album with messages of the same type"). tdesktop keeps Music as its own album type for this reason. A tray of `a.mp3` and `b.txt` may be refused or split by TDLib. Treat audio files as their own album type in the split above. That also fixes this.

## Non-blocking findings

### N1. N62: closing the editor when the Send goes out drops the user's edits without a word

`composer_sends.rs:409-416` closes an editor open on the sent slot. The test opens the second card's editor after Send by assigning `viewer.photo_editor` directly. In the app, `open_photo_editor` (`ui/photo_editor.rs:360-381`) does let a tap on a card that is not rendering open an editor while a Send is held. The user can then rotate and crop, and the edits vanish with no note when the first render lands. N63's test makes the other order consistent: a Send cancels a tap. Fix: while a Send is held for the open slot, refuse the tap, with a note that the photos are sending. The close in `finish` then stays as the guard for an editor that was already open, and no edit is lost silently.

### N2. N57: reorder

The PR shows that the tray has no control that moves a card. I found no drag or move handler on tray cards either, so "reorder" is only the shift after a removal, which the new test drives. This is not a defect.

### N3. N60: the bin crate's tests still share one writer

`global()` is per thread only under `cfg(test)` of the lib. The UI tests are in the bin crate and link the lib without `cfg(test)`, so they use the one process-wide writer. They have no gates, so nothing is held, and I found no shared-state failure. Note it in the PR text. Not a defect today.

### N4. The paste of a picture over files becomes a file

N64's fix sends a picture pasted over a document as an uncompressed file, the way the existing drop rule does. tdesktop keeps the photo a photo and gives it its own group (the split in B5). With B5 fixed, `append_dropped_files` could keep a pasted or dropped picture as media in a mixed tray. That is a product choice tied to B5, not a separate bug.

## Item by item

| Id | Verdict | Notes |
| --- | --- | --- |
| robustness-8 | Not done | B2: forward names, call names, member lists, window titles. Fix at the name boundary. |
| N55 | Fixed, but adds B1 | A failed write stays queued and readable; the test fails without the fix. It needs the discard on account delete. |
| N56 | Fixed | One note from the writer. A serialization error goes to the same note. Every caller compiles against the new `()` return. |
| N57 | Fixed | 13 cards, remove 1st and 11th, send: 10 plus 1 in order. It also fixed the demo album JSON brace and the over-10 album. There is no reorder control (N2). |
| N58 | Fixed | `leading && at == 0` in `source_of`. Contiguous ranges cannot count a dropped run twice, since only a range starting at 0 takes `at == 0`. |
| N59 | Fixed | The copy string is built in the callback. Per paint only a SharedString clone, a move of the map and the ranges `to_vec` remain. |
| N60 | Half | Tests: per-thread writer, no timeouts. Quit: the deadline is gone with no bound in its place (B3). |
| N61 | Fixed | The test now asserts `secs <= played` and `played - secs <= block`. No engine change was needed. |
| N62 | Fixed, with N1 | The editor closes when its slot's Send finishes. Edits are lost silently (N1). |
| N63 | Fixed | Two UI tests, both shown failing without the fix per the PR text. |
| N64 | Fixed | The album-full branch and string are gone, and the document is kept. |
| L06 | Half | Albums of 10 for all-file trays and the Group option apply. Missing: split by type (B5) and showing received file albums (B4). |

## What I ran

- `git merge-tree` against origin/main: clean.
- The B1 repro test, with the lib tests for `settings::deferred`, `settings::accounts::deletion` and `display_text`, in a scratch worktree (nothing pushed to the PR branch):

  `cargo test --lib -- settings::accounts::deletion settings::deferred display_text`: 24 pass, 1 fails, and the one that fails is the B1 repro. It prints `REVIEW stale_read=true folder_back=true`: after the delete, the deleted account's bytes can still be read, and the next save elsewhere recreates its folder. Every PR test in those modules passes.

## What I did not run

- No full `cargo test`, no clippy, no macOS or Windows build. The UI tests (bin crate) were read, not run.
- B4 and N1 come from reading the code; I did not drive them in the app.
- B5's TDLib audio-in-document-album point needs a live TDLib.

## The B1 repro test

```rust
#[test]
fn review_a_failed_queued_write_brings_the_deleted_folder_back() {
    let root = tmp_root("review-failed");
    let work = add_account(&root, "Work").unwrap();
    let paths = AccountPaths::for_root(&root, &work);
    std::fs::create_dir_all(&paths.root).unwrap();
    // A regular file where the write wants a folder: create_dir_all fails.
    std::fs::write(paths.root.join("sub"), b"x").unwrap();
    let prefs = paths.root.join("sub").join("prefs.json");
    crate::settings::write_json_deferred(&prefs, &["#queued"]);
    delete_account_folder(&root, &work).unwrap();
    assert!(!paths.root.exists(), "deleted");
    let stale = crate::settings::read_with_pending(&prefs).is_ok();
    // Any later settings save, anywhere, retries the failed write.
    crate::settings::write_json_deferred(&root.join("other.json"), &1);
    crate::settings::flush_deferred_writes();
    eprintln!("REVIEW stale_read={stale} folder_back={}", paths.root.exists());
    assert!(!stale && !paths.root.exists(), "the deleted account's folder came back");
    let _ = std::fs::remove_dir_all(&root);
}
```
