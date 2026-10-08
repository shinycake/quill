# Composer polish: formatting shortcuts, Save/slow-mode send button, plain paste

Branch `codex/composer-polish` (second round; the first merged round was the one-row composer).

## tdesktop references

- Shortcuts: `lib_ui/ui/widgets/fields/input_field.h:40` (`kClearFormatSequence` ctrl+shift+n, `kStrikeOutSequence` ctrl+shift+x, `kBlockquoteSequence` ctrl+shift+., `kMonospaceSequence` ctrl+shift+m, `kEditLinkSequence` ctrl+k, `kSpoilerSequence` ctrl+shift+p) and `input_field.cpp:2041` (Bold/Italic/Underline are `QKeySequence::*`). Qt `ctrl` is Cmd on macOS.
- Format submenu with `\tchord` labels: `input_field.cpp:6095`.
- Send button: `ui/controls/send_button.cpp` (`setState` cross-fade over `st::universalDuration` = 120 ms, `paintSlowmode` "m:ss"), type choice in `history/history_widget.cpp:5944` (`computeSendButtonType`) and `:6733` (`updateSendButtonType`: slow-mode delay is zeroed for Save/Cancel/Stop), menu only for Send (`sendButtonMenuDetails`).

## Choices

- **Platform modifier.** The format defaults use only `cmd-` on macOS and `ctrl-` elsewhere (`primary!` macro). The old `ctrl-b/i/u` bound Ctrl on macOS, where Ctrl+B/I/U are text-field editing keys. New rebindable ids: `format-strikethrough/-monospace/-blockquote/-spoiler/-clear`.
- **Monospace** is inline code for one line and a code block when the selection spans lines (tdesktop toggles code/pre the same way).
- **Cmd/Ctrl+K.** A fixed binding `ComposerEditLink` is scoped to key context `QuillComposer > Input` (wrapper div around the Textarea). It ties with the unscoped quick-switch binding on depth, and the later-added binding wins a tie, so `composer_bindings()` is added after the rebindable rows. Inside the composer with a selection it opens a link dialog; with no selection the handler runs quick switch itself (`link_chord_target`). A user-rebound quick-switch chord is unaffected. The link chord is not rebindable.
- **Link dialog.** Quill had none (only `[]()` insertion). A small inline panel above the composer (URL field, Enter/Save applies, Esc/Cancel closes) wraps the selection as `[text](url)`; a scheme-less address gets `https://`. The toolbar "Insert link" opens it when text is selected.
- **Menus.** The composer right-click native menu gains a Formatting submenu (labels carry `\t<chord>` like tdesktop, items disabled without a selection) and "Paste as Plain Text". It still goes through `deferred_input_menu` (spellcheck re-entrancy fix kept). The toolbar dropdown shows the chords through each item's `action`; its click handlers still apply the format directly because focus is on the menu.
- **Plain paste (Cmd/Ctrl+Shift+V).** Quill's composer holds markup text and the clipboard carries no entities, so "without formatting" means: insert the clipboard text only. Images/files never become attachments on this path, CRLF is normalized, and markers are inserted literally.
- **Send button.** One element (`src/ui/send_button_ui.rs`) replaces the separate record/send blocks. State choice and timeline are pure (`src/send_button.rs`). Save (check, tooltip "Save", no options menu) while editing; Slowmode shows `m:ss` (tooltip "Slow mode: 1 minute 5 seconds left"); Schedule (clock) when a send time is set and keeps the options menu so the time can be changed; Record keeps right-click mode flip. The old slow-mode banner is removed (tdesktop has none; the button carries the countdown).
- **Animation.** 120 ms linear cross-fade plus 0.5 to 1.0 glyph scale, fill fades between the accent circle and bare glyph. Driven by a monotonic timestamp and `request_animation_tick(60)`, requested only while animating. Only entering/leaving slow mode fades, not each second (tdesktop `hasSlowmodeChanged`). The kit `custom` button variant washes its fill, so it is used only during the fade; settled states use `primary()` / `ghost()`.

## Verified

- Unit tests: shortcut to entity mapping, monospace inline/block, Cmd+K target (`link_chord_target`), keymap resolution of every chord and of Cmd+K by key context (composer vs elsewhere, rebound quick switch), button-state selection, slow-mode formatting, morph timeline, link URL normalization.
- Demo-capture screenshots (normal, editing, slow mode) inspected: `/private/tmp/claude-501/composer/out/{ready-chats-composer,ready-edit-delete,ready-slow-mode}.png`.
- Not verified interactively (no input scripting in demo capture): real key delivery of Cmd+Shift+. on macOS, native menu tab rendering, the fade frames.
- Linux/Windows: no platform-specific code; chords use `ctrl-`, the in-window popup/native-menu fallbacks render the tab label.
