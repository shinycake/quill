# Chat window state for work that finishes later

Audit finding 13. A chat window's state lives in the shared `QuillApp`
fields only while that window is drawn or handles input:
`set_window_scope` swaps the window's `WindowSlot` (composer, history,
message menu, pickers, viewer, share box, chat search field) and the
session's view (open chat and topic, thread, Saved Messages views, chat
search, gallery) into place, and swaps them back at the main window's
render, its input and the poll loop. Anything that finishes later (a
timer, a file picker, a background job, a TDLib answer) ran with whatever
was in place then, which is usually the main window's state.

## What was affected

Async completions in `src/ui` that read or write window state, as of
`origin/main` d3e5b79e (the first version of
`scripts/check-window-scope.py` listed 57 against that tree; the version
tightened in the review round lists 82 against `origin/main` 80fb887c, see
"Review round"):

| Path | What went wrong in a chat window |
| --- | --- |
| `search_ui/mod.rs:596` chat search debounce | The commit ran with the main window's search state: the window's query was sent for the main window's chat. |
| `session_apply_messages.rs:293-313`, `domains/search/error.rs`, `domains/media/error.rs`, `session_thread.rs`, `session_saved.rs`, `session_search.rs:59` (answers) | Every TDLib answer applies in the poll loop with the main view in place. Chat search hits, gallery pages, thread and Saved Messages pages and jump-around pages asked for by a window were checked against the main view and dropped. If the main view had a search with the same generation (each view counts its own), the window's hits landed in the main window's search. |
| `media_viewer/actions.rs:184` saved toast | The window's toast never hid. When the counters matched, the window's timer hid the main window's toast early. |
| `media_viewer/actions.rs:523` controls auto-hide | The window's `hide_timer` flag stayed set, so its controls never hid again; the timer judged and hid the main viewer's controls instead. |
| `composer_ui/send_sticker_pick.rs:166` link preview, `inline_mode.rs:285` inline bot query | The debounce compared its token with the main composer's and dropped the work: no link preview and no inline results in a window's composer. |
| `chat.rs:76` slow mode, `secret_chats.rs:129` self-destruct, `live_location_tick.rs:39` | The 1 s tick checked the main window's open chat, stopped after one tick and left the window's guard set, so the countdown froze. |
| `composer/copy_inline_text.rs:34`, `edit_media_tools.rs:223` file pickers | The picked replacement file or cover went into the main window's pending edit, or nowhere. |
| `hdr_image/viewer.rs:201,229` | With matching tokens, a window's decoded HDR image went into the main viewer. |
| `media_viewer/playback.rs:510`, `media_viewer/controller.rs:268`, `media_viewer/pip.rs:77`, `selection_drag.rs:308`, `autoscroll_ui.rs:63`, `send_box_options.rs:133`, `send_box_thumbnails.rs:53,63`, `video_editor*.rs`, `profile_photo_sources/video.rs`, `drop_zones.rs:111`, `polls/option_attach.rs`, `sticker_set_box/creator.rs:170`, `custom_emoji_card.rs:59`, `entity_links.rs:367`, `inline_video.rs:327`, `history_a11y.rs:396`, `demos/payments.rs:71` | Same pattern: window state touched by a timer, worker or deferred step without its scope. |
| `app_render/mod.rs:105` video message auto-send, `inline_playback.rs:272` recording tick, `recording.rs:501` finished video message (fixed in the review round) | The recording is the app's, not a window's. A video message recorded in a window that hit the 60 s limit was sent from the main window's render, to the main window's chat. The tick sent "recording…" to whichever chat was in place. A finished video message whose window closed was dropped without a word. |
| `chat_window/slot.rs` `in_window_scope` | The existing scoped paths (history paging defers, pull-to-next, attach picker) fell back to the main window's state when their window had closed. |

The rest of the 57 are app-wide work (poll loop, background accounts,
passcode, spellcheck, mini apps, story viewer, playback tick); they now say
so with `spawn_main`.

Proven by tests that fail on `origin/main`: the chat search answer (core
and driver), the saved toast timer in a window and from a closed window
(UI), and the closed window's search answer landing in the main view
(core).

## Designs considered

