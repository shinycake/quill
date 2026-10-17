# Notification volume per scope and per chat

## What tdesktop does
- `settings/sections/settings_notifications_type.cpp:383-389, 514-526`: each scope page (private chats, groups, channels) has a "Notifications volume for ..." slider (`AddRingtonesVolumeSlider` with `DefaultRingtonesVolumeController`), shown under the tone button while sound is on.
- `boxes/ringtones_box.cpp:339-362`: the per-chat sound box has a "Volume" slider (`ThreadRingtonesVolumeController`), hidden while "no sound" is picked.
- `data/notify/data_peer_notify_volume.cpp:27-60`: values are 1-100, stored per session, 0 = not set, shown as 100.
- `window/notifications_manager.cpp:766-785` and `media/audio/media_audio_track.cpp:155-160`: the chat value wins, then the scope value; only when both are unset does the app-wide `notificationsVolume` apply. A scope or chat value replaces the app-wide volume, it is not multiplied with it.
- Releasing the slider plays the chosen sound at the new volume.

## What changed
- `src/notify_volume.rs`: per-account scope and chat percents in `notify_volumes.json`, plus the pure resolution (`resolve_percent`, `resolve_gain`).
- `Session::notification_volume_override` feeds `NotificationSounds::play_at` for active and background account notifications.
- Kit `Slider` rows in the scope defaults dialog (`scope_volume_row`) and the per-chat notification panel (`chat_volume_row`), in `notification_settings/scoped_volume.rs`.
- Demo `ready-mute-archive` now loads default scope settings so the scope rows show.

## Notes
- The per-chat panel applies on change (Quill's sound picker applies immediately); tdesktop saves on its box's Save button.
- A custom sound that finishes downloading after the notification (`pending_sound_plays`) plays at the app-wide volume.

## Verified
Unit tests for the resolution, a GPUI UI test for the sliders, and light and dark demo captures.
