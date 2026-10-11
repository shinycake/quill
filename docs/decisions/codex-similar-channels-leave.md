# Similar channels after leaving a channel

## What tdesktop does

tdesktop has no box after leaving. Its similar-channels UI is the
`info/similar_peers` list (the profile's "Similar channels" page) and a
`SimilarChannels` media strip under the local "You joined" service message
(`MessageFlag::ShowSimilarChannels`, `history_view_similar_channels.cpp`),
fed by `channels.getChannelRecommendations`.

## What changed

The parity item asks for suggestions after leaving. Quill already fetches and
caches `getChatSimilarChats` for the channel profile. Rendering inside the
message history is out of scope here, so the suggestions open as a dialog
right after a successful leave:

- `src/ui/similar_after_leave.rs` (new): `note_left_channel` remembers the
  channel (channels only) and requests the similar list; the dialog
  (`register_dialogs!`, priority 7460) opens once the cached list has at
  least one known chat. Rows open the channel; Close dismisses.
- `suggestions` drops the left channel, repeats and unknown chats (unit
  tested).
- Both leave paths call it: the channel footer button
  (`build_group_confirm_dialog.rs`) and the confirm dialog's Leave action
  (`open_custom_title_dialog.rs`). One state field in `dialogs_state.rs`.
- Demo `ready-similar-after-leave` (fixture only).

## Verification

Unit tests, gate, and a demo-capture screenshot of the dialog. A real leave
was not run against Telegram.
