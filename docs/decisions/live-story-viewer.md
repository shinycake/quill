# Live story viewer

Design: `quill-tools/designs/group-call-streams.md`, build plan item 3
(with its "Corrections after PR 1–2"). tdesktop references are to
`Telegram/SourceFiles` at `d8594c0`. Where tdesktop and the design differ,
Quill follows tdesktop.

## What tdesktop does

- `media/stories/media_stories_controller.cpp`: a live story joins its
  video stream as soon as it is shown, and leaves when the viewer moves
  on or closes. A live story never finishes on its own (no progress bar,
  no timer); when the stream ends the viewer closes.
- `media/stories/media_stories_header.cpp`: for a video stream the header
  shows the red "LIVE" badge (`lng_video_stream_live`,
  `groupCallMessageBadge` style) and the viewers line; the caption,
  counters and reaction row are hidden.
- `calls/group/calls_group_messages_ui.cpp`: comments sit over the lower
  part of the stream, 8 px in from the left, the frame's width less 16,
  newest at the bottom, scrolling. Each comment is a rounded block
  (`groupCallMessagePadding` 8,3,8,2, 20 px userpic) on a dark background
  at `kMessageBgOpacity` 0.8; a paid one takes its level's color at
  `kColoredMessageBgOpacity` 0.65 and shows its Stars; the owner's shows
  the LIVE badge and "admin" (`lng_admin_badge`, `kAdminBadgeTextOpacity`
  0.6). The top three donors get crowns with their place. Paid comments
  are pinned along the top (22 px userpic, at most 96 px wide) with a
  shrinking pin time until it runs out. Right click: the date
  ("Sent today at ..."), Copy Text, and Delete when it can be deleted; a
  moderator's delete box adds Report Spam and "Delete all from {user}".
- `calls/group/calls_group_messages.cpp`: comment levels (tdesktop
  "colorings") decide pin time, text and emoji limits and colors; at most
  100 comments are kept; Stars sent show as a rising effect over the
  stream; top donors are polled every 30 s (`kStarsStatsShortPollDelay`).
- `media/stories/media_stories_reply.cpp`: the composer for a stream is
  the comments toggle, the send-as button (only with more than one
  sender), the field ("Comment" or "Comment for {n} Stars",
  `lng_video_stream_comment_ph` / `_paid_ph`), emoji, an "edit Stars"
  button, send, and the Stars button. On your own live story comments
  are free (`canManage` → price 0). With comments off, "Comments are
  disabled." replaces the field.
- `calls/group/calls_group_stars_box.cpp`: the Stars box, starting on
  `kDefaultStars` 10 with a slider up to the app config maximum
  (`kMaxStarsFallback` 10000). Three uses: a paid comment (level blocks
  for pin time, characters and emoji, "Highlight and Pin"), a reaction
  (title, about, top donors, terms link, "Send ★N") and the owner's own
  story (`lng_paid_admin_title`, `lng_paid_react_admin_cant`, OK).
- Paid reactions (`data/data_message_reactions.cpp`,
  `kPaidAccumulatePeriod`): taps add up and are committed 5.5 s after the
  last one; a toast "Stars Sent!" with a countdown ring and Undo shows
  meanwhile.

## What changed

- Core (`src/live_story.rs`, `src/state/domains/stories/live.rs`): the
  levels table and its normalizing, which comments stay, the comments
  toggle state, the pending reaction, and `LiveStoryState` (streamer,
  donors, senders, send-as, effects).
- Connect (`src/connect/group_calls/live_story.rs`, `src/connect/stories`):
  the join answer asks for the streamer, the top donors and the
  available senders together; comment send with send-as and Stars;
  reaction add, commit after the window, and undo; delete with report
  and delete-all; a looked-up call never replaces the joined one; leaving
  drops pending Stars. Frame accessors read the stream slot (RTMP) or the
  streamer's video tile.
- UI (`src/ui/story_viewer/live_*.rs`): header badge and viewers, stream
  picture (`StreamTile`, Cover), comments, pinned strip, effects, toast,
  composer, Stars box and delete box (kit `Dialog`, `Slider`, `Checkbox`,
  `Textarea`, `Button`, `Icon`).
