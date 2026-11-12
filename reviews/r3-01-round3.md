# Re-review of r3-01 second fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-01.

Commit checked: d83dcd299. Where each round-2 bullet stands:

- 03-002, delete refetch raced the delete: resolved. `delete_folder_invite_link` no longer refetches at send time. It only outdates a loading list (`src/connect/folder_links.rs:200`), so the in-flight answer is dropped. `refetch_outdated_folder_links` returns early while a `DeleteChatFolderInviteLink` for the folder is in `requests` (`src/connect/folder_links.rs:209-216`). The refetch goes out from the post-ingest hook `refetch_open_folder_links` (`src/ui/notifications/mod.rs:510`, `src/ui/folder_share/mod.rs:422-428`) on the first poll after the delete's answer has removed that request. The same guard also holds back a create/edit refetch while a delete is out. A refused delete fails the list with Retry (`src/state/domains/chat_list/error.rs:71-81`); the list is then not `Outdated`, so nothing refetches by itself. The test answers the delete before it expects the second list request (`src/connect/tests/sweep_r3_01.rs:62-85`).
- Stale baseline line: resolved. `unsent:apply_ready_stickers` is gone from `scripts/request-match-baseline.txt`, and `check-request-match.py` reports ok with no prunable lines.

New-defect pass on d83dcd299: no new flags, no new matching outside RequestId, no new unwrap/expect/indexing in non-test code, no new baseline or allow-list entries (one was removed). Every `scripts/check-*.py` guard passes on the tip, along with `check-file-size.sh` and `test_shape_guard.py`. (`check-bundle-dlopen.py` needs a bundle argument and is not a source guard.)

None.

Notes: the doc comment on `delete_folder_invite_link` (`src/connect/folder_links.rs:172`) still says "a failure refetches the list", but a failure now ends in the error line with Retry. Change it to say that. There is one narrow race this round did not address: if the user closes the Share Folder box and reopens it before the delete is answered, `open_folder_share` calls `fetch_folder_share` (`src/ui/folder_share/mod.rs:118`), which does not check for an in-flight delete. That list can still come back holding the deleted link. Moving the delete check into `fetch_folder_share`'s links half would close it.
