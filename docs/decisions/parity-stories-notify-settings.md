## Parity slice — Story notification settings (2026-09-30)

- **Scope:** per-chat story controls in the notifications panel (`parity:stories-notify-settings`). Parse (`chatNotificationSettings` story fields), scope-defaults UI (story mute + poster toggles), and the request builders already existed; the per-chat panel had no story controls.
- **Built:**
  - Driver (`connect/settings.rs`): `set_chat_story_mute`, `set_chat_story_poster`, `set_chat_story_sound` — clone the chat's current settings, change only the story fields (clearing the `use_default_*` flag on explicit set, Unigram-style), send full object via `setChatNotificationSettings`.
  - State (`state/session_notifications.rs`): `effective_story_muted`, `effective_story_poster` — chat flag, or the scope default when the chat keeps `use_default_*` (same rule as `effective_muted`/`effective_preview_allowed`).
  - UI (`ui/notification_settings.rs`): "Mute story notifications" and "Show story poster" Switches plus a "Story sound" row reusing the saved-sound picker via a new `SoundPickerTarget::ChatStory` variant (Default/None/custom, same shape as the message sound).
  - Test: scope-default → per-chat-exception override for both effective helpers.
  - README `parity:stories-notify-settings` checked.
- **Key decisions (ponytail):** scope-level story sound stays omitted (prior deliberate call — the per-scope message-sound picker covers it); story sound UI is per-chat only. No story-notification *generation* exists yet (the OS notification path is message-only), so these settings take effect for the TDLib-side behavior and any future story toast path.
- **Out of this slice:** story notification toasts themselves (no such path exists); `parity:stories-live-play`, `parity:stories-custom-reactions`.