- Strings in `lang/en/stories_live.strings`, copied from tdesktop.
  `qll_media_rtmp_playback_is_not_supported` is removed: the RTMP
  placeholder in the viewer is gone, every live story with a call is
  played.

## Not like the design (tdesktop wins)

- The stream ending closes the viewer, as tdesktop does, instead of a
  2 s "Live stream ended" placeholder and advancing.
- No progress bar, caption, counters or reaction row on a live story.
- The LIVE badge is tdesktop's `groupCallMessageBadge` (11 px, radius 5,
  8 px semibold) rather than a 20 px `danger` pill, and the header reads
  "{count} viewers" (`lng_group_call_rtmp_viewers`), not "watching".
- Comments follow tdesktop's layout (a scrolling list over the lower
  part with backgrounds by level) instead of pills fading after 8 s.
- The top donors are shown as crowns on their comments and in the Stars
  box, not as a row under the header.
- Added from tdesktop and missing from the design: the pinned strip, the
  comments toggle, the comment context menu and delete box, the three
  Stars box modes, the "edit Stars" button and the send button's
  "Remove Stars" menu.
- Send-as shows only with two or more senders (as the design says, and
  as tdesktop does).
- Reactions batch for 5.5 s with an Undo toast, not one request per
  500 ms.
- Volume: the header has a sound toggle (mute) only, as tdesktop does; no
  volume slider.

## Not like tdesktop (Quill)

- One call slot: watching a live story uses the same tracked group call
  as a video chat, so the video chat bar and window ignore a live story
  call (`group_call_panel.rs`, `group_call_window.rs`).
- Stars effects are drawn with GPUI (a rising ★N pill and a six-star
  burst, `live_view.rs`), not tdesktop's Lottie animation.
- The toast's countdown ring is a small `canvas` arc: the kit has no
  circular countdown component.
- Comments are plain text: entities are not drawn and custom emoji are
  inserted as their fallback emoji. Stickers and GIFs from the emoji
  panel are ignored while watching a live story.
- No "sending" arc on an outgoing comment; a failed one stays, in
  italics, with "Failed to send the message." (Quill-only string).
- "Admin" means TDLib's `can_be_managed` on the call.
- Ban User is not offered in the delete box: tdesktop shows it for some
  chats, and Quill has no ban flow from here yet.
- No top-donor polling: TDLib sends `updateLiveStoryTopDonors`.
- "This live story can't be played in Quill yet." is a Quill-only state
  for a stream the engine reports as unsupported.
- Clicks on the stream and the composer no longer fall through to the
  viewer's backdrop (which closed the viewer).

## Boundary notes

Outside `src/ui/story_viewer/*`: `src/ui/stories_state.rs` (one field for
the live UI state), `src/ui/group_call_window.rs` and
`src/ui/group_call_panel.rs` (ignore a live story call, see above).
`src/ui/media_viewer/*` and `src/ui/video_player/*` are untouched.

## How verified

- Tests: `src/live_story/tests.rs`, `src/state/domains/stories/live/tests.rs`,
  `src/connect/tests/live_story.rs` (seven driver tests: the join answer,
  Star taps, no Stars on your own story or past the balance, comments,
  send-as, leaving, looked-up calls) and `src/ui/story_viewer/live_tests.rs`
  (send button rules, pinned comments, effect timing, and GPUI flows:
  layout, Star tap and Undo, comments toggle, Stars box, comments
  disabled, delete from the menu, stream end closes the viewer).
- Screenshots from the `ready-live-story`, `ready-live-story-stars` and
  `ready-live-story-box` demos (`QUILL_DEMO_CAPTURE`).
- Not tried against a real live story: the brief forbids joining one.
  Whether TDLib echoes your own committed paid reaction as an effect is
  unverified.

## Parity

No fragment for `stories-live-stream-playback`: the list above has
deviations (plain-text comments, no Lottie effects, no Ban User, one
call slot), so it does not fully match tdesktop yet.
