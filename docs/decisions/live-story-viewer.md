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
  shows the poster's name, the red "LIVE" badge (`lng_video_stream_live`,
  `groupCallMessageBadge` style) and the viewers line; the caption,
  counters and reaction row are hidden.
- `media/view/media_view.style`: the viewer is dark in every theme. Its
  controls take the group call palette: `storiesComposeBg` =
  `groupCallMembersBg`, white text, `storiesComposeGrayText` for the
  placeholder, `storiesComposeGrayIcon` for the icons,
  `storiesComposeBlue` for Send.
- `media/streaming/media_streaming_utility.cpp` `DecideFrameResize`: the
  picture fills the frame only when nothing is cut at the sides and at
  most a quarter of its height is; otherwise it is shown whole over
  `PrepareBlurredBackground` (the middle at the frame's proportions,
  scaled to 40 px, blurred, darkened by 48/255).
- `calls/group/calls_group_messages_ui.cpp`: comments sit over the lower
  part of the stream, 8 px in from the left, the frame's width less 16,
  newest at the bottom, scrolling. Each is "[crown] **short name** text"
  with the comment's own formatting and custom emoji (`nameText`); the
  crown is white with the donor's place knocked out of it (`CrownMask`).
  In video stream mode only two kinds get a background: the owner's on
  `groupCallBg` at `kMessageBgOpacity` 0.8 (with the LIVE badge and
  "admin", `kAdminBadgeTextOpacity` 0.6), and a paid one in its level's
  color at `kColoredMessageBgOpacity` 0.65 with its Stars. Paid comments
  are pinned along the top (22 px userpic, at most 96 px wide) with a
  shrinking pin time. Right click: the date ("Sent today at ..."), Copy
  Text, and Delete when it can be deleted; a moderator's delete box adds
  Report Spam, "Delete all from {user}" and Ban User (`deleteConfirmed`:
  `chatParticipants().kick` in a channel, `blockedPeers().block` for a
  user).
- `calls/group/calls_group_messages.cpp`: comment levels decide pin time,
  text and emoji limits and colors; comments allow bold, italic,
  underline, strikethrough, spoiler, custom emoji and dates; at most 100
  are kept; Stars sent show as a rising effect; top donors are polled
  every 30 s (`kStarsStatsShortPollDelay`).
- `history/view/controls/history_view_compose_controls.cpp`
  (`updateControlsGeometry`, `paintBackground`) with
  `media/stories/media_stories_reply.cpp`: the comments toggle on its own
  round background, 8 px, one rounded field (radius 21) holding send-as
  (two or more senders), the text ("Comment" / "Comment for {n} Stars"),
  emoji, Edit Stars and Send, 8 px, and the Stars button with the total
  under the star. The emoji panel has no sticker or GIF tabs. On your own
  live story comments are free. With comments off, "Comments disabled."
  takes the field's place. Right click on Send: Edit Stars, Remove Stars.
- `calls/group/calls_group_stars_box.cpp`: the Stars box (default 10, a
  slider up to the app config maximum) for a paid comment, a reaction or
  the owner's own story.
- Paid reactions (`data/data_message_reactions.cpp`,
  `kPaidAccumulatePeriod`): taps add up and are committed 5.5 s after the
  last one under a "Stars Sent!" toast with a countdown ring and Undo.
  Past the balance, `MaybeRequestBalanceIncrease` opens the small balance
  box ("{n} Stars Needed") to buy Stars.
- `calls/calls_instance.cpp`: a live story's stream is a call of its own
  (`registerVideoStream`), beside the current call.

## What changed

- Core (`src/live_story.rs`, `src/state/domains/stories/live.rs`): the
  levels table, which comments stay, the comments toggle, the pending
  reaction, `LiveStoryState`; `comment_text` turns the field's markup
  into a comment's text and allowed entities; the Stars total counts
  committed Stars until the donors update arrives; the spendable balance
  counts Stars held by pending reactions and paid messages.
