# macOS in-place updater

Idan asked for this in the project thread on 2026-10-17 23:43Z: "build the macOS in-place updater", after picking "Build it now" and after #994 ("build the opt in auto update toggle"). Before this, a Mac showed "Install updates automatically" greyed out with "On macOS, Quill updates through the release package", and the Linux and Windows package installers had no macOS half.

## What tdesktop does

- `core/update_checker.cpp` (`checkReadyUpdate`, macOS branch): the downloaded package is unpacked, the updater is `Updater` inside the new bundle, and the app relaunches it. `platform/mac/launcher_mac.mm` and `_updater_mac.m`: the updater waits for the app to exit, moves the old bundle aside, moves the new one in, and starts it with `NSWorkspace launchApplicationAtURL`, so LaunchServices (the Dock, the running-application list, the `tg:` handler registration) sees the swapped bundle. Every failure puts the old bundle back and starts it.
- tdesktop checks an RSA signature over the package. Quill's releases carry no signing key; the trust anchor is the release's SHA-256 (the GitHub asset digest fetched over HTTPS), plus the code signature on the bundle itself.

## What Quill does

The flow is #994's handoff (`update_install.rs`: InstallPlan, claim files, `--apply-update` helper, receipts, manifest), with a macOS half in `update_install/macos.rs` and `update_install/macos/codesign.rs`:

1. Stage. `quill-macos-aarch64.zip` is downloaded (size and SHA-256 checked, as on Linux and Windows) to a `0600` file in the private update folder (below), on the bundle's volume, and unpacked with `/usr/bin/ditto -x -k` into `.quill-update-<stamp>.unpack`. The zip crate would drop the resource forks and extended attributes the signature seals. The unpack must hold exactly one `.app` directory, which is renamed to `.quill-update-<stamp>.new`.
2. Verify, at staging and again in the helper before the swap (`codesign::verify`). The staged bundle passes `SecStaticCodeCheckValidity` with `kSecCSStrictValidate | kSecCSCheckNestedCode | kSecCSCheckAllArchitectures | kSecCSNoNetworkAccess` under the requirement `identifier "<running copy's signing identifier>"`; both `Info.plist` files name the same `CFBundleIdentifier`; the staged `CFBundleShortVersionString` is the release version; then the identity (below). Then the version probe (`--version`, `--build-info`) runs the new executable, and the manifest records every file's size and SHA-256.
3. Quarantine. After verification only, `com.apple.quarantine` is removed from every entry of the staged bundle (`removexattr`, `XATTR_NOFOLLOW`, depth-limited walk). Quill's own download carries no quarantine attribute (ureq does not set it; `Info.plist` has no `LSFileQuarantineEnabled`, and none is added), so this is defensive, for a bundle that picked one up another way. A quarantined, non-notarized bundle would be refused by LaunchServices after the swap.
4. Apply at the next start, through #994's claim and helper. The start claims the plan, spawns `quill --apply-update <plan>` from the running copy, and exits. The helper waits for the parent to exit, checks the manifest and the signature again, writes the receipt (`last-update.json` in the update folder), clears or recovers a stale backup, and swaps.
5. Swap (`swap::swap_dir`). `renamex_np(staged, install, RENAME_SWAP)` exchanges the two bundles in one step; where the file system refuses (`ENOTSUP`, `EINVAL`, `EXDEV`, `EPERM`), two renames. The old bundle ends up at `.Quill.app.update-backup` in the update folder.
6. Launch through `/usr/bin/open -n -a <bundle> --args ...`. If LaunchServices refuses, the helper rolls back: `restore_dir` exchanges back and deletes the failed tree, and a receipt with `failed: true` shows "Update installation failed. Your previous version was restored." at the start.
7. Backup until the first start. The new version's `finish_previous_update` deletes `.Quill.app.update-backup` on a background thread once it runs. A version that never starts leaves the backup, which the next update's helper either recovers (install missing) or clears (install intact).

## The update folder and who may write it

Plans, claims, receipts, the archive, the staged bundle and the backup all live in `<bundle's folder>/.quill-updates-<uid>`, a folder Quill creates with mode `0700` on first use (`ensure_private_dir`) and refuses when it exists with other permissions or another owner. A file added under `Contents/` would break the seal, which the next update's strict check would refuse. The folder is a sibling of the bundle, so the swap never crosses a volume. `/Applications` itself is `root:admin 775`: shared with every administrator, which is why nothing in it is trusted on its own.

Every update file is checked before it is read (`trusted_file`, `trusted_dir` in `update_install.rs`):

