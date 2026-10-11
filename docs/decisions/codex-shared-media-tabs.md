# Shared media tabs (codex-shared-media-tabs)

## What tdesktop does

`info/profile/info_profile_inner_widget.cpp` builds the profile tab strip
(`info/profile/tabs/`). In order, each tab shown only while its count is above
zero (`shown = count > 0`):

1. Posts (a channel) or Stories: `getChatPostedToChatPageStories`
   (`info_profile_tab_stories.cpp`), a thumbnail grid.
2. Gifts: received gifts of the user or channel
   (`info_profile_tab_peer_lists.cpp`), a grid of gift tiles.
3. Media, one combined tab. The experimental `profile-media-tabs-expanded`
   option (Settings, Experimental, Profile; `MediaTabsExpandedOption`, a local
   client option, not app config or a TDLib option, default off) replaces it
   with Photos and Videos. Never both at once.
4. Saved: the Saved Messages sublist of that peer (not music).
5. Files, Links, Music, Polls, Voice, GIFs. Voice is `RoundVoiceFile`: voice
   messages and round videos together.
6. Common groups and similar channels (not shared media).

Saved music is not a tab. `info_profile_music_button.cpp` shows the first
saved track as a button in the profile; it opens the "Playlist" page
(`info/saved/`, `lng_media_saved_music_title`).

## What changed

- `SharedMediaTab` gained Photos, Videos, Polls, Stories and Gifts
  (`src/state/shared_media_types.rs`). Voice uses
  `searchMessagesFilterVoiceAndVideoNote` so round videos show under it.
- `src/state/shared_media_profile.rs`: tab order (`DISPLAY`), which chat kinds
  can have which tab, `visible_tabs` (hidden while the count is zero or
  unknown; the active tab always shows), and the reducers for Stories, Gifts
  and the playlist.
- Photos / Videos split is the local option `profile_media_tabs_expanded` in
  `chat_prefs.json`, default off, with a switch under Settings (appearance
  section "Split profile media tab"). There is no button in the gallery.
  Changing it while the gallery is open moves the active tab and fetches it.
- Counts come from the existing `getChatMessageCount` path (also asked when
  the gallery opens), plus Poll and VoiceAndVideoNote (`EXTRA_COUNT_FILTERS`,
  keys 7 and 8). Stories and Gifts counts come with their first page.
- Stories and Gifts are thumbnail grids (`src/ui/shared_media_profile.rs`).
  Story tiles draw the cached photo or video thumbnail; gift tiles reuse the
  Received gifts dialog's sticker renderer (`sticker_tile`) and backdrop
  gradient. Thumbnails are downloaded after the page lands
  (`maybe_download_profile_thumbs`, same hook as chat-list avatars). A story
  tile opens the story viewer; a gift tile opens the Received gifts dialog.
- Saved music: `getUserProfileAudios` (new request, `audios` parser and
  `MediaPayload::ProfileAudios`), kept per user in `media.playlists` and asked
  when a user's info panel opens. The info panel shows tdesktop's music button
  (first track) which opens the Playlist page in the right column. Rows play:
  a track has no message, so the player gets the negated file id as its message
  id (`playlist_message_id`) and `toggle_audio_playback` runs unchanged; the
  player bar looks the title up in the playlist (`track_meta_for`). Tracks
  download on first play through the existing pending-play path.
- The main profile tab mapping now sends Posts to Stories and Gifts to Gifts.
- The calendar button stays on Media, Files, Music, Links and Voice only: the
  existing calendar filters have no Photos, Videos or Polls kind.

## Differences from tdesktop

- tdesktop's tab context menu also has Expand / Collapse for the media tab;
  Quill only has the setting.
- The Playlist page opens in the right column over the gallery slot, not as an
  info section. Adding, removing and reordering tracks
  (`parity:settings-profile-music`) is not done here.
- The profile music button is only in the user info panel. The "playing"
  indicator on it is not drawn.
- Photos, videos and GIFs in the gallery are still rows, as before.
- The player bar's jump-to-message does nothing for a playlist track.

## How verified

- Unit tests: tab order and filters, visibility and split rules
  (`shared_media_profile.rs`); reducer tests for audios (paging, failure),
  gifts, stories, thumbnail queueing, round-video and poll labels, extra counts
  (`state/tests/shared_media_profile.rs`); driver tests for requests on open,
  split and the playlist (`connect/tests/shared_media_tabs.rs`); `audios`
  parser and request shape; the main profile tab mapping; the pref round trip.
- Visual: a temporary demo fixture (not committed) showed the Stories grid,
  the Gifts grid with a collectible backdrop, and the Playlist page.
- Not verified against a live account: answers of
  `getChatPostedToChatPageStories`, `getReceivedGifts` for a channel and
  `getUserProfileAudios`, real thumbnails in the grids, and actual playback of
  a profile track (the player path is the existing audio path).
