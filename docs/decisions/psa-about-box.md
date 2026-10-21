# One-time "about this announcement" box

README id: `chat-psa-about-box`.

## What tdesktop does

`HistoryWidget::showAboutTopPromotion` (and the same in `ChatWidget`) runs
when a history is shown. If `History::useTopPromotion()` holds and the box
was not shown for that history, it marks it shown and calls
`showInfoTooltip`: a `HistoryView::InfoTooltip` toast attached to the top of
the conversation, with the info icon, up to 380 px wide, on screen
`clamp(characters / 14 s, 5 s, 8 s)`, hidden when another chat opens.
Text: no promotion type (proxy sponsor) uses `lng_proxy_sponsor_about`; a
type uses the server's `cloud_lng_about_psa_<type>` when the language pack
has it, else `lng_about_psa_default`. `**x**` is bold. 

"Shown" is a `seen^` prefix on the history's promotion type, in memory only.

## What changed

- `src/psa_about.rs`: the state (`Session::chat_list.psa_about`), the
  trigger in `Session::open_chat`, the duration (over the shown text, in
  UTF-16 units), the `**bold**` runs and the saved flag
  (`psa_about_seen.json`, per chat id). The driver's poll tick reads it on
  a background thread, once, and saves new flags on a background thread
  merged with the file, so two accounts or processes do not overwrite each
  other. A chat opened before the flags are in waits for them.
- The box is visible while the open chat is its chat and its time has not
  run out, so leaving the chat or popping it out hides it. The frame clock
  repaints at 2 fps while it is up.
- `PsaLabels` also fetches `cloud_lng_about_psa_<type>` with the badge keys.
- `src/ui/psa_about_ui.rs`: draws the box. It is an anchor in the
  conversation's bar stack (`chat_top_bars`) with a deferred floating card,
  so it sits 13 px under the header and bars without a magic offset. Custom layer rather than `Notification`, which is placed at a
  window corner; this one sits at the top of the conversation like
  tdesktop's.
- Strings `lng_proxy_sponsor_about` ("Quill Settings" instead of "Telegram
  Settings") and `lng_about_psa_default`.

## Deviation

tdesktop forgets the flag when the app restarts, so the box returns each
launch. The brief asked for a flag that survives restarts, so Quill saves
it. Chat ids are shared across accounts, so the flag is too.

## Verification

Unit tests (trigger, once, restart, text per type, duration), a GPUI test
and light/dark captures of `ready-promoted-chat-about`.
