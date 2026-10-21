# Reaction strip loading state

## What tdesktop does

- It never waits. `AttachSelectorToMenu` (history/view/reactions/history_view_reactions_selector.cpp:1339-1344) builds the selector from `Data::LookupPossibleReactions(item, true)`, which reads reactions already cached on the client. There is no loading placeholder.
- No reactions: `if (reactions.recent.empty()) return base::make_unexpected(AttachSelectorResult::Skipped)` (history_view_reactions_selector.cpp:1394-1396). The menu pops up without a strip: `history_inner_widget.cpp:3917-3921` calls `popup(desiredPosition)` for anything but `Attached`.
- Errors: there is no request, so there is no error path. The only failure is `AttachSelectorResult::Failed` (geometry), which drops the menu (history_inner_widget.cpp:3912).

## Quill before

The strip drew six skeleton circles whenever `message_reaction_options` was `None` or belonged to another message. A failed `getMessageAvailableReactions`, a failed send, or no driver (demos) left the skeleton for good; an empty answer drew an empty row.

## What changed

- `MessageReactionsState` (src/state/media_library.rs) replaces `Option<MessageReactionOptions>`: `Idle`, `Loading`, `Ready`, `Unavailable`, each tied to a message.
- `fetch_message_reactions` sets `Loading` before sending. A send error or a TDLib error reply (routed in `apply_messages_error`) sets `Unavailable`, only if that message is still the loading one. An answer with no reaction at all is `Unavailable` too, matching `recent.empty()`.
- `reaction_strip` draws the placeholder only for `Loading`, the strip only for `Ready`, and nothing otherwise, as tdesktop's skipped selector.
- Demos seed options through one helper, `seed_reaction_options` (src/ui/demos/mod.rs). `open_message_menu` calls it when there is no driver, so a right-click in any demo shows real emoji.

## How verified

Driver tests (loading, error, empty, stale error), a reducer test, and UI tests in src/ui/reaction_strip_state_tests.rs (absent for `Unavailable` and `Idle`, placeholder for `Loading`, strip for `Ready`). `ready-audio` right-click captured in light and dark.
