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
`origin/main` d3e5b79e (`python3 scripts/check-window-scope.py` lists all
57 against that tree):

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
| `media_viewer/playback.rs:510`, `media_viewer/controller.rs:268`, `media_viewer/pip.rs:77`, `selection_drag.rs:308`, `autoscroll_ui.rs:63`, `send_box_options.rs:133`, `send_box_thumbnails.rs:53,63`, `video_editor*.rs`, `profile_photo_sources/video.rs`, `recording.rs:501`, `drop_zones.rs:111`, `polls/option_attach.rs`, `sticker_set_box/creator.rs:170`, `custom_emoji_card.rs:59`, `entity_links.rs:367`, `inline_video.rs:327`, `history_a11y.rs:396`, `app_render/mod.rs:105`, `demos/payments.rs:71` | Same pattern: window state touched by a timer, worker or deferred step without its scope. |
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
- `scripts/check-window-scope.py` (CI step "window scope") flags any
  `cx.spawn`, `spawn_in`, `cx.defer`, `defer_in` or `on_next_frame` in
  `src/ui` whose closure touches window state, directly or through a
  `QuillApp` method that does, unless it uses one of the above or carries
  `// window-scope: <why>`. It reports all 57 sites on `origin/main`.

Trade-off: B is enforced by a lint, A would be enforced by the type system.
The lint matches methods by name, so it can flag something harmless (then
add the marker) or miss state reached through a receiver other than
`self`/`this`/`app`. B also drops work whose window has closed. That's
right for window state, but app-wide flags set from a window's work have to
use `spawn_main`, or they would stay set forever.

This is not performance work. The added cost is one `BTreeMap` remove and
insert on the session per scope change, which already swaps eight structs.

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
- Design A if the lint gets noisy.
