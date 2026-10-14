# Transcribe: no free-trial text for Premium, errors wrap

## What was wrong

Every voice message showed "You have used all your free transcriptions this
week. Wait until January 1, 1970…" next to Transcribe on a Premium account.
TDLib sends `updateSpeechRecognitionTrial` with `weekly_count`,
`left_count` and `next_reset_date` all 0 when there is no free quota
(checked in the live TDLib log), and Quill rendered that as "used up".
Transcription itself worked (verified on a voice note in Saved Messages).
The line also ran past the bubble's edge, as could a long error.

## What tdesktop does

- `Transcribes::trialsSupport()` (`api_transcribes.cpp`) is true only when
  the quota has a count or a reset date.
- The quota is never drawn in the bubble: `ShowTrialTranscribesToast`
  (`history_item_helpers.cpp`) toasts it when a transcription changes the
  remaining count (`trial_remains_num`), with the same two strings.

## What changed

- `SpeechTrialUpdate::supported` and `speech_trial_toast`: a toast when the
  remaining count changes, never for Premium or without a quota, never for
  the first update of a session.
- The voice row no longer shows the quota.
- The transcription error is a column: the text wraps inside the bubble,
  Retry sits under it.

## Verified

Unit tests for the toast rules (including the all-zero Premium update) and
the toast filter. The wrapping was not captured on screen.
