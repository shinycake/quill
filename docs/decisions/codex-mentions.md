# codex/mentions — @ member suggestions in the composer

- Typing `@` plus name characters at a word start in a group opens a suggestion list above the
  composer showing avatar, name and @username. Up/Down move the highlight, Enter or Tab
  completes, Esc or blur dismisses, and a click picks.
- A completion inserts `@username `, or for users without one a `[Name](tg://user?id=…) ` link,
  which TDLib turns into a mention-name entity.
- Core:
  - `composer::mention_trigger` / `complete_mention` (unit-tested).
  - `searchChatMembers` request (`RequestPurpose::SearchMentionMembers`).
  - `Session::mention_search`: chat, query, matching user ids (self excluded), and the
    in-flight request. Answers to an older query are dropped. While a new query loads, the
    previous matches stay visible.
  - `ConnectDriver::search_mentions`: groups only, deduped by query. Driver test covers
    private chats, staleness, dedupe and clearing.
- Up-to-edit (#354) yields to an open suggestion list.
- Demo `ready-mentions` shows the list.
