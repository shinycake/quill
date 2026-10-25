# Message effects and video userpics in chat windows

Parity: `parity:premium-message-effects-play` (closes its last gap).
Follows `message-effects-playback.md` and `animated-userpics-history.md`.
tdesktop sources are under `Telegram/SourceFiles/`.

## What tdesktop does

- Every window has its own history widget, and each widget its own
  `HistoryView::EmojiInteractions` (`history_inner_widget.cpp:378`,
  `history_view_list_widget.cpp:541`). The widget's paint collects the
  messages whose effect has not played and calls `playEffectOnRead` for
  them while its window is marking contents read
  (`HistoryWidget::markingContentsRead` → the window's `markingAsRead`).
  The watched flag lives on the message, so a message plays its effect once
  whichever window shows it.
- Premium sticker effects and emoji interactions play from the same
  per-widget object; `_animatedStickersPlayed` is per widget.
- `validateVideoUserpic` keeps one `VideoUserpic` map per widget, so the
  history of a separate window plays its senders' clips with its own
  players, paused for that window's `GifPauseReason`s.

## What Quill had

Chat windows reuse the main window's conversation code with their state
swapped in (`chat_window/slot.rs`), but:

- The effect overlay (`emoji_effect`) was built by the main window's
  conversation slot only, badges painted outside the main window's chat
  were not noted, and the play pass dropped everything not in the main
  window's chat.
- The userpic pool had one history surface. The main window's and a chat
  window's history passes dropped each other's players, so a chat window's
  clips never played.
- The driver described effects and fetched thumbnails (effect icons and
  animations included) for the main window's chat only. A message arriving
  in a chat window had no badge icon and no animation to play.

## What changed

- One overlay per window, the same code: `emoji_effect_overlay` handles
  the chat of the window being built (`viewed_chat`) and takes that
  window's activity (`HistoryUi::window_active`, which the main window
  already clears while locked). The chat window's body builds it before its
  history, as the main window does, and draws it over its conversation.
- The paint notes (emoji anchors, effect badges, Premium stickers) stay
  keyed by chat; each chat shows in one window at most. A history pass
  forgets only its own window's notes. Waiting and playing effects of
  another window's chat wait for its pass; those of a chat no window shows
  go. Badge clicks are taken by the window of their chat.
- Premium sticker "already played" sets are per chat and dropped when no
  window shows the chat (was: reset whenever the viewed chat changed, which
  two windows would do every frame).
- `Session::chat_is_shown` also counts the main window's chat while a chat
  window's view is in place; `Session::chats_shown` lists every chat on
  screen.
- Userpic pool: `Surface::History(Option<ChatId>)`, one per window, with
  pass counts and redraw targets per surface. "Held" splits into
  `inactive` (no Quill window in front, as before) and per-window covers,
  so the main window's media viewer does not freeze a chat window's clips.
  A sweep after a pass judges only the surfaces that rendered, so a pass in
  one window no longer pauses another window's clips that had not painted
  yet; the timer sweep judges every slot not held. A closed window's
  players go.
- Driver: `maybe_download_shown_thumbs` runs the open-chat thumbnail pass
  for the main window's chat and then each window chat
  (`with_window_chat`), so live messages there get their effect described
  and its icon and animation downloaded. Photo thumbnails of live messages
  in chat windows now download too.
- Demos: `QUILL_DEMO_EFFECTS=window`, `QUILL_DEMO_USERPICS=window`.

## Not done, or different

- The deviations listed in the two earlier docs still hold (unknown read
  marker counts as read; Autoplay GIFs gates userpics; 24-decoder cap,
  now shared by every window).
- The userpic decode size uses the main window's display scale for every
  window; a chat window on a display with another scale decodes at the
  main window's.
- Inline GIF and round video players (`inline_players`) still share one
  history pass count between windows. Not part of this item.

## Verification

- Driver test `a_live_message_gets_its_effect_ready_in_the_main_window_and_a_chat_window`
  (`src/connect/tests/message_effects.rs`): an `updateNewMessage` alone,
  in the main window's chat and in a chat window's, leaves the effect to
  play, asks `getMessageEffect`, and after the answer downloads the icon
  and the animation.
- GPUI tests: `a_chat_in_its_own_window_plays_its_effects_there`
  (`src/ui/send_effects/tests.rs`: an unread message arriving in a chat
  window plays its effect there, a main-window draw leaves it playing, a
  badge tap in the window replays it) and
  `a_chat_in_its_own_window_plays_its_senders_userpics`
  (`src/ui/video_player/tests/userpics.rs`: players per sender on the
  window's surface, still playing after the main window draws, dropped
  when the window closes). Unit tests for the per-window badge notes,
  clicks and Premium sticker sets.
- `/verify` light and dark, chat window captured with
  `QUILL_DEMO_CAPTURE_WINDOWS=1`: the effect plays beside the arriving
  message's badge, placed as in the main window; the group's senders show
  their clips in the chat window.
- Not run on a live account. Real effect animations, real Premium users'
  clips, and two windows on displays with different scales are untested.
