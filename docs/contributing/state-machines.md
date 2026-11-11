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
- An enum that is the old flag renamed: `Idle`/`Asking` with nothing in
  `Asking`, so the answer is still matched by purpose, kind or chat id.
- A stage that names a timer or a watcher whose task is `.detach()`ed, with
  a counter to tell old wake-ups from the current one.
- A timer in a loop that keeps running while the window is hidden, the app
  is locked or the account is logged out.
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

### The shape guards

Two full sweeps of the source (2026-11-09 and 2026-11-11) kept finding the
same few shapes in code that had just been fixed. Each of these checks
holds one of them. They work like the check above: every hit in the tree
when the check was added is in a baseline file, the baseline only shrinks,
and a new hit fails. A hit is listed by the type and function it is in
(`rule:Type::function count`), not by path or line, so moving code does
not change it. `--list` prints every hit with its file and line.

When you fix a hit, run the check with `--prune` and commit the smaller
baseline. A baseline line that is too high is printed but does not fail
the build, so a fix that lands on another branch never breaks yours.
`--prune` only lowers the list; nothing adds to it except a reviewed edit.

#### `check-stage-enums.py`: an in-flight stage holds what it waits for

`enum Load { Idle, Loading }` is `loading: bool` with a new name. It
cannot tell its own answer from another request's, and it cannot cancel
its work when it is replaced. The check rejects a variant named as work
in flight (`Loading`, `Asking`, `Sending`, `Sent`, `Waiting`, `Pending`,
`Running`, ...) that has no data (`bare`), or whose data holds no
`RequestId`, `Task`, channel end or deadline (`unkeyed`).

- Fix: `Loading { request: RequestId }`, compared when the answer comes,
  or `Fetch<T>`, which already does this. For local work hold the `Task`
  (`Decoding { _job: Task<()> }`), so leaving the stage cancels it.
- Not a stage of anything Quill started (a TDLib state shown as is, a
  step the user leaves): `scripts/stage-enums-allow.txt` with the reason.

#### `check-request-match.py`: an answer is matched by its `RequestId`

The check rejects two shortcuts. `purpose`: a call to
`requests.has_purpose(..)`, `take_purpose(..)` or `has_auth_submit()`.
"Some request with this purpose is in flight" is also true for another
window's request, an older one, or one for another chat, so the busy
state is wrong and a late answer or error ends the wrong stage.
`unmatched`: a call to `Fetch::succeed`, which takes the answer whatever
request it came from.

- Fix: record the request in the stage when it is sent
  (`fetch.sent(request)`), and settle it with
  `fetch.answered(Some(pending.id), value)` / `failed(..)`. Ask "is mine
  in flight" with `fetch.waits_for(id)` or `is_loading()`.
- Needs no matching: `scripts/request-match-allow.txt` with the reason.

#### `check-owned-timers.py`: a timer belongs to the stage that waits for it

`detached`: a task that sleeps on a timer, or loops as a watcher,
and is then `.detach()`ed. Nothing can cancel it, so it fires after its
box, chat, account or window is gone, and the code grows a generation
counter to ignore the old wake-ups. `poll`: a `loop` or `while` that
sleeps on a bare timer. It keeps waking while the window is hidden, the
app is locked or the account is logged out.

- Fix for `detached`: hold the `Task` in the stage
  (`Shown { toast, _hide: Task<()> }`). A new toast replaces the value and
  the old timer goes with it, so the counter is not needed.
- Fix for `poll`: wait for the event. A subscription or observer, a
  channel, `wake_wait::sleep_or_wake` on a `quill::wake` signal (it sleeps
  until poked), or the frame clock (`request_animation_tick`) for
  anything that only matters while it is drawn.
- Must run on a clock: `scripts/owned-timers-allow.txt` with what ends
  it and why there is no event.

#### `check-refcell-borrows.py`

The fourth guard counts `RefCell` borrows that can panic. It is described
in [panic-safety.md](panic-safety.md), rule 6.

#### What the guards do not cover

Reviewers still check these by hand: a request sent again from render or
upkeep after it failed, two fields that must change together, an `Option`
used as a "started" flag, a two-variant enum that is a bool with other
names, and state that outlives its owner.

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