**A. Per-window state keyed by window, no swapping.** `QuillApp` would hold
every window's state in a map and code would reach it through an accessor
that takes the window. Wrong-window writes become a type error. But the
swapped fields are read about 2,000 times across `src/ui`, and every one of
those call sites would need the window passed in. That is a rewrite of the
conversation, composer and overlay code, not one PR.

**B. Keep the swap for code that runs inside a window; make later work
carry its window. (Chosen.)** Rendering and input already know their
window, so the swap is correct there. What breaks is work that outlives
the scope. B gives that work the window as a key:

- The session keeps each window's view by chat (`WindowViews`, which
  replaces the `window_chats` set) and puts one in place with
  `Session::enter_view`. Each request records the view in place when it was
  registered (`PendingRequest::window`). `Session::apply` routes an answer
  whose purpose lands in a view (`lands_in_view`) to that view. An answer
  for a window that has closed applies with a throwaway view, so its
  messages and files still land but no window's view is touched. The UI
  slot no longer carries a copy of the session view.
- UI async work starts with `spawn_scoped` / `spawn_scoped_in` /
  `defer_scoped`. The task receives a `ScopedApp` instead of the app's weak
  handle, and each `update` puts the starting window's state in place
  first. App-wide work uses `spawn_main` / `spawn_main_in`.
- `in_window_scope` runs nothing for a window that has closed, so that work
  can't fall back to the main window's state.
- `scripts/check-window-scope.py` (CI step "window scope") flags async
  calls in `src/ui` whose closure touches window state without a scope.
  What it checks, the `// window-scope:` marker and its limits are under
  "Review round"; `docs/contributing/structure.md` (Window scope) has the
  rules for new code.

Trade-off: B is enforced by a lint, A would be enforced by the type system.
The lint matches methods by name, so it can flag something harmless (then
add the marker) or miss state it can't see (listed under "Review round").
B also drops work whose window has closed. That's right for window state,
but app-wide flags set from a window's work have to use `spawn_main`, or
they would stay set forever.

This is not performance work. The added cost is one `BTreeMap` remove and
insert on the session per scope change, which already swaps eight structs.

## Review round

The review (BLOCK) found a wrong-recipient send the table above had called
fixed, and blind spots in the lint. Changes:

**The recording belongs to its window.** `RecordingUi::window` records the
window a recording started in (`begin_voice_capture`,
`begin_video_note_capture`; the permission prompt's continuation now runs
with `spawn_scoped`, so it starts in the window that asked). From there
(`chat_window/recording.rs`):

- `send_recording` runs the send inside `in_window_scope(origin)`, whichever
  window asks, so it goes to that window's chat and uses its composer.
