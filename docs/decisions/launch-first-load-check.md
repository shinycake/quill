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
same way (Zed's 177 MB executable takes 1.4 s); several older installs are not, and the
log does not say what set them apart.

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

## What is left
The executable's own check, 730 to 900 ms before `main`, is still there. Nothing in the
process can run before it. What would shorten it:

- A smaller executable. 19.5 MB of the 108 MB is the symbol table (about 130 ms), which
  `crash_log` needs for function names; it could move to a file beside the executable.
- An executable that is only a stub, with the app in a library `settle`d next to TDLib.
  The same bytes are scanned, partly in parallel.
- Apple caching the verdict. Worth a Feedback report.

## Rules
- A new bundled library is loaded through `first_load::settle` first.
- A launch number for a release is measured on the notarized bundle with `--recycle`,
  next to `--cold`.
