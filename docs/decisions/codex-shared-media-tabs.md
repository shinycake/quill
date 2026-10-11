# Shared media tabs (codex-shared-media-tabs)

## What tdesktop does

`info/profile/info_profile_inner_widget.cpp` builds the profile tab strip
(`info/profile/tabs/`). In order, each tab shown only while its count is above
zero (`shown = count > 0`):

1. Posts (a channel) or Stories: `getChatPostedToChatPageStories`
   (`info_profile_tab_stories.cpp`), a grid of the stories on the profile.
2. Gifts: `getReceivedGifts` for the user or channel
   (`info_profile_tab_peer_lists.cpp`).
3. Media, one combined tab. The `profile-media-tabs-expanded` option splits it
   into Photos and Videos (`MediaTabsExpanded`); the tab's context menu has
   Expand / Collapse for it. Never both at once.
4. Saved: the Saved Messages sublist of that peer (not music).
5. Files, Links, Music, Polls, Voice, GIFs. Voice is `RoundVoiceFile`: voice
   messages and round videos together. Polls is `SharedMediaType::Poll`.
6. Common groups and similar channels (not shared media).

Saved music is not a tab. `info_profile_music_button.cpp` puts the first
track as a button in the profile header; it opens a "Playlist" page
(`info/saved/`, `lng_media_saved_music_title`) fed by the
user's saved music list.

Only the tabs that can be set as the main profile tab are Posts, Gifts, Media,
Files, Links, Music, Voice and GIFs (`MediaProfileTab`).

## What changed

- `SharedMediaTab` gained Photos, Videos, Polls, Stories, Gifts and Playlist
  (`src/state/shared_media_types.rs`). Voice now uses
  `searchMessagesFilterVoiceAndVideoNote` so round videos show under it, as in
  tdesktop (the search filter chips already did this).
- `src/state/shared_media_profile.rs`: tab order (`DISPLAY`), which tabs a chat
  kind can have, `visible_tabs` (hidden while the count is zero or unknown;
  the active tab always shows), the split state, and the reducers for the tabs
  that do not list messages.
- Counts come from the existing `getChatMessageCount` path
  (`chat_media_counts`, now also asked when the gallery opens). Two extra
  filters: Poll and VoiceAndVideoNote (keys 7 and 8, `EXTRA_COUNT_FILTERS`).
  Stories, Gifts and Playlist counts come with their first page (keys 9 to 11).
- Stories reuse the chat-page stories already cached for the story page
  (`apply_stories` hands the merged ids to the gallery); a row opens the story
  viewer over the loaded stories. Gifts use `getReceivedGifts` through a media
  purpose, so the Received gifts dialog's list is untouched; a row opens that
  dialog. The Playlist tab uses `getUserProfileAudios` (new request, parser and
  payload, `src/telegram/envelope/profile_audios.rs`).
- `src/connect/profile_tabs.rs`: first pages of the three profile tabs when
  the gallery opens, load-more for them, and the split toggle.
- UI (`src/ui/shared_media_profile.rs`): a Split / Merge button while a media
  tab is shown, "Load more" under every list that has more, row actions per
  tab. The tab bar shows only the visible tabs and scrolls with an overflow menu.
- The main profile tab now maps Posts to Stories and Gifts to Gifts
  (`src/profile_tab.rs`).
- The calendar button stays on Media, Files, Music, Links and Voice only:
  `getChatMessageCalendar` has no Photos, Videos or Polls filter in the
  existing filter chips.

## Differences from tdesktop

- Playlist is a gallery tab, because Quill has no profile header music
  button; it shows when the user has saved tracks. Rows are read-only:
  playback is keyed by message in Quill's player, and a profile track has no
  message. Adding, removing and reordering tracks (`parity:settings-profile-music`)
  is not done here.
- Stories are rows (caption and date, grouped by month), not a thumbnail grid.
  Gifts are rows too (price or collectible name and the sender's note); the
  existing dialog has the cards.
- Photos and Videos split is a session setting, off by default like the
  option. It is not remembered across launches.
- Saved Messages, common groups and similar channels tabs are not shared media
  and are unchanged.

## How verified

- Unit tests: tab order and filters (`shared_media_profile.rs`), visibility
  rules and the split, reducer tests for audios, gifts, stories, failures,
  round-video and poll labels and the extra counts
  (`state/tests/shared_media_profile.rs`), driver tests for the requests sent
  on open and on split (`connect/tests/shared_media_tabs.rs`), request shape and
  schema check for the extra filters, `audios` parser, main-profile-tab mapping.
- Visual: a temporary demo fixture (not committed) showed the tab bar with
  Stories, Gifts, Media, Playlist and the overflow menu, the Stories list under
  month headers, and the split state with Photos, Videos and the Merge button.
- Not verified against a live account: the answers of
  `getChatPostedToChatPageStories`, `getReceivedGifts` for a channel and
  `getUserProfileAudios` come from schema and fixtures only.