- Connect (`src/connect/group_calls/live_story*.rs`, `src/connect/stories`):
  join, the extras after the join answer, comments with entities and
  Stars, reactions, delete, Ban User, custom emoji lookups for comments.
  A call you only looked at gives way to a live story (dropped locally,
  nothing sent); a joined call keeps the slot; an update for another
  call never replaces the watched live story or a joined call (checked
  first in `accept_group_call_update`); a late `getGroupCall` answer for
  a live story you stepped past is ignored; leaving an unjoined live
  story sends nothing. A call dropped locally (only looked at, or given
  up for a live story) also loses its native transport
  (`drop_group_call_locally`); the pump's before/after check only sees
  reducer updates. After `leaveGroupCall`, a late joined update for that
  call cannot take the slot back until TDLib says it is left
  (`left_group_call`). Group call video slots hold `Arc<VideoFrame>`, so a
  reader no longer copies the picture while the slot is locked.
- UI (`src/ui/story_viewer/`): `chrome.rs` (the fixed dark palette, used by
  the whole viewer, normal stories too), `live_view.rs` (header, picture
  with `DecideFrameResize` and the blurred backdrop, toast, effects),
  `live_comments.rs` (comments with formatting, custom emoji and the
  knocked-out crown, kit context menu, pinned strip), `live_composer.rs`
  (the tdesktop layout, kit menus for Send and send-as),
  `live_stars_box.rs` (Stars box, delete box with Ban User, the
  leave-other-call confirm). `src/ui/calls/`: `frame_expands` and
  `frame_backdrop` beside the stream tile and picture.
- Strings in `lang/en/stories_live.strings`, copied from tdesktop, plus
  two Quill ones for the comments toggle's accessibility label.

## Not like the design (tdesktop wins)

- The stream ending closes the viewer instead of a 2 s placeholder.
- No progress bar, caption, counters or reaction row on a live story.
- The LIVE badge is tdesktop's `groupCallMessageBadge`, and the header
  reads "{count} viewers" (`lng_group_call_rtmp_viewers`).
- Comments follow tdesktop's scrolling list instead of pills fading
  after 8 s; top donors are crowns on their comments and in the Stars
  box, not a row under the header.
- Added from tdesktop: the pinned strip, the comments toggle, the comment
  menu and delete box, the three Stars box modes, Edit Stars and the
  Send menu.
- Reactions batch for 5.5 s with an Undo toast, not one request per
  500 ms.
- The header has a sound toggle (mute) only, as tdesktop does.

## Not like tdesktop (Quill)

- One call slot. tdesktop's current call keeps the slot (a 1:1 call, or a
  video chat that is joined, joining, reconnecting after a network loss,
  or scheduled), and the viewer asks first, with tdesktop's
  `confirmLeaveCurrent` words (`lng_call_leave_to_other_sure_channel`,
  `lng_group_call_leave_to_other_sure_channel`, ...), to leave it. The
  join is retried until the slot is free; after 10 s (a 1:1 call stuck
  hanging up) the viewer says it could not join and stops. tdesktop plays the stream
  beside the call instead. Running two calls at once needs a second
  call slot in `Session` and the engine, which is beyond this PR.
- Stars effects are drawn with GPUI (a rising ★N pill and a burst of
  stars), not tdesktop's Lottie, on purpose. TDLib serves no animation
  for a paid reaction: `reactionTypePaid` has no fields, and
  `getEmojiReaction` only covers emoji, so there is nothing to download.
  tdesktop bundles its own `Resources/animations/star_reaction/effect1-3.tgs`,
  part of the GPL tdesktop tree, and Quill is MIT. Using those files is
  the owner's call.
- The crown's digit is drawn with stroked paths knocked out of an SVG
  crown (`live_comments.rs`), rather than tdesktop's 20 px icon and 9 px
  font.
- The composer's buttons are 36 px instead of 42 so that "Comment for
  1,000 Stars" fits the story's 360 px; the field's placeholder is drawn
  by Quill (the kit's takes the theme's muted color).
