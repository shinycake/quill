# launch-first-load-check: macOS rescans a notarized Quill on every cold start

## Problem
On 2026-11-09 Idan reported that a cold start was slow again, on release 2026.1109.5357.
The launch benchmark disagreed: with the file cache emptied (`launch-bench.py --cold`) the
window was on screen 756 ms after `open` (median of 6, 689 to 866), the same as on
2026-11-07. No change merged on 11-09 was slower in it.

His Mac had restarted six minutes before the slow launch. The run's log shows about 735 ms
between `exec` and the first line of `main`, where the benchmark sees about 130 ms.

## What it was
`purge` empties the file cache and leaves the kernel's per-file state alone. A restart
loses both, and so does enough other file activity (a build, an index): the kernel keeps
state for `kern.maxvnodes` files, 250,174 here, and drops the oldest. Looking at 375,000
unrelated files before each launch (`launch-bench.py --recycle`) reproduces the slow start
every time, on the unmodified release bundle and the real account:

| before the launch | `exec` to `main` | window on screen |
| --- | ---: | ---: |
| nothing (warm) | 25 ms | 362 ms |
| `purge` | 130 ms | 726 to 866 ms |
| recycle | 785 to 907 ms | 1850 to 2404 ms |

A system-wide Time Profiler recording of a recycled launch puts all of the difference in
`syspolicyd`, in `yr_scanner_scan_file` and SHA-256: XProtect's YARA rules run over a whole
Mach-O file the first time it is mapped executable after the kernel forgot it, and the
mapping waits. It costs 7 to 9 ms per MB:

| file | size | wait |
| --- | ---: | ---: |
| `quill` | 108 MB | 730 to 900 ms, before `main` |
| `libtdjson.dylib` and OpenSSL | 48 MB | 390 to 510 ms |
| `libquill_calls.dylib` and FFmpeg | 25 MB | 320 to 430 ms |

The libraries are loaded by the early-connect worker (`launch-time-early-connect.md`), so
their wait sat inside `dlopen`, under the dynamic loader's lock. The main thread needs that
lock while it builds the application object, and stood still for about 720 ms.

### Why it repeats, and why now
`syspolicyd`'s log (`log stream --level debug`, subsystem `com.apple.syspolicy.exec`), macOS
27.2:

- Evaluating a notarized bundle logs `Skipping XProtect scan on seen software` and
  `Setting YARA scan threshold`: the bundle is trusted without a scan, and each file is
  left to be checked when it is mapped.
- Each mapped file then logs `Outer bundle for: … is …`, `Recording cache miss`, the scan,
  and `No malware match`. It never logs `Updating cached scan`: the verdict is kept with
  the kernel's state for the file and goes when that does.
- A bundle that is signed and not notarized logs `Doing an XProtect scan b/c not exempt`
  once, for the whole bundle, and its files are not scanned again: the same binary re-signed
  locally starts in 71 ms after a recycle, against 865 ms notarized.

