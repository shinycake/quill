# Story messages and paid media

## What tdesktop does

- `MediaStory` (`data/data_media_types.cpp`) handles `messageStory`. A
  forwarded story shows the story's own photo or video with its caption; once
  `Data::Stories` says it is gone, the item shows "This story has expired."
  (`lng_forwarded_story_expired`). A story mention is a service box
  (`history_view_story_mention.cpp`): "{user} mentioned you in a story" (or
  "You mentioned {user} in a story"), the story as a round picture and a
  "View Story" button; an expired one reads "The story where {user}
  mentioned you is no longer available." Clicking opens the story.
- Paid media (`history_view_photo.cpp` with `extendedMediaPreview`, and
  `MakePaidMediaLink` in `history_view_media_common.cpp`): the inline preview
  with a spoiler tag "Unlock for {price}"; a press starts the Stars checkout
  of the message. After payment the message is replaced by the real photo or
  video.

## What changed

- `ServiceAction::Story` keeps `story_poster_chat_id` and `story_id`.
  Mention wording names the chat peer like tdesktop.
- `telegram/envelope/paid_media.rs`: a bought `messagePaidMedia`
  (`paidMediaPhoto` / `paidMediaVideo`) parses as an ordinary photo or video
  message, so the existing renderers draw it. Locked media stays a
  `ServiceAction::PaidMedia`.
- `state/session_message_story.rs`: the open chat's story messages feed the
  existing reply-story `getStory` prefetch and picture download; a failed
  `getStory` marks the story expired (`StoriesState::missing`).
- `ui/message_media/paid_media.rs`: the locked card uses the photo frame
  policy, blurs the minithumbnail (`wallpaper_raster::blur`, cached), shows
  "Unlock for ★N" and opens the Stars checkout (`InputInvoice::Message`,
  the flow added in #718). Quill never pays without the checkout's Pay press.
- `ui/message_media/story_card.rs` + `action_card.rs`: forwarded stories are
  bubble cards (picture, "Story" badge, video duration, caption, expired
  note); mentions stay service rows with the round picture and "View Story".
  Opening a story that is not in the poster's tray fetches it and opens the
  viewer on that single story (`story_viewer/mod.rs`).

## Follow-up in the same PR

- Paid-media albums render every item. Locked: each preview in Quill's album
  layout (`quill::album`), all under the spoiler surface and one unlock
  overlay. Bought: `ServiceAction::PaidMedia.unlocked` holds the photos and
  videos, drawn with the album tiles; the media viewer pages through them
  (`collect_media_items` emits one item per bought item, `open_media_viewer_at`
  picks the tapped one). A single bought item is still an ordinary photo or
  video message.
- The locked preview wears the spoiler surface (`spoiler_surface` in
  `message_media/surfaces.rs`, split out of `spoiler_cover`): the cached blurred
  minithumbnail, the darkening and the shared speck "mess". This replaced the
  first version's own blur.
- "Media Unlocked. N Stars transferred to {chat}." goes to the status bar after
  a successful Stars payment for a paid-media message
  (`Session::note_paid_media_unlocked`, drained in `notifications/mod.rs`).
  Quill has no toast widget; the status bar is its notice surface.
- The mention picture sits in the shared story ring (`with_story_ring`): the
  unread gradient while the story is active and newer than the poster's read
  mark, the thin muted ring otherwise.

## Not done

- A bought album's tiles are not grouped with neighbouring messages.
- Nothing else known.

## Verification

- Unit tests: envelope parse (locked, bought photo, bought album, story ids),
  service wording, state card state, unread ring state, Media Unlocked note
  (paid media yes, plain message no), viewer items for a bought album, checkout
  routing, frame policy, album layout, expired mention text, card selection.
- Temporary demo captures (not committed) showed: the locked single card, the
  locked three-item album under the specks, a bought album, mention rows with a
  clipped round picture and the unread ring, the expired forwarded story card
  and the expired mention wording.
- Not exercised: a real purchase, a real story viewer open from a card.
