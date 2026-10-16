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

- `src/native_crash/unix.rs`: `sigaction` for the five signals, `SA_ONSTACK`
  with a 256 KiB `sigaltstack`, `SA_RESETHAND | SA_NODEFER`. The handler uses
  `write(2)`, `time(2)`, `backtrace(3)`, `backtrace_symbols_fd(3)` and `raise(3)`
  on a file descriptor opened at startup (`logs/native-crash-active.txt`); the
  header (version, OS, pid) is built at install. It writes the signal name, fault
  address, time, header and backtrace, then re-raises with the default action so
  the OS crash reporter still runs. A second fault inside the handler skips
  straight to the default action.
- `src/native_crash/windows.rs`: `SetUnhandledExceptionFilter` writing the
  exception code, address and a `MiniDumpWriteDump` minidump, then chaining to the
  previous filter. Adds the `Win32_System_Diagnostics_Debug` and
  `Win32_System_Kernel` windows-sys features.
- `src/native_crash/mod.rs`: at the next start (`crash_log::begin_run`)
  `promote_previous` turns a non-empty active report into `crash-<ts>.txt`
  (and the dump into `crash-<ts>.dmp`). The existing notice already reveals the
  newest crash file, so "Show log" now opens the native report; no new UI text.
  The generic "closed without finishing" report is skipped when a native one
  exists. Only the newest `KEEP_CRASH_FILES` reports (and their dumps) stay. A
  clean quit deletes the active files.
- The handlers are installed from `begin_run_with_native_handlers` (called by the
  crash notice setup), not from `begin_run`, so tests never install signal handlers.

## Limits

`sigaltstack` is per thread: only the main thread survives a stack overflow.
Faults on other threads run the handler on their own stack. The handler replaces
Rust's own stack-overflow message. `backtrace()` is not formally async-signal-safe;
it is warmed up at install so the first call does not load libraries. Linux
without glibc (musl) writes the report without a backtrace.

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
