# "Hide new chats?" suggestion and the archive-and-mute switch

## What tdesktop does

- `SessionController::suggestArchiveAndMute` (window_session_controller.cpp)
  reacts to the server's archive-and-mute suggestion: it reloads the global
  privacy settings and, if archive-and-mute is off, shows a box. The box has
  the title `lng_suggest_hide_new_title`, the bold-marked text
  `lng_suggest_hide_new_about`, a "Go to Settings" button that opens Privacy
  and Security, and Cancel. A click outside does not close it. Any close
  calls `dismissArchiveAndMuteSuggestion`, which hides the suggestion on the
  server.
- `SetupArchiveAndMute` (settings_privacy_security.cpp) shows the "Archive and
  mute new chats" switch only while the server allows it
  (`autoarchive_setting_available`), the setting is on, or the account has
  Premium. Once the switch has been shown it stays. `ArchiveSettingsBox` uses
  the same rule.

## What changed in Quill

- `src/archive_mute.rs` holds the rule. `ArchiveMute` tracks the TDLib option
  `can_archive_and_mute_new_chats_from_unknown_users` (TDLib's name for
  `autoarchive_setting_available`) and whether this session has seen the
  setting on. `switch_shown` and `suggestion_due` are pure functions.
- When `updateSuggestedActions` adds `suggestedActionEnableArchiveAndMuteNewChats`,
  the driver fetches `getArchiveChatListSettings`. The box shows once the
  setting is known to be off.
- `src/ui/archive_mute_suggestion.rs` adds the box as a registered dialog
  whose open state comes from session state. Cancel, Escape, the close button
  and "Go to Settings" all send `hideSuggestedAction`. "Go to Settings" then
  opens Privacy and Security. A refused hide puts the suggestion back.
- In both Privacy and Security and the archive settings box, the
  archive-and-mute row is hidden unless `switch_shown` is true. The other two
  archive rows stay.

## Deviations

- tdesktop reloads the settings every time the suggestion arrives. Quill uses
  the cached archive settings if this session already loaded them.
- tdesktop keeps a shown switch for as long as that settings section is open.
  Quill keeps it for the whole session, because its section is rebuilt on
  every render.

## Verification

- Unit tests in `src/archive_mute.rs`.
- Driver tests in `src/connect/tests/archive_mute.rs`. They cover the fetch
  when the suggestion arrives, `hideSuggestedAction` with the right action
  type, the cached second arrival, no box while the setting is on, the server
  option, and a refused hide.
- UI tests in `src/ui/archive_mute_suggestion_tests.rs`. They cover "Go to
  Settings", Cancel, Escape, no box while the setting is on, and the
  Privacy switch for non-Premium, Premium and server-allowed accounts.
- `/verify` with the `ready-archive-mute-suggestion` demo in light and dark:
  the box, the Privacy page after "Go to Settings", and the Archive and Mute
  card without the switch for a non-Premium account.
