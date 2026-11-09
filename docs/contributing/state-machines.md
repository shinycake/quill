# State machines: one value per thing that changes stage

Quill's UI is a pure function of its state. When one thing (a request, a
box, a call, a player, a send) moves through stages, those stages are one
value: an enum whose variants carry the data each stage needs. Not a set
of bools and options that code keeps in step by hand.

Fourteen review rounds of the state machine audit (#1392, #1397 to #1405
and on) found hundreds of bugs. Nearly all of them had one of the shapes
below. This page is the rule every person and agent follows when they add
or change state, and the list reviewers check.

## The rule

1. **A stage is a variant, not a flag.** `loading: bool`, `pending: bool`,
   `busy: bool`, `sent: bool`, `failed: bool` and friends become an enum.
   A fetched resource is `Fetch<T>` (`src/state/fetch.rs`): idle, loading,
   ready or failed, never two at once.
2. **Every stage has a way out.** For each variant, name what leaves it:
   the answer, the error, a cancel, the owner going away (account switch,
   chat change, window close, lock, log-out). An error arm and a dropped
   continuation must leave the in-flight stage too. If nothing can leave
   it, it sticks.
3. **Answers are matched to what asked.** A request carries the key of
   the thing it is for (an owner key from `src/ids.rs`, a query, a token),
   and its answer is applied only when that key is still current. "A
   request is pending, so skip" is checked against the same key.
4. **State lives with its owner.** Per-account UI goes in `OnScreen`,
   per-chat UI in `ChatUi`, per-window UI in the `WindowSlot`, and ids
   are owner keys (`OfAccount`, `ChatMessage`, `AccountChat`, `OwnedBy`),
   not bare ids (docs/decisions/account-scoped-ui-state.md).
5. **Resets happen in one place.** When a value's owner changes, the value
   is replaced whole (`sync_chat_ui`, the `OnScreen` swap, a `Farewell`),
   not field by field at each call site.
6. **The lock and the window decide too.** A starter checks
   `refused_while_locked` or queues an unlock intent
   (docs/decisions/lock-is-a-render-state.md), and a box opens in the
   window that asked for it (`chat_window/dialog_hosts.rs`).

## Shapes that kept coming back

- A flag set before a request and cleared only on success, so it sticks
  when the request fails or its continuation is dropped.
- A `loading` bool next to an `error` field (both can be set; that is
  `Fetch<T>`).
- "Pending, so skip", checked before a reset, or an answer not matched to
  its query.
- Several fields of one mode (an edit target, its text, its confirm box)
  cleared by hand in some places and missed in others.
- Upkeep that only the main window runs, so chat windows miss it.
- A bare message, chat, story or call id where the owner matters.
- Session-wide UI state drawn in every window; one `Entity` drawn in two
  windows (a redraw loop, docs/decisions/lock-field-per-window.md).
- A box whose open flag lives in the `Session` with no closer in its
  `Farewell`, so it comes back after switching accounts and back.
- A per-item panel that does not reset when its item changes.

## What CI checks

`scripts/check-implicit-state.py` fails when a struct gains a `bool` field
whose name names a stage (`pending`, `loading`, `busy`, `in_flight`,
`requested`, `sending`, `sent`, `failed`, `done`, `started` and more), or
holds such a flag beside an `error` field. The fields that were there when
the check was added are listed in `scripts/implicit-state-baseline.txt`.
That list is debt: it only shrinks, and a line whose field is gone must be
removed. When you touch one of those structs, convert the flag.

A bool that really is not a stage (a user setting, a capability TDLib
reports, a layout choice) goes in `scripts/implicit-state-allow.txt` with
the reason. Renaming a flag to dodge the word list is not a fix; reviewers
reject it.

The other guards cover the rest of the rule: `check-screen-state.py`
(owners, `OnScreen`, `ChatUi`, session dialogs, lock surfaces),
`check-window-scope.py` (async work names its window) and
`check-window-root.py`.

## Checklist for a change that adds state

- What are the stages? Write the enum first.
- What leaves each stage, including the error arm, a dropped
  continuation, and the owner going away?
- What key does each answer carry, and where is it compared?
- Who owns it: the process, the account, the chat, the window, the item?
- What happens to it under the lock, in a chat window, and after an
  account switch and back?
- A test that walks the transitions, with a manual clock if time matters
  (never a sleep or a timeout).