- A plan, claim, receipt or refusal note counts only when it is a regular file (never a link) owned by the effective user with no group or world write bit (`mode & 0o022 == 0`), in a folder owned by the effective user that the world cannot write to (`mode & 0o002 == 0`; group write stays allowed for Linux private-group umasks). Quill writes its own files `0600`.
- `staged::update_files` lists only such files, so a plan, claim or receipt planted by another account is invisible: it cannot make a start delete anything, exit, or report a failure. A planted `.applying` with a live lock does not make Quill exit at every launch (`a_planted_locked_claim_does_not_stop_quill_from_opening`). `claim::take_over_with` checks the file again after opening it.
- `staged::discard` deletes a staged package only when the path read from the plan is a temp-prefixed entry (`.quill-update-*`) right inside this install's stage folder (`staged_for`); any other path in the JSON is left alone. The same guard applies to the helper's own cleanup (`apply_plan`, `apply_claimed`).
- `installable` reports `OwnedByAnother` when the bundle or an existing update folder is not this user's own: another user's install could never be updated (its backup could not be cleared, its files could not be trusted), so the switch says so instead of every later update failing on the backup.
- A release refused as signed differently is noted in `refused-release.json` in the update folder and not downloaded again at every check (`refused_before`); a new version clears the note.

Windows has no owner check (ACLs; the install directory itself is the plans folder); Linux keeps the plans inside the install directory as before, with the same file and folder rules.

## Identity

