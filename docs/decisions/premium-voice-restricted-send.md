# Voice and video messages to a user who refuses them

## What tdesktop does

A Premium user can stop others from sending them voice and video messages.
The server reports it as `userFull.voice_messages_forbidden`, which tdesktop
keeps as `UserDataFlag::VoiceMessagesForbidden` (data/data_user.cpp).

- `PeerData::amRestricted` (data/data_peer.cpp) treats `SendVoiceMessages`
  and `SendVideoMessages` as restricted for that user. No other right
  changes.
- `Data::RestrictionError` (data/data_chat_participant_status.cpp) words it
  `lng_restricted_send_voice_messages` / `_video_messages`:
  "{user} doesn't accept voice messages." with the user's name.
- `HistoryWidget::updateSendButtonType` sets `SendButton::State::forbidden`
  when the record button's right is refused. `SendButton::paintRecord` /
  `paintRound` then draw the icon at `kForbiddenOpacity` (0.5) and skip the
  ripple. The button stays clickable.
- The record bar's start filter shows the refusal as a toast and does not
  start a recording.
- `HistoryWidget::handlePeerUpdate` requests the full user while its block
  or calls status is unknown, so the flag is known soon after a chat with
  that user first opens.

## What Quill does

- `userFullInfo.has_restricted_voice_and_video_note_messages` is parsed in
  the users domain and stored on `UserFullInfoData::voice_notes_restricted`.
  It is the only copy. `updateUserFullInfo` replaces it.
- Opening a private chat with a person (not a bot, not Saved Messages)
  sends `getUserFullInfo` once per user (`maybe_fetch_open_user_full_info`).
  Bots already fetch theirs in `maybe_fetch_bot_info`.
- `SendRights::for_chat` carries the chat title as `voice_refused_by`.
  `allows` and `denial` refuse voice and video messages with the tdesktop
  sentence. All other kinds stay allowed, and the composer stays.
- The record button draws its glyph at 0.5 opacity when the open chat
  refuses the current record mode (`record_forbidden`). This also covers
  groups that restrict voice or video messages, as in tdesktop. A click
  shows the refusal in the status note, Quill's toast, through the existing
  `deny_send` path.
- A video-message attachment to the same user is refused through
  `attachment_denial`, as tdesktop's `requiredSendRight`.

## Deviation

tdesktop's start filter always checks `SendVoiceMessages`, so in round mode
the toast still says "voice messages". Quill checks the mode's own right and
says "video messages", using tdesktop's `lng_restricted_send_video_messages`
string. For this user both rights are refused together, so only the noun
differs.

## Verified

- Unit tests in `send_rights.rs`, driver tests in
  `connect/tests/voice_restricted.rs`, UI tests in `ui/send_limits_tests.rs`.
- `/verify` with the `ready-voice-restricted` demo, light and dark. The mic
  is dimmed next to the `ready-chats` baseline. A click shows
  "Demo chat A doesn't accept voice messages." and starts no recording.
