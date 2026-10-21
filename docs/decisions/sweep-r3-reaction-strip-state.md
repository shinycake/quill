# Reaction strip loading state

## What tdesktop does

- It never waits. `AttachSelectorToMenu` (history/view/reactions/history_view_reactions_selector.cpp:1339-1344) builds the selector from `Data::LookupPossibleReactions(item, true)`, which reads reactions already cached on the client. There is no loading placeholder.
- No reactions: the check is on the account-wide `reactions.recent` list from the local cache, not on a per-message answer: `if (reactions.recent.empty()) return base::make_unexpected(AttachSelectorResult::Skipped)` (history_view_reactions_selector.cpp:1394-1396). The menu pops up without a strip: `history_inner_widget.cpp:3917-3921` calls `popup(desiredPosition)` for anything but `Attached`.
- Errors: there is no request, so there is no error path. The only failure is `AttachSelectorResult::Failed` (geometry), which drops the menu (history_inner_widget.cpp:3912).

## Quill before

The strip drew six skeleton circles whenever `message_reaction_options` was `None` or belonged to another message. A failed `getMessageAvailableReactions`, a failed send, or no driver (demos) left the skeleton for good; an empty answer drew an empty row.

## What changed

- `MessageReactionsState` (src/state/media_library.rs) replaces `Option<MessageReactionOptions>`: `Idle`, `Loading`, `Ready`, `Unavailable`, each tied to a message.
- `fetch_message_reactions` sets `Loading` before sending. A send error or a TDLib error reply (routed in `apply_messages_error`) sets `Unavailable`, only if that message is still the loading one. An answer with no reaction at all is `Unavailable` too, matching `recent.empty()`.
- `reaction_strip` draws the placeholder only for `Loading`, the strip only for `Ready`, and nothing otherwise, as tdesktop's skipped selector.
- Demos seed options through one helper, `seed_reaction_options` (src/ui/demos/reaction_fixtures.rs), called from the demo builders (`ready-reactions`, `ready-audio`, and the others that open a message menu) for the message they open. Production `open_message_menu` has no demo branch.

## Empty, in Quill terms

Quill has no account-wide cache to read, so it asks TDLib per message and counts an answer as empty when top, recent and popular hold no emoji or custom emoji and the answer does not offer the paid (Star) reaction. tdesktop adds the paid reaction to `recent` when paid reactions are enabled (data/data_message_reactions.cpp:240-252), so a chat that offers only Stars still gets its selector; Quill follows by counting `Paid` as an offered reaction. `paid_picker_cell` draws that Star cell in the strip row from `payments.paid.offered`.

## Deliberate difference: the loading skeleton

tdesktop never waits. Quill fetches per message, and the menu is placed before the answer arrives. The six-circle skeleton (src/ui/reactions.rs, `reaction-strip-loading`) keeps the menu height stable so the menu does not jump when the strip appears. It shows only while `Loading` for that exact message.

## Rules

- Stale answers: an answer (or error) is applied only when the state is `Loading` for the same chat and message. Otherwise it is dropped, and it does not touch the paid offer either. A late answer for message A never overwrites B's `Loading` or `Ready`.
- Retry: only `Ready` for the same message covers it. `Loading` and `Unavailable` do not: `fetch_message_reactions` sends again unless a request for that message is actually pending. So reopening after an error, or after a lost reply left a `Loading` with nothing pending, asks again and cannot leave the skeleton stuck.
- Paid: an answer offering only the Star reaction is `Ready`, with `payments.paid.offered` set for that message.
- Reset: closing the message menu (Escape, click-away, `dismiss_context_menus`) goes through `close_message_menu`, which calls `Session::reset_message_reactions`: state back to `Idle`, paid offer cleared. A reply arriving after that is ignored by the stale rule. Menu items that close the menu themselves leave the state as it is. It stays bound to its message: a reopen of the same message keeps `Ready` (the answer is still right), a `Loading` with no pending request refetches, and opening another message replaces it (50 sites close the menu from an item click, so resetting at each was not an option). `payments.paid.offered` survives such a close too; that is harmless because the Star cell only draws inside a ready strip for that message.

## How verified

Driver tests (loading, error, empty, stale error, lost reply then reopen, reopen while pending), a UI test that feeds a real `ConnectDriver` answer into the app and checks the render,, a reducer test, and UI tests in src/ui/reaction_strip_state_tests.rs (absent for `Unavailable` and `Idle`, placeholder for `Loading`, strip for `Ready`, none for another message's `Ready`, open then close resets). Driver tests cover the stale-answer, retry, paid-only and reset rules. `ready-audio` right-click captured in light and dark.
