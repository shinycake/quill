# Star reactions and Star packs

## What Telegram Desktop does

- `data_message_reactions.cpp`, `history_view_paid_reaction_toast.cpp`: a
  tap on the Star chip adds one Star and holds it for 5.5 seconds
  (`kPaidAccumulatePeriod`). More taps add up and restart the timer. A toast
  says "Star sent!" with "You reacted with N Stars." and an Undo button with
  a countdown. When the time runs out the total is sent.
- `payments_reaction_process.cpp`, `payments_reaction_box.cpp`: choosing the
  Star in the reaction picker, or opening the chip, shows a box with an
  amount slider (default 50, cap `stars_paid_reaction_amount_max`), the Top
  Senders, a "Show me in Top Senders" choice and a Send button. Stars
  waiting to be sent are locked against the balance. A balance too small
  offers a top-up.
- The Star chip leads the reaction row.
- `settings_credits.cpp`: balance, All / Incoming / Outgoing history and a
  list of Star packs; a pack is bought with `inputInvoiceTelegram` and
  `telegramPaymentPurposeStars`.

## What changed in Quill

- `src/paid_reaction.rs`: the pending state machine (add, undo, due, cap,
  balance reservation), slider mapping, `paidReactor` type. Tests in
  `src/paid_reaction/tests.rs`.
- `src/telegram/requests/paid_reaction.rs`: `addPendingPaidMessageReaction`,
  `commitPendingPaidMessageReactions`, `removePendingPaidMessageReactions`,
  `getStarPaymentOptions` (names checked against TDLib 1.8.68).
- `src/connect/paid_reaction.rs`: driver methods. A tap sends the pending
  request at once and the commit goes out from the poll tick when the window
  ends; Undo sends the remove request. Errors roll the Stars back.
- `paid_reactors` is parsed into `MessageReactions`. The Star chip, toast
  (`chat_bars.rs` hook), picker cell and amount box are new UI files
  (`paid_reaction_chip.rs`, `paid_reaction_ui.rs`, `paid_reaction_state.rs`).
- Stars dialog: "Buy more Stars" lists the packs (`premium_ui/star_packs.rs`).
- `updateOwnedStarCount` already updated the balance; a connect test now
  covers it.

## Not done

- Buying a pack. Quill's checkout builds `inputInvoiceMessage` only; it has no
  `inputInvoiceTelegram` path, so the list is read-only and the dialog still
  points to the official apps or @PremiumBot. Sending Stars as gifts is not
  in this change either. `parity:premium-stars-balance` stays open.
- The empty Star chip follows tdesktop: shown when the message has other
  reactions and the chat lists the Star reaction as allowed
  (`chatAvailableReactionsSome` containing `reactionTypePaid`; `...All` never
  includes it).
- The chip count adds the Stars still waiting for their Undo window. If TDLib
  already includes pending Stars in `updateMessageInteractionInfo`, the count
  would be doubled for those 5.5 seconds; this was not checked against a live
  account.
- The cap comes from `updateOption paid_reaction_star_count_max` (10,000 until
  it arrives).
- The box offers "Send as <channel>" for the channels from
  `getChatAvailablePaidMessageReactionSenders` (`paidReactionTypeChat`).
- `updateChatBoost` is for bots only in TDLib, so nothing to apply for
  `parity:updates-stars-boosts` beyond the Star count.

## How it was verified

Unit and connect-driver tests (parse, request JSON, the undo timer, rollback,
balance reservation). A temporary demo capture (not committed) showed the
chip, toast with Undo, the amount box with Top Senders and the pack list.
No Stars were spent and no real message was reacted to.
