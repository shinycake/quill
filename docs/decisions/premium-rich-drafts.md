# Rich message drafts

## What tdesktop does

A cloud draft can hold a rich message (`draftMessageContentRichMessage`). While no
message is being edited, `HistoryWidget::shouldShowRichDraftPreview` swaps the field
for a `RichDraftPreview`. The preview draws the draft's blocks, is capped at the
field's maximum height and fades out at the bottom when there is more. Clicking it
opens the draft in the rich editor.

`initDiscardRichDraftButton` puts a discard button over the attach button. It asks
"discard this message?" first, and Ctrl-click skips the question. `clearRichDraft`
drops the draft but keeps the reply.

`sendRichDraft` sends the blocks for Premium users. Without Premium, if the draft uses
Premium formatting:

- When the text can stand alone (no media, not empty, within the length limit),
  `OfferRichMessagePremiumChoice` shows "Remove Formatting" with three buttons:
  Subscribe to Premium, Send Without Formatting and Cancel.
- Otherwise `ShowRichMessagesPremiumToast` explains that rich articles need
  Telegram Premium.

Pasting structured text into the plain field shows `rich_paste_toast`, for 9 seconds
and at most once every 30 seconds. It closes on the field's next change. Premium
users get an Open button that moves the text into the editor. Others get a Premium
link and a "Got it" button that stores `rich_paste_toast_hidden`.

## What changed

- `RichDraft` (`src/rich/draft.rs`) holds a draft's blocks. It converts them to and
  from the composer's block markup, builds the input blocks for
  `draftMessageContentInputRichMessage`, and reports whether they need Premium
  formatting.
- `ChatDraft.rich` parses incoming rich drafts. `set_rich_draft_message` saves them.
  `DraftContent` (text or rich) is what the composer stores, debounces and flushes
  when you leave a chat, search or edit a message, so an empty field never
  overwrites a rich draft.
- `src/ui/rich_draft/` holds the preview, which is derived from the stored draft
  (the UI follows state), the discard button and its confirm box, the Remove
  Formatting box, the Premium toast and the paste notice.
- Send, the editor's close button, chat switching, search and deep links all go
  through the draft content instead of the field text.
- The demo `ready-rich-draft` (`QUILL_DEMO_RICH_DRAFT`) shows each state.

## Differences from tdesktop

- GPUI reads no HTML from the clipboard, so only pasted Markdown text triggers the
  paste notice. The field's "Apply Markdown" and Plain/Undo offers are not built.
- Quill's rich editor is the composer in block-markup mode, not a separate box.
- The discard button stays visible whenever the preview shows, in the AI button's
  slot. Ctrl-click maps to the platform's secondary modifier (Cmd on macOS).
- A draft from another app with media or buttons shows and syncs, but Quill can't
  edit or send it yet. It says so instead of dropping the media.
- The Premium toast uses the kit notification's own timing.
- The `rich_message_posting` app config is not read.

## How verified

- Unit tests cover parsing, markup round trips (headings, task lists), the request
  JSON, and draft storage across the composer paths.
- UI tests (`src/ui/rich_draft/rich_draft_tests.rs`) cover the preview replacing the
  field and surviving a chat switch, the editor opening and closing back into the
  preview, discard asking before dropping, and a non-Premium send offering Remove
  Formatting.
- Demo captures in light and dark of the preview, discard box, editor, Remove
  Formatting box, Premium toast and both paste notices.
- Not run against a live account. The TDLib round trip, the server's answer to a
  non-Premium rich send and the timing of the `clear_draft` echo are untested.
