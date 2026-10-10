# Composer leftovers: restricted placeholder, voice pause

Two items from the `composer-core` leftovers landed in full. The send box options were left alone (see the end).

## Restricted placeholder (`composer-restricted-placeholder`)

In tdesktop: `Data::CanSendAnyOf` allows a right only when the group's default rights and the member's own restriction both allow it, and administrators are never restricted. When nothing can be sent, `HistoryWidget::computeSendRestriction` replaces the composer with one centered sentence from `Data::RestrictionError`. The sentence has three forms: everyone is restricted ("Sending photos isn't allowed in this group."), only this member is ("The admins of this group restricted you from sending photos here."), or the member is restricted until a date. A single refused kind (a voice message, a sticker, a file) shows the same sentence when you try it.

In Quill before: the member's own rights were never stored, and `chat.permissions` holds only the group defaults, so the composer always showed and TDLib refused the send.

Now:
- `ParsedChatMember.restriction` parses `chatMemberStatusRestricted` (end date and rights). The existing own-member path (`getChatMember(me)` and `updateChatMember`) stores it on `ChatSummary.my_restriction`. The membership probe now also runs for supergroups, not only channels, and a failed probe marks the rights as fetched so it does not repeat.
- `src/send_rights.rs` is the pure decision: `allows`, `blocks_everything`, `denial` and `composer_block`, with every sentence copied from tdesktop's strings. A restriction whose end date has passed is ignored, and one more than 366 days away counts as permanent. Unknown default rights allow everything, so a chat that has not loaded never blocks.
- The conversation shows the sentence instead of the composer when everything is refused. Starting a voice or video message, sending a sticker or GIF, opening the poll dialog, and sending text or attachments check their own kind and show the matching sentence in the status line.

Not copied: tdesktop's "frozen account", "Premium required" and "boost to lift" variants of the bar (separate parity items), and the voice-message refusal for private chats whose user blocks voice messages.

## Voice pause, resume, preview and Play once (`composer-voice-pause`)

In tdesktop: a locked recording can be paused. A paused recording shows a play button and the waveform, and resuming continues the same message. A "Play once" toggle sends the message as a one-time message.

In Quill:
- `VoiceCapture::pause` and `resume` release and reopen the microphone and stop the clock (`RunClock`), so paused time is not counted. The encoder thread flushes what it has on a pause (`VoiceEncoder::pause`): the last 30 ms fade out, every whole frame is written and a page is flushed, so the file is playable. Under 20 ms stays queued and joins the audio after the resume, which fades in over 30 ms. One Ogg stream comes out, with no audio lost or added across the pause (checked by a decode test). Finishing right after a pause still writes the end-of-stream mark.
- A paused bar shows a play button that previews the file through the existing player, with the time and waveform following the playhead. Starting any other track, resuming, sending or discarding ends the preview. The record action is not sent to the chat while paused.
- "Play once" appears in private chats only (TDLib accepts self-destruct there only). It adds `messageSelfDestructTypeImmediately` to `inputMessageVoiceNote.self_destruct_type`, which the send path already had as a null field. The driver drops it in any other chat kind. The recipient's bubble uses the existing self-destruct badge; there is no dedicated one-time bubble.
- `record_controls` is the pure table of which controls the bar shows.

Not copied: trimming the recorded clip before sending.

## Not done

- `composer-send-options`: HD photo has no TDLib parameter (the client chooses the size), so it cannot be built honestly. GIF with caption, the paid media price (`inputMessagePaidMedia`, which also needs `supergroupFullInfo.has_paid_media_allowed` and the star limit option, neither stored) and the video cover (`inputVideo.cover`) are still unbuilt. The send menu keeps silent, schedule (with "send when online" in its picker) and link preview.

## Verification

- Unit tests: send-rights decision table and wording, the restricted-status parse, pause and resume in the encoder (decodable partial file, fade, exact total duration, finish after pause), the run clock, the control table, and the `inputMessageVoiceNote` request shape plus the driver's private-chat gate for Play once.
- Demo captures `ready-restricted-composer` and `ready-voice-pause` (English fixtures) were rendered and inspected.
- Not exercised: a real microphone pause on any platform (the backend may refuse `Stream::pause`; the encoder drops audio either way), real audio playback of the preview, and a live restricted account.
