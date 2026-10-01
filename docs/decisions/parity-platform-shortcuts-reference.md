## Slice parity:platform-shortcuts-reference — KEYBOARD SHORTCUTS REFERENCE (2026-09-30)

- **Built:**
  - `src/ui/keybindings.rs`: the hardcoded `bind_keys` list is now generated
    from a single `shortcut_rows()` table (`ShortcutRow { keystroke, label,
    section, binding }`) — the same rows the app binds are the rows the
    reference dialog renders, so the reference can never drift. Sections:
    General / Navigation / Search / Media viewer / Composer.
  - `src/ui/shortcuts.rs` (new): read-only kit `Dialog` (`DialogKind::Shortcuts`,
    `window.open_dialog` pattern) rendering the rows grouped by section with
    kit `Kbd` chips; labels sharing bindings (`cmd-q`/`ctrl-q`) render as one
    entry with both chips. Single Close button in the footer; Esc/backdrop/✕
    clear `shortcuts_open`.
  - New `OpenShortcuts` action; Help menu gains "Keyboard Shortcuts" above
    "Quill on GitHub"; `DialogKind::KINDS` 36→37.
  - README `parity:platform-shortcuts-reference` checked.
- **Also in this PR:** the checklist item `parity:bots-streaming-draft-stop`
  ("Stop button for streaming bot drafts") is a real, unimplemented TDLib
  1.8.67 feature — the schema's draft/pending-message surface
  (`sendTextMessageDraft`/`sendRichMessageDraft` with `can_stop`, plus
  `updatePendingMessage`, `updateStopMessageDraft`, `stopPendingMessage` in
  `schema/td_api.tl`). The earlier schema search only matched the word
  "stream" (video/RTMP/group-call streaming) and missed it because TDLib
  describes the concept with "draft"/"pending message" vocabulary. The item
  is NOT a phantom: the unchecked checklist line is restored (denominator
  stays 525), and implementing the feature itself is out of this slice's
  scope.
- **Key decisions (ponytail):** no new binding for the dialog itself — Help
  menu entry is the standard discoverability path; no display-string field in
  the table — kit's `Kbd` renders the keystroke with platform glyphs; no
  separate grouping struct — `shortcut_rows()` is written in section order so
  first-seen grouping is the display order.
- **Out of this slice:** `parity:platform-custom-keybindings` (rebinding UI);
  showing context-gated shortcuts' active context (viewer-only, composer-only).

