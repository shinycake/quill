# Refactor 1: screenshot demos register themselves

Part 1 of the structure refactor (fewer merge conflicts between parallel
feature PRs). Pure refactor: every demo kind, its fixture and its capture stay
the same.

## Problem

Adding one `--screenshot-demo` kind meant editing five shared places: the
`ScreenshotDemo` enum (`ui/screenshot_demo.rs`), `DEMO_TABLE` and the
ready-marker `match` in `main.rs`, the seed table in `ui/app_demo.rs` (often
a long or-pattern line), and a `demo_setup_*` function in `ui/demo_setup.rs`
or a new call in `new_with_demo`. Over 30 hours those files were touched by
38 to 50 merged PRs each and caused most of the conflicts.

## Decision

Each demo is a `DemoSpec` value that registers itself next to its setup
code:

```rust
register_demos![
    // Doc for the capture.
    DemoSpec::chats("ready-proxy", "screenshot demo — proxy settings")
        .setup(QuillApp::demo_setup_proxy),
];
```

`ui/screenshot_demo.rs` now holds only the registry: `DemoSpec` (kind, how
the app starts, setup fn, attachments, window title, tray, capture timing)
and `ScreenshotDemo`, a copyable handle to a registered spec
(`from_kind`, `all`, `named` in tests). `main.rs` reads everything from the
handle: the kind list for the error message, the marker name
(`.quill-ready-<kind>`, which matched the kind for all 280 demos), the window
title, tray and timings. `new_with_demo` calls `demo.start()`,
`demo.attachments()`, `demo.auth_inputs()` and, once the app is built,
`demo.setup(..)`.

Where the code lives now:

- A feature with its own module registers there (`share_demo.rs`,
  `viewer_demo.rs`, `proxy.rs`, `chatlist_rows_demo.rs`,
  `message_menu_demo.rs`, ...). Modules that serve several kinds keep one
  setup fn and a small private enum for the variant.
- Everything that lived in `demo_setup.rs` and the seed table moved to
  `ui/demos/<area>.rs` (signin, platform, composer, messages, chat_list,
  media, groups, security, calls, privacy_media, payments, stories,
  groups_admin, bots_profile), the same grouping as the old
  `demo_setup_<area>` functions. Each demo became one method; blocks that
  served several kinds became one method with a private enum. All files are
  under 1000 lines (largest: `privacy_media.rs`, 860).
- `demo_setup.rs` is gone; `app_demo.rs` lost the seed table, the
  attachment list, the sign-in fixtures and the setup call list (3031 to
  about 1410 lines). `main.rs` lost `DEMO_TABLE` and the marker match (1898
  to about 1300 lines). `screenshot_demo.rs` went from a 915-line enum to the
  registry (about 270 lines).

### Registry: `inventory`

`inventory` 0.3 (dtolnay, MIT OR Apache-2.0, both on the `deny.toml`
allow list) is already in the lockfile because GPUI uses it for action
registration, so this adds no new crate. It works on all shipped targets
(macOS, Linux, Windows). `linkme` (same licences) would be a new crate, and
each entry needs its own named static, which is noisier per demo. A
`build.rs` collector would need source scanning and would not see
`#[cfg]`s. `inventory` is an optional dependency behind the `ui` feature.

### Ordering

Setup order could only change behaviour for a kind that ran code in more
than one place. A script split every `demo_setup_*` body into its guarded
blocks: each kind hit at most one block, except `ready-channels-admin` (two
blocks, kept in their original order in one method). The sign-in fixtures
that used to run before `Self { .. }` was built now run in the setup, after
construction, before the first render; the pending attachments are still set
at construction. Nothing reads either in between.

### Small visible differences

None in the UI. The stderr line `quill screenshot-demo: <kind> → <dir>` now
prints the kind (`ready-chats`) instead of the old variant name
(`ReadyChats`). The unknown-kind error is byte-identical.

## Verification

- `every_earlier_kind_is_still_registered` (`ui/demos/mod.rs`): the 280 kinds
  from the old `DEMO_TABLE` (frozen in `ui/demos/kinds_before_registry.txt`)
  all resolve. New kinds need no entry there.
- `every_demo_kind_parses`, `demo_kinds_are_unique_sorted_and_listed`
  (`main.rs`), `kinds_are_unique`, `kinds_are_cli_names`
  (`screenshot_demo.rs`).
- Pixel comparison of every demo before and after: see the PR.
- Tests: CAPTURE_TEST_COUNTS.

## How to add a screenshot demo now

1. Write the fixture and setup in the feature's own module (or in
   `ui/demos/<area>.rs` if the feature has none). The setup is a
   `fn(&mut self, window: &mut Window, cx: &mut Context<Self>)` method, or a
   closure `|app, window, cx| ..` for anything else.
2. Register it in the same file:

   ```rust
   use super::screenshot_demo::{DemoSpec, register_demos};

   register_demos![
       // What the capture shows; env switches it reads.
       DemoSpec::chat_list("ready-my-feature").setup(QuillApp::demo_my_feature),
   ];
   ```

   `DemoSpec::chat_list(kind)` starts from the standard chat list with the
   default status note, `chats(kind, note)` with your note,
   `ready(kind, seed, note)` from another seeded session, and
   `signed_out(kind, || (status, note, auth))` on a sign-in screen. Add
   `.attachments(..)`, `.window_title(..)`, `.tray()` or
   `.timing(ready_ms, linger_ms)` when needed.
3. Run it: `quill --screenshot-demo ready-my-feature <dir>`.

No shared file changes: not `main.rs`, not `screenshot_demo.rs`, not
`app_demo.rs`.
