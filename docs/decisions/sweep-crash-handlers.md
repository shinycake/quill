# Native crash handlers

Sweep stability item 2 follow-up. #861 added the `logs/running` sentinel and the
"closed unexpectedly" notice, but a crash in native code (TDLib, ntgcalls, FFmpeg,
AppKit, a GPU driver) left a generic report with no hint of what crashed.

## What tdesktop does

`Telegram/SourceFiles/core/crash_reports.cpp` installs a signal handler on
Unix (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT, on an alternate stack) that writes
the signal and a raw backtrace to a pre-opened dump file, and an unhandled
exception filter with a minidump on Windows. The next start reads the dump and
shows the crash window.

## What changed

- `src/native_crash/unix.rs`: `sigaction` for the five signals with
  `SA_SIGINFO | SA_ONSTACK` (no `SA_RESETHAND`/`SA_NODEFER`). The handler uses
  `write(2)`, `time(2)`, `nanosleep(2)`, `backtrace(3)`, `backtrace_symbols_fd(3)`
  and `raise(3)` on a file descriptor opened at startup
  (`logs/native-crash-active.txt`); the header (version, OS, pid) is built at
  install. Order in a fault:
  1. the first thread writes signal, fault address, time, header, up to 64
     frames and an "end of report" line;
  2. a second thread that faults meanwhile waits up to 2 s (`nanosleep` loop) for
     that, instead of dying mid-report. The disposition stays ours until the
     first thread is done, which is why `SA_RESETHAND` is not used;
  3. the handler that was installed before ours is called (saved per signal,
     `SA_SIGINFO` or plain), so Rust's "thread has overflowed its stack"
     message still prints;
  4. the default action is restored. A hardware fault just returns, so the
     instruction faults again under the default action and the OS crash report
     keeps the real faulting frame, `si_code` and `si_addr`. SIGABRT and signals
     sent by a process (`raise`, `kill`, detected through `si_code`) are re-raised.
  A fault inside the handler itself happens with its signal blocked (no
  `SA_NODEFER`), so the kernel ends the process instead of looping.
  The alternate stack: the thread's existing one is kept (Rust's, with a guard
  page); our own 256 KiB leaked stack is installed only if there is none.
- `src/native_crash/windows.rs`: `SetUnhandledExceptionFilter`. Install opens the
  report and dump files and starts a `native-crash-dump` thread. The filter only
  stores the exception pointers and thread id, `SetEvent`s the request event,
  waits (15 s at most) on a done event and chains to the previous filter; no
  `format!`, `File::create` or heap use inside it. The dump thread writes the
  text report and `MiniDumpWriteDump` for the blocked faulting thread. A second
  thread that faults meanwhile waits for the same done event. Adds the
  `Win32_System_Diagnostics_Debug` and `Win32_System_Kernel` windows-sys features.
- Install timing: `crash_log::begin_run_with_native_handlers` now runs at the top
  of `ui_main` (after the single-instance check, before `sweep_media_caches`,
  GPUI, AppKit, TDLib and the call/video libraries), no longer from the crash
  notice setup. The notice's lookup still runs after it.
- `src/native_crash/mod.rs`: at the next start (`crash_log::begin_run`)
  `promote_previous` turns a non-empty active report into `crash-<ts>.txt`
  (and the dump into `crash-<ts>.dmp`). The existing notice already reveals the
  newest crash file, so "Show log" now opens the native report; no new UI text.
  The generic "closed without finishing" report is skipped when a native one
  exists. Only the newest `KEEP_CRASH_FILES` reports (and their dumps) stay. A
  clean quit deletes the active files.
- - `crash_log::begin_run` alone installs nothing, so unit tests never install signal handlers.

## Limits

`sigaltstack` is per thread: the main thread and threads Rust spawns have a
stack with a guard page, so a stack overflow is reported; a thread created by
native code without an alternate stack runs the handler on its own stack.
`backtrace()` is not formally async-signal-safe; it is warmed up at install so
the first call does not load libraries. Linux without glibc (musl) writes the
report without a backtrace. On Windows a stack overflow leaves the filter only
the stack Rust reserves for it; the filter does a few atomic operations and two
kernel calls, and if it cannot run, the process dies without a report as before.

## Verified

- Unit tests: promotion, empty report, rotation with dumps, clean-quit cleanup,
  and the notice wiring (`begin_run_for_session` then `pending_crash_before`
  returns the native report and writes no second report).
- `tests/native_crash.rs`: a child process (the test binary itself) installs the
  handler and raises SIGSEGV; the parent asserts it died of SIGSEGV and the file
  has the signal name and a backtrace. Windows analog (access violation, expects
  the exception code and a non-empty minidump) runs in CI only.
- Not verified locally: Linux and Windows builds (pushed to CI), an interactive
  crash of the real app.
- Review fixes (#880): `tests/native_crash.rs` children set `RLIMIT_CORE` to 0
  (Unix) or `SetErrorMode(SEM_NOGPFAULTERRORBOX | SEM_FAILCRITICALERRORS)`
  (Windows), and the parent polls with a 30 s deadline and kills a hung child.
  New Unix tests: a real invalid write dies of SIGSEGV with `fault address: 0x10`
  in the report; two threads faulting at once still leave a complete report
  ("end of report"); a previously installed handler still runs.
- macOS reports `raise`d signals with a hardware-looking `si_code` and `si_addr` 0, so there a fault at address 0 is re-raised instead of re-executed (the process still dies of the signal; only the faulting-frame detail is lost for null dereferences).
