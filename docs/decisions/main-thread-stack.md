# The main thread's stack on Windows

Windows gives the main thread 1 MB of stack, and the debug `windows-launch-smoke` build runs on it. PR #1082 (the QR code box) made every smoke demo exit with "thread 'main' has overflowed its stack" (0xC00000FD). main passed the same job.

## Why

A debug build keeps several copies of `QuillApp` on the stack while the first window opens: in `new_with_demo`, which builds it, and in the closures that hand it to `cx.new`. `QuillApp` was 109 KB, and two fields made up 74 KB of it:

- `live: Option<LiveConnect>`, 38 KB (the driver holds a `Session`);
- `demo_session: Option<Session>`, 36 KB.

main already sat within a few KB of the limit. The QR box added about 2.5 KB across those frames (`PeerQrUi` is 432 bytes, plus a few more in `Session`), which was enough to tip it over.

## What changed

`demo_session` is `Option<Box<Session>>`. Callers that need a plain `&Session` or `&mut Session` use `as_deref` or `as_deref_mut`. Everything else derefs through the box unchanged. `QuillApp` is now 73 KB.

`src/ui/stack_budget_tests.rs` bounds `QuillApp` at 80 KB and `Session` at 40 KB, so the next large value held by value fails a test instead of the Windows launch. The fix for a failure is to box the new value, not to raise the stack size.

## Numbers

Measured on macOS, debug `demo-capture` build. macOS sets the main thread's stack from `RLIMIT_STACK` at exec, so a capped limit stands in for Windows' 1 MB. Frame sizes come from lldb: the stack-pointer difference to the caller, at the overflow.

| | main (d7a44918b) | PR before | PR after |
|---|---|---|---|
| `QuillApp` | | 109,048 B | 73,128 B |
| `new_with_demo` frame | 484,992 B | 486,544 B | 342,896 B |
| `cx.new` closure for `QuillApp` | 217,664 B | 218,496 B | 146,656 B |
| Smallest stack `ready-chats` launches with | over 1000 KB | about 1005 KB | 928 KB or less |

On macOS, main itself overflows with a 1000 KB stack, so it only fits Windows' 1 MB because MSVC frames are smaller.

## What is left

The deepest point is now the first render, not the launch: `folder_tabs` takes 323 KB and `sidebar` 192 KB in a debug build. Boxing `live` would take another 38 KB off each `QuillApp` copy. Neither is needed to pass today. They are the next places to look if the stack gets tight again.
