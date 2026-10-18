# PSA and promoted-chat rows

Parity id: `chatlist-psa-promoted`.

## What tdesktop does

- A chat the server promotes sits at the top of the main list, but only while
  `History::useTopPromotion()` holds (`history/history.cpp:3361`): it has a
  source, is not pinned, and is either a channel the account is not in or a
  bot whose history is empty.
- Where the date goes, the row paints a badge
  (`dialogs/ui/dialogs_layout.cpp:571-586`, `PaintRowTopRight`): "Proxy
  sponsor" when the promotion has no type, otherwise the server's text for the
  announcement type (language-pack key `cloud_lng_badge_psa_<type>`),
  otherwise "PSA". Search results never show it.
- When the announcement has text, the row's second line is that text. The
  branch comes before the draft and last-message branches
  (`dialogs_layout.cpp:642`, `topPromotionMessage`).
- The chat menu of a promoted chat drops Archive (`CanArchive`,
  `window_peer_menu.cpp:4619`), Pin (`addTogglePin` returns on
  `fixedOnTopIndex()`, `:554`) and, for a typed announcement, "Open in new
  window" (`:785-790`). It adds "Hide this announcement" for a typed
  announcement (`addHidePromotion`, `:511`), which calls
  `help.hidePromoData`.

## What changed

- `src/promotion.rs`:
  - `Promotion` models the source (`MtprotoProxy`, or `Psa { kind, text }`).
  - `top_promotion(session, chat)` is `useTopPromotion` as one pure
    predicate: a source, not pinned, and a channel or supergroup the account
    is not in, or a bot with no messages. The badge, the preview, the menu
    and the hide request all go through it.
  - `PsaLabels` holds the server's badge text for one language pack at a
    time.
  `ChatPositionUpdate.is_promoted` became `promotion: Option<Promotion>`, and
  `chat_list.promoted_chats` maps chat id to its promotion (pull-to-next-channel
  still skips these chats).
- The row shows the badge in the date slot and the announcement text as the
  preview, ahead of a draft and the last message.
- The chat menu skips Archive and Pin for a promoted chat and "Open in new
  window" for a typed announcement, and offers "Hide this announcement" for a
  typed one.
- Badge text is asked for once per type and language pack with
  `getLanguagePackStrings` and the keys `cloud_lng_badge_psa_<type>`
  (`pump_psa_labels` in the poll loop). The pack is the one the interface
  uses, or the one it is switching to. A new pack drops the old text, asks
  again, and ignores a late answer for the old pack (an epoch on the request).
  Until the text arrives, or when the pack has none, the badge reads "PSA".
- "Hide this announcement" sends `deleteChatHistory` with
  `remove_from_chat_list: true`, which TDLib turns into `help.hidePromoData`
  for a promoted announcement. Only where `top_promotion` applies and the
  announcement has a type, so a channel the account has just joined is never
  sent. It has its own request purpose, so a refusal reads "Could not hide the
  announcement", not the delete-chat text.
- `chat_row/selected_fg.rs` holds the selected-row foreground helper that
  moved out of `chat_row/mod.rs` to keep it under the file-size limit.

## Deviations

- tdesktop picks the badge by the type: no type gives "Proxy sponsor". Quill
  picks it by the TDLib source kind, which is more precise: a proxy source
  gives "Proxy sponsor", and an announcement with no type (or a source kind
  this build does not know) gives "PSA" and has no hide row.
- The badge text is fetched by the keyed request, not read from the full pack
  that the interface installs, because the built-in English interface has no
  pack to read from.

## Not done

- tdesktop also shows a one-time "about this announcement" box when the chat
  opens (`history_widget.cpp:4425`, `lng_about_psa_<type>`).

## How it was verified

- Unit tests: source parsing, labels, one request per type and pack, a new
  pack dropping old text and late answers, and `top_promotion` for a channel
  not joined, joined, pinned, a basic group, a user and a bot with and
  without messages. The pull-next test covers proxy, announcement and no
  source. The driver test covers the keyed request, the second pump sending
  nothing, hide for a typed announcement only, no hide for a joined channel,
  and a language switch asking again.
- GPUI test `a_promotion_shapes_the_menu_like_use_top_promotion`: normal
  chat, proxy sponsor, joined channel and typed announcement, checking the
  hide, Archive, Pin and new-window rows.
- Demo `ready-promoted-chats` (an announcement channel with text, a proxy
  channel, and a bot with no messages, none of them joined) captured in light
  and dark, with the chat menu open on the announcement row.
- The hide request was not run against a real account (no announcement was
  available); it relies on TDLib handling `deleteChatHistory` on a promoted
  announcement as `help.hidePromoData`, as Telegram X does.
