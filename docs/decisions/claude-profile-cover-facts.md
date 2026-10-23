# claude-profile-cover-facts: what the user profile cover shows

## Problem
The user info panel's cover showed a photo, a name and a plain status line. Bots read "bot",
support and service accounts read like any user, the cover avatar ignored stories, the Stars
rating TDLib sends was dropped, a contact whose last seen your own privacy hides gave no way to
fix it, and the Saved Messages chat's info showed your user profile.

## tdesktop
- `Info::Profile::OnlineTextSpecial` (info_profile_values.cpp) picks a special status first:
  the service notifications account (777000 / 333000), then support, then bots. A bot with an
  active user count reads "N monthly users" (`lng_bot_status_users`), otherwise
  `BotStatusText` adds a suffix from the username (app, agent, ai, game; "gay" reads
  "non-binary"). Any other user whose id is a multiple of 1000 counts as support.
- The cover (info_profile_cover.cpp) draws the story outline around the userpic when the peer
  has stories, segmented by read state. Clicking opens the stories; without stories it opens the
  photo viewer.
- `setupShowLastSeen` adds a "when?" link next to the status when the status is hidden by your
  own privacy settings, the user is not online, you are not Premium, Premium is possible and the
  peer is not you, a bot or a service account. It opens `Ui::ShowOrPremiumBox` with
  `ShowOrPremium::LastSeen`: "Show My Last Seen" sets last seen to Everybody with no exceptions
  and shows a toast; the second half offers Premium.
- The rating badge next to the name opens `AboutRatingBox`: a level bar adjusted by
  `AdjustByReached`, "Level N" and "Level N+1" labels (or "Negative Rating"), the explanation,
  and three rows saying what adds to or deducts from the rating. On your own profile the box
  says "your" (`lng_stars_rating_about_your`, `lng_stars_rating_negative_your`) and, when
  TDLib has a pending rating, adds "The rating will update in N days" with a Preview / Back
  toggle that puts the future rating on the bar under the "Future Rating" title
  (`AboutRatingBox`, `info_profile_top_bar.cpp` passes `pendingStarsRating` only for
  `_peer->isSelf()`).
- Only the Saved Messages chat's own info is Saved Messages. `TopBarWidget::infoClicked` and the
  third column use `Info::Memento::Default(peer)`, which gives the `Data::SavedMessages` key
  when `peer->savedSublistsInfo()` (`isSelf()` with saved sublists, `data_peer.cpp`). That key
  shows the bookmark userpic, the Saved Messages name and title, "N chats" (or loading) as the
  status, and no details, rating, story ring or photo click (`TopBar::nameValue`,
  `Widget::title`, `InnerWidget` with `_savedMessages`). Every other way to your own user,
  `SessionNavigation::showPeerInfo(peer)` from a member list, a message author or a mention,
  builds `Info::Memento(peer)` with the Profile section: your real profile with name, phone,
  bio, username, photos and the rating, titled User Info. The main menu's My Profile opens
  `Info::Stories::MakeMyProfile`, also your own profile.

## Quill
- `ParsedUser` keeps `is_support`, `bot_active_users` (from `userTypeBot.active_user_count`)
  and `status_hidden_by_me` (from `by_my_privacy_settings`). `updateUserStatus` carries the
  flag too, and the redraw check compares it.
- `quill::stars_rating` parses `userFullInfo.rating`, `pending_rating` and
  `pending_rating_date` into `UserRatings`, stored on `UserFullInfoData.ratings`, and builds
  the level bar with the same rules as `AdjustByReached`.
- `quill::profile_cover` decides what the cover shows: `SpecialStatus` in tdesktop's order,
  `shows_last_seen_button` with tdesktop's conditions, and the Saved Messages status text.
  `Session::chat_info_target` is the info a chat itself opens: `InfoPanelTarget::SavedMessages`
  (a new variant, the counterpart of the `Data::SavedMessages` key) for the Saved Messages
  chat, else `info_panel_target_for_chat`. The chat header title, the header menu's View
  Profile, the chat-list and top-peer menus and the info column following the open chat use
  it. Member lists, message authors, statistics, stories, the media viewer and the main menu's
  My Profile keep `InfoPanelTarget::User(me)`, the self profile, titled User Info.
- `src/ui/profile_cover` renders it. The avatar gets the chat-list story ring when the tray has
  the user's stories and opens the first unread story ("View story" label and tooltip),
  otherwise the photo viewer, so the video avatar cover still plays. Opening a user's info loads
  their active stories if the tray lacks them, and opening the Saved Messages info loads the
  saved topics for the chat count. The rating pill is a kit `Button`; the rating box and the
  last-seen box are kit dialogs registered with `register_dialogs!`.
- Strings use tdesktop's `lng_*` keys in `lang/en/profile_cover.strings`.

## Deviations
- The last-seen box shows an eye icon in a filled circle instead of tdesktop's lottie.
- The rating badge is a small primary button with the level number (danger with a warning icon
  when negative) instead of tdesktop's level shapes.
- The pending text's inline "Preview ›" link is a small ghost button under the text.
- "Your payments are unreliable" text is used only on your own profile; tdesktop picks it by
  a negative star count alone.
- The story ring reuses the chat list's ring widths rather than tdesktop's thinner profile
  outline.
- Regular last-seen text stays Quill's existing English status formatting.

## Verification
- Unit tests: special status order and bot suffixes, the last-seen button conditions, the
  Saved Messages facts, the rating parser and level bar, count formatting.
- Envelope test `cover_facts_parsed_from_user_status_and_full_info` and session test
  `hidden_last_seen_and_rating_reach_the_cover`.
- GPUI tests in `src/ui/profile_cover/tests.rs`: the ringed avatar opens the first unread
  story, the rating pill opens and closes the rating box, "when?" opens the box and "Show My
  Last Seen" saves and toasts, the Saved Messages info is the Saved cover, selecting the
  Saved Messages chat turns your open profile into the Saved cover and My Profile turns it
  back, your rating box previews the pending rating and toggles back, and others' boxes have
  no preview. Session test `only_the_saved_messages_chat_opens_the_saved_messages_info`.
- Screenshot demos `ready-info-cover-bot`, `-stories`, `-last-seen`, `-self`, `-saved`, plus
  clicks into both boxes and the pending preview, captured in light and dark with /verify and read back.
- Not verified: real TDLib data, the live `getChatActiveStories` and privacy save, the
  Premium page opening, story playback from the cover, Linux and Windows.
