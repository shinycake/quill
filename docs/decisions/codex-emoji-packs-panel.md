# Custom emoji packs reach the panel; media tools found outside a shell

## Problems (seen live, compared with Telegram Desktop)
- **No custom emoji in the composer panel.** Loading installed custom emoji
  makes TDLib send `updateInstalledStickerSets` (custom emoji) before the
  answer arrives. `refresh_emoji_pack_catalog` dropped the in-flight
  `GetInstalledEmojiSets` request and re-sent it only while the Settings →
  Emoji Sets page was open. The panel's answer was ignored, so the packs
  appeared only after visiting that page.
- **Pack icons in the panel footer stayed blank** until their set was
  scrolled into view.
- **ffmpeg / ffprobe / ffplay were invoked by bare name.** An app opened
  from Finder or the Dock gets launchd's minimal `PATH`, which misses
  Homebrew, so video stickers, GIF and video frames, video notes and voice
  recording all failed outside a terminal launch.

## Decision
- The refresh re-requests installed custom emoji sets when it cancels an
  in-flight request, when a list is already cached, or when the settings
  page is open. Covered by a connect test that replays the update before
  the answer.
- The footer queues the sets and icon files its pack icons need, after the
  visible rows' requests (loads stay capped at `MAX_LIBRARY_LOADS`).
- `media_tools::command` resolves a tool next to the app binary, then on
  `PATH`, then in `/opt/homebrew/bin`, `/usr/local/bin` and `/opt/local/bin`,
  once per process.