- Past the balance, Quill shows "{n} Stars Needed" and opens its Stars box
  with the packs ("Buy more Stars") instead of tdesktop's small balance
  box; the comment or reaction is not resent after a purchase, it stays
  for another try, and the Stars box opens again on the amount chosen.
- The toast hides once the Stars are due, since Undo can no longer take
  them back.
- No "sending" arc on an outgoing comment; a failed one stays, in
  italics, with "Failed to send the message." (Quill-only string).
- "Admin" means TDLib's `can_be_managed` on the call.
- No top-donor polling: TDLib sends `updateLiveStoryTopDonors`.
- "This live story can't be played in Quill yet." is a Quill-only state
  for a stream the engine reports as unsupported.
- Clicks on the stream and the composer do not fall through to the
  viewer's backdrop (which closes the viewer).

## Numbers

Reading the live story's picture from a video slot: before, the slot's
`VideoFrame` was cloned while the lock was held (3,686,400 bytes for a
1280 × 720 frame) and then copied again into the GPUI image; now an `Arc`
is taken under the lock and the only copy is the image, outside it. That
is one 3.7 MB copy per frame fewer, and none under the lock. The blurred
backdrop is at most 40 × 40 pixels (6.4 KB) per frame, made in the same
background task.

## Boundary notes

Outside `src/ui/story_viewer/*`: `src/ui/stories_state.rs` (one field, and the
search field draws no placeholder of its own),
`src/ui/group_call_window.rs` and `src/ui/group_call_panel.rs` (ignore a
live story call; `ACTIVE_FG` shared with the viewer palette),
`src/ui/story_more.rs` (Space and the arrows go to the focused comment
field; the viewer palette, the share and close-friends search fields and
checkbox labels included), `src/ui/dialogs/group_call_create.rs`
and `src/ui/group_calls.rs` (drop a call with its transport), `src/ui/app_render/mod.rs` (the window passed
to that check), `src/ui/media_panel/custom_emoji_cell.rs` (emoji tab only
for a live comment), `src/ui/calls/stream_tile.rs` and
`stream_picture.rs` (fit and backdrop), `src/connect/calls/mod.rs`,
`src/connect/call_screen.rs` and `src/connect/types.rs` (`Arc` slots).
`src/ui/media_viewer/*` and `src/ui/video_player/*` are untouched.

## How verified

- Tests: `src/live_story/tests.rs`, `src/state/domains/stories/live/tests.rs`,
  `src/connect/tests/live_story.rs` (15 driver tests, among them stepping
  past a live story before its call arrives, a looked-at video chat
  giving way, a joined one keeping the slot, another live call not
  replacing the watched one, pending Stars counted against a paid
  comment, the total holding until the donors update, comments keeping
  their formatting, Ban User, a reconnecting video chat keeping the
  slot, a dropped call losing its transport, a late joined update for
  the call just left, a scheduled video chat being asked about), `src/ui/calls/stream_tile.rs` and
  `stream_picture.rs` (fit and backdrop), `src/ui/story_viewer/live_comments.rs`
  (the comment line and crowns) and `src/ui/story_viewer/live_tests.rs`
  (GPUI flows: layout, Star tap and Undo, the toast leaving when due,
  comments toggle, Stars box and the Send menu, comments disabled,
  delete from the kit menu, the stream ending, typing "a b" with Space,
  landscape and portrait fit, the composer layout and placeholder, the
  Stars top-up and the box keeping its amount, the leave-other-call
  confirm, a join that cannot proceed, the emoji panel closing with the
  reply row).
- Screenshots in light and dark themes from `ready-live-story`,
  `ready-live-story-stars`, `ready-live-story-box` and
  `ready-live-story-portrait` (`QUILL_DEMO_CAPTURE`).
- Not tried against a real live story: the brief forbids joining one.
  Whether TDLib echoes your own committed paid reaction as an effect is
  unverified.

## Parity

No fragment for `stories-live-stream-playback`: one call slot and the
GPUI Star effect still differ from tdesktop, and neither is an engine
limit only.
