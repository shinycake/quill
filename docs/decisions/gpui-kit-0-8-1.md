# gpui-kit 0.8.1 and gpui-pre 0.3.10

gpui-kit, gpui-base and gpui-component go from `=0.8.0` to `=0.8.1`. The
default GPUI backend moves from the gpui-pre 0.3.9 snapshot to 0.3.10 (and the
optional, unbuilt gpui-fast crates to 0.1.12). This is the first GPUI snapshot
move since Kit 0.7.3, so all eight crates in `third_party/` were re-vendored.
The snapshot brings wgpu 30 (the Linux renderer) and taffy 0.15.

## What tdesktop does

Nothing relevant. This is Quill's own dependency policy
(`docs/dependency-updates.md`).

## What changed

### Vendored crates

One pristine commit (`6be84cd7`) adds the unmodified registry copies of all
eight crates. As before, it leaves out `.cargo-ok`, `.cargo_vcs_info.json`,
`Cargo.lock`, `Cargo.toml.orig`, and gpui-pre's `examples/` with their
`[[example]]` tables. The next commit three-way merges Quill's patch onto it.
The patch for each crate is the diff from its previous pristine copy to main.
The recorded pristine commits for gpui-base and gpui-component (`6d03a41e`,
`9f91f2c9`) were gone from history after a squash, so their 0.8.0 registry
copies were downloaded again and used as the base. For the six gpui-pre crates,
the recorded pristine commits were checked first: each one matches its 0.3.9
registry copy file for file.

| Crate | What had to change |
|---|---|
| gpui-base | `lib.rs` exports both upstream's new `last_horizontal_scroll_claim` and Quill's `ScrollBounceDisplacement`. In `element.rs`, upstream's new `request_reveal` sits next to Quill's `layout_bidi_token_row`. The display map gains both upstream's `set_tab_size` and Quill's `text_style`. In `text_selection.rs`, 0.8.1 splits multi-click carets into `unit_carets` and adds drag-extend by words and lines. Both read the run's geometry through `run.geo()`, as the rest of Quill's bidi patch does, so a right-to-left run extends by its real glyph positions. `TextSelectionRun::with_geometry` sets the new `triple_click` field. |
| gpui-component | Merged cleanly. The TabBar jump patch stays. |
| gpui-pre | Merged cleanly, including the list padding fix from 0.8.0. |
| gpui-pre-macos | 0.3.10 moves window code from `cocoa` to `objc2`. The idle display link, the occlusion handler (release idle drawables, visibility flag) and the floating window's tracking area were ported to the typed `native_window()` and `msg_send!`. `keyboard.rs` keeps reading input source ids and names lossily: upstream's `NSStringExt::to_str` still unwraps the UTF-8. |
| gpui-pre-windows | 0.3.10 checks a per-process validation number on GPUI's window messages (`handle_gpui_events`). Quill's `WM_GPUI_DESTROY_AFTER_DEVICE_LOST` is now posted with that number and goes through the same check, and an unknown id returns `None` instead of `unreachable!()`. `handle_device_lost` keeps Quill's retry and close instead of panicking, and also has upstream's null `lparam` guard. |
| gpui-pre-wgpu, -apple, -linux | Only file header comments conflicted. |

### GPUI API changes in Quill

- `ScrollWheelEvent`, `on_scroll_wheel` and `PlatformInput::ScrollWheel` are
  renamed to `ScrollEvent`, `on_scroll` and `PlatformInput::Scroll` (47
  files). Synthesized scroll events (agent driver, autoscroll, demo input,
  tests) say `source: ScrollSource::Pointer`.
- `Modifiers::secondary()` takes the `App`. Click and key handlers pass their
  `cx`.
- `Keystroke::parse` and `KeyBinding::new` take only static strings. User
  chords (`keybinding_for`, `parse_chord`) parse with `Keystroke::parse_with_os`
  and build through `KeyBinding::load(…, cx)`. `unparse` takes the OS.
  `src/ui/host_os.rs` names the OS Quill was built for (`HOST_OS`), for code
  without an `App`: chord parsing, the demo input script and tests. The tests
  that build a rebindable binding need an `App` now, so they run with
  `demo-capture` (`TestAppContext`), as the other UI tests do.
- `ZoomPlatform` forwards the new `Platform::platform_info`, scales the new
  `WindowInsets::corner_radii`, and maps `PlatformInput::BackGesture`'s start
  position into the zoomed space.

### Not adopted yet

These 0.8.1 additions are worth using in later work:

- Text selection publishes snapshots once per frame, and dropped `TextView`s
  release their shaped text on the next frame. Both apply automatically
  wherever Quill uses the kit's text selection.
- `Dialog::centered`, `Button::track_focus` with a caller-owned handle, and
  `TableDelegate::cell_label` for screen readers in data tables.
- `TextView` style overrides (`inline_code_style`, `list_style`,
  `blockquote_style`) and `image_source`.
- The kit deferred new `MotionTokens` fields and a Textarea associated type
  to 0.9, so expect those breaking changes next.

## How verified

See the PR for the gate result, screenshots and idle CPU numbers.
