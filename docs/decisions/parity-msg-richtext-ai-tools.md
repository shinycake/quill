## Slice parity/msg-richtext-ai-tools (2026-09-30)

**Scope:** `parity:msg-richtext-ai-tools` — AI tools in the rich-text composer
(`composeTextWithAi`, `composeRichMessageWithAi`, `createRichMessageWithAi`,
`fixTextWithAi`, `fixRichMessageWithAi`; schema 1.8.67).

- **Built:**
  - Request builders in `src/telegram/requests/messages.rs` for all five
    methods, with envelope parsing for the `fixedText` / `formattedText` /
    `richMessage` answers.
  - Driver methods on `ConnectDriver` (`src/connect/messages.rs`). All five
    refuse in secret chats and reject an empty draft client-side. Answers
    stash into `Session::ai_composer_text` / `ai_composer_blocks`; the UI
    drain replaces the open chat's draft (rich blocks flatten via
    `copy_text()`). A late answer for a different chat is dropped.
  - Rich-editor bar ghost buttons (`src/ui/composer.rs`): **✨ Fix**
    (`fixTextWithAi`), **✨ Rewrite** (`composeTextWithAi`), **✨ Create**
    (`createRichMessageWithAi`), **✨ Fix rich** (`fixRichMessageWithAi`),
    **✨ Rewrite rich** (`composeRichMessageWithAi`). The rich pair parses
    the draft with the same markup parser as the live preview.
  - Failures land in the status note. `AICOMPOSE_FLOOD_PREMIUM` reads
    "AI limit reached — Telegram Premium is required for more requests".
    Nothing is silent and nothing reports fake success.
- **Key decisions:**
  - Secret-chat refusal covers all five methods. The schema says
    `fixTextWithAi` and `composeTextWithAi` must not be used in secret
    chats; the other three get the same refusal so an end-to-end draft is
    never sent to a server-side model.
  - Rewrite sends the schema's documented no-op defaults: empty
    `translate_to_language_code` (no translation), empty `style_name`
    (keep the current style), `add_emojis: false`. This slice has no
    translate / style / emoji picker.
  - Create sends `language_code` from
    `session.language_prefs.system_language_code`. The schema documents no
    empty-string default for that parameter, so a real code is always sent.
  - `fixedText.diff_text` is not parsed. Nothing renders a diff, so the
    field is not stored.
  - The button row wraps (`flex_wrap`) so the AI buttons stay on screen
    next to the block buttons.
- **Proof:** `docs/screenshots/ready-rich-ai-tools.png`, captured from a
  real GPUI window (`quill --screenshot-demo ready-rich-ai-tools`). Injected
  Ready session, no live Telegram. Live AI round-trips need credentials
  this environment does not have; the driver path is covered by
  `src/connect/tests/ai_tools.rs`.
- **Out of this slice:**
  - Translate, style, and emoji pickers for the compose methods.
  - A diff view of `fixedText.diff_text`.
  - Premium gating of the rich-text editor itself
    (`parity:msg-richtext-premium-gate`).
