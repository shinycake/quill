# gpui-kit 0.11.0

gpui-kit, gpui-base and gpui-component go from `=0.10.0` to `=0.11.0`, and
every gpui-pre crate goes from `=0.3.12` to `=0.3.14`. All seven vendored GUI
crates were re-vendored: gpui-pre, gpui-pre-apple, gpui-pre-macos,
gpui-pre-linux, gpui-pre-wgpu, gpui-pre-windows, gpui-base and gpui-component.
The kit's new `x11` feature (on by default upstream) is named in Quill's
feature list, since Quill turns the kit's defaults off. The vendored
gpui-kit skill moves to the v0.11.0 tag.

## What tdesktop does

Nothing relevant. This is Quill's own dependency policy
(`docs/dependency-updates.md`).

## What changed

### Vendored crates

Each crate gets two commits: "Vendor the unmodified … 0.3.14 registry copy of
…" (or "Vendor gpui-base and gpui-component 0.11.0 unmodified from
crates.io"), then "Reapply Quill's patch to …", a three-way apply of Quill's
diff against the old registry copy. Each crate's `QUILL-CHANGES.md` names its
pristine commit and has a 0.3.14 (or 0.11.0) note.

gpui-pre 0.3.14 rebuilt the scene, and that is where most of the work was:

