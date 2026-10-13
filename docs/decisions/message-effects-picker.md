# Message effect picker, "Send with Effect", composer Date and Unicode menu

Parity items `parity:render-message-effects` (finished here) and
`parity:composer-unicode-menu` (what tdesktop really has, see below).

## Send menu effect picker

What tdesktop does (`menu/menu_send.cpp`, `AttachSendMenuEffect`,
`LookupPossibleEffects`, `EffectPreview`; `history_widget.cpp`
`sendButtonDefaultDetails`):
- The picker is offered when the chat is with a user (`_peer->isUser()`,
  so Saved Messages and bots too) and there is something to send.
- `LookupPossibleEffects` takes every effect; Premium ones only where
  Premium can be bought (`premiumPossible`). Effects with an around
  animation are "recent" (reaction effects), the rest are sticker effects.
  No reaction effect: no selector at all.
- Above the menu: "Add an animated effect" and a strip of reaction effects
  (40 px high, 32 px cells, 26 px images, 7 px skip), at least seven
  columns; when not all fit, the last cell expands. The menu takes the
  strip's width. A strip cell plays its select animation on hover.
- Expanded: a search field, every reaction effect (Premium ones locked
  without Premium), then "Effects from stickers" in 48 px cells (the emoji
  in the corner with Premium, a lock without). An empty search result reads
  "No effects found." Power saving stops the emoji and sticker panels.
- Choosing an effect opens the `EffectPreview` card centred on the cell and
  kept inside the menu: the 224 px effect area on the chat background with
  a sample incoming message ("Good morning", replying to Bob Harris's "Do
  you know what time it is?"), its badge a third of the width left of the
  centre, the effect looping mirrored over it. Below, a 39 px "Send with
  Effect" button, or for a Premium effect without Premium "Subscribe to
  Telegram Premium to add this animated effect." whose link opens the
  Premium preview of effects. 400 ms `easeOutCirc` fade; a spinner where the
  badge goes until the icon and the animation are loaded; a click on the
  card hides it. A right-click on the button offers silent and scheduled
  sending with the effect (no silent in Saved Messages).

What Quill does: all of the above.
- Core, tested (`src/message_effects_picker.rs`): the lists, the strip
  layout, the grids, the panel and card geometry, the gating.
- Driver (`src/connect/message_effects.rs` `load_effect_picker`): when the
  menu opens, `getMessageEffect` for every available effect not yet
  described, then each icon's download, once.
- UI (`src/ui/send_effects/`): the strip and the kit menu in one column;
  the expanded list and the card over it; the card's own menu. The kit
  `PopupMenu` stays the menu (same look as before); a press on the picker's
  parts no longer dismisses it. Animations use the frame clock.
- Sending: `ComposerUi.send_effect_once` rides `composer_send_options()`
  for one send, like `send_silent_once`; the schedule picker carries
  `send_effect` to its send.
- The card's Premium link opens Premium on the "Message effects" feature
  (`show_premium_effects_preview`, the stickers preview helper generalised).

Differences from tdesktop:
- The card stays inside the window as well as the menu (the Quill menu
  can sit at the window's bottom edge, and the card is taller than it).
- Effect images in the picker decode at the 56 px emoji size Quill already
  caches; tdesktop decodes at the cell size.
- Quill's separate media send box has no send menu at all (no silent or
  schedule either), so no picker there; media sent from the composer gets
  it.
- `sendButtonDefaultDetails` also allows effects when only a link preview
  is shown; Quill has nothing to send in that case and offers none.

## Composer: Insert Unicode, subscript, superscript, date, formula

What tdesktop has in the message field's right-click menu
(lib_ui `InputField::contextMenuEventInner` and `addMarkdownActions`):
- Qt's standard text menu, whose last entry is the "Insert Unicode control
  character" submenu when `QStyleHints::useRtlExtensions()` is set. Qt sets
  it only on Windows, when an installed keyboard layout's locale is
  right-to-left (`QWindowsInputContext::hasRTLInputLanguage`, the font
  signature bit). tdesktop only translates the title (`lang_translator.cpp`).
- Formatting > Bold, Italic, Underline, Strikethrough, Quote, Monospace,
  Spoiler; Create link; **Date** (Ctrl+Shift+D, `editMarkdownDate`), off
  without a selection unless the caret is on a date; Clear formatting.
- No subscript, superscript, marked or formula items: those strings
  (`lng_menu_formatting_subscript` and so on) are used only by the Instant
  View editor (`iv/editor/iv_editor_box.cpp`), not the message field.

What Quill does now:
- Formatting > Date and Cmd/Ctrl+Shift+D (`src/ui/composer_date.rs`): the
  "Choose date and time" box with Save, starting at midday today or at the
  date under the caret. The date is a `tg://time?unix=N` link in the field
  and in markup (`quill::composer::date_link`), sent as
  `textEntityTypeDateTime` with no formatting type (tdesktop's empty
  `FormattedDateFlags`), and read back from drafts and edits. tdesktop
  first shows a calendar for a new date and then the date-time box; Quill's
  date-time field has its calendar built in, so it is one box.
- "Insert Unicode control character" (`src/ui/composer_unicode.rs`): Qt's
  fourteen characters in its order and wording, after Select All, on Windows
  when a right-to-left keyboard layout is installed; never on macOS or
  Linux, as in tdesktop.

README wording for `parity:composer-unicode-menu` should become something
like "Insert Unicode control character menu and Date formatting in the
composer menu"; subscript, superscript and formula are Instant View editor
features, not composer ones.

## Tests
- Unit: picker lists, strip and grid layout, card placement, gating;
  date link round trip through markup, `textEntityTypeDateTime` JSON and
  `from_entities`; the Cmd/Ctrl+Shift+D chord; midday default.
- Driver (recording sender): a message's effect is asked for once and its
  icon downloaded; a 404 is not asked again; the picker describes every
  effect then fetches each icon once; a send carries `options.effect_id`.
- GPUI (`send_effects/tests.rs`, `composer_date_tests.rs`): a badge click
  plays the effect; right-click Send shows the strip, a cell opens the card,
  the menu stays open, Send with Effect sends once and closes it; a card
  click hides it and a click outside closes the menu; the full list shows
  every effect and searches; a Premium sticker effect offers Premium; no
  strip with nothing to send; the card's schedule carries the effect; the
  date box via the chord and Save; a control character replaces the
  selection.
- Visual: `ready-message-effects` with `QUILL_DEMO_EFFECTS=badge|strip|
  expanded|card|premium`, and `ready-composer-date`, captured and checked.
  The fixture sticker is a red dot, so the effect itself is a placeholder
  there.
- Not verified: a live account (effects from the server, a real send), the
  Windows keyboard check on a Windows machine (it is type-checked for
  `x86_64-pc-windows-msvc`).
