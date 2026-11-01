File albums show as one bubble, mixed trays split by type, and a deleted account's settings stay deleted

Follow-up to PR 1313 (bundle B14), fixing every finding in the review on claude/review-b14. Branch claude/fix-b14 starts from sweep-all, with one commit per finding.

## What was wrong and what changed

- B1. A settings write that failed stayed queued. When an account was deleted, the next save anywhere retried that write and recreated the deleted account's folder, and reads still returned its bytes. Deleting the folder now drops every write queued under it and waits for a write already on its way there. The writer also checks that an entry is still queued before writing it, so a batch taken earlier cannot write a discarded entry. The review's repro is now a regression test, and it fails without the fix.
- B3. The quit flush waited with no bound, so a hung disk froze quit. The flush, and the wait during account deletion, now give up after 10 s in which no write finishes. Each finished write starts the 10 s again, so a slow disk still writes the whole queue. A deletion whose write is stuck fails and stays recorded, and the next start tries it again. The test passes a zero budget with a held gate and shows that flush returns while the bytes stay queued. No sleep is involved.
- B2. Names are now filtered once, where they are parsed: user first and last names, chat titles (new chat and title updates), forward origin names and signatures, message author signatures, import sender names and saved-topic sender names. Every surface gets the same text, including forward headers, call tiles, member lists and invite links. The window title also runs the filter, since tag characters crash some Linux window managers. The per-surface name filters from PR 1313 are removed. Previews and message text keep their per-run filtering.
- B5. The tray is now split in order, as tdesktop GroupRanges does. A new message starts where the album type changes (photos and videos, files, audio files) or when an album reaches 10. A video turned into a GIF and a video note go alone. The all-or-nothing `albumable` gate is gone from both send paths. `[photo, photo, txt]` sends a photo album and then the file. Audio files form their own albums, which also covers TDLib's rule against mixing audio with other files.
- B4. Consecutive files, or audio files, that share a media_album_id now form one album row. Their file rows stack in one bubble with the caption under the last one, as tdesktop GroupedMedia does with documents. Selection, delete and dragging act on the group, as they already did for photo albums. A UI test receives a two-file album and checks that it builds one row and paints both files in order inside one bubble. A `ready-file-album` screenshot demo is added.
- N1. A tap on a tray card while a Send waits for a render now opens nothing and shows "The photos are being sent." (new lang key `qll_ops_photos_are_sending`). Before, the editor opened and its edits were dropped without a note when the Send went out. The close in `finish_pending_send` stays as the guard for an editor that was already open.
- N4. With B5 in place, a dropped or pasted photo or video stays media next to files and goes out in its own album, as in tdesktop. A tray switched to "send without compression" still takes it as a file. The N64 paste test now expects a photo beside the document.
- Not a review finding: sweep-all's merge had dropped the `Warmed` import that the decoder_load tests use, so the bin tests did not compile with `--features demo-capture`. One commit puts it back.

## Not defects

- N2: the tray has no control that reorders cards, so reorder is only the shift after a removal, and PR 1313 already tests that.
- N3: the bin crate's UI tests share the process-wide settings writer, since `global()` is per thread only under the lib's `cfg(test)`. Those tests hold no gates, so nothing waits on a held write. No shared-state failure was found.

## What ran

- cargo fmt.
- cargo clippy with -D warnings for `--features demo-capture --all-targets` and for `--no-default-features --all-targets`: clean.
- scripts/check-hotspots.sh: OK.
- `cargo test --features demo-capture`. The lib passes in full: 6973 passed, 0 failed. The bin crate had 2530 passed and 2 failed:
  - `a_sticker_tick_renders_the_layers_alone` also fails on sweep-all with only the import fix applied, so it is not caused by this branch.
  - `the_chat_behind_the_viewer_holds_still_until_it_closes` passed on sweep-all and was flaky here: 2 of 3 reruns passed. When it fails, the sticker animation layer draws no frames at all, which is the same symptom as the first test. It does not touch albums, names, the tray or settings.
- New tests, each shown failing without its fix: the account-deletion regression and the writer discard and zero-budget tests (B1, B3), the parse and window-title filter tests (B2), the mixed-tray split test (B5), the file-album row and paint test (B4), and the held-Send tap test (N1).

## What did not run

- The demo capture. Under Xvfb the app exits right after it writes its ready marker, for `ready-file-album` and for the existing `ready-chats` alike, so no screenshot was taken here. A Mac run of `--screenshot-demo ready-file-album` should check how the bubble looks.
- No macOS or Windows build. No code here is platform-specific. The Windows target is not installed in this container.
- No live TDLib. Sending audio files as their own album follows tdesktop and the TDLib error text quoted in the review. Whether TDLib refuses a mixed audio and document album was not tried.
- B3 has no number to measure. The 10 s stall budget is a choice, as the review suggested.

## What stays open

- The tdesktop-style per-file caption in a file album is drawn once, under the last file, as the review asked. tdesktop draws a caption under each file that has one. That can follow if wanted.
- Hiding the windows before the quit flush (optional in the review) was not done. With the bound in place, the flush can hold quit for at most 10 s after the last finished write.
