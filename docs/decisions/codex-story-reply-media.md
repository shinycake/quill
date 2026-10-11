# Story replies with stickers, emoji, GIFs and voice

## What tdesktop does

`media/stories/media_stories_reply.cpp` builds the story reply field from the
full `HistoryView::ComposeControls`. That brings the emoji / sticker / GIF
panel (`stickerOrEmojiChosen`) and the voice and round-video recorder
(`recordMediaMessage`, `voiceLockFromBottom`). Sends go to the poster chat as
a reply to the story: `sendExistingDocument` for stickers and GIFs,
`sendVoiceMessage` for a recording, and `finishSending()` closes the field
afterwards. Custom emoji go in the text with entities.

## What changed

- `telegram/requests/story_reply_media.rs`: sticker, GIF and voice-note
  `sendMessage` builders that reuse the ordinary builders and set
  `reply_to` to `inputMessageReplyToStory` (TDLib 1.8.68, `td_api.tl:3099`),
  with no topic and no "play once".
- `connect/stories/reply_media.rs`: `send_story_reply_sticker`,
  `send_story_reply_animation`, `send_story_reply_voice`, gated like the text
  reply (`story.can_be_replied`, supported chat) plus the Premium sticker
  check. The voice send takes the poster chat explicitly, not the open chat.
- `ui/story_viewer/reply_media.rs`: the reply row now has an emoji button
  (the same shared media panel the composer uses, drawn beside the story), a
  mic that records with the composer's `VoiceCapture` and record bar, and Send
  once there is text. The picks route here only while the panel was opened
  from the reply row (`reply_panel`), so a panel opened from the main composer
  still sends to the open chat. A successful media send closes the row.
- One-line routes at the top of `send_sticker_pick`, `send_gif_pick`,
  `insert_panel_emoji`, `insert_panel_custom_emoji` and
  `send_voice_recording`; the composer's own panel is hidden while the story
  panel is open; two Escape layers (recording, panel) sit above the viewer.

## Not done

- Round video replies: the mic records voice only.
- Custom emoji inserted from the panel become their plain emoji, since the
  story reply text is sent without entities.

## Verification

Unit tests for the request JSON (sticker, GIF, voice, stale reply and topic
dropped), the driver gates and wire shape, and the routing predicates. The
gate passes. No demo opens the story viewer, so the new row and panel were not
screenshotted, and nothing was sent to a real story.
