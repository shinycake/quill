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

## Session's domains (follow-up)

A screenshot demo launched with a 1 MB stack on macOS crashed in `new_composer_field` on 2026-10-21, inside `new_with_demo`. `Session` was still 36 KB, and every copy of `live` carries one through its driver. `Session` now boxes its twelve largest domains (`payments`, `groups`, `settings`, `messages`, `media`, `stickers`, `stories`, `bots`, `calls`, `threads`, `chats_state`, `chat_list`). Field access derefs through the box, so no caller changed.

| | after #1082 | Session boxed |
|---|---|---|
| `Session` | 35,800 B | about 3.6 KB |
| `QuillApp` | 73,128 B | about 40 KB |
| Stack to build the app in a debug test thread | 640 to 768 KB | 384 to 448 KB |

The size bounds are now 48 KB for `QuillApp` and 8 KB for `Session`. `app_builds_within_the_stack_budget` builds the app on a 576 KB thread; main before this change overflowed it.

## Windows main stack

`build.rs` links `quill.exe` with an 8 MB main stack (`/STACK` on MSVC, `--stack` on GNU), the size macOS and Linux give the main thread. The launch fits in 1 MB again, but the first frame does not have much room: in a debug build `folder_tabs` takes 323 KB and `sidebar` 192 KB, and GPUI draws the whole tree on the main thread. The size bounds above still fail on the next large field, so the rule stays: box the value, don't count on the larger stack.

## What is left

The deepest point is the first render: `folder_tabs` (323 KB) and `sidebar` (192 KB) in a debug build. Those frames come from GPUI's by-value builders (`TabBar`, `Tab`, `Div`), not from Quill's state.
