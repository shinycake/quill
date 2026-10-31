# Side tables of chats no longer kept

Status: done. Found in the 2026-10 memory sweep, auditing the state that
lives as long as an account (`Session`, `ConnectDriver`) or the app
(`QuillApp`) for tables that only grow. Follows
`sweep-prune-closed-histories.md`, whose "Not covered" list named the
sponsored messages and media counts.

## What was wrong

A chat that leaves the closed-chat LRU (or that was left) drops its rows,
replied-to messages, pinned list and translations. Four tables filled when
a chat or its info panel opens stayed for the whole session:

| Table | Filled by | Per entry |
| --- | --- | --- |
| `messages.sponsored` | every channel opened without Premium | 1 to 3 ads (888 B each before their text, sponsor and files): about 1.5 to 5 KB |
| `messages.poll_messages` | every poll row loaded, in any chat | about 150 B; only an `updatePoll` for that poll pruned it |
| `groups.supergroup_members`, `basic_group_members` | the info panel's members block, the member dialogs, and every basic group opened (its bot commands come with the member list) | one page of up to 200 members (96 B each plus titles): about 20 KB per group |
| `media.chat_media_counts` | the info panel and the gallery | about 250 B |

Telegram Desktop keeps ads and participants on the history and peer it
shows; Quill now lets them go at the same point as the rows.

## What changed

`Session::forget_side_tables_once_unloaded` (`state/history_prune.rs`),
which runs once nothing of a chat is kept, also drops the chat's sponsored
messages and its poll rows from the poll index. Opening the chat fetches
the ads again (`sponsored_fetch_due` sees no fetch time), and its rows
index their polls again as they load.

The ads go, but what the user did with them stays
(`forget_sponsored_messages`): the ids of ads they reported or hid are
kept on the chat's entry, so if TDLib's next list still has a reported ad
it stays hidden, as before. That is a few ids per chat where the user
reported an ad. The viewed ids go with the list: they only spare a
repeated `viewMessages`, and TDLib itself reports each ad of its cached
list as viewed once. A report whose answer arrives after the list went
still records the hidden id.

`forget_info_panel_data` drops the media counts and member lists.
Answers still on their way cannot bring them back:

- a `count` answer, and the gallery's Stories and Gifts answers and their
  failures (`set_profile_count`), only fill the entry the counts fetch
  opened (`fetch_chat_media_counts`, which `fetch_profile_tabs` runs
  before asking for any tab). A dropped entry stays dropped and the next
  panel or gallery asks for every count again, instead of a partial entry
  from late answers stopping that fetch for the rest of the session (no
  media rows in the panel, no Files, Links or Voice tabs). This also
  covers a send that failed halfway through the counts;
- the first-page member requests in flight for the chat (any
  `getSupergroupMembers` filter, `getBasicGroupFullInfo`) leave the
  request registry, as translation jobs do, so their answers are ignored
  and the next fetch is not held back by them;
- ads answers already land only for a chat a view shows.

What is open on screen holds its chat from pruning (`ui/history_holds.rs`),
as a pending forward or a reply bar does, so nothing it reads goes from
under it:

- the group or channel the info panel shows (`Session::info_panel_chat`,
  which looks at the open chat first, so a frame does not search every
  chat when the panel shows the open chat);
- the chat of an ad being reported (`Session::sponsored_report_chat`),
  from the first request until the last answer or Cancel: the option
  picker stays open across chat switches, and an option picked after the
  ad went would send nothing yet say "Report sent";
- the member dialog, the admin and exception pickers, the restrict and
  ownership dialogs, and the Edit box (its Manage rows count the admins).

These stay open when the main chat changes under them (a notification
click), so without the hold 16 more chats would have taken the list and
the box would have shown "Administrators 0" or "Loading". While held, the
chat's history stays whole; once the surface closes the chat becomes an
ordinary closed tail and later goes with everything beside it. The info
panel used to be an exemption inside `forget_info_panel_data`, which
left the lists of a chat dropped while the panel showed it in memory
after the panel closed; the hold has no such leftover, and the panel
reads the same lists and counts as before. A member list that does go
marks the chat in `member_list_stale`, as a membership change does, so
any other reader asks again.

The gallery needs no exemption: it always shows the open chat of its own
view (`ConnectDriver::open_shared_media`) and closes when that view
switches chats (`Session::open_chat`, `leave_open_chat`), and a chat a
view shows keeps its history entry (`prepare_history_view`), so nothing
of it is forgotten. Each chat window's gallery is swapped in with its
view (`Session::swap_view`) and follows the same rule.

Nothing changes for a chat that keeps a tail (the 16 most recently used),
so going back to a recent chat shows its panel at once.

## Measured

State tests in `state/tests/history_prune.rs`:

- `side_tables_of_dropped_chats_go_back_to_empty`: 64 chats, each with
  ads, a poll row, counts and member lists; afterwards each table holds
  exactly the 16 kept chats, and after 17 more chats with none of them,
  every table is empty again. Fails on the old code (the tables held all
  64 chats).
- `an_open_info_panel_keeps_what_it_shows`,
  `a_basic_group_panel_keeps_what_it_shows`: with the UI's claim, the
  group the panel shows keeps everything through 17 chat switches, and
  nothing of it stays once the panel closes and the chat is dropped.
- `a_reported_ad_stays_hidden_after_its_chat_is_dropped`: report an ad,
  drop the chat, reopen it with the same ad in the answer: it stays
  hidden. `a_report_answered_after_the_drop_still_hides_its_ad`: the same
  when the report's answer lands after the drop.
- `a_chat_an_open_box_holds_keeps_its_member_lists`: a held chat keeps
  everything through 17 chat switches; once the hold lifts it goes like
  any other chat.
- `a_forum_forgets_its_side_tables_with_its_last_topic`: the topic LRU
  path.
- `the_open_chat_keeps_its_gallery_counts_while_its_topics_go`.

Connect tests in `connect/tests/history_prune_answers.rs`:
`answers_for_a_dropped_chat_bring_nothing_back` (counts and member
answers that arrive after the drop leave no entry, and opening the chats
again sends every request again) and
`late_gallery_answers_leave_no_partial_counts` (the gallery's counts,
Stories answer and Gifts failure land after the drop: no entry, and the
reopened gallery asks for every count again).

UI tests in `ui/history_holds_tests.rs`: the Edit box, the member dialog,
the info panel and an ad's report picker each hold their chat while open
and let it go when closed.

Not measured on a running app: the bytes above are `size_of` of the
types on a 64-bit build (`ParsedChatMember` 96, `SponsoredMessage` 888)
plus an estimate of their heap and the `HashMap` overhead.

## Audit: other state that grows

Kept on purpose (bounded by the account's chats and users, mirrors what
TDLib and Telegram Desktop keep; dropping them would blank names, rows or
headers until TDLib resent them): `Session::users`, `Session::chats`, the
per-supergroup and per-chat flag maps in `GroupsState` and `ChatsState`,
`user_full_infos`, `supergroup_full_infos`, `bot_info`,
`group_bot_commands`, `forum_topics`.

Small and driven by explicit actions (left as is): profile photo
galleries, playlists, admin panels (invite links, join requests, event
logs), per-message toggles in `MessageUi` (spoilers, fact checks,
keyboards), `PlaybackUi::positions`, the notification panel's per-chat
volume sliders, `NewChatState::creations`.

Already bounded by earlier work: `media.files`, custom emoji, histories
and topic histories, scroll memory, the retry stash, the request registry
(`sweep-settle-lost-requests.md`), every UI image, text and glyph cache.
