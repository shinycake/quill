# Message menu gaps

## What tdesktop does

All in `history/view/history_view_context_menu.cpp` unless noted.

- **Original date.** `AddWhenEditedForwardedAuthorActionHelper` (line 2436) adds a
  `WhenReadContextAction` row for a message with `HistoryMessageForwarded`
  that is not a story and has no `psaType`. The row is an icon plus
  `FormatReadDate(originalDate)` ("today at 12:34", "yesterday at ...",
  "Apr 15 at ..."), built by `Api::WhenOriginal` (`api/api_who_reacted.cpp:831`).
  In a private chat it sits above the read-time row, elsewhere below the
  "N Seen" row (`AddWhoReactedAction`, lines 2547-2578). Imported messages carry
  the same component, so they get the row too.
- **Send Another Gift.** The gift branch of `FillContextMenu` (lines 1865-1890)
  adds `lng_context_gift_send` for a `MediaGiftBox` when the peer is not an
  inaccessible or service user, the gift is not a giveaway prize (slug set and
  channel set), it is "outgoing" (inverted for an upgrade message) and not a
  birthday suggestion. It opens `ShowStarGiftBox(peer)`.
- **Block sender in Replies.** `AddBlockSenderAction` (line 1279) adds
  `lng_profile_block_user` when no multi-selection exists and the chat is the
  Replies chat. It opens `BlockSenderFromRepliesBox` (`window/window_peer_menu.cpp:2835`),
  i.e. `PeerMenuBlockUserBox` with report-spam, "Delete this reply" and "Delete
  all replies from {user}" checkboxes, all checked, sending
  `contacts.blockFromReplies` and toasting `lng_new_contact_block_done`.
  `AddReportAction` (line 1253) skips the Replies chat.
- **Save as HTML.** `Iv::AddSaveRichMessageHtmlAction` shows "Save as HTML" for
  exactly one selected rich message. The action runs `RichMessageHtmlExport`
  (`iv/iv_rich_message_html_export.cpp`, 2191 lines): a rich-page to HTML
  serializer, a media download-and-copy job queue, a folder picker, progress
  and toasts. Not built here (see below).

## What Quill does now

- Original date row: text goes through the lang keys `lng_mediaview_today`,
  `_yesterday` and `_date_time` (`original_date_text`). `message_menu_origin::original_date` (forward not marked as
  an announcement, or an import) feeds an info row with the Forward icon and
  `read_date_label`, placed like tdesktop. TDLib has no story forwards in
  `forward_info`, so only the PSA check applies. `psa_type` is now parsed.
  A zero date shows no row (tdesktop would show an empty one).
- Send Another Gift: `send_another_gift_offered` maps TDLib's service actions
  onto tdesktop's flags (giveaway code and prize are not gifts; an upgrade
  inverts the side). The recipient check is `Session::gift_recipient`, which
  excludes bots (tdesktop does not; Quill's send-gift dialog has no bot
  recipient, so this stays a deliberate deviation). The service account
  (777000) and deleted accounts are excluded as in tdesktop, and a refunded
  upgraded gift and a code made by a channel follow tdesktop's flags. Opens the existing send-gift dialog for the message's
  chat.
- Block user in Replies: the Replies chat is the id in TDLib's option
  `replies_bot_chat_id` (`Session::is_replies_chat`; the test environment
  differs). The row shows for every message there, after Select (tdesktop's
  history view order), for a user, a channel or anonymous-admin origin (the
  box then names the chat and has no confirmation line, as for a non-user peer
  in `PeerMenuBlockUserBox`) and for a hidden sender. A hidden sender is a
  deliberate deviation: tdesktop's `originalSender()` has no peer there, but
  TDLib's request works by message id, so Quill keeps the row,
  a kit dialog (`BlockFromReplies`) with three checkboxes, `blockMessageSenderFromReplies`
  via `submit_replies_block`, the toast on confirm, an error note if the send
  fails. State lives in `UsersState::replies_block`, so the dialog renders from
  it. The Report row is hidden in the Replies chat as in tdesktop.

## Not built

`menu-save-selection-html` stays open: the real work is the rich-page HTML
exporter above, and it also needs the single-selection menu in Quill's
multi-select mode. That is a PR of its own.

## Verification

Core tests for the three rules and the request shape, a driver test with the
recording sender, four UI tests over the new `ready-origin-menus` demo, and
light and dark captures of the three menus and the block dialog.
