# Composer bundle: repeat schedule, Up on pending media, emoji replacement, shortcut commands

Parity: `composer-repeat-schedule`, `composer-up-edit-media`, `emoji-replace-auto`,
`platform-shortcut-commands`.

## Repeating scheduled messages

tdesktop (`ScheduleBox`, `Ui::ChooseRepeatPeriod`): a "Repeat: Never" button under the date,
menu Never / Daily / Weekly / Biweekly / Monthly / Every 3 month / Every 6 month / Yearly (plus
1 and 5 minutes on the test DC); a lock and a Premium promo without Premium; the button shows
the first entry at or above the stored period; scheduled messages show "daily 12:30" style
words (`SchedulePeriodText`); a repeating send is a batch of one (`optionsRequireSingle`).

Quill: `messageSchedulingStateSendAtDate.repeat_period` is parsed and sent
(`SendOptions::schedule_repeat_period`); the picker has the same row, menu, lock and note;
`schedule::repeat_*` hold the lists. A repeating composer send is never grouped into an album.
The scheduled list shows the period word.

Found on the way: media sends ignored silent and scheduled options (`options: null`), so a
scheduled photo went out at once. `with_message_send_options` now puts them on media and
albums.

Differences, all TDLib: the repeat period is accepted only by `sendMessage` and a one-message
`forwardMessages`. So no Repeat row when rescheduling an existing message
(`editMessageSchedulingState` has no repeat; it is sent with 0), no row for a multi-message
forward, and an album carries no period.

## Up on a media message being sent

tdesktop: Up in an empty field with no reply and no edit: if the last message is local, a media
one opens its caption box, anything else lets Up scroll; otherwise the newest message that
`allowsEdit` (not forwarded or via-bot). It needs `canWriteMessage`.

Quill: same order in `Session::last_editable_message`, plus the reply, can-post and
forwarded/via-bot rules. TDLib cannot edit a message with a temporary id, so
`edit_pending_caption` keeps the caption, shows it on the row at once and sends
`editMessageCaption` when `updateMessageSendSucceeded` gives the real id (dropped on failure).
Not done: tdesktop's edit time limit (TDLib exposes it only through message properties).

## Replace emoji automatically

tdesktop installs `InstantReplaces::Default()` and binds the setting to
`setInstantReplacesEnabled`: off means nothing is replaced, dashes included; no replacement
inside code or pre; Backspace right after puts the typed text back; the shortest matching
ending wins; there is no word-start rule; `:o ` and `xD ` match exactly.

Quill had the setting but kept dashes when off, had a word-start rule, matched case loosely and
had no revert. Fixed all of those; also works while editing. Unverifiable: the emoticon table
comes from tdesktop's codegen, which is not in the reference checkout, so Quill keeps its
existing table.

## Shortcut commands

tdesktop ships these without chords except Ctrl/Cmd+Shift+T (reopen closed window); users set
them in Settings. Quill: nine rebindable actions in the keyboard shortcuts section (no default
except reopen), applied like tdesktop: Just send / Silent send (not for reminders) / Schedule
need something to send; Schedule asks for the time and then sends; Scheduled messages needs
write access; Toggle link preview needs a link or a removed preview and resets the choices when
turned back on; Record Round Message starts a locked video recording in an empty composer;
Archive chat toggles the open chat; Recent Actions for channel and supergroup admins; Reopen
closed window brings back the chat window closed last (16 kept, with their position). Chat
windows handle all of them except archive and recording.

Differences: "selected chat" is the open chat (as for Ctrl+R already); Quill shows recent
actions in the chat info panel, so that panel opens.

## Verified

Unit and request-JSON tests, driver tests for the pending caption and media options, GPUI
tests for the picker, each command, the chord, emoji replace and revert, Up, reopen. A temporary
capture showed the Repeat row for Premium and locked without it.
