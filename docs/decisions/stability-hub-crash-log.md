# Stability: receive-thread survival, unfinished-run notice, listener limits

Findings S-01, S-02 (sentinel and TDLib fatal breadcrumb), S-07, S-08 of the
quality sweep (stability audit).

## What tdesktop does

`Core::CrashReports::Start` (core/crash_reports.cpp) looks for `tdata/working`.
If the file exists, the previous launch did not finish, and the crash report
window is offered. It then reopens the file for writing; `Finish` closes and
unlinks it on a normal quit. Quill already had the panic-hook half
(`crash-*.txt`, `clean-exit` marker); it had no equivalent of the `working`
file, so a native fault, a kill or a power cut left no trace.
tdesktop also installs signal handlers; that is not part of this change.

## What changed

- S-01, `src/telegram/hub.rs`: each receive-loop pass (wait, `td_receive`,
  parse, deliver) runs under `thread_guard::ffi_boundary`. A panic loses that
  one envelope, is filed as a caught panic, and the loop continues. After 16
  consecutive panicked passes the loop ends and the hub reports itself
  stopped. `TdHub::shared` now replaces a stopped hub, reusing the already
  loaded and secured `libtdjson` so Retry reconnects instead of reusing a dead
  thread.
- S-02, `src/crash_log.rs`: `begin_run` (called from `ui/crash_notice.rs`
  install, after the single-instance check) writes `logs/running` (pid,
  version, start time) and, when the previous run left one without a newer
  `crash-*.txt`, files a report for it, so the existing "Quill closed
  unexpectedly" notice, its once-only acknowledgement and the tray-hidden
  persistence all apply. `mark_clean_exit` removes the file. TDLib's fatal log
  callback (verbosity 0, `telegram/ffi.rs`) writes `logs/tdlib-fatal` (time and
  version, never the message text); the next launch mentions it in the report
  and deletes it.
- S-07, `src/single_instance.rs`: a failed `accept` sleeps 250 ms before
  retrying; at most 8 client threads run at once (extra connections are
  dropped); client reads time out after 5 s on Unix sockets (Windows named
  pipes refuse timeouts, so the cap is the only bound there).
- S-08, `src/crash_log.rs`: a caught panic from one code location is filed at
  most once per 10 s (checked before the backtrace is captured; table capped at
  32 sites). `calls/capture/pipewire_dl.rs`: `on_process` queues the PipeWire
  buffer back outside the guarded closure, and the callbacks use
  `ffi_boundary` so their panics are classified as caught.

## Follow-up (not in this PR)

Signal handlers (sigaltstack on Unix, `SetUnhandledExceptionFilter` on
Windows) that write a minimal native-crash report. The sentinel already
turns those crashes into a notice; the handler would add the signal and
address.

## How verified

Unit tests: a source that panics once then yields a valid envelope reaches the
bridge and leaves the hub running; `reusable` classifies running, stopped and
empty slots; a source that keeps panicking stops the hub; sentinel left by a
killed run becomes a pending crash exactly once and a clean quit clears it; a
panic report is not doubled by its sentinel; the TDLib breadcrumb is reported
and consumed; the per-site limiter; client slot cap. Gate: GATE OK.

## Not verified

- `pipewire_dl.rs` is Linux-only and was not compiled or run here (macOS host).
- `TdHub::shared` replacing a dead hub over a real `libtdjson` is covered
  through `reusable`, not end to end.
- The fatal breadcrumb path was not exercised with a real TDLib abort.
- A second instance started when single-instance setup failed would overwrite
  the first one's `running` file.
