# TDLib 1.8.70

Pin moved from `4c6377aa7386c39217878a9a56d581922ad37ca4` (1.8.69) to
`23383aef218a3123f02b744bfa91bde21056a814` (tdlib/td master, "Update version
to 1.8.70"), 137 upstream commits, issue #1407. tdlib/td stays read-only for
us. Procedure: the "TDLib" section of `docs/dependency-updates.md`.

## What changed in the pin

`src/pins.rs` (`TDLIB_GIT_COMMIT`, `TDLIB_CMAKE_VERSION`, `TD_API_TL_SHA256`
`fe37b2e8…c9d11145`, `TD_API_TL_BYTES` 1,216,190 to 1,236,740,
`TD_API_TL_UPSTREAM_URL`), `schema/td_api.tl` re-vendored with
`scripts/vendor-td-schema.sh`, `schema/README.md`, `docs/build.md`,
`THIRD_PARTY.md`, `ROADMAP.md`, the README toolchain line and the agent
guide's version. The takeout patch applied with offsets only and was
regenerated from `git -C native/td diff` (same hunks). The README badge is
left to the merge pipeline.

Schema diff: 43 added, 1 removed, 17 changed constructors (ten of them TON
wallet types whose amounts moved from `int53` to `int64`; Quill parses none).

| Change | Quill impact |
|---|---|
| `inputStoryAreaTypeFoundVenue` removed | Never sent by Quill. |
| `toggleSupergroupIsForum` + `has_forum_sidebar`; `supergroup.has_forum_sidebar`; `userTypeBot.has_topic_sidebar` | Quill reads `has_forum_sidebar` and sends it back when the owner toggles topics, so the toggle doesn't clear a layout set from another app. Telegram Desktop has no sidebar layout yet (only the `forum_sidebar` flag in its scheme), so Quill doesn't draw one. |
| `community.are_chats_collapsed`, `communityFullInfo.notification_settings` and the new community methods | Built: see `communities-1-8-70.md`. |
| `messageWidget`, widget queries and `getWidgetPublicData` | A placeholder card, see below. |
| `story.music`, `postStory.music`, `editStoryMusic` | Not built: Telegram Desktop has the field in its scheme but no UI that plays or picks story music. |
| `recognizeSpeechInFile`, `updateSpeechRecognitionInFile`, `getSpeechRecognitionLanguages`, `fixRecognizedTextWithAi`, `fixRecognizedRichMessageWithAi` | Not built: Telegram Desktop never calls `messages.transcribeAudioFile` or `getTranscriptionLangs`. Voice message transcription (`recognizeSpeech`) is unchanged. |
| `getTextEmojiRanges` | Not used; Quill finds emoji in its own text pipeline. |
| Bot-only methods (`editMessageWidgetData`, `editWidgetData`, `answerWidgetQuery`, `inputMessageWidget`, `inputMessageSharedWidget`, `updateNewWidget*Query`) | Quill is a user client. |

## Message widgets

`messageWidget` is a bot's HTML widget with its own data and queries.
Telegram Desktop doesn't use a web view for it: it runs the widget script in
QuickJS inside a separate sandbox library (`ThirdParty/widget-sandbox`) and
draws the validated DOM with its own renderer (`Telegram/SourceFiles/tgw/`,
about 17,600 lines). Building that is a project of its own, so this round
parses the message into `MessageContent::Widget` (size, caption, caption
position) and shows a card that names the widget and says it doesn't run in
Quill, with the caption below or above it like any media caption. The chat
list shows the caption, or "Widget" (`lng_in_dlg_widget`). The HTML and the
widget data are dropped at parse time. Captions can't be edited (only the
bot edits a widget).

## How verified

- `vendored_schema_equals_official_commit` and the core library tests pass
  (`cargo test --no-default-features --lib`, 7,333 tests).
- New tests: `a_widget_message_keeps_its_caption_and_size` (envelope),
  `forum_toggle_sends_tabs_only_when_enabling` now checks the sidebar flag,
  `topics_toggle_keeps_the_forum_sidebar_flag` (driver).
- Patched TDLib built on this Mac with
  `TDLIB_BUILD_TARGET="tdjson tdjson_static" bash scripts/build-tdlib.sh`;
  `libtdjson.1.8.70.dylib` is 41,918,568 B (1.8.69: 41,388,568 B). Offline
  probe (ctypes, no network): `getOption("version")` = 1.8.70,
  `commit_hash` = 23383aef…, `getTextEmojiRanges` answers.
- `/verify` capture of the `ready-unsupported-message` demo, which now also
  holds a widget message with a caption, in light and dark.
- Not run: a live widget message (no bot sends one to the test account yet)
  and the Windows and Linux TDLib builds (both read the pin from `pins.rs`;
  CI caches are keyed on it).