So the cost came with notarization (`macos-developer-id.md`, first notarized release
2026.1103.4940), not with a code change, and no local build ever showed it: a bundle signed
by `launch-bench.py` is in the second group, and so was every release before that one. On a
Mac that builds all day the kernel forgets Quill's files within minutes, so there nearly
every start paid it. It is not particular to Quill. Of the apps
installed on the same Mac, the notarized ones updated in the last week are rescanned the
same way (Zed's 177 MB executable takes 1.4 s); several older installs are not. The next
section says what sets them apart.

### Why Telegram and Ghostty are not rescanned
Found the same evening, from the kernel's policy log (`process == "kernel" AND sender ==
"AppleSystemPolicy"`, one `evaluation result:` line per decision), `syspolicyd`'s code and
copies of the release in a cache folder.

`syspolicyd` keeps one record per bundle: when it was last scanned and with which rules.
The kernel has a file checked again when the file changed after that time (`changetime` in
the log). For a notarized bundle whose scan was skipped the time is "never", so every file
fails that test on its first mapping after the kernel forgot it. The check that follows
is saved only when the file has a record of its own, which a file inside a bundle does
not: it logs `Recording cache miss` for the bundle and nothing else.

Three things give a bundle a real time:

- The first launch of a download. Approving "downloaded from the Internet" stamps the
  bundle with that moment (`Marked application as user approved`). A copy installed from
  the DMG is quiet from its first start.
- A background job, `com.apple.security.syspolicy.invalidate`. It scans every bundle that
  has cache misses and saves the result. It runs only while the screen is asleep, about
  three days after its last run, or one day after a restart (`dasd` logs each run). The
  stored times of Zed and Tern are inside its last two runs, 2026-11-02 21:27 and
  2026-11-06 16:48, to the second.
- Not being notarized (the scan is not skipped).

Telegram, Ghostty and Fork were last updated weeks ago and have been through the job. An
update that Quill installs itself is never approved by anyone, and with a release most
days the job never reaches the installed build before the next one replaces it.
`gktool scan` does not help: it skips the scan the same way, for the bundle and for a file
in it.

A file that is not inside a bundle is treated differently. Its first check is saved
(`Updating cached scan for … with scantime … and version …`), keyed by the file itself,
and that record is found first when the file is later mapped from inside a bundle.
`syspolicyd` takes a folder for a bundle only when its name has an extension and it holds
`Contents/MacOS`, `Contents/Info.plist` or the like. The same notarized release, after a
recycle, each row measured twice:

| copy | `quill --version` | mapping `libtdjson.dylib` | scans |
| --- | ---: | ---: | ---: |
| copied as `Quill.app` | 888 to 909 ms | 338 to 348 ms | 12 of 12 files |
| copied as `stagedir`, every code file mapped once, renamed to `Quill.app` | 64 ms | 5 to 6 ms | 0 |

The order matters. A folder that was first seen under a bundle's name (`123.new`) stays a
bundle to the system after it is renamed, and its files get no records of their own.
Renaming a folder keeps the files' change times; renaming a file does not, and a record
older than the file's change time is not used.

## What did not help
- `spctl --assess` on a fresh copy before its first launch, and a quarantine attribute
  marked as approved: the bundle is still trusted without a scan, and its files are still
  rescanned.
- Asking for the scans of all the libraries at once: 522 ms against 660 ms one after
  another, and TDLib's own finishes later (519 ms against 340 to 450).

## Decision
`src/first_load.rs`: `settle(library)` maps the library, and the libraries beside it that
it links to (read from its load commands), as code and drops the mapping. The system runs
its check on that mapping, before it decides whether to allow it, so the wait happens on
the calling thread and outside the loader's lock. The `dlopen` that follows finds the
verdict and is quick. Nothing is skipped: it is the same check, asked for one call
earlier. Every load of a bundled library goes through it: TDLib (`telegram/ffi.rs`), the
call library (`quill-calls-sys`'s `load_default_after`), the video decoder and the WebM
sticker decoder. Off macOS it does nothing.

Measured with `scripts/launch-floor/settle-probe` (the real `settle`, hardened runtime,
the release bundle's libraries, after a recycle, three runs each):

| load | worker | longest main-thread wait |
| --- | ---: | ---: |
| TDLib, `dlopen` alone | 457 to 517 ms | 456 to 515 ms |
| TDLib, `settle` then `dlopen` | 432 to 464 ms, then 21 ms | 20 to 21 ms |
| call library, `dlopen` alone | 430 ms | 429 ms |
| call library, `settle` then `dlopen` | 326 ms, then 53 ms | 53 ms |

With the libraries' verdicts already in place, a recycled launch of the release put the
window on screen in 1203 to 1212 ms against 1928 ms. That is what this change should give
a notarized build; it can only be measured on one, so the number for the app itself comes
from the first release that has it:

```
scripts/launch-bench.py --recycle --onscreen old=/path/to/old/Quill.app new=/path/to/new/Quill.app
```

When there is nothing to check, `settle` costs the worker 2 ms for TDLib's three files
and 4 ms for the call library's seven (the probe on a locally signed copy, file cache
purged). The window does not notice: one build launched with `settle` and with its first
instruction patched to return, file cache purged, ten launches each in turn, put the
window on screen in a median 946 ms and 949 ms.

## Updates are checked before they become a bundle
The updater used to unpack a release into `.quill-update-<stamp>.new`, which the system
takes for a bundle, and run the new executable there for its version probe. From then on
the files could only be settled by the background job.

- The staging folder is named `.quill-update-<pid>-<time>-new`: no dot after the first
  character (`update_install::staged_dir_name`). A plan staged under the old name still
  installs.
- After the signature and version checks, `first_load::settle_package` maps every Mach-O
  file of the package once. The system scans each (about 3 s in all, on the download
  thread, before the update is offered) and saves the verdicts.
- The swap renames the folder, as before, so the verdicts arrive with the files.

Nothing is skipped here either: each file gets the same XProtect scan, with the current
rules, once instead of at every cold start. A file that changes afterwards is scanned
again, and so is every file when the rules change (once, and saved).

Checked with the real updater: a local build of this branch, signed like the release and
given an older version number, downloaded the notarized 5357 release from a local fixture
(`QUILL_UPDATE_FIXTURE_URL`) and staged it. The log shows one `Updating cached scan` line
for each of the 12 code files, 2 s in all. The staged folder was then exchanged with an
installed `Quill.app` the way `swap_dir` does it (`renamex_np`, `RENAME_SWAP`):

| cold start of the installed copy | `quill --version` | mapping `libtdjson.dylib` | cache misses |
| --- | ---: | ---: | ---: |
| first after the install | 619 ms | 6 ms | 0 |
| second | 65 ms | 6 ms | 0 |
| third | 67 ms | 6 ms | 0 |
| the same release copied as `Quill.app` | 895 ms | 340 ms | 12 |

The first start after an install still spends about 0.55 s on the system's evaluation of
the new bundle as a whole (signature and notarization ticket, no scan). That happens once.

The first release with this change is still unpacked by the old updater, so the gain
starts with the release after it. Installing from the DMG was never affected.

## Measuring a local build against a release
Two things made a local build look 30 to 50 ms slower than the release before the
comparison above was done on one build:

- A plain `cargo build` targets macOS 11 and has no chained fixups; the release targets
  macOS 14. Build with `MACOSX_DEPLOYMENT_TARGET=14.0`, or compare two local builds.
- A local build reports Cargo's version, which is older than every release. Signed with
  the release's identity it downloads the latest release, and the next start installs
  it (`apply_on_start`): in two runs the binary under test was replaced part of the way
  through. `launch-bench.py` now keeps
  each bundle in a read-only folder, where Quill does not update itself, and stops if a
  bundle's executable changes.

## What is left
A build that reaches a Mac without either the approval prompt or the new updater still
pays the executable's own check, 730 to 900 ms before `main`, until the background job
has run: the first release after this change, and a copy installed by a tool that strips
the quarantine mark. Nothing in the process can run before that check. What would shorten
it:

- A smaller executable. 19.5 MB of the 108 MB is the symbol table, which `crash_log`
  needs for function names; it could move to a file beside the executable. Measured on
  two copies of the release executable, one with local symbols stripped (85 MB against
  102 MB), five first mappings each in turn: 957 to 1009 ms against 1076 to 1144 ms,
  about 115 ms.
- Apple saving the verdict for a file in a bundle as it does for a file outside one.
  Worth a Feedback report.

After an XProtect rules update (about weekly) each file is scanned once more on its next
cold mapping, and the result is saved.

Apart from the check: loading the call library on the early-connect worker costs the main
thread about 30 ms of a launch with a purged file cache (`main` to the app object built:
298 ms with the library in the bundle, 269 ms without, eight launches each in turn). It
could load after the first frame, at the price of holding the loader's lock then.

## Rules
- A new bundled library is loaded through `first_load::settle` first.
- Nothing runs or maps code from an unpacked update while its folder has a name with an
  extension, and nothing renames or rewrites its files after `settle_package`.
- A launch number for a release is measured on the notarized bundle with `--recycle`,
  next to `--cold`.
