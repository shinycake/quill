# Reaction limits and Premium reactions (`parity:premium-reaction-limits`)

## What tdesktop does

- `SentReactionsLimit` (`data/data_message_reactions.cpp`) reads the app
  config: `reactions_user_max_premium` with Premium,
  `reactions_user_max_default` without.
- `MessageReactions::add` keeps the user's first `limit - 1` own reactions
  in list order and takes back the rest before it adds the new one, so the
  message updates before the server answers. Stars are not counted.
- `Window::ShowReactPremiumError` (`window/section_widget.cpp`) runs before
  a reaction is sent:
  - On Saved Messages tags, a free account gets the "Tags for Saved
    Messages" Premium preview.
  - A reaction already on the message, or any reaction in a broadcast
    channel, is free. A custom emoji reaction otherwise opens the
    "Unique reactions" (`InfiniteReactions`) Premium preview.
- In the expanded selector (`chat_helpers/emoji_list_widget.cpp`), custom
  emoji packs on a free account that can buy Premium get `premiumRequired`.
  The header shows a lock before the title and a "Restore" button for
  installed packs. The button closes the selector and opens Premium on
  `infinite_reactions`.
  The free account can expand the strip whenever custom reactions are
  allowed and Premium can be bought (`customAllowed = premiumPossible`).
- No per-reaction lock icon is drawn in the strip. The `premium` flag of an
  available reaction only marks message effects.

## What changed in Quill

- TDLib keeps the `reactions_user_max_*` options internal and never sends
  them as `updateOption`. Quill reads the same keys from
  `getApplicationConfig`, which it asks for when a reaction menu opens
  (`quill::reaction_limit::ReactionLimits`). No limit is hardcoded: until
  that answer arrives there is no local trim, and TDLib's own
  `updateMessageInteractionInfo` does it.
- `with_own_reaction` applies tdesktop's add and remove rules. The driver
  applies it to the session as soon as `addMessageReaction`,
  `removeMessageReaction` or `setMessageReaction` is sent, and TDLib's next
  update replaces it. The demo uses the same session call.
- The `needs_premium` flag of `getMessageAvailableReactions` is kept per
  message (`MessageReactionOptions::premium_locked`).
  `Session::react_premium_feature` implements `ShowReactPremiumError` with
  that flag added. A tap on a locked reaction, in the strip or the
  selector, opens the Premium dialog on that feature instead of the old
  status note.
- Custom emoji pack headers in the reaction selector show the lock and
  "Restore" (`lng_emoji_premium_restore`) for a free account. The strip's
  expand chevron now also shows for free accounts where Premium can be
  bought.

## How it was verified

- Unit tests for the app config keys and the trim rules.
- Driver tests with the recording sender cover:
  - the app config request
  - a free account swapping its one reaction before TDLib answers
  - Premium keeping three reactions, through the quick reaction too
  - no trim without the config
  - `needs_premium`
  - free custom reactions on the message and in channels
  - Saved Messages tags
- GPUI UI tests on the demo app cover:
  - a locked strip reaction opens the "Unique reactions" preview and sends
    nothing
  - a free account's strip tap swaps its reaction
  - "Restore" on a locked pack closes the selector and opens the preview
  - a Premium account sees no lock
- The `ready-reaction-packs` demo was captured in light and dark.
