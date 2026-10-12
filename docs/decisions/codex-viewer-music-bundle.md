# Viewer countdown and saved music

Items: `viewer-ttl-countdown`, `viewer-save-music`, `settings-profile-music`.

## What tdesktop does

- Viewer (`media_view_overlay_widget.cpp`, `ui/controls/ttl_media.cpp`): a timed
  photo or video is headed "Disappearing Photo/Video". Once the timer runs, the
  date line gains "Disappears in Ns" and a 44 px ring badge shows the seconds
  with an arc that shrinks. Opening an incoming message starts the timer
  (`markTimedMediaRead`). Save, Share and Draw are hidden for it.
- Song menu (`history_view_save_document_action.cpp`): "Save to..." with
  Profile (only if not saved yet), Saved Messages (not in Saved Messages), and
  Downloads, then the line "Choose where you want this audio to be saved.".
  A song that is saved and sits in Saved Messages only gets "Save As...".
  Saving toasts "Audio added to your Profile.".
- Playlist page (`info_saved_music_widget.cpp`, `data_saved_music.cpp`): your
  own list reorders by drag; the list changes locally first, then
  `account.saveMusic`. Delete asks "Do you want to remove this file from your
  profile?" and toasts "Audio removed from your Profile.". The row menu is
  Show in Folder, the save action, Delete (own list).

## What changed

- TDLib names (schema 1.8.68): `addProfileAudio`, `removeProfileAudio`,
  `setProfileAudioPosition`, `isProfileAudio` (404 = not saved; tdesktop reads
  the id list), `message.self_destruct_type/self_destruct_in`,
  `openMessageContent`.
- `src/state/profile_music.rs`: local add/remove/move first, failures reload
  the playlist. `src/ui/playlist_manage.rs`: drag reorder, track menu, remove
  confirmation. The song menu checks `isProfileAudio` when it opens.
- `src/media_viewer/ttl.rs`, `src/ui/media_viewer/ttl_ui.rs`: header, date
  suffix, ring badge, timer start on open (`updateMessageContentOpened` sets
  the end, as TDLib's `ttl_on_open` does), save/copy/forward blocked. A timed
  item opens alone so browsing never starts another timer.

## Differences from tdesktop

- View-once media (`messageSelfDestructTypeImmediately`) still does not open.
  TDLib's `open_message_content` deletes it at once (`ttl_on_open`), so the
  viewer would have nothing to show; tdesktop keeps it only locally. The
  item asks for timed photos and videos, which work.
- Playlist menu has no Forward: a saved track has no message in TDLib. "Saved
  Messages" on someone else's playlist sends the same file as a new audio
  message (`inputFileId`) instead of forwarding a fake item.
- The Save-to choices stay a second menu page (existing Quill style), not a
  fly-out submenu.
- No multi-select on the Playlist page; Delete works per track.

## Verification

Unit tests (requests, state, ttl math, viewer items, menu rules), driver tests
with the recording sender, GPUI tests (reorder, menu, remove confirm; timed
photo opens alone with countdown). Visual check by demo capture (temporary
demos, not committed): viewer header and ring, playlist menu, Save to page.
Not verified against a real account.
