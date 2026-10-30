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
the ads again (`sponsored_fetch_due` sees no entry), and its rows index
their polls again as they load.

`forget_info_panel_data` drops the media counts and member lists, except
while the info panel or the gallery shows that chat. A member list that
goes marks the chat in `member_list_stale`, as a membership change does,
so a member dialog still open on it asks again instead of showing
"Loading". Reopening the panel asks for both, as it does the first time.

Nothing changes for a chat that keeps a tail (the 16 most recently used),
so going back to a recent chat shows its panel at once.

## Measured

State tests in `state/tests/history_prune.rs`:

- `side_tables_of_dropped_chats_go_back_to_empty`: 64 chats, each with
  ads, a poll row, counts and member lists; afterwards each table holds
  exactly the 16 kept chats, and after 17 more chats with none of them,
  every table is empty again. Fails on the old code (the tables held all
  64 chats).
- `an_open_info_panel_keeps_what_it_shows`: the chat the panel shows
  loses its ads and poll index but keeps its counts and members.

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