| Crate | What had to change |
|---|---|
| gpui-pre | A layer's primitives now enter the bounds tree when the outermost layer closes, primitives that share an order draw by kind, paths are curves binned into tiles (`PaintedPath`), sprites carry an animation slot and native views are a primitive. `scene.rs` is the 0.3.14 file with Quill's changes written in again: the bounds tree keeps `insert_span`/`insert_band` on the new `(ordering, kind)` keys, recorded primitives keep their order for `replay_as_one`, a lens is a kind of its own, and a glass label's ink shares the sprite's animation slot. Backdrop glass hashes and maps tiled paths (`PathTiles::mapped`). |
| gpui-pre-apple, -wgpu, -windows | Upstream draws opaque quads first, in a depth pre-pass. A frame with glass skips that pass: a lens batch ends the render pass (and Metal's memoryless depth with it), and opaque quads drawn ahead would show in the backdrop a lens copies. The quad shader culls opaque rectangles only when the pre-pass ran. The 8-bit/EDR and scRGB wrappers cover the new opaque-quad, path-tile and native-view shaders. On DirectX, the HDR and NV12 surface pipelines keep a `draw_with_texture` of their own, since upstream's pipelines now draw from a batch start and these shaders index from zero. |
| gpui-pre-macos | Quill's on-demand `CAMetalDisplayLink` stays; upstream's native-view pointer check runs after each step. |
| gpui-pre-linux | Applied unchanged. |
| gpui-base | Upstream moved the input to a new row model (`f64` scroll, rows placed by `local_row_y` and aligned one by one by `row_x`) and added unshaped far rows, natural alignment (#4016) and width caches. Quill's engine (bidi rows, formatting spans, `row_wrap.rs`, the `LineWindow` long-line layout) sits on the new row model: a right-to-left paragraph aligns its rows to the right edge, and rows a long line leaves out count toward row offsets (`LineLayout::row_count`). Upstream's unshaped rows, natural alignment and width caches are left out, since `LineWindow`, the per-paragraph bidi direction and `row_wrap.rs` already do that job; its single-pass line walk, `f64` scroll and `prepare_wrap_width` are kept. |
| gpui-component | Applied unchanged. |

Patches dropped because upstream now does the same: the shadow
`painted_bounds` margin (a shadow's `bounds` is now its painted area), the
in-place primitive sort (upstream's `sort_in_place`; Quill keeps only the
reused key buffer), the duplicate `Window::display_id` (upstream caches the
same id), the macOS notification worker thread (upstream posts from its own
worker; the notification extras ride on its commands), and wgpu's
instance-data split (`write_instance_data` is upstream's now).

One bug turned up in the carry. Under 0.3.14's deferred layers, a replayed
range pushed its primitives in array order, so a nested layer drew under what
its outer layer painted after it. `replay_as_one` now pushes them in the order
painting puts them in the scene (`deferred_layer_sequence`), and
`a_nested_layer_replays_above_what_its_outer_layer_paints_after_it` covers it.
The merge had also lost two closing braces from `shaders.metal`.

### API changes in Quill

- `IntoElement` is a blanket impl for every `Element`, so sixteen hand-written
  impls go.
- `Path` is no longer generic and is painted as a `PaintedPath`; the location
  picker's marker builds an `Option<Path>`, and the spoiler effect paints into
  a layer that takes no bounds.
- `dismiss_system_notification` returns a task, which callers detach; the
  interface-zoom window forwards the new native-view, title-bar and
  debug-state hooks.
- Business hours and scheduled times format with jiff's `strftime`
  (gpui-component moved its date components from chrono to jiff, #4015).
  chrono stays in the tree: gpui-pre itself still depends on it.

### Kit 0.11 features Quill now uses

- **Button expanded state** (#4035): the search's top-people header link (Show
  more / Collapse / Show all) reports whether the grid is expanded, and its
  test reads that from the accessibility tree.

These reach Quill without code changes, through the vendored crates:

- Kit notifications (crash notice, open failures, call notices) now sit above
  dialogs and sheets (#3577).
- A menu that fits no longer shows a scrollbar (#4014); Quill's menu,
  submenu-aim and glass patches sit on the same files.
- The input ends an IME composition on blur (#4032), redacts the IME's
  surrounding text in masked fields such as the cloud password (#4059), stops
  Ctrl+Delete at white space and line breaks (#4034), drops tabs from a
  single-line field as it drops newlines (#4071), and keeps less
  accessibility text-run memory on large drafts (#4090).
- Markdown and inline-flow speedups (#4029, #4058, #4063, #4087) reach the
  What's New view, Quill's only `TextView`.
- The scrollbar drags only on the primary button (#4038).

### Looked at, no use in Quill

- **Input suggestion list** (#3785, `<input list>`): Quill's completions are
  the composer's `@`, `#`, `:` and bot-command popups, which open above the
  composer with avatars and sections (`composer_suggest.rs`), and the country
  box, a dialog with its own list. Neither is a plain list under a field.
- **`SelectableText::reveal_range`** (#4052): message text is Quill's own
  selectable element (`selectable_text.rs`, styled runs, links, spoilers), and
  in-chat search highlights and scrolls to the whole message, so nothing asks
  for a range inside one.
- **Left-side vertical scrollbar** (#4078): Quill does not mirror its
  interface for right-to-left languages (see the 0.10.0 notes); scrollbars
  stay on the right.
- **`DockSkin`** (#4084) and **resizable drag state** (#4039): Quill uses
  neither Dock nor the kit's resizable panels; the chat-list column resize is
  its own (`shell_layout/`).
- **`Presence` reports Absent** (#4062): Quill's reveals use its own typed
  `Presence<T>` (`surface_reveal.rs`), which keeps the last value through the
  exit; the kit's is unused.
- Natural alignment of text-area paragraphs (#4016): see gpui-base above;
  Quill already lays out each paragraph in its own direction.
- Editor gutter style, folding API, diff spans, WebView fixes, shell plugins:
  components Quill doesn't use.
- Button expanded state on menu triggers: the kit's popover tells its
  trigger `open`, but a `Button` can't tell "a closed trigger" from "not a
  trigger", so the 53 `dropdown_menu` buttons can't report it without a kit
  change. Worth proposing upstream (owner's call, case by case).

### Questions from this round

- **x11rb 0.14**: held. gpui-pre-linux 0.3.14 and zed-xim still ask for 0.13,
  and Quill's own x11rb (record, screensaver) shares that copy.
- **ashpd / zbus**: unchanged. gpui-base, gpui-pre-linux, oo7, accesskit_unix,
  atspi and ksni still use ashpd 0.13 on zbus 5; Quill's own screencast portal
  is already on ashpd 0.14 (zbus 6), and Quill's direct zbus stays on 5 with
  ksni.
- **gpui-fast**: goes from 0.1.16 to 0.1.19 in the lockfile only. Quill does
  not turn the kit's `gpui-fast` feature on (see the 2026-11-11 evaluation).

## How verified

- Gate: fmt, both clippies, core and UI tests, license list.
- `cargo test -p gpui-base --features test-support --lib` (1,833 pass) and
  `cargo test -p gpui-pre-apple --lib` (45 pass, including the 11
  `metal_renderer` tests).
- The scene replay fuzz tests and backdrop-lens tests pass, with the new
  nested-layer case.
- Linux and Windows `cargo clippy --target … --features demo-capture` from
  macOS with stub C tools: Linux is clean; on Windows the vendored crates are
  clean, and the remaining warnings are in Quill code this change does not
  touch (`capture_access.rs`, `winreg.rs` and a few test files).
- All `scripts/check-*.py` and `check-file-size.sh`.
- Demo captures before (origin/main) and after, light and dark (listed below).

MEASUREMENTS
