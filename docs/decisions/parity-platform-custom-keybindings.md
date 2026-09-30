# Customizable key bindings

Slice `parity:platform-custom-keybindings`. Users can rebind 16 shortcuts (focus, search, formatting, media viewer) via a "Keyboard shortcuts" section in the Appearance dialog. Changes apply immediately, persist to `prefs.json`, and are re-applied at startup.

## Decisions

- Window-chrome and app-lifecycle bindings (quit, close, minimize, fullscreen) are not rebindable. Rebinding those risks stranding the user with no way out. Only the 16 user-facing actions are in `REBINDABLE_ACTIONS`.
- `KeyBinding::new` unwraps the keystroke parse (panics on invalid input), so `keybinding_for` validates with `Keystroke::parse` first. A corrupt saved pref falls back to defaults and never panics startup.
- An override replaces all default keystrokes for its action (for example both cmd-1 and ctrl-1 for Focus chat list). One custom keystroke per action.
- The capture UI uses a focused div with `on_key_down`. `Keystroke::to_string()` gives the canonical keystroke string. Escape cancels capture.
- Default pairs longer than 20 characters (currently only Previous search result, `cmd-shift-g / ctrl-shift-g`) stack on two lines so the label does not paint under the keystroke chip.
- Startup application is a one-shot in `poll_live` when the live driver (and its prefs paths) is first ready. `bind_keys` in main.rs runs before prefs are loadable.
- `apply_custom_bindings` does not wipe the whole keymap. It snapshots `cx.key_bindings()`, keeps non-Quill bindings (action name not under the `quill_ui::` namespace — kit's List/command-palette keys), then clears and re-adds the kept bindings plus the Quill set.
- Custom keystrokes colliding with a fixed binding (cmd-q, cmd-w, cmd-m, f11, …) are rejected and fall back to defaults, since GPUI resolves same-keystroke ties later-added-wins.

## Tests

`keybinding_for_valid_id_and_keystroke`, `keybinding_for_unknown_id_is_none`, `keybinding_for_invalid_keystroke_is_none`, `rebindable_ids_are_unique` (UI tests, run with the ui feature).

## Out of this slice

None identified.
