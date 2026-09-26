# Quill roadmap — toward Telegram parity

Phase 0–1 are done (see README status checklist). This document plans Phase 2+
toward full Telegram desktop parity. Slice naming follows the repo convention
(`Phase N: <short name> (#PR)`); one slice per PR, each with replay tests and a
DECISIONS.md entry.

Sources of truth, in order: `DECISIONS.md` per-slice specs (the "Out of this
slice" lists are the near-term backlog), this roadmap, the vendored
`schema/td_api.tl` (official TDLib 1.8.67 — never invent constructors).

## Phase 2 — Sponsored content + channels (keystone)

Unlocks the README blocker: channels/bots are gated until sponsored-content
handling exists.

- **2.1 Sponsored-message handling.** ✅ Done (2026-09-26). Fetch
  `getChatSponsoredMessages` for the open channel; render sponsored rows with a
  **Sponsored** / **Recommended** label (`sponsoredMessage.is_recommended`);
  `reportChatSponsoredMessage` flow; `viewSponsoredChat` typed driver
  (integration deferred to 2.2+). Sponsored
  media follows the existing download sandbox (thumbs at priority 1, click for
  full at 32). Channel gate retained for 2.2.
- **2.2 Broadcast channels.** ✅ Done (2026-09-26). Ungated
  `chatTypeSupergroup` with `is_channel`; channels appear in the chat list;
  history renders broadcast posts (author = channel,
  `message.interaction_info.view_count`, live via
  `updateMessageInteractionInfo`); composer hidden in channels
  (`ChatSummary::can_post()` false — driver rejects channel sends the same
  way); `joinChat` / `leaveChat` for public channels with own-membership
  probe (`getMe` → `getChatMember`, `updateChatMember` refresh).
  `ScreenshotDemo::ReadyChannels` → `docs/screenshots/ready-channels.png`.
  Composer hidden for admins too in this slice (admin posting is 2.3).
- **2.3 Channel admin posting.** ✅ Done (2026-09-26). Admins get the
  composer in channels: posting rights derive from own membership (Creator,
  or Administrator with `rights.can_post_messages` true); `sendMessage`
  with channel semantics through the existing send path (the server echoes
  `message.is_channel_post` / `sender_id: messageSenderChat`); view-count
  updates live via `updateMessageInteractionInfo`. Non-admins keep the
  hidden composer + join/leave footer. `ScreenshotDemo::ReadyChannelsAdmin`
  → `docs/screenshots/ready-channels-admin.png`.

## Phase 3 — Bots

- **3.1 Bot chats.** Private chats with `userTypeBot` users ungated; bot info
  panel (`getUserFullInfo` → `bot_info`: description, commands).
- **3.2 Inline keyboards.** Render `replyMarkupInlineKeyboard` under bot
  messages; callback buttons → `answerCallbackQuery`; URL buttons → OS open;
  `switchInline` buttons → insert query in the chosen chat's composer.
- **3.3 Bot commands menu.** Composer `/` menu from `bot_info.commands` /
  `getCommands`; tap inserts the command.

## Phase 4 — Message richness

- **4.1 Text entities.** Bold, italic, underline, strikethrough, spoiler,
  `code`, `pre` (`textEntityTypeBold` … `textEntityTypePreCode`) in message
  text and captions. (URLs already done.)
- **4.2 Polls.** `messagePoll` display with bars and voter counts;
  `setPollAnswer` voting; `inputMessagePoll` creation from the composer.
- **4.3 Location / venue / contact.** `messageLocation`, `messageVenue`,
  `messageContact` display rows (map link via OS open for locations).
- **4.4 Dice.** `messageDice` animated emoji + value display.
- **4.5 Fullscreen media viewer.** Click a photo/video opens a viewer overlay
  (from the photo/document "Out of this slice" backlog).
- **4.6 Seek bars.** Audio and voice-note rows get scrubbing (backlog).

## Phase 5 — Supergroups: forum topics

- **5.1 Topics.** `getForumTopics` list for forum supergroups; per-topic
  history via `searchChatMessages` with `topic_id`; topic badges in the
  chat list.

## Phase 6 — Contacts & profiles

- **6.1 Contacts.** `getContacts` list; `addContact` flow.
- **6.2 Info panels.** User info (`getUserFullInfo`: bio, photo) and
  supergroup info (`getSupergroupFullInfo`: description, member count) panels.

## Phase 7 — Folders & discovery

- **7.1 Chat folders.** Folder tabs beyond Main/Archive (`getChatListsToAddChat`
  / folder UI).
- **7.2 Public username lookup.** `searchPublicChats` in global search
  (backlog).

## Phase 8 — OS notifications

- **8.1 Desktop notifications.** `updateNewMessage` → OS notification for
  unread incoming in background; click focuses the chat. (Linux: `notify-send`
  path; macOS: native center via gpui-kit.)

## Phase 9 — Stories (stretch)

- **9.1 Story viewing.** `getStory` / story list for contacts; viewer overlay.
  Posting stays out.

## Folded-in backlog (from per-slice "Out of this slice" lists)

Scheduled into the phases above or as small follow-ups: sender/tag filters
and calendar jump for in-chat search; quote-from-selection replies;
external/story replies; draft `reply_to` persistence; ShareBox comment on
forward; hide-sender / `send_copy` forward; folders in the forward picker;
custom emoji / sticker reactions and `getMessageAvailableReactions`;
unread-reaction badges; multi-pin list and `unpinAllChatMessages`; custom mute
picker and `loadChats` of the archive list at startup; `toggleChatIsMarkedAsUnread`;
recording/upload/sticker chat actions; per-user typing names in groups;
inline GIF search (`animation_search_bot_username`); `addSavedAnimation` /
`removeSavedAnimation`; document/audio albums; spoiler rendering;
`tg://` link handling; link-preview options on send; streaming/HLS video.

## Explicitly out of scope

Per DECISIONS.md, unchanged: secret chats, voice/video calls, payments and
Telegram Stars, multi-account, telemetry, AI features, App Store /
notarization / distribution pipeline.

## Blocked (cannot be proven on this Linux agent)

- VoiceOver pass on macOS (README unchecked item) — needs a Mac session.
- Real IME composition on macOS — same.
- Native tdjson build + rpath verification on Apple Silicon.