- A video message at its time limit is sent at the next draw of its own
  window (`flush_recording_auto_send`, called by the main window's render
  and by each chat window's), not by whichever window draws first.
- The 100 ms tick runs as a `spawn_main` task whose every step goes through
  `in_window_scope(origin)`, so `sync_voice_action` addresses the right chat.
- Closing the window (`forget_chat_window`, also on an account switch) ends
  the recording with a status note ("recording discarded — its chat window
  closed"). A video message whose window closes while the file is being
  finished is deleted with a note ("video note not sent — its chat window
  closed").
- The record bar, its Esc layers, and the cancels on a chat switch or the
  media panel opening use `recording_here()`: only the recording's own
  window shows or ends it.

Demo mode could not show a sent voice message at all (`apply_demo_voice`
reused a stale update number and a negative message id, so the session
dropped it); it now numbers it like `apply_demo_outgoing`. The UI tests
need that.

Tests (`chat_window/tests.rs`), each failing with the routing removed: a
video message at its limit from a chat window is not sent by the main
window's draw and goes to the window's chat on that window's draw; Send
asked with the main window's state in place still goes to the recording's
window (and a main-window recording still goes to the main chat); closing
the window ends the recording, and nothing reaches the main window's chat.

**Subscriptions and observers are scoped too.** GPUI calls a
`subscribe_in` / `observe_*` / `on_focus_*` callback with the window it was
made in but with whatever state is in place, and an edit made inside
another window's `in_window_scope` (restoring a draft into a new window's
composer) is delivered after that scope ends. `in_window_of(window, ...)`
runs a callback with its window's state in place; the compose field, the
chat search and share search fields, the link and code-language dialogs,
the send menu and its effect search, the start-files and suggested-price
fields, the passcode field, the history focus and the chat-width probe use
it. Viewer sliders (no window in `cx.subscribe`) record the scope they were
made in. Main-window-only work says so: the chat list search's Enter, the
history scroller observer, the secret codes (now ignored in chat windows),
notification popups, and "open in the main window" from a chat window.

**Lint.** `scripts/check-window-scope.py` now:

- also checks `window.spawn`, `window.defer`, `cx.subscribe*`,
  `cx.observe*`, `on_focus*` and `on_blur`;
- matches the swapped fields only on the app (`self`/`this`/`app` or the
  first parameter of any closure in the call: `|q, cx| q.composer`), which
  removes false hits like `this.stories.viewer`;
- follows `ConnectDriver` and `Session` methods in `src` that read or change
  the session's view, so `live.driver.commit_debounced_chat_search()` or
  `send_voice_note()` in a closure counts and `download_file(file_id)` does
  not;
- reads `QuillApp` methods only from `impl QuillApp` blocks, and only the
  closure itself (not the entity or window passed before it);
- skips only what runs inside `in_window_scope(...)` / `in_window_of(...)`,
  not the whole closure because the name appears somewhere in it.

Counts: the first lint found 57 on `origin/main` d3e5b79e and 0 on this
branch. The tightened lint finds 82 on `origin/main` 80fb887c and found 25
on this branch before this round, among them the recording tick
(`inline_playback.rs:272`). After this round it finds 0: 23 are fixed in
code, 2 carry a marker (the chat window's own open bookkeeping, and the
draft commit, whose pending draft names its chat).

Marker: a comment `// window-scope: <why>` on one of the three lines above
the call, for work that needs no window's state.

Limits the lint keeps: methods are matched by name, not type; a field
reached through a binding made outside the closure or through a helper on
another type is not seen; a function pointer passed in (`with_capture_access`
takes `start: fn(...)`) is not followed; `spawn_scoped*` / `spawn_main*`
bodies are not checked, they name their window by construction; and it
cannot tell a wrongly chosen window, which is how the recording send got
past it (`defer_scoped` from the main window's render captured the main
window).

**Session side.**

- An answer for a closed window applies with a throwaway view, and requests
  registered while it applies are stamped with that closed window, so their
  answers land nowhere either (they were stamped as the main window's). No
  apply path registers a request today, so this has no test of its own; the
  test checks the stamp is cleared afterwards.
- `Session::enter_view` records a `no-window-view` diagnostic when asked for
  a chat with no window. `set_window_scope` no longer puts a window's slot in
  place over the main window's session view when that happens, and
  `in_window_scope` then runs nothing. A chat window whose session view is
  gone closes at its next render.
- `GetChatSponsoredMessages` lands in the asking view, so a channel shown in
  its own window gets its sponsored rows.
- A test for an answer to one window arriving while another window's view
  is in place: the asking window gets its hits, the other keeps its view
  and its own search.

**Other.** The viewer's Now Playing tick clears the OS widget when its
window closes (unless something else plays).

## Follow-ups

- Updates (as opposed to answers) that touch a view still reach only the
  main view: thread rows on send succeeded/failed and on edit
  (`domains/messages/apply.rs:207,258`, `session_updates.rs:233`) and the
  chat-search jump on delete (`domains/messages/apply.rs:391`). They should
  run for every view in `WindowViews`.
- The driver keeps one `pending_typed_chat_search`, so two windows typing
  within one debounce race each other.
- State that is per window but kept per session: `messages.unread_jump`
  (jump to unread from a window is dropped), `messages.composer_preview`
  (one link preview per session) and `stickers.custom_emoji_preview` (the
  card shows in every window).
- A Picture-in-Picture window opened from a chat window outlives it,
  because the link lives in the closed window's viewer.
- A jump-around page (`GetHistoryAround`) for a window that closed is
  dropped, even if the main window now shows that chat. Left so: applying
  it would jump the main window to a message it never asked for.
- `drop_zones.rs:111` leaves its temporary archive behind when the window
  that asked has closed (from the review; not checked here).
- The tightened lint's limits above; Design A if it gets noisy.
