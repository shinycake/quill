# Full member and subscriber counts in the header and profile cover

## What tdesktop does

Every status line that uses `lng_chat_status_members` or
`lng_chat_status_subscribers` fills `{count}` with `lt_count_decimal`, the full
number with group separators (`FormatCountDecimal`, `lang/lang_tag.cpp:949`):

- the chat header: `history_view_top_bar_widget.cpp:1942-2004`;
- the profile status label: `info_profile_status_label.cpp:50-57`;
- the profile actions block: `info_profile_actions.cpp:3292`.

It never pairs these keys with `lt_count_short`. For a group with people
online the header joins `lng_chat_status_members_online`
("{members_count}, {online_count}") with `lng_chat_status_online`, which uses
the plain `lt_count` (`history_view_top_bar_widget.cpp:1954-1957`), and only
when someone other than you is online.

## What Quill did

The chat header and the profile covers (supergroup/channel and basic group)
printed the compact "12.3K subscribers" via `compact_count`, with hardcoded
English nouns.

## What changed

- `inline_members::member_status_line` builds the header line: `members_title`
  (the lang keys through `lang::trn_decimal`, so plurals and translations
  apply) plus the existing ", N online" suffix with the plain online number.
- The header meta line and both profile covers use `members_title` /
  `member_status_line`. Channels say subscribers, groups say members.

## Verification

Unit tests of the status line for a channel and a group, singular and plural,
with and without online members. Light and dark captures of
`ready-info-channel`, and of the channel conversation header.
