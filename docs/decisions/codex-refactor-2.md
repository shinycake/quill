# Dialog registry (refactor 2)

## Problem

Every kit dialog was listed four times in `src/ui/shell.rs`: a `DialogKind`
enum variant, an arm in `dialog_is_open`, an arm in `dialog_builder`, and a
slot in the `KINDS` priority list. Any two feature PRs that added a dialog
touched the same four places, so they always conflicted.

## What changed

- `DialogKind` is now a `Copy` newtype around the kind's name. The variants are
  gone. Each kind is an associated const (`DialogKind::Settings`, ...), so the
  ~170 call sites (`on_close_kind`, `close_kit_dialog_if_done`) did not change.
  `Debug` still prints only the name.
- Each dialog module registers its own kinds with `register_dialogs!`, usually
  at the end of the file that defines the builder. The macro adds the
  associated const and submits a `DialogSpec` to a link-time `inventory`
  collection. A spec holds an explicit priority, the open-flag check and the
  builder.
- The shell sorts the collected specs by priority once (`OnceLock`) and scans
  them in that order on each render. The first open flag still wins, and the
  passcode lock still hides every dialog.
- 73 dialogs across 50 files. `NotificationDefaults` is registered in
  `settings_ui.rs` for now, because `notification_settings.rs` is being split
  in a parallel PR. Its entry can move next to the builder once that split
  lands.

## Choices

- **Same crate and pattern as the demo registry (#621).** That is `inventory`
  0.3, an optional dependency behind the `ui` feature (MIT OR Apache-2.0, and
  already in the lockfile through GPUI). One `macro_rules!` expands to one
  `::inventory::submit!` per spec, and each spec is built with a `const fn`.
  The `Cargo.toml` hunk is the same as #621's, so the two branches merge
  cleanly.
- **Priorities are numbers with gaps.** Today's `KINDS` order became 100, 200,
  ..., 7300. Lower wins. A new dialog takes an unused number between its
  neighbours, so nothing else gets renumbered.
- **Duplicate names fail at compile time.** Two entries with the same name
  define the same associated const twice. Duplicate priorities fail a test.
- **Kinds stay named consts, not strings at call sites.** If a dialog is
  removed, every leftover `DialogKind::X` still fails to compile, as it did
  with the enum.

## Tests

- `registry_keeps_the_legacy_kinds_order`: every kind from the old `KINDS`
  list is registered, in exactly that order. Today the registry and the list
  are equal. The list is frozen: a new dialog can take any free number without
  editing it, and the test still fails if a change moves an existing dialog.
- `dialog_priorities_and_names_are_unique`.
- `dialog_kind_debug_prints_the_bare_name`.

Gate before: core=2990, ui=224. After: core=2990, ui=227 (+3 tests above).

## Cost

This change has no performance goal, so I took no timings. Per render the shell
still does one linear scan over 73 open-flag checks. The checks are now
indirect calls through `fn` pointers rather than `match` arms. The sorted list
is built once and holds 73 pointers (584 bytes).

## How to add a dialog now

1. Write the builder in your dialog module, as before:
   `fn build_foo_dialog(app: &Entity<QuillApp>, shell: &Entity<QuillShell>, dialog: Dialog, cx: &mut App) -> Dialog`.
2. In the same file, at module level and above any `#[cfg(test)] mod tests`,
   register it:

   ```rust
   crate::ui::shell::register_dialogs! {
       /// What the dialog is (becomes the doc of `DialogKind::Foo`).
       Foo => DialogSpec::new(
           // Explain the priority if it is not obvious.
           4150,
           |app| app.foo_dialog.is_some(),
           QuillApp::build_foo_dialog,
       ),
   }
   ```

   `DialogSpec` is in scope inside the macro, so you need no import. Put
   several dialogs in one block by separating the entries with commas.
3. Pick the priority. Lower numbers win when several open flags are set. Find
   the dialog yours should sit just after, and use a free number between it
   and the next one. To list the current numbers, run
   `grep -rn -A2 "=> DialogSpec::new" src/ui`.
4. Use `DialogKind::Foo` with `QuillShell::on_close_kind` and
   `close_kit_dialog_if_done`, as before.
