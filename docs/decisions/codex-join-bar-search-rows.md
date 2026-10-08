## codex/join-bar-search-rows (2026-10-08)

Two live bugs: the channel bar stuck on "Checking channel membership…" for a public channel the viewer has not joined, and a search-result row whose preview spilled onto a second line.

### Bug 1: join bar never resolves

- The bar (`channel_footer`, `src/ui/groups.rs`) already has every tdesktop state: Join channel (Left), Mute/Unmute (Member), composer (admin with post right), banned, unknown. The only gap was resolution: `my_member_status` is filled solely from `getChatMember(me)` / `updateChatMember`.
- For a channel opened from search or a `t.me` link the viewer is not a participant, and `getChatMember(me)` does not produce a `chatMember` (it errors, "Member not found"), so the status stayed `None` forever and the pending-request error was ignored.
- TDLib already sends the answer: `supergroup.status` (`chatMemberStatusLeft`) in `updateSupergroup` (before `updateNewChat`) and in the `getSupergroup` response. Quill stored it in `supergroup_member_status` but never fed it to the chat.
- Fix (`Session::adopt_supergroup_status`): an unresolved channel chat adopts the supergroup status; a resolved non-admin chat follows Left/Member/Banned/Restricted changes (join/leave); an administrator status never overwrites a resolved chat, so admin rights still come from `getChatMember`. Adoption runs on `updateSupergroup`, on `getSupergroup`, on `updateNewChat`, and as a fallback when `getChatMember` returns an error.
- Tests: `tests/replay_channels.rs` (non-member channel with recorded-shape JSON, failing `getChatMember`, later join, and the chat-first/`getSupergroup` order). Join is never clicked in any test or capture.
- Not changed: tdesktop's "View discussion" bar variants are not in Quill's footer; out of scope here.

### Bug 2: search row preview on two lines

- Cause: `search_result_row` rendered the raw `sidebar_preview()` through `StyledText`. GPUI lays a literal newline out as a second line even under `truncate()`. The preview here is "🫠 Galaxy, we have a problem" plus a newline and more text. Normal chat-list rows already replace newlines with spaces (`chatlist_style.rs`), so they were fine; the search rows (Chats, Public chats, Messages) and in-chat search hits were not.
- Fix: `one_line_preview` turns control characters and U+2028/2029 into spaces before highlight ranges are computed, in both `search_result_row` and `chat_search_hit_row`. The row keeps `truncate()` for the ellipsis. Matches tdesktop's one-line dialog preview.

### Verification

Demo kinds `ready-join-bar`, `ready-search-previews`, `ready-multiline-rows` (fixtures in `src/ui/chatlist_demo.rs`). Before/after PNGs are in the PR description. Unit test `one_line_tests`.
