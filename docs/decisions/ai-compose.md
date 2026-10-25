# ai-compose: Telegram Premium AI compose (`parity:premium-ai-compose-box`, `parity:premium-ai-compose-styles`)

## tdesktop
`boxes/compose_ai_box.cpp`, `ui/controls/compose_ai_button_factory.cpp`, `boxes/create_ai_tone_box.cpp`,
`boxes/preview_ai_tone_box.cpp`, `api/api_compose_with_ai.cpp`, `core/local_url_handlers.cpp` (`ShowAiStyle`),
`history/view/history_view_web_page.cpp` (the `telegram_aicomposetone` card).

- An "AI" button with two sparkles sits at the top of the attach column while the style list is
  non-empty, the draft is at least three lines tall, fits the message length limit and has a
  non-blank character. A one-time tooltip explains it.
- The box opens on the Style tab and asks nothing until a style or Emojify is picked. Translate
  sends `composeTextWithAi` with the target language (app language if it differs from the detected
  one, else English, else Spanish) and an optional style; Fix sends `fixTextWithAi` and renders
  the `diffText` (inserts underlined green, replaced text struck red then the new text, deletes
  struck). The card collapses the original to one line, offers a copy button, an Emojify checkbox
  and a "Style by {user}" line for styles with a creator. The footer is Apply / Apply Style,
  disabled "Select Style", or "Increase Limit x50" after `AICOMPOSE_FLOOD_PREMIUM`, which also
  shows the Premium promo toast. `INPUT_TEXT_TOO_LONG` is a toast.
- Styles come from `updateTextCompositionStyles`. Custom styles have a right-click menu: bind the
  apply hotkey, Edit (creator), Share (`t.me/addstyle/<name>`), Delete (creator) / Remove. The
  create/edit box has an icon (custom emoji), a name and a prompt bounded by TDLib options, and
  "Add a link to my account". `TONES_SAVED_TOO_MANY` shows the saved-style limit toast (free count
  from `added_text_composition_style_count_max`, Premium count from
  `premiumLimitTypeCustomTextCompositionStyleCount`). `addstyle/<name>` links search the style
  and open a preview with examples, "Used by N people. Created by {user}" and Add / Remove Style.
  Link previews render the style's emoji with a "View Style" button. The apply hotkey rewrites the
  selection (or the whole field) with the bound style in place.

## Shape
- `src/ai_compose/`: pure types (`TextCompositionStyle`, `StyleList`, `DiffText` →
  `diff_segments`), the box as a state machine (`AiComposeBox`: every transition returns the
  request to send, answers are matched by token), `default_translate_to`, `ai_button_visible`,
  the saved-style limit notice and the local prefs (`ai_compose_prefs.json`: tooltip flags, the
  bound style name).
- Envelope: `MessagesPayload::AiCompose(AiComposePayload)` (one variant, its own enum, so the
  >40-variant enums grow by one); `fixedText` moved there and now carries entities and the diff.
  `ErrorClass` gains `InputTextTooLong`, `StylesSavedTooMany`, `StyleInvalid`.
- State: `session.messages.ai_compose` (style list, option limits, one-shot answer slots the UI
  takes). Purposes under `MessagesPurpose::AiCompose(AiComposePurpose)`.
- UI: `src/ui/ai_compose/` with one kit dialog whose sub-views (card, language picker, About
  Cocoon, style editor, style icon) never stack, a second dialog for the style preview, kit pill
  tabs, buttons, checkbox, skeleton, notifications; the composer button fades in on the frame
  clock. A new rebindable action `ai-compose-apply` with no default key.

## Deviations
- The draft's line count is its newline count; the kit textarea exposes no wrapped-line count.
- The sparkle blink is one icon fading, not tdesktop's two stars; the "Choose Style" hint is drawn
  inline under the pills; the icon picker lists topic icons and installed emoji sets without set
  tabs or search; the flood and limit toasts use inline links instead of tdesktop's toast layout.
- Share hands the link to Quill's share chooser (puts it in the chosen chat's composer); tdesktop
  sends it directly.
- The apply hotkey works in the main window only.
