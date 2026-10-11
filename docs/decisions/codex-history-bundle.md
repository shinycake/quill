## codex/history-bundle (2026-10-11)

Three history items in one slice: the similar-channels strip after joining
a channel (`parity:render-similar-channels`), the fact-check block under
messages (`parity:render-fact-check`), and the reply-thread leftovers of
`codex-thread-section.md`.

### 1. Similar channels strip

#### What tdesktop does
- `History::insertJoinedMessage` adds a local "You joined this channel"
  service message (`GenerateJoinedMessage`, flag `ShowSimilarChannels`) at
  the channel's invite date, when the inviter is known.
- Its media is `HistoryView::SimilarChannels`
  (`history/view/media/history_view_similar_channels.cpp`): a bubble with
  a small arrow pointing at the notice, the title "Similar channels", one
  tile per channel (50px photo, a subscriber-count badge in the photo's
  average color, the name on two lines), 12px between tiles, at most 424px
  wide and horizontally scrollable; "View all" appears at the right when it
  scrolls and opens the profile's similar-peers section.
- `countOptimalSize` lists the first `recommended_channels_limit_default`
  (10) channels; when the server counted more than the list holds, or the
  list is more than twice the limit, the last shown tile becomes "More
  Channels" with a "+N" badge, stacked thumbnails of the next channels,
  and a lock without Premium. The locked tile shows a Premium promo toast
  ("Subscribe to Telegram Premium to unlock up to {count} similar
  channels."); with Premium it opens the similar-peers section.
- The strip shows only while the channel's `SimilarExpanded` flag is set:
  `ApiWrap::joinChannel` sets it (and loads the list); clicking the service
  text toggles it (`ensureToggleLink`). There is no close button.
- Opening a tile goes through the channel's `openLink`; TDLib's equivalent
  is `openChatSimilarChat`.

#### What changed
- Reducer (`state/session_similar.rs`): `GroupsState::similar_strips`
  keeps `SimilarStrip { joined_at, expanded }` per channel joined in this
  session (`accept_join_chat_result` → `note_channel_joined`, channels
  only); `toggle_similar_strip` flips it and bumps `Session::revision` so
  the rows rebuild; `joined_notice_row` is the synthetic
  `ServiceAction::YouJoinedChannel` row (id `JOINED_NOTICE_ROW_ID`, a
  negative id `report_visible_messages` never sends); `similar_tiles` is
  tdesktop's `countOptimalSize` arithmetic (`SimilarTile::Channel` /
  `More { count, locked, stack }`); `similar_strip_tiles` combines the
  profile panel's `getChatSimilarChats` list, the new
  `UsersState::similar_chat_totals` (`chats.total_count`, noted in the
  chat-list domain's `Chats` arm) and `is_premium()`.
- Driver: `open_similar_chat` sends `openChatSimilarChat` (schema 1.8.68
  line 12016, purpose `UsersPurpose::OpenChatSimilarChat`). The list itself
  reuses `fetch_similar_chats` (deduped), requested from the strip when
  missing.
- UI (`ui/similar_strip.rs`): the strip under the notice in
  `service_row.rs`; the notice toggles it; tiles open the channel
  (`openChatSimilarChat` + select); "More Channels" and "View all" open the
  channel's info panel (its "Similar channels" section lists everything);
  the locked tile puts the Premium line in the status note. The badge uses
  the accent color (no average-color pass over the photo). The conversation
  appends the notice to the main history rows (`conversation_part1.rs`).
- `ServiceAction::YouJoinedChannel` / `ThreadDivider` are Quill-local
  variants with tdesktop's wording in `service_text/actions.rs`.
- Limits: tdesktop reads `recommended_channels_limit_default/premium` from
  app config; TDLib exposes them as `getPremiumLimit
  (premiumLimitTypeSimilarChatCount)`. Quill uses the tdesktop fallbacks
  (10 / 100) as constants instead of an extra request.

#### Not done
- tdesktop keeps the joined notice across restarts (invite date from the
  server); Quill shows it only for channels joined in this session. TDLib
  has `chatMember.joined_chat_date` from `getChatMember`, but
  `ParsedChatMember` does not carry it yet.
- `getChatSimilarChatCount` is not used; `chats.total_count` of the list
  answer gives the "+N" count.

### 2. Fact-check block

#### What tdesktop does
- `HistoryMessageFactcheck` (from `message.factcheck`, requested with
  `messages.getFactCheck` when `need_check`) renders as a `WebPage` of type
  `Factcheck` (`history_view_web_page.cpp`): a quote block in color index 0
  (red) with site name "Fact Check" and the text as description, collapsed
  to `kFactcheckCollapsedLines` (3) with an expand/collapse chevron at the
  bottom right (always expandable "because of the footer"); expanded, a
  30% separator and the footer `lng_factcheck_bottom` ("This clarification
  was provided by a fact checking agency assigned by the department of the
  government of your country ({country}) responsible for combatting
  misinformation.") in the small font. A "what's this?" hint next to the
  title shows the same text as a 5-second toast. The block sits after the
  text/media and before the bottom info.
- TDLib: `message.fact_check` (`factCheck text:formattedText
  country_code:string`, schema 1.8.68 line 3418) and
  `updateMessageFactCheck` (line 10826); TDLib handles the fetch, so there
  is no `need_check` state.

#### What changed
- `MessageExtras::fact_check` is now `Option<FactCheck { text, entities,
  country_code }>` (was the plain text); `parse_fact_check` drops empty
  texts; `MessagesPayload::UpdateMessageFactCheck` carries the option. The
  menu's Add/Edit entry, the edit dialog's optimistic demo write and the
  driver test follow.
- UI (`ui/history/fact_check_block.rs`): the block inside the bubble's
  `extra` column (after the caption, before keyboards and reactions):
  danger-colored left bar and tinted background, "Fact Check" plus
  "what's this?" (status note), the text with entities through
  `rich_text_line`, three lines when collapsed (`max_h` = 3 × 1.4 × font),
  the chevron, and the separator + footer when expanded. Expansion lives in
  `MessageUi::fact_check_expanded`, is part of `HistoryRowInputs`
  (`renders_like`) and of `history_rows_ui_hash`, so the row remeasures.
  The footer's country name comes from the sign-in country list when
  loaded, else the ISO code (tdesktop falls back the same way).
- `tail_empty` counts the block, so the time footer takes its own line.

### 3. Reply-thread leftovers

#### What tdesktop does
- `RepliesList::injectRootDivider`: after the root, a `FakeHistoryItem`
  service row dated like the root, "Discussion started" when replies follow
  in the slice, "No comments here yet..." otherwise, only once the slice
  reaches the root (`skippedBefore == 0`).
- `TopBarWidget::paintTopBar` in `Section::Replies` (non-topic): the title
  is `lng_manage_discussion_group` ("Discussion") and the status line is
  the custom title "N comments" / "N replies" (`ChatWidget::setReplies`).
  The brief assumed the count is the title; it is the status line.
- Drafts: `ChatWidget` saves the thread draft through the compose controls
  (`saveCurrentDraftToCloud` with the reply-to root), and `RepliesList`
  seeds it from `messages.getDiscussionMessage`'s draft.

#### What changed
- `ThreadView::rows()` returns the list rows with the divider
  (`THREAD_DIVIDER_ROW_ID`, `ServiceAction::ThreadDivider`) after the last
  root row once `loaded_complete`; the conversation uses it for the thread
  list.
- The thread header title is "Discussion"; the count stays the status line
  (`conversation/mod.rs`).

#### Not done
- Thread drafts: Quill saves composer drafts for private chats only
  (`accepts_composer_draft`, `note_composer_draft`), keyed by chat. Hosting
  `messageThreadInfo.draft_message` and `setChatDraftMessage` with
  `messageTopicThread` needs group drafts keyed by `(chat, topic)` first;
  not wired, so a thread keeps the group's composer text.

### How verified
- Reducer tests (`state/tests/similar_strip.rs`): tile arithmetic (no
  overflow, server total beyond the list, list over twice the limit,
  Premium lock), join of a channel vs. a group, the `chats` answer with
  `total_count`, the empty answer, toggle + revision bump.
- Thread tests (`state/tests/threads.rs`): divider after the root once
  complete, wording with and without replies, no rows before the root.
- Wording test (`service_text_tests`), parser tests
  (`envelope/tests_messages.rs`: entities, country, absent/null/empty,
  `updateMessageFactCheck`), driver tests (`connect/tests/message_menu.rs`
  updated; `profile_panels.rs`: `openChatSimilarChat` shape).
- Demo captures (temporary fixtures on `ready-channels` /
  `ready-channels-admin`, not committed): the collapsed and expanded fact
  check, the strip with six tiles and "View all", the locked "+10" More
  tile, the thread with "Discussion" / "2 comments" and the divider.
- Gate (fmt, clippy, core + UI tests), `check-hotspots.sh`,
  `check-file-size.sh`.
