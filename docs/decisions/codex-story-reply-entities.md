# Story replies: entities and round video

Follow-up to `codex-story-reply-media.md`.

## What tdesktop does

The story reply field is the full compose control, so its text carries
entities (custom emoji, formatting) and its mic records both voice and round
video messages (`media_stories_reply.cpp`, `recordMediaMessage`).

## What changed

- The text reply now goes through the same markup-to-entities path as the
  main composer (`formatted_caption`), so formatting and custom emoji reach
  the wire. The reply field draws custom emoji as inline tokens and reads its
  content as composer markup (`field_doc(...).to_markup()`).
- A custom emoji picked in the shared panel inserts as a token in the reply
  field (Premium check unchanged).
- The reply row's mic follows the composer's record mode (right-click flips
  voice and video) and a round video is sent with `inputMessageVideoNote` and
  `inputMessageReplyToStory` via `send_story_reply_video_note`.
- Composer edits: `emoji_token` is visible to the UI, `composer_token_renderer`
  delegates to a new `field_token_renderer(field)`, and
  `send_video_note_recording` gets a one-line route.

## Verification

Unit tests: video note request JSON, driver video note reply, and a text reply
whose custom emoji markup becomes a `textEntityTypeCustomEmoji` entity.
Not verified: camera capture and the live camera circle (it draws in the
chat composer behind the viewer, so with no chat open there is no preview),
and no screenshot since no demo opens the story viewer. Nothing was sent to a
real story.