- A running copy signed with a certificate (a Developer ID, or the owner's Apple Development identity) has a designated requirement that names its leaf certificate and team. The staged bundle must satisfy that requirement (`SecCodeCopyDesignatedRequirement` of the running copy, `SecStaticCodeCheckValidity` of the staged one). The keychain approval for the TDLib database key is bound to this requirement, so a bundle signed by anyone else never installs and the approval survives an update.
- A running copy with an ad-hoc signature (GitHub releases built without the signing secret, `macos-release-signing.md`) has no identity to pin: its designated requirement is its own cdhash, which a new version never satisfies. For an ad-hoc running copy the trust anchor is the release SHA-256 fetched over HTTPS from GitHub (through `latest.json`), plus the strict signature check with the running identifier. So an ad-hoc copy accepts any strictly valid ad-hoc bundle with the same identifier whose bytes match the digest GitHub published.
- Mismatch (certificate-signed running copy, staged bundle signed by someone else, or an unsigned running copy) is `Rejection::Mismatch`, shown as "This copy of Quill is signed differently from the release, so it updates manually." Nothing is installed, and the version is remembered (`refused-release.json`) so it is not downloaded again at every check. A broken signature, another identifier or another version is `Rejection::Invalid`: "The downloaded app did not pass verification. Nothing was installed." Both leave the running copy alone, and a staged bundle that failed keeps any `com.apple.quarantine` mark it had (quarantine is removed only after the whole verification).
- `judge(&Facts)` holds the whole policy and is unit-tested on its own; `gather` collects the facts once per bundle.

## Where the switch is greyed out

`update_install::self_update_availability()` returns `Result<(), Unavailable>`, and `auto_update::row_state` shows the reason as the row's hint. On macOS, in this order (`macos::judge_location`):

- `Translocated`: the path has an `AppTranslocation` component. Gatekeeper runs the app from a random read-only copy; nothing beside it is the install. "macOS opened Quill from a temporary copy. Move Quill.app to your Applications folder and open it from there."
- `DiskImage`: the bundle is under `/Volumes` and `hdiutil info -plist` lists its mount point. Only volumes under `/Volumes` are asked, so a bundle on the system volume costs no process. "Quill is running from a disk image. Drag Quill.app to your Applications folder and open it from there."
- `ReadOnly`: `statfs` reports `MNT_RDONLY`. "Quill is on a read-only disk, so it cannot replace itself."
- `PackageManager`: a `Caskroom`, `/opt/homebrew` or `/usr/local/Cellar` path (canonicalised, so the Applications symlink resolves). "A package manager installed this copy of Quill and installs its updates."
- `OwnedByAnother`: the bundle, or an existing update folder beside it, belongs to another user or is open to others. "Another user installed this copy of Quill, so it cannot replace itself."
- `NotWritable`: the bundle's folder (the swap renames the bundle) or the update folder (its parent when it does not exist yet) cannot be written. "You do not have permission to change the folder Quill is installed in." An `/Applications` owned by an admin while Quill runs as a standard user falls here.
- `NotPackaged`: a bare cargo build, not inside `<x>.app/Contents/MacOS/`.

The answer is computed once (`OnceLock`), on the cleanup thread at start (`finish_previous_update` probes it first), never on the render path: `self_update_availability()` only reads the cache (`availability_now`) and, until the thread has filled it, answers `Checking` ("Checking whether this installation can update itself…"), so the row is briefly greyed out instead of a frame spawning `hdiutil` or probing folders. Linux and Windows keep their reasons (`NotPackaged`, `PackageManager`, `NotWritable`, `OwnedByAnother`).

## Launch: `open -n -a`, nothing forwarded

`open -n -a <bundle>` goes through LaunchServices, so the relaunched app is registered as the bundle at its path: the Dock icon, the running-application list and the `tg:`/`t.me` handler all point at the swapped bundle. Running the executable directly would start a process the system treats as unrelated to the bundle: the Dock shows a generic icon, and the next `open` of a link may start a second copy. tdesktop's updater uses `launchApplicationAtURL` for the same reason.

`open` passes the caller's environment to the launched app (`man open` does not say so; a compiled fixture bundle launched through `launch` recorded the variable it was given, `macos::tests::open_starts_the_bundle_with_its_arguments_and_environment`, and a shell check on 2026-10-17 saw `QUILL_API_ID` and a test variable arrive). So `launch` forwards no `--env`: the relaunched copy inherits the data root and the fixture URL of a test build the way a Linux relaunch does, and `QUILL_API_ID`/`QUILL_API_HASH` never appear in another process's argument list (`ps` shows arguments to every user; the environment only to the owner and root). The launch's arguments go after `--args`, as on the other platforms.

`open` returns success once LaunchServices has accepted the launch, before the new version reaches its window. The version probe before the swap catches a binary that cannot start; a crash later in startup is not rolled back automatically (the backup stays beside the bundle for a hand rollback). A post-launch watchdog is out of scope.

## Quarantine and Gatekeeper, checked on this Mac

- `ureq` writes the archive with plain `File` writes; nothing sets `com.apple.quarantine`. The bundle in `Info.plist` does not opt into `LSFileQuarantineEnabled`, and this change does not add it: a bundle the app itself extracted would otherwise be quarantined and, not being notarized, refused.
- Apple Silicon kills a process whose signature is broken, quarantined or not. The strict check runs before the swap, so a broken bundle never reaches the install path.
- A copy the user once approved with "Open Anyway" keeps opening after a swap, because the swapped-in bundle is not quarantined and Gatekeeper's assessment is only made for quarantined files.
- `codesign --verify --deep --strict` is what the Security framework calls do in process; a bundle renamed to `.quill-update-<stamp>.new` still verifies (the seal covers the contents; the bundle's name is outside it).
- A `com.apple.quarantine` mark on a bundle does not break its signature, but running the executable of a quarantined bundle makes Gatekeeper assess the bundle first, which on this Mac did not return within the version probe's 5 s: such a bundle fails the probe and is refused with its mark in place. Quarantine is removed only once the signature check and the probe have passed.
- A `SecStaticCode` object remembers its verdict; `verify` creates a new one for every check, so a bundle changed after an earlier check is never taken for verified.

## Tests

Unit (`update_install/tests.rs`, `update_install/macos/tests.rs`; the macOS fixtures are compiled C programs inside a bundle, ad-hoc signed with `codesign --deep`):

- `judge`: unsigned running copy, broken staged signature, foreign identifier, wrong version, ad-hoc accepts, certificate requires its designated requirement.
- `verify` on real bundles: a matching bundle passes; a changed byte, an added file, another identifier or another version fails; an unsigned running copy is a mismatch.
- Location: translocated beats disk image beats read-only beats package manager; `AppTranslocation` and `Caskroom` paths; `hdiutil` plist mount points; a read-only `statfs` flag.
- Unpack: exactly one `.app`, two entries refused; `__MACOSX` consumed by ditto.
- Quarantine removal clears every entry and the signature still verifies.
- Apply: install and backup kept until the next start; bad probe keeps the old bundle and writes a failed receipt; tampered package fails manifest and signature; second helper on the same plan does nothing; interrupted update cleared before staging; `installable` for a bare build, a bundle, a non-writable parent, a Linux read-only install directory, an update folder open to others.
- Trust: `trusted_file`/`trusted_dir` for another owner, group and world write bits, links, `0700` creation; a plan with a world write bit and a plan in a world-writable folder ignored; a planted locked `.applying` lets Quill open; `discard` leaves any path outside the stage folder; a refused version remembered only from this user's own note.
- Signature details on real bundles: a tampered helper inside `Contents/MacOS` passes the default check (its cdhash is sealed, not its pages) and fails the strict nested check; an ad-hoc signature has no certificate and a cdhash designated requirement that only the same bundle satisfies; a bundle that fails keeps its quarantine mark, one that passes loses it; with an Apple Development identity on the host (`security find-identity`, skipped on CI), a certificate-signed copy accepts the same identity's new version and refuses an ad-hoc one.
- Launch: `open -n -a` starts a fixture bundle with its arguments and the caller's environment.
- `renamex_np` exchange on APFS; a swap whose backup folder cannot be written (the App Management case) fails with both trees intact, on the exchange path and the plain path.

Real install, on this Mac on 2026-10-17, with two `demo-capture` builds stamped `QUILL_RELEASE_VERSION=2026.1017.9000` (N) and `9001` (N+1), packaged by `scripts/macos-package-smoke.sh` and signed with the owner's Apple Development identity, N installed in a folder under `~/Library/Caches` (never `/Applications` or the real install), its own `QUILL_DATA_ROOT`, and a local `latest.json` plus `quill-macos-aarch64.zip` served on `127.0.0.1` through `QUILL_UPDATE_FIXTURE_URL`:

- Good N+1: N, started with `open -n -a`, fetched `latest.json`, downloaded the zip and staged the plan beside the bundle in 7 s. After `--quit` and a second `open`, the helper swapped the bundle in 2 s, N+1 started through LaunchServices, wrote the `failed: false` receipt, and its first start removed `.Quill.app.update-backup`. `codesign --verify --deep --strict` passes on the installed bundle.
- Tampered N+1 (one byte appended to a sealed resource after signing, re-zipped, digest updated in the fixture): refused at staging with "the staged bundle failed the strict signature check"; nothing left beside the bundle, N still 9000 and verifying.
- Ad-hoc N+1 against the certificate-signed N: refused with "the staged bundle does not satisfy the running copy's designated requirement"; same clean folder.

After the review (the private update folder, nothing forwarded by `launch`), the same cycle ran again on 2026-10-17 with both builds signed ad-hoc, as GitHub releases are, N in `~/Library/Caches/quill-updater-test/apps` with its data root and fixture URL set only in the environment of the `open` that started it: N staged the plan and the bundle in `apps/.quill-updates-501` (`drwx------`) 7 s after the start; after `--quit` and a second `open`, the helper swapped the bundle and N+1 was running through LaunchServices 7 s later, with the `failed: false` receipt at `.quill-updates-501/last-update.json`, the backup cleared by its first start, and nothing else left in the folder. N+1 answered `--quit` over the single-instance socket of that data root, so it had inherited `QUILL_DATA_ROOT` from the helper through `open` without `--env`. `codesign --verify --deep --strict` passes on the installed bundle.

A first attempt with the install on the external SSD stalled: an app launched through LaunchServices from a removable volume gets the "would like to access files on a removable volume" consent prompt, which a Terminal launch inherits from Terminal instead. That is a property of the test location, not of the updater.

## Limits

- The disk-image check is a heuristic (`hdiutil info` mount points, 5 s timeout); a bundle on an image mounted elsewhere than `/Volumes` is treated as a normal volume and fails later on the write probe.
- The helper is the old executable, running from the bundle that gets moved to the backup name during the swap. APFS keeps the running image valid; the helper exits right after `open` returns.
- TCC permissions (notifications, screen recording for calls) are granted to the bundle identifier and the designated requirement, so they survive an update of a certificate-signed copy. For an ad-hoc copy macOS may ask again after an update, as it does for any rebuilt ad-hoc app.
- `QUILL_UPDATE_FIXTURE_URL` and `QUILL_DATA_ROOT` are read only under `feature = "demo-capture"`; release builds have no release-URL override. A fixture URL counts only as a bare loopback origin (`http://127.0.0.1:<port>`, no user info, path or other host, `updater::local_fixture_url`), and fixture downloads follow no redirects; GitHub downloads stay HTTPS-only with up to three redirects.
- App Management (macOS 13+): a process without the App Management permission may not change another developer's app bundle in `/Applications`; `rename` and `renamex_np` fail with `EPERM`. The exchange reports `Unsupported`, the plain path's first rename fails, so nothing has moved: the helper writes a `failed: true` receipt ("Update installation failed. Your previous version was restored.") and starts the old version. Quill does not ask for App Management; `installable` cannot see this permission in advance (the write probe creates a file, which App Management allows), so such a copy learns it at the first attempt. The failing-backup swap test covers the same code path; the real permission was not exercised (no scratch install in `/Applications`, by the test rules).
- Homebrew: a cask whose `app` artifact moves the bundle into `/Applications` leaves no `Caskroom` path to detect; such a copy is treated as a hand-installed one and updates in place, while `brew upgrade` would replace it again. A `Caskroom`, `Cellar` or Homebrew-prefix path is detected and left to Homebrew.
