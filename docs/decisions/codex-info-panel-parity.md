# Info panel parity (tdesktop)

Compared with Telegram Desktop's third column on the same chats.

**Follows the open chat.** tdesktop's info column always shows the
current chat. Quill's kept the old target after switching chats. Now
`select_listed_chat` retargets an open panel to the new chat
(`info_panel_target_for_chat`), or closes it when the chat has none.

**Contact details.**
- Phone numbers group by country code, as Telegram's formatter does for
  common codes (+972 50 200 2287, +1 555 010 1031, +44 2071 838750…).
  Unknown codes keep their digits. The label is "Mobile".
- Birthday from `userFullInfo.birthdate`: "Jul 30, 1965 (61 years old)",
  or just the day when the year is hidden.
- A Mute/Unmute tile next to Call / Video / Secret chat.

**Shared-media rows.** tdesktop lists "161 photos · 10 videos · 11 files ·
2 audio files · 46 shared links · 10 voice messages · 7 GIFs · 6 groups
in common". Quill now:
- fetches seven `getChatMessageCount` counts (photo, video, document,
  audio, URL, voice note, animation; server counts) once per chat when the
  panel opens (`RequestPurpose::GetChatMessageCount`, a `count` payload);
- shows a row per non-zero count, plus groups in common
  (`group_in_common_count`), in user, supergroup and basic-group panels;
- opens the shared-media gallery on the matching tab when a row is clicked.

**Invite links.** `getChatInviteLinks` was sent with `creator_user_id: 0`,
which TDLib rejects (400 "Could not load invite links" in group panels).
It now passes your user id, as the schema requires for non-owners.

Verified live: Mom's and Thuong's panels (counts, birthday, phone), and
switching chats with the panel open.
