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

## First-frame render builders

After the launch fix, the deepest point was the first render. In a debug build `folder_tabs` and `sidebar` took 315 KB and 187 KB of stack (the 323 KB and 192 KB above are in thousands of bytes; from here on KB means 1024 bytes). The demos needed a 912 KB main thread, and `ready-info-group` needed 976 KB.

### Why the frames were large

At opt-level 0, rustc gives every by-value step of a builder chain its own stack slot, and that slot lasts as long as the function. Nothing shares slots and nothing is inlined. So a builder function reserves the sum of every chain it writes, on entry, even when it returns `None` on its first line. Measured sizes in this build:

| Value | Size |
|---|---|
| `Div`, `Stateful<Div>` | 1,360 B |
| `StyleRefinement` | 640 B |
| `Button` | 6,760 B |
| `Tab` | 4,352 B |
| `TabBar` | 11,584 B |
| `AnyElement` | 24 B |

A plain `Button::new(..).label(..).ghost().on_click(..)` costs about 30 KB of its caller's frame. `folder_tabs` built the `TabBar`, every `Tab`, the fades, the "more" menu button and the manage button in one function, and the sizes added up.

Frames only add up along one call chain. A helper that builds part of an element and returns an `AnyElement` frees its frame before the caller moves on to the next child. Five sibling chains then cost the largest one, not the sum of all five. GPUI renders views lazily in `prepaint_view`, so `QuillApp::render` (204 KB) does not sit under its child views.

### What changed

Three patterns, all in Quill code. Nothing drawn changed.

- **Split builders.** `folder_tabs` builds its `TabBar` in `folder_tab_bar` and its menu and manage buttons in their own functions. `sidebar` (moved to `chat_list/sidebar.rs`) hands each mode to its own builder. The conversation header's identity and actions live in `conversation/header_view.rs`. The selection bar builds each button in `selection_bar.rs`. The composer's option chips, the group panel's identity block and invite-link rows, the bot card's command buttons and the pinned bar's buttons are also split out.
- **Guard splits.** A builder that usually shows nothing (a dialog, an overlay, a bar) keeps its cheap check in the outer function and does its building in `<name>_body`, marked `#[inline(never)]`. That marking is only on per-frame builders, never on per-row ones. Frames where the dialog is closed skip the big frame. This was applied to 43 builders, among them `photo_editor_overlay` (349 KB), `poll_dialog_panel` (298 KB), `bot_info_panel` (419 KB) and `share_content_panel` (176 KB).
- **Button helpers.** `ui/lean_buttons.rs` builds the common ghost buttons (text, icon with tooltip, icon without) and returns an `AnyElement`. The group admin sections, the event log, the bot card and the pinned list use them, and `group_management_section` builds each of its rows in `management_row` / `management_toggle_row`. These helpers and the per-message corner buttons carry no `#[inline(never)]`. A debug build never inlines them anyway, and a release build may inline them as before, so per-row code costs the same in release.

### Numbers

All numbers are from macOS arm64, a debug `demo-capture` build, at 1200x800.

- **Frame sizes:** read from each function's prologue (`sub sp`), the same in every run.
- **Main-thread stack:** the smallest `RLIMIT_STACK` that still launches the demo and writes its capture, bisected in 16 KB steps. macOS sizes the main thread's stack from `RLIMIT_STACK` at exec.
- **Test thread:** the smallest stack the new test passes with.
- **Deepest chains:** found with lldb breakpoints on every function entry over 16 KB, recording `sp`. They were measured before the changes and during them, but not on the final build, because lldb could no longer attach.

Own frame of each builder (KB):

| Function | Before | After | Body (when shown) |
|---|---|---|---|
| `folder_tabs` | 315 | 35 (+ `folder_tab_bar` 91) | |
| `sidebar` | 187 | 27 (largest mode builder 40) | |
| `composer_options_row` | 207 | under 16 | |
| `with_selection_bar` | 160 | under 16 (+ `selection_bar` 36) | |
| `session_history` | 122 | 52 | |
| `conversation_header` | 76 | 41 | |
| `message_corner_actions` (per row) | 70 | 17 | |
| `bot_info_panel` | 419 | under 16 | 141 |
| `invite_links_section` | 376 | 84 (+ row 81) | |
| `join_requests_section` | 255 | 123 | |
| `event_log_section` | 215 | 109 | |
| `administrators_section` | 210 | 78 | |
| `group_management_section` | 195 | 90 | |
| `inline_members_section` | 192 | 74 | |
| `pinned_message_banner` | 178 | under 16 | 65 |
| `supergroup_info_panel` | 127 | 83 (+ identity 45) | |
| `photo_editor_overlay` | 349 | under 16 | 349 |
| `poll_dialog_panel` | 298 | under 16 | 298 |

Smallest stack the first frame draws with (KB):

| Demo | Main thread before | Main thread after | Test thread after |
|---|---|---|---|
| `ready-chats` | 912 | 736 | 688 |
| `ready-showcase` | 912 | 736 | 688 |
| `ready-bot-chat` | 912 | 736 | 688 |
| `ready-media` | 912 | 736 | 688 |
| `ready-info-group` | 976 | 800 | 704 |
| `ready-settings` | 656 | 656 | 608 |

Before the change, the test thread needed 864 KB for `ready-chats`. `ready-settings` did not move, because its deepest point is in the kit's `Button`.

Captures of all six demos, light and dark, decode to the same pixels before and after. The one exception is `ready-showcase`: two runs of the unchanged build already differ there, in the scrolled history area. Its hover corner buttons, which this change rebuilt, match pixel for pixel.

### The regression test

`the_first_frame_draws_within_the_stack_budget` (`src/ui/stack_budget_tests.rs`) opens the main window on five demos (`ready-chats`, `ready-showcase`, `ready-bot-chat`, `ready-media`, `ready-info-group`). Each runs on a 768 KB thread, draws one frame, and checks that the conversation slot was laid out. The build before this change overflows that thread on `ready-chats`. When the test fails, split the builder that grew; don't raise the budget.

### What is left

On `ready-chats` the deepest point is no longer in Quill code. It is in GPUI and the kit:

- GPUI's message `List` starts laying out its rows about 315 KB down: one prepaint level per nested view and div, about 13 KB each.
- Each div level inside a row adds about 12 KB in `request_layout`.
- The kit's `Button::render` takes 152 KB, plus 58 KB in one of its closures, at the bottom of a message row.

Two ways to go lower, neither taken here:

- **Build dependencies at opt-level 1 in dev.** Tried with a test profile: GPUI's per-level cost dropped by about 40% (the conversation pane's render started 125 KB down instead of 205 KB), and the deepest point of the unchanged code fell from 872 KB to 704 KB. A clean debug build went from 4:25 to 5:43. Incremental builds are not affected, but the change touches every developer's setup, so it is a project decision of its own.
- **Split the kit's `Button::render`** in the vendored `third_party/gpui-component`. This would save up to about 150 KB per button. It would be one more patch to carry in `QUILL-CHANGES.md` on every kit update.

Frames that only run when something opens were left alone: `message_menu_overlay` (784 KB) and `story_page_overlay` (734 KB) run only while a menu or a story is open, on an 8 MB main thread. `session_chat_row` (142 KB per chat row) is a per-row builder, so it was left as it is rather than broken into more calls.
