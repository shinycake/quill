# claude-profile-cover-facts: what the user profile cover shows

## Problem
The user info panel's cover showed a photo, a name and a plain status line. Bots read "bot",
support and service accounts read like any user, the cover avatar ignored stories, the Stars
rating TDLib sends was dropped, a contact whose last seen your own privacy hides gave no way to
fix it, and opening your own profile showed a regular user cover with your phone and actions.

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
  and three rows saying what adds to or deducts from the rating. The pending rating preview is
  only for your own profile.
- Your own profile in the info section is Saved Messages: the bookmark userpic, the Saved
  Messages name, "N chats" (or loading) as the status, and no details, rating, story ring or
  photo click.

## Quill
- `ParsedUser` keeps `is_support`, `bot_active_users` (from `userTypeBot.active_user_count`)
  and `status_hidden_by_me` (from `by_my_privacy_settings`). `updateUserStatus` carries the
  flag too, and the redraw check compares it.
- `quill::stars_rating` parses `userFullInfo.rating`, `pending_rating` and
  `pending_rating_date` into `UserRatings`, stored on `UserFullInfoData.ratings`, and builds
  the level bar with the same rules as `AdjustByReached`.
- `quill::profile_cover` decides what the cover shows: `SpecialStatus` in tdesktop's order,
  `shows_last_seen_button` with tdesktop's conditions, `CoverFacts::for_user` for the Saved
  Messages switch, and the Saved Messages status text.
- `src/ui/profile_cover` renders it. The avatar gets the chat-list story ring when the tray has
  the user's stories and opens the first unread story ("View story" label and tooltip),
  otherwise the photo viewer, so the video avatar cover still plays. Opening a user's info loads
  their active stories if the tray lacks them, and opening your own loads the Saved Messages
  topics for the chat count. The rating pill is a kit `Button`; the rating box and the
  last-seen box are kit dialogs registered with `register_dialogs!`.
- Strings use tdesktop's `lng_*` keys in `lang/en/profile_cover.strings`.

## Deviations
- The last-seen box shows an eye icon in a filled circle instead of tdesktop's lottie.
- The rating badge is a small primary button with the level number (danger with a warning icon
  when negative) instead of tdesktop's level shapes.
- The pending rating ("Future rating") preview is parsed but not shown: TDLib sends it only for
  the current user, and your own profile is the Saved Messages cover, which has no rating.
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
  Last Seen" saves and toasts, your own profile is the Saved Messages cover.
- Screenshot demos `ready-info-cover-bot`, `-stories`, `-last-seen`, `-saved`, plus clicks
  into both boxes, captured in light and dark with /verify and read back.
- Not verified: real TDLib data, the live `getChatActiveStories` and privacy save, the
  Premium page opening, story playback from the cover, Linux and Windows.
