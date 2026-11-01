Quill now reads the disk more carefully, keeps calls and other accounts running when it can, and limits its cache from the start
## What was wrong

The full-disk guard from #1284 left a list of known limits, and the read-only audit of main at 5072e9662 (bundle B2) added more. Every item in the bundle:

- N01: every UI test read the machine's real disk, so a test could pass or fail by how full the CI disk was.
- N02: arming TDLib's fatal breadcrumb rewrote its slot with `fs::write` on each library load, which could blank a breadcrumb not read yet, and in a capture build it recorded the `QUILL_DEMO_DISK_FREE_MB` stand-in instead of the real disk.
- N04: the low-disk bar says automatic downloads wait, but a release check still started the update download.
- N05: a new session allowed automatic downloads until the next disk reading, up to about 17 s while low.
- N06: on macOS `statfs` leaves out purgeable space, so Quill could read Low or Full while Finder showed room.
- N07: on Linux a user quota (`EDQUOT`) was invisible, and a zero reading on btrfs or as root was taken as a full disk.
- N08: a symlinked `accounts`, `tdlib` or `files` folder on another volume was never read.
- N09: one low reading moved Healthy straight to Full and closed every client.
- N10: listed downloads a low disk paused never resumed, and one resumed while low was paused again without a word.
- N11: the open floor was fixed, so a TDLib migration of a large database could fill the disk inside TDLib's open.
- N12: reads were spaced for a 64 MiB/s writer, so a faster local copy could pass the close floor between reads.
- N13: a failed read changed nothing, so a pulled drive or a read-only remount went unnoticed.
- N16: Manage local storage scrolled with the layout at the click, and rows that loaded later pushed the list out of view.
- growth-2: TDLib's cache had no limit until the user moved a slider, so an account that never opened Data and Storage kept every cached file for good.
- Owner decisions, with the default applied: L38 (scale the close floor by accounts), L39 (reveal the cache folder, delete nothing), L40 (keep running clients when one open is refused), N06 (one macOS source), N14 (keep a call's client open until the call ends), N15 (one system notification per Full edge), N108 (keep the message database, show its size).

## What changed

One commit per item, in this order: N01, N02, N04, N05, N08, N07, N06, L38, N11, N09, N12, N13, L40, N14, N15, L39, N10, N16, N108, growth-2. A last commit fixes a clippy lint in the N14 test, and one merge brings in origin/main.

Reading the disk (`src/disk_guard`):

- A read now returns free bytes, no sizes, or unwritable. Unwritable means the read failed, the mount is read-only (`MNT_RDONLY`, `ST_RDONLY`, `FILE_READ_ONLY_VOLUME`), or the data root is gone and its nearest parent is on a different device than the root was. A folder deleted to free space still reads its parent on the same volume. Unwritable counts as no room, and the screen says the disk is missing or read-only.
- The watcher reads the data root, `accounts`, and each account folder with its `tdlib` and `files` folders. Symlinks are resolved, and the least free space wins.
- macOS reads `volumeAvailableCapacityForImportantUsage` through NSURL, with `statfs` only as the fallback for a volume that does not answer it and for the read-only flag.
- Linux reads the user's block quota with `quotactl_fd`, falling back to `quotactl` on the mount's device from `/proc/self/mountinfo`, and counts the room under the quota as the free space. A zero on btrfs, or as root with reserved blocks free, is checked with a 64 KiB write and sync: Full only if that fails with `ENOSPC` or `EDQUOT`, no reading otherwise.
- A first reading below close only arms the check, and the next read comes `CONFIRM_READ` (2 s) later. Full needs that one below close too.
- The watcher measures the fall between its last two readings. When it is faster than 64 MiB/s, the next read is spaced by that rate, down to 250 ms.

The floors (`DiskFloors::for_data`):

| Floor | Before | Now |
| --- | --- | --- |
| close | 160 MiB | 224 MiB + 14 MiB per account with a database |
| open | 288 MiB | close + 128 MiB + the largest account's `db.sqlite` and WAL |

The 224 MiB fixed part is one read step at 64 MiB/s (64 MiB), the 2 s confirm wait at the same rate (128 MiB) and the 32 MiB APFS reserve. One account now closes at 238 MiB and six at 308 MiB. The 14 MiB per account is the owner's default (a 10 MiB reindex and a 4 MiB WAL checkpoint). The confirm wait is new room that N09 needs, so close for one account is higher than the 160 MiB before.

What the user sees and what clients do (`src/ui/disk_state.rs`):

- A refused on-screen open shows the full-disk screen for that client only, and retries at the open floor. Background accounts keep running and keep starting until a reading falls below close.
- At Full, a client with a call or a joined voice chat stays open. The bar then reads "Disk is full. Quill will disconnect when the call ends.", and the first reading after the call ends closes it. Background clients close at once. A call still ringing in does not hold the close.
- Each time the full-disk screen goes up, the main window comes forward and the system shows one notification.
- The full-disk screen has a Show cached files link that reveals the on-screen account's `files` folder. Quill deletes nothing while TDLib is closed.
- The update download waits while the disk is low or full, and the return to healthy starts it.
- A client gets the last disk reading when it opens.
- Downloads the low disk paused are resumed when it is healthy. While it is low, a resume that would not fit above close is refused with a note, and one resumed elsewhere is paused again with the same note.
- Manage local storage shows the message database size, and scrolls again once the automatic download rows have loaded, unless the user scrolled first.
- UI tests start no watcher, and the demo stand-in lives in the UI's watcher only.

Cache limits: the first Ready of an account sets tdesktop's starting limits, 1 GiB and 31 days. I confirmed both numbers in `lib_storage` `storage/cache/storage_cache_types.h` (`totalSizeLimit = 1024 * 1024 * 1024`, `totalTimeLimit = 31 * 24 * 60 * 60`), and both are steps on Quill's sliders. A `storage_limits_seeded` flag in `data_storage.json` makes this happen once per account, background accounts included, so a later "No limit" sticks. An account whose TDLib options already show the optimizer on, or Quill's "No limit" values, keeps them.

Breadcrumb: `arm_native_fatal_breadcrumb` keeps a slot that is already full size and only writes a blank slot when there is none.

New strings are in `lang/en/disk.strings`.

## What ran

On Linux x86_64 in this cloud session:

- `cargo fmt`, and `cargo clippy -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`, both clean after the merge with main.
- `scripts/check-hotspots.sh`, `check-file-size.sh`, `check-lang.py` and `check-app-name.sh`: all OK. No hotspot file changed.
- Library tests: 6874 passed, 13 ignored. The one failure is `settings::accounts::account_registry_tests::atomic_write_failure_keeps_old_file`, the chmod 0555 test that fails as root.
- UI tests (`--bin quill`, `demo-capture`): 2451 passed, 2 ignored. The one failure is `ui::background_accounts::deleting_tests::a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`, another chmod 0555 test that fails as root.
- After the merge: the 30 disk UI tests, plus 178 library tests matching disk_guard, crash_log, downloads, account_hygiene and closing, all pass.
- New tests, each written to fail without its fix:
  - `disk_guard/tests.rs`: confirmation, the fall-based cadence, floors by accounts and by database size, symlinked folders, unwritable folders and a vanished volume, and on Linux the quota, mountinfo and zero-write rules.
  - `probe.rs`: the three-way reading.
  - `crash_log.rs`: re-arming keeps a breadcrumb, and the breadcrumb ignores the demo variable.
  - `connect/tests/messaging/downloads.rs`: resume on healthy, refused resume, re-pause count.
  - `connect/tests/closing.rs`: `call_in_progress`.
  - `connect/tests/account_hygiene.rs`: the seed on first Ready, once only, and an earlier "No limit" kept.
  - `ui/disk_state_tests.rs`: no real read in tests, the seed on open, one low reading closes nothing, a faster fall reads sooner, the unwritable screen, the call hold bar, one notice per edge, the reveal link, the storage scroll after rows load, and the database size row.
  - `advanced_ui_tests.rs`: the held update download.
  - `background_accounts/disk_tests.rs`: a refused screen leaves background accounts starting.
- I checked by mutation that the N01 and N16 tests fail with the fix taken out.
- Four existing connect tests counted the requests a Ready sends. They now clear the recorder after Ready, or list the four `setOption` requests of the seed.
- `src/disk_guard/probe.rs` type-checks and passes clippy as a standalone crate for `aarch64-apple-darwin` (with and without the `ui` feature) and `x86_64-pc-windows-gnu`.

## What did not run

- Nothing ran on macOS or Windows. The NSURL read, `MNT_RDONLY`, `GetVolumePathNameW` and `GetVolumeInformationW` were only type-checked as above. A Mac check should run the disk tests and read a volume with local snapshots against Finder.
- `cargo check --target x86_64-pc-windows-gnu` for the whole crate stops in the `ring` build script, because this session has no MinGW C toolchain.
- No real disk was filled for this round, so the new floors, the 2 s confirm and the faster reads have no before and after numbers. The 173.0 GiB against 184.0 GiB macOS figure is from #1284, not measured here.
- On Linux, `quotactl_fd` and `quotactl` were never run against a volume with a quota. The btrfs and root zero-write path was never run on a full volume, because this container cannot mount one.
- A `LiveConnect` cannot exist in a test. So the open paths that seed a new client, the call hold on a real call, and the close after the call ends are covered through the driver methods and the UI state, not end to end.
- The system notification was counted, not watched on screen on any OS.
- No screenshots were taken of the unwritable screen, the call bar, the reveal link or the database row.

## What stays open

- growth-2 GC: TDLib's collector exempts only stickers, profile photos, thumbnails, wallpapers, backgrounds and ringtones, plus files changed within its immunity delay (`FileGcWorker.cpp`). Files the user downloaded by hand that sit in `tdlib_files` can go once they pass 31 days without access or the cache passes 1 GiB, and an existing user's first collection can free a lot at once. This needs a line in the release notes.
- growth-2 option order: the seed trusts that TDLib's `updateOption` values for the storage options arrive before the first Ready. That was not checked against a real signed-in account.
- N04: an update download that was already running when the disk went low is not stopped. Only the start waits.
- N10: a download the user paused by hand after the disk paused it is still resumed on healthy.
- N12: a writer faster than the reads can still pass close between two reads. Then TDLib's abort and the breadcrumb on the next launch are what remain.
