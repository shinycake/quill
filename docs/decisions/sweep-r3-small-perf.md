# Round 3 small items: composer keystroke, idle menu rebuilds, story emoji retry

Items 10 and 11 of the round 3 performance and memory review, and the
follow-up from the #1000 review. The composer and story-emoji changes keep behaviour. The menu change reads the
registry less often but is built to show the same changes as before (see its
section). No speed-up is claimed, nothing was measured.

## Composer, per keystroke (item 10)

`on_composer_event` ran, for every edit, the sticker suggestion update and the
animated-emoji update, each followed by its own `notify_composer`, and each
keystroke spawned a timer task for the debounced draft save.

- `sync_composer_suggestions` replaces the two calls. It asks the driver only
  when `Driver::composer_suggestions_idle(text)` is false. The driver is idle
  for a text when the path is live, the text does not end in an emoji, and no
  sticker suggestion, animated emoji, marker or request is held; in that case
  both updates would only clear what is already clear. A held suggestion is
  still cleared by the next plain keystroke, and a path that is down still
  shows the error note, as before. The composer is notified once, always, as
  it was (other parts of the composer redraw on that notify).
- `DraftTimer` (`ui/draft_timer.rs`) is the one timer behind the debounced draft
  save. Every keystroke moves the due time and token; one task waits for the due
  time and commits the latest token, then ends. Before, a task per keystroke
  slept and all but the last found a stale token. The delay is the same
  `DRAFT_SAVE_TIMEOUT_MS` quiet window from the last keystroke, and the driver's
  token check is unchanged.
- The two text reads that remain (the event's text and the one after a markdown
  replacement) read different text, so they stay.

## Idle wakeups, Dock and Window menus (item 11)

Each applied update batch pokes `wake::CHROME`, and the menu task then read the
account registry from disk and built the Dock model. `MenuSync`
(`ui/os_menus/sync.rs`) now builds the model from memory against the registry
last read, and reads the registry again only when that model or the Window menu
state differs from what is installed, when the last read is 10 s old (the
timeout that already existed), when nothing was read yet, or when something the
registry depends on moved. That last case is the part memory cannot see: the
model built from a cached registry stays equal after a sign-in or an account
removal, so it would not show until the 10 s were up. Two checks
force the read on the next poke instead:

- Every write of `accounts.json` goes through `save_registry`, which counts it
  (`quill::settings::registry_writes`). The menu task compares the count with
  the one stored at the last read.
- The keys of the background clients (added and removed in many places, from
  `ensure_background_accounts` to the account switch) are compared with the keys
  at the last read, so no add or remove path has to remember to invalidate.

A change the menus show, such as an account signing in, the lock, the
notification toggle or the player, still reads the registry and rebuilds as
before. The
`DockModel` has no unread count; the badge has its own sync.

## Story emoji fetch error (follow-up to #1000)

`GetStoryCustomEmojiStickers` had no error handler, so after a TDLib error the
viewer's `emoji_warm` record (recorded when the request was sent) kept
the fetch from running again while the viewer stayed on one story.

`apply_stories_error` now calls `StoriesState::custom_emoji_fetch_failed`. It
raises `custom_emoji_failures`, which is part of the `emoji_warm` key
(`(story, ids, stickers known, failures)`), so the record no longer matches, and
sets `custom_emoji_retry`, which doubles from 5 s up to 5 min like the other
story fetches, so a persistent error does not ask every 100 ms tick. An answer
clears the wait. The UI keeps no second copy of the state: the key is derived
from the session. The wording in `sweep-r3-overlay-notify.md` now says the
record is made when the request is sent, not when it succeeds.

## Tests

- `ui/draft_timer.rs`: a burst of keystrokes spawns one task; the latest
  keystroke decides when and what commits; a new window after a commit spawns
  again.
- `connect/tests/composer_suggestions_idle.rs`: plain typing is idle, a trailing
  emoji is not, held suggestions are cleared by the next plain keystroke, a down
  path is never idle.
- `ui/composer_keystroke_tests.rs`: after each keystroke the field text, the
  remembered previous text and the stored draft agree, and the composer slice
  redraws.
- `ui/os_menus/sync.rs`: batches that change nothing rebuild nothing and read no
  registry; a change reads the registry and rebuilds the Dock; an old registry
  is read again; a registry write or a change in the background client keys
  reads it at once.
- `settings/accounts.rs`: adding, switching, removing and marking an account
  for deletion, and the notify setting, each move `registry_writes`.
- `state/tests/story_tray_load.rs` and `ui/story_viewer/tick_notify_tests.rs`: a
  TDLib error for the emoji fetch is counted and backs off; the next tick after
  the wait fetches again and records again.

## Verified

The `ready-chats-composer` demo in light and dark shows the composer rendering
(its text is seeded; scripted key presses did not change it). The demo has no
live driver, so sticker and animated-emoji suggestions, the draft timer commit
and the Dock menu are covered by the tests above, not by a capture.
