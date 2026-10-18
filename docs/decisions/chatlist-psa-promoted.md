# PSA and promoted-chat rows

Parity id: `chatlist-psa-promoted`.

## What tdesktop does

- A chat the server promotes (a proxy sponsor, or a public service
  announcement) sits at the top of the main list (`History::useTopPromotion`).
  Where the date goes, the row paints a badge instead
  (`dialogs/ui/dialogs_layout.cpp:571-586`, `PaintRowTopRight`): "Proxy
  sponsor" for a proxy, otherwise the server's text for the announcement type
  (language-pack key `cloud_lng_badge_psa_<type>`), otherwise "PSA".
  Search results never show it.
- The chat menu has "Hide this announcement" for a promoted chat that has an
  announcement type (`window/window_peer_menu.cpp:511`, `addHidePromotion`),
  which calls `help.hidePromoData`. A proxy sponsor has no such row; it goes
  away with the proxy.

## What changed

- `src/promotion.rs` models the source (`Promotion::MtprotoProxy` or
  `Promotion::Psa { kind }`) and the badge text (`PsaLabels`).
  `ChatPositionUpdate.is_promoted: bool` became `promotion: Option<Promotion>`,
  and `chat_list.promoted_chats` is a map from chat id to its promotion
  (pull-to-next-channel still skips these chats).
- The row shows the badge in the date slot (`session_chat_row`), in the muted
  text colour with no status mark.
- The server's badge text is asked for once per type with
  `getLanguagePackStrings` and the keys `cloud_lng_badge_psa_<type>`
  (`pump_psa_labels` in the poll loop, `LanguagePurpose::PsaLabels`). Until it
  arrives, or when the pack has no text, the badge reads "PSA".
- "Hide this announcement" is in the chat menu for a typed announcement only.
  It sends `deleteChatHistory` with `remove_from_chat_list: true`, which
  TDLib turns into `help.hidePromoData` for a promoted announcement; the
  chat then leaves the list through `updateChatPosition`. A proxy sponsor row
  gets the badge but no hide row.
- `chat_row/selected_fg.rs` holds the selected-row foreground helper that
  moved out of `chat_row/mod.rs` to keep it under the file-size limit.

## How it was verified

- Unit tests: source parsing and labels (`src/promotion.rs`), position
  updates set and clear the promotion (`pull_next` test), the keyed
  `getLanguagePackStrings` request, and the driver test
  `announcement_labels_are_fetched_once_and_hide_deletes_the_history`.
- GPUI test `only_a_typed_announcement_offers_hide`: no hide row for a normal
  chat or a proxy sponsor, a hide row for an announcement, and clicking it
  closes the menu.
- Demo `ready-promoted-chats` captured in light and dark (badge text in the
  date slot) and with the chat menu open on the announcement row.

## Not done

- tdesktop shows the announcement text as the preview when the chat has no
  message. TDLib sends the announcement as the chat's last message, so the
  row already shows it.
- The hide request was not run against a real account (no announcement was
  available); it relies on TDLib handling `deleteChatHistory` on a promoted
  announcement as `help.hidePromoData`.
