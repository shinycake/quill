# Refactor 3b: composer, message, playback, viewer, search and share state

Part 3b of the structure refactor (3a: `codex-refactor-3.md`). Pure refactor,
no behaviour change.

## What moved

184 `QuillApp` fields moved into nine structs, each in its own
`ui/<feature>_state.rs`:

| `QuillApp` field | Struct | Module | Fields | What it holds |
| --- | --- | --- | --- | --- |
| `composer_ui` | `ComposerUi` | `composer_state.rs` | 45 | `/` and `@` menus, suggestions, inline results, attachments, send options, link preview, scheduling, reply and edit drafts, drop zones, poll and checklist boxes |
| `spell` | `SpellUi` | `spell_state.rs` | 7 | spell checker, results, dictionaries box |
| `recording` | `RecordingUi` | `recording_state.rs` | 9 | voice and round video recording |
| `message_ui` | `MessageUi` | `message_state.rs` | 27 | message menu, selection, links, keyboards, spoilers, sponsored rows, poll voters, fact check |
| `playback` | `PlaybackUi` | `playback_state.rs` | 39 | voice, music, GIF, sticker and inline video playback, speed and volume |
| `pickers` | `PickerUi` | `pickers_state.rs` | 7 | the emoji/sticker/GIF panel and its search fields |
| `viewer` | `ViewerUi` | `viewer_state.rs` | 36 | media viewer, zoom, video, controls, photo editor |
| `search_ui` | `SearchUi` | `search_state.rs` | 2 | global search and search in chat fields |
| `share` | `ShareUi` | `share_state.rs` | 12 | forwarding and the share box |

`QuillApp` went from 410 fields (after 3a) to 235. `ui/app.rs` went from
1,150 to 722 lines and `ui/app_demo.rs` from 1,283 to 1,088.

Field names lose the prefix the struct now carries: `composer_silent` is
`composer_ui.silent`, `viewer_zoom` is `viewer.zoom`, `media_viewer` is
`viewer.state`, `playback_speed` is `playback.speed`, `spellchecker` is
`spell.checker`. Fields without a prefix keep their name
(`message_ui.selection_anchor`). Existing feature structs stay whole and are
nested: `suggest` (in `composer_ui`), `message_menu_ui` (now
`message_ui.menu_ui`), `media_panel` (in `pickers`), `player` (in
`playback`). Types, doc comments and initial values are unchanged.

`composer_ui` and `search_ui` carry the `_ui` suffix because `composer` is
the composer text field and `session.search` already exists. The other
names are free in `ui`.

## How it was done

A script did the whole move, so it can be rerun on a newer `main` when the
PR conflicts:

1. It reads the field list per struct, cuts each declaration (with its doc
   comment) out of `QuillApp` and each initializer out of the
   `new_with_demo` literal, and writes `ui/<feature>_state.rs` with the
   struct and a `new`.
2. A text field built by a `let x = cx.new(...)` that nothing else in
   `new_with_demo` reads moves into `new`, which then takes
   `window, cx`. Fields whose local is also read by a subscription there
   (the search fields, the reaction search) stay where they are and are
   passed to `new`. So does the shared audio output.
3. It rewrites every `.old_name` field access under `src/ui` to
   `.feature.new_name`, leaving method calls with the same name alone.
4. It runs `cargo check --features demo-capture --all-targets` and reverts
   each rewrite the compiler rejects with "no field `feature` on type T":
   another type with a field of the same name (57 sites: `.audio`,
   `.player`, `.suggest`, `.selection_*` on other structs). One round left
   no errors.
5. `cargo fix` and a pass over the unused-import warnings tidy the imports.

A rewrite could only go wrong silently if some other type had both the old
field and a field named like the new struct field. No struct outside
`QuillApp` declares a field called `composer_ui`, `spell`, `recording`,
`message_ui`, `playback`, `pickers`, `viewer`, `search_ui` or `share`, so
that cannot happen. The only rewrites in code macOS does not compile are
`self.spell.checker` in a `cfg(not(target_os = "macos"))` demo branch and
two test lines under `demo-capture`; both read `QuillApp`.

## Side effects

- Text fields that moved into a struct's `new` are now created when the
  `Self { .. }` literal reaches that struct, a little later than before.
  Their entity ids change. Nothing depends on creation order.
- Fields drop in a different order when `QuillApp` drops (at quit). The
  `Drop` impl still stops GIF and sticker playback first.

## Verification

Gate (fmt, core and UI clippy, core and UI tests): core 3019, UI 234, the
same as `main`.

## Adding state now

Put a new field in the feature's struct and its `new` and use it as
`self.<feature>.<field>`. The change stays in `ui/<feature>_state.rs`. See
"How to add feature state now" in `codex-refactor-3.md`.
