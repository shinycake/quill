# Decisions (Phase 0 / early Phase 1)

Research snapshot 2026-09-16, pin recheck **2026-09-17**.

## Product

- Fresh MIT repository **Quill**. Ideas-only from ZapFast / Paper Plane / Mezon / Coop. Not a fork.
- **Repo visibility: Public.**
- **Primary runners: Idan's personal Mac + Linux** (supported build/run targets, not deferred). GHA remains best-effort when billing allows; Linux CI runs unit/replay tests; `macos-latest` can compile the GPUI binary and assemble a dummy `.app` when jobs start.
- **No App Store / notarization / distribution pipeline.** Ad-hoc Apple Developer signing only if needed for Idan's personal Mac.
- **Credentials:** `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` (or gitignored local `.env` / `quill.local.env`) load into the app. When credentials are present **and** `tdjson` is available (`QUILL_TDJSON_PATH` or bundled next to the executable), Quill opens `LiveTdJson`, sends `setTdlibParameters`, and drives auth updates to `WaitPhoneNumber`. Missing tdjson shows an honest install/build halt. Still no secrets in git. See `docs/credentials.md`.
- One account, cloud chats only. Channels/bots gated until sponsored-content handling exists. No secret chats, calls, telemetry, or AI.
- Storage: TDLib DB + small prefs. No second message database.

## UI family

- Toolchain pin: **Rust 1.98.1** (2026-09-03 stable). GPUI Kit 0.6.1 / gpui-pre 0.3.5 require at least 1.92 (`oo7`).
- Hello-world follows Kit docs: `gpui_kit::application()` + `gpui_kit::init` + `Root`.
- Default Kit features kept to `component` + `assets`. No tree-sitter / inspector / editor language packs.
- Synthetic chat uses Kit `MessageScroller` (auto-measured mixed heights, prepend, `remeasure_items`) and `TextareaState::submit_on_enter(true)`.
- Composer send policy is tested without GPU: IME composition and Shift/secondary Enter do not send.
- Kit `InputEvent::PressEnter` (**gpui-base 0.6.1**) has `{ secondary, shift }` only — no composing flag. `InputBaseState::enter` always emits `PressEnter` and does **not** consult `ime_marked_range` (Escape does). Quill reads `EntityInputHandler::marked_text_range` at PressEnter time via `enter_event_from_kit`. Do not hardcode `composing: false`.
- **VoiceOver** is a manual macOS follow-up. This agent has no GUI/VoiceOver runner on Linux. Do not claim the Phase 0 a11y gate until a Mac session records it.
- **Screenshots:** real GPUI window on Linux xvfb + lavapipe, `docs/screenshots/synthetic-chat.png` (plus composer and unsupported-auth shots), connect surfaces via `quill --screenshot-demo`, Phase 1 `ready-chats` / `ready-chats-composer`, unread proof `ready-unread` / `ready-unread-read`, media receive `ready-media`, outgoing attach/send `ready-send-media`, global search `ready-search`, in-chat search `ready-search-in-chat`, reply `ready-reply`, edit/delete `ready-edit-delete`, forward `ready-forward`, reactions `ready-reactions`, pin/unpin `ready-pin`, mute/archive `ready-mute-archive`, typing `ready-typing`, stickers `ready-stickers`, voice notes `ready-voice`, link previews `ready-link-preview`, saved GIFs `ready-gifs`, and media albums `ready-albums` (injected Ready + inbox/outbox/media/search/reply/edit/forward/reaction/pin/mute/archive/typing/sticker/voice updates, no live Telegram). The stray “X” in an early capture was the X11 cursor, not a jump button. VoiceOver remains a macOS follow-up.

## TDLib

- Runtime + schema: official TDLib **1.8.67**, commit `d1085f9cebc5a62379991ae1652673954f229c1f`.
- Vendored `schema/td_api.tl` is the official file: **1,152,505 bytes**, SHA-256 `326b65b41442901ad6bf0ca2f7c356ae54365d6c343956a62e06a8b3cb305e87`. Tests fetch that commit’s `td/generate/scheme/td_api.tl` from GitHub and require **byte equality** (a 1,000,001-byte truncated copy with stripped `vector<T>` parameters is invalid). Refresh with `scripts/vendor-td-schema.sh`.
- **Rejected** as-is `tdlib-rs`: schema targets 1.8.61, receive splits responses vs updates, unknown variants log raw JSON.
- **Chosen path:** bind official `tdjson` C JSON API (`td_create_client_id` / `td_send` / `td_receive`) with one receive thread, copy-before-next-call, monotonic sequence, single reducer. Typed coverage for Phase 0/1 constructors; unknown variants keep only `@type`.
- `@extra` is a decimal **string** (no float IDs). `int64` (chat order) parsed from JSON strings. `sendMessage` uses schema `topic_id`.
- Native log callback counts only; it does not read or forward the C string. No TDLib calls from that callback.
- Ordinary builds do not download tdjson. Optional source build: `scripts/build-tdlib.sh`. Loader never searches Homebrew.

## Phase 1 (no secrets)

- Account-scoped directories under the app data dir.
- Database key: 32 random bytes in Keychain on macOS (`org.shinycake.quill` / `db-key:{account}`). **Linux live path:** `FileSecretStore` — `{app_data}/accounts/{account}/db-encryption.key`, mode `0600`, zeroize after read into `DatabaseKey`. `MemorySecretStore` is **tests only** (not `bootstrap_connect` on Linux). `KeychainSecretStore::get` maps `errSecItemNotFound` to missing (`Ok(None)`) and user-cancel / auth-failed / interaction-not-allowed / keychain-unavailable to `Locked`. Missing key + existing DB → halt, never mint a replacement.
- Auth view is a pure function of `updateAuthorizationState`. Premium / email / registration / unknown → unsupported halt UI. No payments, auto-register, or password reset.
- Chat list / history / send reducers with replay fixtures. After `Ready`, `ConnectDriver` pages `loadChats` until 404; UI reads `Session::ordered_chats()`. Composer submit freezes a `ComposerSnapshot` and sends `sendMessage`. Logout invalidates pending requests. Close ≠ logOut.
- Unread / read receipts (TDLib **1.8.67**): `updateChatReadInbox` (`unread_count` + `last_read_inbox_message_id`) and `updateChatReadOutbox` (`last_read_outbox_message_id`) are typed. The chat list badge is that inbox count. Opening a supported chat sends `openChat` then `viewMessages` (`messageSourceChatHistory`, `force_read: true`) for loaded history; switching away sends `closeChat`. Unread is never zeroed locally — only the inbox update is authoritative. Outgoing rows show **read** when `last_read_outbox_message_id >= id`, otherwise **sent** (schema supports outbox; `0` means nothing outgoing has been read yet).
- Photo / document receive: typed `messagePhoto` / `messageDocument` + `file`/`localFile`/`updateFile`; `downloadFile` for thumbs (priority 1) or click (32). Display paths sandboxed under account `tdlib_files` (demo fixtures allowlist).
- Photo / document **send**: composer Attach photo / Attach file freezes a `ComposerAttachment` via `pick_send_path` (explicit user pick only — never TDLib JSON paths). `sendMessage` uses `inputMessagePhoto`/`inputPhoto`/`inputFileLocal` or `inputMessageDocument`/`inputDocument`/`inputFileLocal` with caption. `RequestPurpose::SendMessage` marks the response `message` pending until `updateMessageSendSucceeded`.
- Credentials load from owner `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` (or local gitignored env files). Connect path: `evaluate_gate` → `prepare_connect` (paths + DB key) → `LiveTdJson` + `setTdlibParameters` → session auth reducers. Phone submit sends `setAuthenticationPhoneNumber`; WaitCode / WaitPassword submit `checkAuthenticationCode` / `checkAuthenticationPassword`. Never pasted into this repo.


## Linux DB key persistence (2026-09-17)

- **Problem:** `bootstrap_connect` used process-local `MemorySecretStore` on Linux. First live run minted a TDLib DB encryption key, then lost it on quit → `MissingKeyAgainstExistingDb` on the next launch.
- **Decision:** `FileSecretStore` under the account-scoped app data dir (`directories` ProjectDirs / `Quill`), file `db-encryption.key`, `0600`, atomic write (temp + rename), zeroize the read buffer after constructing `DatabaseKey`. Wired as the Linux live-connect store; macOS stays Keychain.
- **Tests:** round-trip across a new store instance (simulates restart); `MissingAgainstExistingDb` when the key file is absent but `tdlib/` already exists; mode `0600` asserted on Unix.

## Licenses

- Quill: MIT (existing LICENSE).
- GPUI Kit / GPUI: Apache-2.0.
- TDLib: Boost Software License 1.0 (not compiled into the default artifact yet).
- No GPL sources were copied.

## Measured prototype notes

- GPUI Kit 0.6.1 hello-world (`application` + `init` + `Root`) compiles on this Linux agent after `libfontconfig1-dev` and Vulkan lavapipe. Linux CI still runs **core only** (`--no-default-features`) so GitHub Ubuntu does not need a GPU.
- `MessageScroller` mixed-height prepend/remeasure works in the live window. Default tail-follow hides the first row when content is taller than the pane; the prototype scrolls to item 0 so the short row is visible in screenshots.
- Kit `TextareaState::submit_on_enter(true)` plus `should_send_on_enter` / `enter_event_from_kit(marked_text_range)` is the composer policy. Kit does not put composing on `PressEnter`; we use the IME mark.
- Official tdjson is **not** compiled here. Connect is proven with injected JSON (`ConnectDriver` + `RecordingSender`): parameter send shape, credential/tdjson gate, WaitPhoneNumber transition. Live `ReceiveBridge::spawn_live` is wired for machines with tdjson. Native bundle steps are documented for Apple Silicon.

## Blockers / follow-up

1. Live TDLib connect is implemented (`src/connect.rs`): credentials + tdjson → `setTdlibParameters` → auth UI. After Ready, `loadChats` + chat list / `sendMessage`. Headless proof: `quill --connect-smoke` (requires `QUILL_TDJSON_PATH`; no secrets in the one-line result). Teardown sends `close` and waits for `authorizationStateClosed` before joining the receive thread and unloading tdjson. Still machine-local: building/bundling tdjson (`docs/native-bundle.md`). No secrets in git.
2. VoiceOver + real IME on a Mac (this environment cannot prove them). Primary Mac runner is Idan's personal machine.
3. Native tdjson build + rpath verification on Apple Silicon (`docs/native-bundle.md`). This Linux agent has no tdjson; UI shows the MissingTdjson halt when credentials are loaded.
4. **No App Store / notarization / distribution pipeline.** Ad-hoc Apple Developer signing only if needed for Idan's personal Mac. Public repo; brand polish is still a follow-up.
5. **GitHub Actions did not run** on 2026-09-17: billing/spending limit. Re-run after billing is fixed. Local gates: `cargo fmt`, `clippy -D warnings`, `cargo test --no-default-features --locked`.

## Global search (Phase 1)

- **Official clients first:** tdesktop `shortcuts.cpp` maps Ctrl+F to `Command::Search`. That command is **not** “open sidebar search”: `history_widget.cpp` handles it at priority 1 as `searchInChat` when history is focused; `dialogs_widget.cpp` focuses the sidebar field only when there is **no** active chat. Sidebar search is **Cmd/Ctrl+K** (and the field / Search button). Unigram (TDLib-shaped) empty field is `searchRecentlyFoundChats`; typed query is `searchChats` then `searchMessages` with `chat_list` **null**; select sends `addRecentlyFoundChat`. Typed live queries debounce **900 ms** (`kSearchRequestDelay` / `AutoSearchTimeout` in tdesktop `config.h`). Skipped this slice: `getTopChats`, `searchContacts`, `searchChatsOnServer`, `searchPublicChats`.
- **Schema (1.8.67, not invented):** `searchRecentlyFoundChats` (`query`, `type_filter` null, `limit` 50) and `searchChats` (`query`, `type_filter` null, `limit` 20) return `chats`. `searchMessages` (`chat_list` null = all lists; schema only Main/Archive; `offset` `""`; `limit` 20; `filter` / `chat_type_filter` null; `min_date`/`max_date` 0) returns `foundMessages`. Select: `addRecentlyFoundChat` then existing `openChat`. Message-result pagination and public username lookup stay out of this slice.
- **UX:** Ready sidebar always shows a Search field (tdesktop dialogs column). Cmd/Ctrl+K focuses it and loads **Recent**. Typing replaces the chat list with **Chats** + **Messages** after the 900 ms debounce. Esc/Clear empties to Recent, then closes (chat list returns). A message hit is upserted into history then opened — no new history pager. Stale `@extra` is ignored.
- **Out of this slice:** public username lookup, archive/secret lists, sponsored results, VoiceOver.

## In-chat search (Phase 1)

- **Official clients first:** tdesktop `history_widget.cpp` `setupShortcuts` handles `Command::Search` (`ctrl+f` / `Command+F` in `shortcuts.cpp`) as `searchInChat` when history is focused. ComposeSearch (`history_view_compose_search.cpp`) types into a top bar; `requestSearchDelayed` uses `AutoSearchTimeout` (same 900 ms as `kSearchRequestDelay`). Empty query does not send a search. Next/prev walk newest-first hits (Unigram `ChatSearchViewModel` Next = newer / lower index). Close/Esc hides the finder and leaves history scroll/open chat alone. Stale request ids are ignored (`api_messages_search.cpp` drops `requestId != _requestId`).
- **Unigram / TDLib:** typed query is `searchChatMessages` (`topic_id` / `sender_id` / `filter` null, `from_message_id` 0, `offset` 0, `limit` 50 = tdesktop `kSearchPerPage`) returning `foundChatMessages`. Jump uses Unigram `LoadMessageSliceImpl`: `getChatHistory(chatId, maxId, -25, 50)`. Already-loaded → highlight; tombstone → deleted; around-load without the id → inaccessible; in-flight → not-yet-loaded.
- **Schema (1.8.67, not invented):** `searchChatMessages chat_id topic_id query sender_id from_message_id offset limit filter = FoundChatMessages`. `foundChatMessages total_count messages next_from_message_id`.
- **Out of this slice:** sender/tag filters, calendar jump, secret-chat `searchSecretMessages`, reactions (edit/delete and replies shipped in later slices).

## Reply to message (Phase 1)

- **Official clients first:** tdesktop `history_view_compose_controls.cpp` `FieldHeader::replyToMessage(FullReplyTo)` shows a compact header above the field; `replyCancelled` / field Escape (`initKeyHandler` Key_Escape → Cancel) clears that header and **does not** wipe typed text (`ComposeControls::clear` wipes text on send, not on cancel). Context-menu / per-message **Reply** is the pick-a-message action (Ctrl+Up/Down `ReplyNextRequest` exists in tdesktop but Quill already binds ⌘/Ctrl+↑ to Load older — do not steal it). Quote-strip activation jumps to the replied id; Unigram `LoadMessageSliceImpl` is `getChatHistory(chatId, maxId, -25, 50)` — reuse the in-chat search jump pipeline (`begin_chat_search_jump` / `jump_to_chat_search_message`).
- **Schema (1.8.67, not invented):** incoming `message.reply_to` is `messageReplyToMessage chat_id message_id quote:textQuote … content:MessageContent`. Outgoing `sendMessage.reply_to` is `inputMessageReplyToMessage message_id quote:inputTextQuote checklist_task_id poll_option_id` (`quote` null = whole message; `checklist_task_id` 0; `poll_option_id` `""`). Cross-chat `inputMessageReplyToExternalMessage` / stories stay out of this slice.
- **UX:** Ready history rows get a **Reply** action; composer shows a **Replying to** quote + **Cancel**; send attaches `reply_to`. Incoming/outgoing reply bubbles show a quote strip; click jumps (already-loaded → highlight; else load-around). Esc: in-chat search, then sidebar search, then cancel reply (keep typed text).
- **Out of this slice:** edit/delete, forward, reactions, voice/video, quote-from-selection (`inputTextQuote` text), external/story replies, draft persistence (`draftMessage.reply_to`), App Store.

## Edit / delete own messages (Phase 1)

- **Official clients first:** tdesktop `FieldHeader::editMessage` + `ComposeControls::cancelEditMessage` (`history_view_compose_controls.cpp`) enter edit mode, then Escape/`Cancel` calls `cancelEditMessage` which clears the edit header and `applyDraft()` restores the **normal** draft (`DraftType::Normal`) — it does not wipe unrelated typed text. Per-message **Edit** / **Delete** are the pick-a-message actions (do not steal ⌘/Ctrl+↑ Load older). Delete confirm is tdesktop `boxes/delete_messages_box.cpp` / Unigram `DeleteMessagesPopup` (`PrimaryButtonText = Delete`, `SecondaryButtonText = Cancel`). Unigram defaults `RevokeCheck.IsChecked = true` for own outgoing that `CanBeDeletedForAllUsers`; tdesktop has used the same default since 4.3.3 (`f064575`). Incoming / others' messages stay out — schema `messageProperties.can_be_edited` / `can_be_deleted_*` plus `getMessageProperties` are the live gate; this slice uses own outgoing as the documented default.
- **Schema (1.8.67, not invented):** `editMessageText chat_id message_id reply_markup input_message_content = Message` (`inputMessageText` + `formattedText`). `editMessageCaption chat_id message_id reply_markup caption show_caption_above_media = Message` for outgoing photo/document captions. `deleteMessages chat_id message_ids revoke = Ok` with `revoke: true`. History applies `updateMessageContent chat_id message_id new_content`; rows leave via existing `updateDeleteMessages` tombstones.
- **UX:** Ready outgoing rows get **Edit** and **Delete**. Edit: composer shows **Editing message** + Cancel, field loads original text. Enter/save sends the edit constructor; Esc/Cancel restores the stashed draft. Delete: confirm banner, then `deleteMessages`. Esc: delete confirm, then in-chat search, sidebar search, reply, then cancel edit.
- **Out of this slice:** others' messages, `editMessageMedia` / replace media, forward, reactions, voice/video, App Store, live Telegram in CI.

## Forward message(s) (Phase 1)

- **Official clients first:** tdesktop per-message **Forward** opens `ShowForwardMessagesBox` (`window/window_peer_menu.h`) with `Data::ForwardDraft` / `MessageIdsList`. The dest UI is ShareBox (`boxes/share_box.cpp`) titled **Forward to…** (issue #28373 / #30168): search the loaded chat list, pick a peer, send. History multi-select is the same draft with several ids. ShareBox comment is a **separate** `sendMessage` after the forward — skipped here to keep the slice tight. Unigram: `ChooseChatsView` + `TelegramClient.ForwardMessages`. Esc closes the box (picker first), then selection.
- **Schema (1.8.67, not invented):** `forwardMessages chat_id topic_id from_chat_id message_ids options send_copy remove_caption = Messages`. `send_copy` false preserves `message.forward_info` attribution (official default; hide-sender copy is `send_copy` true). `remove_caption` ignored unless copy. `topic_id` / `options` null. Ids strictly increasing, at most 100, only if `messageProperties.can_be_forwarded` (this slice: already-sent, non-pending). Response `messages` may contain nulls for failed ids. Incoming attribution: `messageForwardInfo origin:MessageOrigin` (`messageOriginUser` / `HiddenUser` / `Chat` / `Channel`).
- **UX:** Ready history rows get **Forward** and **Select**. Select accumulates a same-chat draft; Forward opens the dest picker (loaded supported chats, local title search). Pick dest → `forwardMessages` → success banner names the dest + forwarded ids. Rows with `forward_info` show **Forwarded from**. Esc: picker, delete confirm, in-chat search, sidebar search, reply, edit, then clear selection / dismiss success.
- **Out of this slice:** ShareBox comment, hide-sender / `send_copy`, `inputMessageForwarded`, `getMessageProperties` live gate, folders in the picker, reactions, voice/video, App Store, live Telegram in CI.

## Emoji reactions (Phase 1)

- **Official clients first:** tdesktop `HistoryView::Reactions` `InlineList` (`history/view/reactions/history_view_reactions.cpp`) paints chips with emoji + `count` and a `chosen` highlight (`is_chosen`). Chip click toggles: chosen → remove, otherwise add (`data/data_message_reactions.cpp`). Unigram `ReactionButton` is the same chip (count + chosen) and drives TDLib `AddMessageReaction` / `RemoveMessageReaction`, not bots-only `setMessageReactions`. Esc closes the picker first (same overlay-first stack as ShareBox).
- **Schema (1.8.67, not invented):** `addMessageReaction chat_id message_id reaction_type is_big update_recent_reactions = Ok` (`reactionTypeEmoji`, `is_big` false for chip/picker click, `update_recent_reactions` true). `removeMessageReaction chat_id message_id reaction_type = Ok`. Incoming chips from `message.interaction_info.reactions` (`messageReaction type total_count is_chosen`) and `updateMessageInteractionInfo`. Picker uses the official default emoji list (tdesktop active-emoji first row / Unigram default). `getMessageAvailableReactions`, custom emoji, and paid stay out.
- **UX:** Ready history rows get **React** (picker of default emoji) plus chips (`emoji + count`; highlight when `is_chosen`). Chip or picker tap on an own reaction sends `removeMessageReaction`. Esc: reaction picker, then forward picker, delete confirm, in-chat search, sidebar search, reply, edit, then clear forward selection / dismiss success.
- **Out of this slice:** custom emoji / sticker reactions, paid/unique, `getMessageAvailableReactions` live list, unread-reaction badges, App Store, live Telegram in CI.

## Pin / unpin messages (Phase 1)

- **Official clients first:** tdesktop history rows expose **Pin message** / **Unpin message**; the conversation shows a **PinnedBar** under the title (`history_widget` `PinnedBar`) with preview text, tap-to-jump, and Unpin/cancel. Unigram mirrors TDLib `PinChatMessage` / `UnpinChatMessage` and a top pinned bar. Multi-pin lists stay out — banner shows the newest loaded pinned message (`getChatPinnedMessage` is newest).
- **Schema (1.8.67, not invented):** `pinChatMessage chat_id message_id disable_notification only_for_self = Ok` (official Pin: both bools false; schema notes notifications are always off in channels/private). `unpinChatMessage chat_id message_id = Ok`. Incoming state from `message.is_pinned` and `updateMessageIsPinned chat_id message_id is_pinned`. Live `getMessageProperties.can_be_pinned` stays out (same default as edit/forward/react: already-sent).
- **UX:** Ready history rows get **Pin** / **Unpin**. Open chat with a pin shows a **Pinned message** bar (preview + Unpin); tap jumps/highlights via the existing in-chat jump pipeline. Esc stack unchanged — pin has no overlay (picker/confirm), so reaction picker → forward → delete → searches → reply → edit still holds.
- **Out of this slice:** multi-pin list UI, `unpinAllChatMessages`, `only_for_self` / notify checkbox, `getMessageProperties` live gate, channels/bots, App Store, live Telegram in CI.

## Mute / unmute and archive / unarchive (Phase 1)

- **Official clients first:** tdesktop chat context menu (`window_peer_menu.cpp`) shows **Unmute** when `notifySettings->isMuted`, otherwise a **Mute** submenu. `menu/menu_mute.cpp` fills that submenu from `SessionSettings::mutePeriods()` (defaults **1 hour / 8 hours / 2 days**, `DefaultTimePickerValues`) plus **Forever** (`kMuteForeverValue` = `numeric_limits<int>::max()`). Unmute sends period `0` (`MuteValue::unmute`), not “use the scope default”. Archive / Unarchive is `ToggleHistoryArchived` → `api().toggleHistoryArchived` (MTProto folder 1 vs 0). tdesktop does not speak TDLib. Unigram maps the same UI onto `SetChatNotificationSettings` (clone settings, `UseDefaultMuteFor = false`, `MuteFor` = seconds or `int.MaxValue`) and `AddChatToList` with `ChatListArchive` / `ChatListMain` (TDLib: a chat cannot be in both lists).
- **Schema (1.8.67, not invented):** `setChatNotificationSettings chat_id notification_settings:chatNotificationSettings = Ok`. `mute_for` is seconds left; longer than 366 days is muted forever. `addChatToList chat_id chat_list:ChatList = Ok` with `chatListArchive` or `chatListMain`. Incoming: `updateChatNotificationSettings`, `updateChatPosition` (order `0` removes from that list only), `updateChatAddedToList` / `updateChatRemovedFromList`, and `updateChatLastMessage.positions` as a full set for both Main and Archive.
- **UX:** Supported open chat header has **Mute** (preset bar) / **Unmute** and **Archive** / **Unarchive**. Sidebar rows show a **Muted** badge. Chats in `chatListArchive` sit under an **Archived** heading. Esc closes the mute bar first, then the existing overlay stack (reaction picker → forward → delete → searches → reply → edit).
- **Out of this slice:** sound / preview exceptions, custom mute picker, `loadChats` of `chatListArchive` at startup, folder UI beyond archive, `toggleChatIsMarkedAsUnread`, scope defaults, channels/bots, OS notifications, App Store, live Telegram in CI.

## Typing indicators (Phase 1)

- **Official clients first:** tdesktop `HistoryWidget::fieldChanged` sends typing only when the field has sendable text, the user is not editing, and the peer is not self / a broadcast channel (`SendProgressManager` skips those, and skips non-support bots and users offline longer than 30s). Repeats use `kSendMyTypingInterval` (5s). Leaving the chat and sending a rich draft call `update(Typing, -1)`, which drops the local action (it does not emit `sendMessageCancelAction`). Unigram `ChatTextBox.OnTextChanged` calls `OutputChatActionManager.SetTyping(ChatActionTyping)` while the field is non-empty, at most every 4s (`delay = 4.0`, same gap on the text box). `CancelTyping` sends `SendChatAction` with `ChatActionCancel` (used when a voice recording stops). Channels and Saved Messages are skipped.
- **Schema (1.8.67, not invented):** `sendChatAction chat_id topic_id business_connection_id action = Ok`. Quill sends `topic_id` null and `business_connection_id` `""`. Action is `chatActionTyping` or `chatActionCancel`. Incoming `updateChatAction chat_id topic_id sender_id action`. TDLib `DialogActionManager::DIALOG_ACTION_TIMEOUT` is 5.5s; the library then emits a cancel action. Quill shows **typing…** while any sender's action is `chatActionTyping`, and clears that sender on `chatActionCancel` or any other action. Repeat interval is Unigram's 4s.
- **UX:** Open supported chat, non-empty composer, not editing → `sendChatAction` typing. Empty field, edit mode, successful send, and switching chats → `chatActionCancel`. Header and sidebar row show **typing…**.
- **Out of this slice:** recording / upload / sticker actions, per-user name strings in groups, offline/bot skip (no user status in this client yet), OS notifications, App Store, live Telegram in CI.

## Stickers — picker and send (Phase 1)

- **Official clients first:** tdesktop opens stickers from the compose emoji/sticker control (`TabbedSelector` → `StickersListWidget` in `chat_helpers/tabbed_selector.cpp`). A click sends the existing document (`Api::SendExistingDocument` in `history_widget.cpp`); tdesktop speaks MTProto, not TDLib. Unigram’s drawer (`StickerDrawerViewModel.GetInstalledSets`) loads `GetInstalledStickerSets(StickerTypeRegular)`, then `GetStickerSet` for the set being shown. Send is `InputMessageSticker(InputSticker(InputFileId(sticker.Sticker.Id), thumbnail?.ToInput(), width, height), emoji)` in `ComposeViewModel`. Esc closes the panel before the rest of the overlay stack.
- **Schema (1.8.67, not invented):** `getInstalledStickerSets sticker_type:StickerType = StickerSets` with `stickerTypeRegular`. `stickerSets` is `stickerSetInfo` rows (`id` int64 as a JSON string, `title`, `name`, `size`, `is_installed`, `is_official`). `getStickerSet set_id:int64 = StickerSet` fills `stickers:vector<sticker>`. Send is `sendMessage` + `inputMessageSticker sticker:inputSticker emoji:string`. `inputSticker` is `inputFileId` plus optional `inputThumbnail` (`inputFileId`, width, height) and the sticker’s width/height. Incoming rows are `messageSticker`. Display uses the WEBP/JPEG `thumbnail` file, or the sticker file itself when `format` is `stickerFormatWebp`. TGS/WEBM are not played.
- **UX:** Ready composer **Stickers** opens the panel (installed regular sets, first set loaded). Tap a sticker to `sendMessage`. History shows the still image or a **Sticker** placeholder that click-downloads. Esc closes the sticker panel first, then mute, reaction picker, forward, delete, searches, reply, and edit.
- **Out of this slice:** custom sticker upload, animated/video playback, favorites/recents, masks, custom emoji, GIF/Tenor, emoji status, channels/bots, App Store, live Telegram in CI.

## Voice notes — record, send, play (Phase 1)

- **Official clients first:** tdesktop shows a microphone when the field is empty and a click opens `VoiceRecordBar` (`history/view/controls/history_view_voice_record_bar.cpp` / compose controls): elapsed time, live waveform, **Cancel**, **Send**. Escape cancels the recording and does not send. While recording, the client sends a voice-recording chat action. History voice rows (`history/view/media`) show a play button, duration, and the 5-bit waveform. Unigram’s compose record button sends `ChatActionRecordingVoiceNote`, then `SendMessage` with `InputMessageVoiceNote` (`InputVoiceNote` of a local Opus/OGG file, duration, waveform). Starting playback calls `OpenMessageContent`; `updateMessageContentOpened` is what marks the note listened. Quill’s composer already uses explicit buttons (attach / stickers), so the mic is a **Voice** button with the same record bar, not a hold-to-slide gesture.
- **Schema (1.8.67, not invented):** send is `sendMessage` + `inputMessageVoiceNote` / `inputVoiceNote` / `inputFileLocal` (`duration` int32, `waveform` bytes, `caption` null when empty, `self_destruct_type` null). Incoming rows are `messageVoiceNote` / `voiceNote` (`duration`, `waveform`, `mime_type`, `voice` file, `is_listened`). Waveform bytes are the Telegram 5-bit packing (tdesktop `documentWaveformEncode5bit`). Playback of a not-yet-local file uses the existing `downloadFile` priority 32 path. `openMessageContent` is sent when playback starts on an unlistened note.
- **UX:** **Voice** starts the record bar (Cancel / Send / duration / bars). Esc cancels the recording first, then the sticker panel, mute, reaction picker, forward, delete, searches, reply, and edit. History shows **Play** / **Pause**, duration, a **New** mark when `is_listened` is false, and waveform bars. Capture uses `ffmpeg` (libopus, mono OGG) when it is installed; Quill does not vendor libopus or invent a silent file when the encoder is missing.
- **Out of this slice:** video notes, round video, speech-to-text (`speech_recognition_result` is parsed as null and not shown), live calls, hold-to-lock gesture, channels/bots, secret-chat self-destruct, App Store, live Telegram in CI.

## GIFs — saved animations (Phase 1)

- **Official clients first:** tdesktop’s compose selector has a GIFs tab (`TabbedSelector` / `GifsListWidget` in `chat_helpers/`). The default section is saved GIFs (`messages.getSavedGifs` / `savedGifs()`), laid out as a grid; a click sends the existing document. Search in that tab is an inline bot, not a third-party GIF API. History playback (`history/view/media`) shows the thumbnail until the file is local, then loops the clip; click pauses. Unigram’s animation drawer loads `GetSavedAnimations` and sends `InputMessageAnimation`. TDLib 1.8.67 has no `searchGifs`. GIF search is `getInlineQueryResults` against `getOption("animation_search_bot_username")`, which needs a bot user id and a new inline-query stack — skipped. No Tenor key.
- **Schema (1.8.67, not invented):** `getSavedAnimations = Animations`. `animations` is `vector<animation>` (`duration`, `width`, `height`, `file_name`, `mime_type`, `thumbnail`, `animation:file`). `updateSavedAnimations animation_ids:vector<int32>` refreshes an open panel. Send is `sendMessage` + `inputMessageAnimation` whose `animation` is `inputAnimation` (since 1.8.65): `inputFileId`, `thumbnail` null (schema: thumbnail file_id upload is not supported; the saved file is already on the server), `added_sticker_file_ids` empty, plus duration/width/height. Caption null, `show_caption_above_media` false, `has_spoiler` false. Incoming rows are `messageAnimation`. Thumbs auto-`downloadFile` at priority 1; Play uses priority 32 on the clip, same as other media.
- **UX:** Ready composer **GIFs** opens the saved grid (mutually exclusive with Stickers). Tap sends. History shows the thumbnail, a **GIF** badge, and **Play** / **Pause**. A local still or extracted MPEG-4/`image/gif` frames loop. Frames live under `quill-gif-frames/{file_id}` and that cache root is on the display allowlist; other temp paths stay blocked. Pause deletes the cache. Play before the file is local resumes when the download finishes. Esc closes the GIF panel first, then stickers, mute, reactions, forward, delete, searches, reply, and edit.
- **Out of this slice:** inline GIF search, `addSavedAnimation` / `removeSavedAnimation`, Tenor, channels/bots, video notes.

## Video messages in history (Phase 1)

- **Official clients first:** tdesktop paints a video in `history/view/media` as the JPEG thumbnail with a corner duration (`formatDuration`) and a play control; a click downloads or streams and plays in the bubble. Pause returns to the still. Secret videos stay blurred until tapped; spoilers cover the preview. Unigram’s video bubble does the same with TDLib `messageVideo` / `video` (thumbnail file, duration, width, height, caption). Channels, bots, and sponsored messages are out of this slice; Quill already hides unsupported chats, so those rows are not given a player.
- **Schema (1.8.67, not invented):** `messageVideo` is `video`, `alternative_videos`, `storyboards`, `cover`, `start_timestamp`, `caption`, `show_caption_above_media`, `has_spoiler`, `is_secret`. `video` is `duration`, `width`, `height`, `file_name`, `mime_type`, `has_stickers`, `supports_streaming`, `minithumbnail`, `thumbnail`, `video:file`. This slice stores the primary file, thumbnail, duration, dimensions, caption, the caption/spoiler/secret flags, `start_timestamp`, `supports_streaming`, and `has_stickers`. Alternative qualities, storyboards, cover, and the minithumbnail blob are left unused. Thumbs auto-`downloadFile` at priority 1 (skip secret/spoiler). Play uses priority 32 on `video.video`, same as other media. `start_timestamp` is passed to ffmpeg as `-ss`.
- **UX:** History shows the thumbnail (or a not-downloaded placeholder with dimensions), a **Video** / **Video · playing** badge, the duration, the caption, and **Play** / **Pause**. GPUI has no video element, so playback matches GIFs: `ffmpeg` writes a short preview under `quill-video-frames/{file_id}`, and that cache root is on the display allowlist. Pause deletes the cache. A path outside the allowlist is not shown. Play before the file is local resumes when the download finishes. Secret and spoiler videos do not auto-download or play.
- **Out of this slice:** streaming/HLS, audio, storyboard scrubbing, alternative qualities, video notes, channels/bots, sponsored content, App Store, live Telegram in CI.

## Video notes — round videos (Phase 1)

- **Official clients first:** tdesktop paints a video note in `history/view/media` as a circle: the JPEG thumbnail, a duration, and a play control. A click downloads the MPEG4 and plays it inside the circle; pause returns to the still. An incoming note that has not been viewed keeps a distinct outline until playback starts (`openMessageContent` / `updateMessageContentOpened`). Secret notes stay covered until tapped. Unigram’s round bubble uses the same TDLib `messageVideoNote` / `videoNote` (`duration`, `length`, thumbnail file, video file). Channels stay hidden (unsupported chat). Bots and sponsored messages are not given a separate player in this slice.
- **Schema (1.8.67, not invented):** `messageVideoNote` is `video_note`, `is_viewed`, `is_secret`. `videoNote` is `duration`, `waveform`, `length`, `minithumbnail`, `thumbnail`, `speech_recognition_result`, `video:file`. The clip is square MPEG4 cropped to a circle. This slice stores duration, the raw waveform bytes, `length` (width and height), the thumbnail file, the video file, `is_viewed`, and `is_secret`. The minithumbnail blob and `speech_recognition_result` are left unused. Thumbs auto-`downloadFile` at priority 1 (skip secret). Play uses priority 32 on `videoNote.video`, same as other media. Playback passes `video/mp4` because the schema says the file is MPEG4; there is no `mime_type` field. `start_timestamp` is not on this constructor, so seek stays 0.
- **UX:** History shows a round thumbnail (or a not-downloaded circle labeled with `length`), a **Video note** / **Video note · playing** badge, the duration, and **Play** / **Pause**. Incoming notes that are not yet `is_viewed` read **New · Video note** and use a blue ring; playing uses a green ring. Frames reuse the video path: `ffmpeg` writes a short preview under `quill-video-frames/{file_id}`, and that cache root is already on the display allowlist. Pause deletes the cache. A path outside the allowlist is not shown. Play before the file is local resumes when the download finishes, then `openMessageContent` marks the note viewed. Secret video notes do not auto-download or play. The waveform is stored and not drawn as bars; official round bubbles do not show a voice-style waveform.
- **Out of this slice:** recording or sending `inputMessageVideoNote`, speech-to-text, a progress ring, channels, bots, sponsored content, secret-chat self-destruct, App Store, live Telegram in CI.

## Audio files in history (Phase 1)

- **Official clients first:** tdesktop paints a music file in `history/view/media` as a row: album cover, title, performer, duration, and a play/pause control. That is `messageAudio` / `audio`, not a voice note. Quill keeps the voice bubble (waveform, **New**, `openMessageContent`) on `messageVoiceNote` only.
- **Schema (1.8.67, not invented):** `messageAudio` is `audio` plus `caption`. `audio` is `duration`, `title`, `performer`, `file_name`, `mime_type`, `album_cover_minithumbnail`, `album_cover_thumbnail`, `external_album_covers`, and `audio:file`. The cover thumbnail auto-`downloadFile`s at priority 1. If that thumbnail is null, the largest external cover is the fallback. Play uses priority 32 on `audio.audio` and the same sandboxed `ffplay` process as voice notes, with a separate playing row so a voice note does not show Pause. Channels, bots, and sponsored messages stay out.
- **UX:** History shows the cover (or an **Audio** placeholder), the title (or file name), performer, duration, and **Play** / **Pause**. A green border means the track is playing. Play before the file is local resumes when the download finishes. Caption edit uses `editMessageCaption`, same as other captioned media.
- **Out of this slice:** sending `inputMessageAudio`, extracting a cover from the downloaded file, a seek bar, audio albums, channels, bots, sponsored content, App Store, live Telegram in CI.

## Send a local video note (Phase 1)

- **Official clients first:** tdesktop’s round-video control sits with the compose field (`history/view/controls`): a click records a square MPEG-4 cropped to a circle, then sends it without a caption. Quill’s composer already uses explicit buttons, so this slice is **Video note** — the same pick path as photo, file, and video (`QUILL_ATTACH_VIDEO_NOTE`, otherwise `docs/screenshots/fixtures/demo-video-note.mp4`). A camera record bar is heavier than a local pick and is not in this slice. The note is not a document and not an album item.
- **Schema (1.8.67, not invented):** `sendMessage` + `inputMessageVideoNote`. `inputVideoNote` is `inputFileLocal`, `thumbnail` (`inputThumbnail` of a JPEG first frame, or null when ffmpeg cannot write one — the schema says pass null to skip), `duration` (0–60), and `length` (square side, 1–640). `self_destruct_type` is null. There is no caption field. Quill does not re-encode. A non-square clip, a side above 640, or a duration above 60 is rejected. After TDLib returns `messageVideoNote`, the existing round **Play** / **Pause** bubble plays it.
- **UX:** Ready composer **Video note**. The chip reads **Video note · filename**. Enter sends `inputMessageVideoNote`. The outgoing bubble is the round note from the receive slice. Channels, bots, and sponsored chats stay gated.
- **Out of this slice:** camera recording, speech-to-text, a progress ring, secret-chat self-destruct, re-encode to a circle, channels, bots, sponsored content, App Store, live Telegram in CI.

## Media albums (Phase 1)

- **Official clients first:** tdesktop paints a photo/video album as one bubble. `HistoryView::GroupedMedia` (`history_view_media_grouped.cpp`) calls `Ui::LayoutMediaGroup` (`ui/grouped_layout.cpp`) for the mosaic and stacks non-photo documents in a column. Quill copies the grid geometry (ratios `w`/`q`/`n`, the 2/3/4 special cases, and the complex row search when there are 5+ items or an aspect ratio above 2). A caption belongs to the album, not each tile; with `show_caption_above_media` false it is the last item’s text. Channels, bots, and sponsored messages stay out.
- **Schema (1.8.67, not invented):** `message.media_album_id` is int64 (`0` means none). Only audios, documents, photos, and videos share an album. This slice groups consecutive photos and videos that share a non-zero id and the same outgoing flag. A lone id stays on the single photo/video path. Send is `sendMessageAlbum` (`chat_id`, `topic_id` null, `reply_to`, `options` null, `input_message_contents` of 2–10 `inputMessagePhoto` / `inputMessageVideo`). Every content uses `show_caption_above_media` false. The composer caption is on the last item. The response is `messages`; each row stays pending until `updateMessageSendSucceeded`.
- **UX:** **Attach photo** / **Attach video** accumulate up to 10 picks (a document still replaces the list and sends alone). Two or more photos/videos send `sendMessageAlbum`. History draws one mosaic. Single photo, video, GIF, sticker, and voice rows are unchanged.
- **Out of this slice:** document/audio albums, album editing, spoilers, secret chats, channels/bots, sponsored content, App Store, live Telegram in CI.

## Send a local video (Phase 1)

- **Official clients first:** tdesktop’s attach menu splits **Photo or Video** from **File** (`history/view/controls`). A picked video is a video message, not a generic document: ffmpeg reads duration, width, and height, and `documentAttributeVideo` carries `supports_streaming` when the MPEG-4 `moov` atom is at the start or the end. The composer text is the caption. Unigram sends TDLib `InputMessageVideo` / `InputFileLocal` the same way. Thumbnail upload is optional; TDLib’s schema says pass null to skip, and the server fills a thumb for small videos.
- **Schema (1.8.67, not invented):** `sendMessage` + `inputMessageVideo`. `inputVideo` is `inputFileLocal`, `thumbnail` null, `cover` null, `start_timestamp` 0, empty `added_sticker_file_ids`, probed `duration` / `width` / `height`, and `supports_streaming` when a `moov` box is present. Caption is `formattedText`. `show_caption_above_media` false, `self_destruct_type` null, `has_spoiler` false. Quill does not re-encode. After TDLib returns `messageVideo`, the existing history player plays it.
- **UX:** Ready composer **Attach video** (same pick path as photo and file: `QUILL_ATTACH_VIDEO`, otherwise the demo clip). The chip reads **Video · filename**. Enter sends `inputMessageVideo`. The outgoing bubble uses the existing **Play** / **Pause** path.
- **Out of this slice:** video notes, albums, spoilers, secret-chat self-destruct, re-encode to H.264, channels/bots, sponsored content, App Store, live Telegram in CI.

## Composer chat drafts (Phase 1)

- **Official clients first:** tdesktop `ComposeControls::saveDraft` (`history_view_compose_controls.cpp`) writes the local draft 1s after the last edit (`kSaveDraftTimeout`) and, if typing continues, at 5s from the first edit in the burst (`kSaveDraftAnywayTimeout`). The cloud copy waits another 14s (`kSaveCloudDraftIdleTimeout`) because tdesktop has its own local DB. Unigram `DialogViewModel.SaveDraft` sends TDLib `SetChatDraftMessage` immediately when leaving the chat (`SaveDraft(false, true)`), when the window deactivates, and before entering edit — it does not keep a second local database. Quill’s one call is `setChatDraftMessage` (TDLib stores it and syncs it), so the quiet timer is tdesktop’s 1s/5s and leaving the chat flushes immediately like Unigram.
- **Schema (1.8.67, not invented):** `chat.draft_message` and `updateChatDraftMessage` (`draft_message` may be null; positions update list order). The schema says an update for the open chat may carry an old draft and must not be applied if the user has changed the text. `setChatDraftMessage` takes `topic_id` null and `draft_message` null to clear. Text content is `draftMessageContentText` / `formattedText`; `link_preview_options` null. `reply_to` is `inputMessageReplyToMessage` (same chat). `date` is 0 and `effect_id` is the int64 string `"0"` (Unigram’s constructor). `inputMessageText.clear_draft` stays true on send and is false on edit, so editing does not wipe the normal draft. Voice, video-note, and rich drafts are not loaded into the text field.
- **UX:** Private chats only. Channels, groups, secret chats, and `userTypeBot` users are skipped. Opening a chat restores draft text and a same-chat reply header. Composer edits debounce, then `setChatDraftMessage`. A successful send clears the draft when the composer is still empty. Sidebar rows prefix **Draft:**.
- **Out of this slice:** topics, rich/voice drafts, external replies, link-preview options on the draft, channels/bots, App Store, live Telegram in CI.

## Link previews (Phase 1)

- **Official clients first:** tdesktop paints webpage cards in `history/view/media/history_view_web_page.cpp` (site name, title, description, thumbnail). Small media sits to the right of the text with a left accent bar; `show_large_media` uses a full-width photo, above or below the description (`show_media_above_description`). `show_above_text` places the card above the message. A click opens the page URL (Quill uses the OS opener, not Instant View or a WebView). Unigram’s message bubble does the same with TDLib `LinkPreview` (site name, title, description, photo). Sponsored / channel promo previews stay out.
- **Schema (1.8.67, not invented):** `messageText` carries `formattedText` plus `link_preview:linkPreview` (the old `webPage` constructor is not in this schema). Entities are `textEntity` with UTF-16 `offset`/`length`. This slice keeps `textEntityTypeUrl` and `textEntityTypeTextUrl`. `linkPreview` fields used: `url`, `display_url`, `site_name`, `title`, `description`, `show_large_media`, `show_media_above_description`, `show_above_text`, and `type`. The photo is the schema `photo` (article/photo/app/web app), or `thumbnail` / `cover` when that type uses those names (`linkPreviewTypeArticle`, `linkPreviewTypePhoto`, embedded players, `linkPreviewTypeVideo`). Thumbs auto-`downloadFile` at priority 1 like message photos. Clicking the link text or the card calls `xdg-open` / `open` for `http`/`https` only.
- **Out of this slice:** Instant View, article reader, bold/italic styling, caption entities, sponsored messages, `tg://` handlers, link-preview options on send.

## Photo / document receive (2026-09-17)

- **Schema (1.8.67, not invented):** `messagePhoto` (`photo.sizes[]` of `photoSize` + `file` / `localFile`, `caption`, `has_spoiler`, `is_secret`) and `messageDocument` (`document.file_name`, `mime_type`, `document` file, `caption`). Progress is `updateFile`; fetch is `downloadFile` (`file_id:int32`, `priority:1-32`, `offset`/`limit` int53, `synchronous:Bool`). `remoteFile.id` is not stored (it can be an HTTP URL).
- **State:** session `files` map keyed by `file.id`. History rows keep file ids only. Unread/read path unchanged.
- **Downloads:** open-chat photo thumbs auto-download at priority 1 (skip secret/spoiler). User click on a placeholder or document chip sends priority 32. `synchronous: false` — the immediate `file@extra` consumes the pending extra; progress continues on `updateFile`. `downloading` unsticks on completed, `!can_be_downloaded`, idle-incomplete `file`/`updateFile` (`!is_downloading_active && !is_downloading_completed`), or error (including after extra already resolved). Nested message files that are still idle do not unstick an in-flight download.
- **UI:** image thumb when `local.is_downloading_completed` and the path is a real file under the account `tdlib_files` (demo allowlist: `docs/screenshots/fixtures`). Bare JSON paths are not shown. Documents are chips (`file_name` · mime · size · state). Secret photos never click-download; spoiler/secret placeholder text follows file state.
- **Out of this slice:** video/voice, sending a local file, fullscreen viewer, App Store, vendoring tdjson.

## Roadmap — Phase 2+ toward Telegram parity (2026-09-25)

- **Rationale:** Phase 0–1 shipped the private-chat core. The README blocker
  (channels/bots gated on sponsored-content handling) plus every per-slice
  "Out of this slice" list now form a sequenced Phase 2+ plan in
  `ROADMAP.md`: sponsored content + channels first (the keystone), then bots,
  message richness, forum topics, contacts/profiles, folders, notifications,
  and stories as a stretch.
- **Official clients first:** tdesktop remains the UX reference for each
  phase; Unigram for the TDLib mapping. Each slice keeps one shippable unit
  with replay tests and a DECISIONS.md entry in the existing format.
- **Schema (1.8.67, not invented):** every future slice verifies constructors
  against `schema/td_api.tl` (e.g. `getChatSponsoredMessages`,
  `reportChatSponsoredMessage`, `sponsoredMessage` for Phase 2.1).
- **Out of this slice:** the implementation slices themselves; macOS-only
  gates (VoiceOver, real IME) stay blocked on a Mac session; secret chats,
  calls, payments, multi-account, telemetry, AI, and App Store distribution
  stay out of scope.

## Phase 2.1 — Sponsored-message handling (2026-09-26)

- **Rationale:** Channels are the README blocker, gated on sponsored-content
  handling. This slice proves the full sponsored pipeline (fetch → render →
  report → click tracking) through replay fixtures and a screenshot demo,
  while keeping the channel gate intact for Phase 2.2.
- **Official clients first:** tdesktop is the UX reference for sponsored row
  presentation (Sponsored/Recommended label, sponsor info, action button,
  report affordance). Unigram guides the TDLib mapping. The label text comes
  from `sponsoredMessage.is_recommended` (`Recommended` when true, `Sponsored`
  otherwise) — not invented.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `getChatSponsoredMessages chat_id:int53 = SponsoredMessages;`
  - `sponsoredMessages messages:vector<sponsoredMessage> messages_between:int32 = SponsoredMessages;`
  - `sponsoredMessage message_id:int53 is_recommended:Bool can_be_reported:Bool content:MessageContent sponsor:advertisementSponsor title:string button_text:string accent_color_id:int32 background_custom_emoji_id:int64 additional_info:string = SponsoredMessage;`
  - `advertisementSponsor url:string photo:photo info:string = AdvertisementSponsor;`
  - `reportChatSponsoredMessage chat_id:int53 message_id:int53 option_id:bytes = ReportSponsoredResult;`
  - `reportSponsoredResultOk`, `reportSponsoredResultFailed`,
    `reportSponsoredResultOptionRequired title:string options:vector<reportOption>`,
    `reportSponsoredResultAdsHidden`, `reportSponsoredResultPremiumRequired`
  - `reportOption id:bytes text:string = ReportOption;` (`id` is base64 bytes,
    preserved verbatim through the option picker)
  - `viewSponsoredChat sponsored_chat_unique_id:int53 = Ok;`
  - `clickChatSponsoredMessage chat_id:int53 message_id:int53 is_media_click:Bool from_fullscreen:Bool = Ok;`
- **Semantic boundary:** `viewSponsoredChat` takes `sponsoredChat.unique_id`
  from sponsored-search results. `sponsoredMessage` carries only `message_id`;
  no mapping is fabricated. The typed driver supports `viewSponsoredChat`, but
  row-level integration waits for a real `sponsoredChat.unique_id`.
- **UX:**
  - Sponsored rows render below the channel header: label badge (Sponsored /
    Recommended), title, content (text/photo/animation/video/document via the
    existing media helpers), sponsor info, additional info, sponsor button
    (opens `advertisementSponsor.url`), and a Report button only when
    `can_be_reported` is true.
  - Report flow: tap Report → `reportChatSponsoredMessage` with empty
    `option_id` → `OptionRequired` opens the option picker → picking an option
    re-sends with the base64 `option_id` → outcome banner shows a fixed
    user-facing message per result (no TDLib text echoed). TDLib errors
    dismiss the picker silently.
  - Row order preserves TDLib's response vector order (the schema promises no
    order; sorting by `message_id` was removed).
- **Downloads:** Sponsored media follows the existing sandbox: sponsor/content
  thumbnails join the priority-1 pass for the open chat; clicking a row's media
  requests the full file at priority 32 via the shared `request_media_download`
  path.
- **Click tracking:** `clickChatSponsoredMessage` fires on sponsor button/link
  opens (`is_media_click=false`) and on media opens/downloads/playback
  (`is_media_click=true`). `from_fullscreen` is false (no fullscreen viewer in
  this slice). Fire-and-forget; the `ok` response is ignored.
- **Channel gate retained:** `ChatKind::gate_reason`,
  `ChatKind::is_supported_cloud_chat`, and the normal-live UI gate are
  unchanged. Selecting a gated channel fetches sponsored rows (no
  `openChat`/`getChatHistory`); re-selecting the open gated channel no longer
  leaks a history fetch. The `ReadySponsored` screenshot demo (CLI
  `ready-sponsored`, `docs/screenshots/ready-sponsored.png`) proves the
  pipeline through injected fixtures.
- **Out of this slice:** un-gating channels (Phase 2.2); `sponsoredChat`
  search-result integration for `viewSponsoredChat`; fullscreen media viewer
  (`from_fullscreen=true` path); sponsored-message inline buttons beyond the
  single sponsor URL button; premium upsell UI for
  `reportSponsoredResultPremiumRequired` (a fixed message is shown).

## Phase 2.2 — Broadcast channels (2026-09-26)

- **Rationale:** Channels were the last gated chat type in the README parity
  checklist. This slice un-gates `chatTypeSupergroup { is_channel: true }`:
  channels appear in the chat list, open normal history, render broadcast
  posts with view counts, hide the composer (admin posting is 2.3), and offer
  join/leave for public channels.
- **Official clients first (verified in source, 2026-09-26):**
  - tdesktop (`dev`, `Telegram/SourceFiles/history/history_item.cpp`):
    `HistoryItem::viewsCount()` reads the per-message
    `HistoryMessageViews.views.count` (line ~4341) — views are a per-message
    count tracked on the item and rendered alongside the post, bottom-right
    next to the time. Channel posts render with the channel as author.
  - Unigram (`develop`, `Telegram/Controls/Messages/MessageFooter.xaml.cs`,
    `UpdateMessage`, line ~399): view label is rendered only when
    `message.InteractionInfo?.ViewCount > 0`, as an eye glyph plus a compact
    number — `"\uEA03\u00A0" + Formatter.ShortNumber(ViewCount)`. Quill's
    footer ("👁 12.4K") follows this exactly: eye glyph, compact thousands,
    shown only for positive counts.
  - Unigram (`develop`, `Telegram/ViewModels/DialogViewModel.cs`):
    `JoinChannel()` sends `new JoinChat(chat.Id)` then routes the result
    through `MessageHelper.HandleChatJoinResult(..., isChannel: true,
    response)` (line ~3803); leaving is `ClientService.Send(new
    LeaveChat(chat.Id))`, fire-and-forget (line ~3958). Quill mirrors this:
    `joinChat` handles all four `ChatJoinResult` variants, `leaveChat` flips
    status on `ok`.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `chatTypeSupergroup supergroup_id:int53 is_channel:Bool`
  - `message.is_channel_post:Bool`; `sender_id` is `messageSenderChat` for
    channel posts (channel = author)
  - `messageInteractionInfo view_count:int32 forward_count:int32
    reply_info:messageReplyInfo reactions:messageReactions`
  - `updateMessageInteractionInfo chat_id:int53 message_id:int53
    interaction_info:messageInteractionInfo`
  - `chatMember member_id:MessageSender status:ChatMemberStatus`
  - `chatMemberStatusCreator`, `chatMemberStatusAdministrator`,
    `chatMemberStatusMember`, `chatMemberStatusRestricted`,
    `chatMemberStatusLeft`, `chatMemberStatusBanned`
  - `updateChatMember ... new_chat_member:chatMember`
  - `getMe = User`; `getChatMember chat_id:int53 member_id:MessageSender =
    ChatMember`
  - `joinChat chat_id:int53 = ChatJoinResult`;
    `chatJoinResultSuccess chat_id:int53`, `chatJoinResultRequestSent`,
    `chatJoinResultGuardBotApprovalRequired bot_user_id:int53 query_id:int64`,
    `chatJoinResultDeclined` (no invite-link variant exists in this schema —
    none invented)
  - `leaveChat chat_id:int53 = Ok`
- **UX:**
  - Channels list with title; history opens via the normal
    `openChat`/`getChatHistory` pipeline (sponsored fetch runs as before).
    Sidebar rows are text-only (title + preview) everywhere in Quill — chat
    avatars are not rendered for any chat type yet, so channels follow the
    same convention rather than gaining one-off photos.
  - History rows show the channel name as author and an "👁 <count>" footer
    when `interaction_info.view_count > 0`; live
    `updateMessageInteractionInfo` bumps the count.
  - Composer stays hidden in every channel in this slice
    (`ChatSummary::can_post()` is false for channels; the driver rejects
    channel sends the same way). The footer instead shows, by membership:
    "Checking…" (unknown), **Join channel** (Left), **Leave channel** + note
    (Member/Administrator), "posting lands in 2.3" (Creator), and fixed notes
    for Banned/Restricted/Unknown.
  - Membership is probed once per open channel: `getMe` (cached) → then
    `getChatMember`; `updateChatMember` for own `user_id` refreshes status.
- **Replay proof:** `tests/replay.rs` covers channel ungating + chat-list
  order, broadcast posts with view counts (including the live bump),
  `can_post`/composer gating, the `getMe`/`getChatMember`/`joinChat`/
  `updateChatMember`/`leaveChat` cycle, non-success `joinChat` results
  keeping the old status, and foreign `updateChatMember` being ignored.
  Request JSON shapes are asserted in `requests.rs`; envelope parse variants
  in `envelope.rs`. Screenshot demo: `quill --screenshot-demo ready-channels`
  → `docs/screenshots/ready-channels.png`.
- **Out of this slice (→ 2.3 and beyond):** admin channel posting
  (`sendMessage` with channel semantics); discussion-group comment links;
  private-channel invite links and join-request approval UI; subscriber
  counts; channel header extras (photo, description, username); chat-list
  avatars for any chat type; comment threading inside channels; admin log;
  boosts/statistics.

## Phase 2.3 — Channel admin posting (2026-09-26)

- **Rationale:** 2.2 ungated channels for reading but hid the composer for
  everyone, including admins. This slice derives posting rights from own
  channel membership and shows the composer for admins, completing the
  channel read/write loop: `getChatMember`/`updateChatMember` already feed
  `ChatSummary::my_member_status`; now they also feed posting rights, and
  both the composer gate and the driver send gate read the same predicate.
- **Official clients (product behavior):** tdesktop and Unigram both render
  the message input in a channel when the viewer can post (owner/admin with
  the right); posts appear authored by the channel with live view counts.
  Non-admins see no input. Quill mirrors this: one `can_post()` predicate
  drives both the composer visibility and the driver-side send rejection.
  (Product-level behavior; no new source-line verification was needed for
  this slice — the schema carries the rights model.)
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `chatMemberStatusCreator is_anonymous:Bool is_member:Bool =
    ChatMemberStatus` — no rights block; the creator always posts
  - `chatMemberStatusAdministrator can_be_edited:Bool
    rights:chatAdministratorRights = ChatMemberStatus;` — there is **no**
    `can_post_messages` on the status itself (nothing invented); the right
    lives on the nested `chatAdministratorRights`
  - `chatAdministratorRights can_manage_chat:Bool can_change_info:Bool
    can_post_messages:Bool can_edit_messages:Bool can_delete_messages:Bool
    can_invite_users:Bool can_restrict_members:Bool can_pin_messages:Bool
    can_manage_topics:Bool can_promote_members:Bool can_manage_video_chats:Bool
    can_post_stories:Bool can_edit_stories:Bool can_delete_stories:Bool
    can_manage_direct_messages:Bool can_manage_tags:Bool
    can_send_welcome_messages:Bool is_anonymous:Bool = ChatAdministratorRights;`
  - `sendMessage chat_id:int53 topic_id:MessageTopic
    reply_to:InputMessageReplyTo options:messageSendOptions
    reply_markup:ReplyMarkup input_message_content:InputMessageContent =
    Message;` — no channel-specific variant exists in this schema; channel
    semantics come from the chat_id, and the server echoes the post with
    `message.is_channel_post:Bool` / `sender_id: messageSenderChat`
- **UX:**
  - Rights rule: Creator → posts; Administrator → posts unless
    `rights.can_post_messages` is explicitly false (a rights block that
    TDLib always sends but a fixture may omit defaults to "no restriction");
    Member / Restricted / Left / Banned / unknown → no composer, as in 2.2.
  - The composer, the footer, and the driver all read
    `ChatSummary::can_post()`; `ParsedChatMember` carries
    `admin_can_post_messages: Option<bool>` from `getChatMember` /
    `updateChatMember`, and `set_member_status` stores it atomically with
    the status.
  - Footer copy updated: admins/creator with rights see "Posting as
    {title}."; an admin whose rights lack `can_post_messages` sees "You are
    an admin, but posting is disabled for you."; member/non-admin copy is
    unchanged.
  - Sending reuses the existing `sendMessage` snapshot path (no new
    request builder); the echo arrives through the 2.2 broadcast pipeline
    (`is_channel_post`, channel author, `updateMessageInteractionInfo`
    view-count bumps).
- **Replay proof:** `tests/replay.rs` gains
  `replay_channel_admin_sees_composer` (admin + rights → `can_post()`),
  `replay_channel_non_admin_composer_hidden` (member hidden; admin with
  `can_post_messages: false` hidden), and
  `replay_channel_admin_status_change_flips_composer` (`updateChatMember`
  member → admin → right revoked → left → creator flips the gate each way);
  the 2.2 `replay_channel_membership_and_join_leave` now expects the admin
  promotion to show the composer. `src/connect.rs` gains the driver test
  `channel_admin_send_succeeds_non_admin_send_rejected` (`sendMessage`
  recorded with the channel chat_id for an admin; `InvalidRequest` for a
  non-posting channel). `envelope.rs` unit-tests the rights parse
  (true/false/missing block/non-admin status). Screenshot demo:
  `quill --screenshot-demo ready-channels-admin` →
  `docs/screenshots/ready-channels-admin.png`.
- **Out of this slice (→ 2.4 and beyond):** suggested posts
  (`suggestedPostInfo`); scheduled channel posts; discussion-group comment
  links; private-channel invite links and join-request approval UI;
  subscriber counts; channel header extras (photo, description, username);
  chat-list avatars for any chat type; comment threading inside channels;
  admin log; boosts/statistics.

## Phase 3.1 — Bot chats (2026-09-26)

- **Rationale:** bots were already detectable (`updateUser` →
  `userTypeBot`) but their info was never fetched and never shown; a bot
  private chat looked like any other chat. This slice keeps bot chats on
  the ordinary private-chat path (list, open, history, composer all work)
  and adds the native bot info panel: lazily fetch `getUserFullInfo` when
  the chat opens, cache `botInfo` (description + commands), and render it
  below the conversation header. Tapping a command inserts it into the
  composer (the full `/` menu is 3.3's job).
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `userTypeBot can_be_edited:Bool can_join_groups:Bool
    can_read_all_group_messages:Bool … = UserType;` (line 816) — bot
    detection already lived on this; nothing new invented
  - `getUserFullInfo user_id:int53 = UserFullInfo;` (line 11501)
  - `userFullInfo … bot_info:botInfo = UserFullInfo;` (line 2468)
  - `botInfo short_description:string description:string …
    commands:vector<botCommand> … = BotInfo;` (line 2430) — commands are
    directly `vector<botCommand>`, not the `botCommands` wrapper (line 829)
  - `botCommand command:string description:string is_ephemeral:Bool =
    BotCommand;` (line 826) — `is_ephemeral` is parsed but intentionally
    not stored: tapping a command always inserts the plain `/command` text
  - `updateUserFullInfo user_id:int53 user_full_info:userFullInfo =
    Update;` (line 10744)
- **UX:**
  - Bot private chats are listed, opened, and messaged exactly like other
    private chats — no bot-specific gating bypass; `is_supported_cloud_chat`
    / `gate_reason` / `can_post()` are untouched, so secret chats and other
    unsupported types gate exactly as before.
  - On first open of a known bot chat the driver sends one `getUserFullInfo`
    (deduped: cached infos and in-flight requests are not refetched);
    send failures drop the pending request so a retry can happen.
  - The panel shows the bot description and one text-chip button per
    command (`/start — Start the bot` style). Tapping inserts `/command`
    into the composer (bare when empty, space-separated after text).
  - Screenshot demo: `quill --screenshot-demo ready-bot-chat` →
    `docs/screenshots/ready-bot-chat.png`.
- **Replay proof:** `tests/replay.rs` gains `replay_bot_chat_ungated_with_history`
  (bot chat listed/ungated/postable, history renders, `bot_user_id_for_chat`
  resolves), `replay_bot_info_cached_from_full_info` (description/commands
  cached from the `userFullInfo` response), `replay_bot_info_refreshed_by_update`
  (`updateUserFullInfo` replaces the cache), and
  `replay_non_bot_chat_gating_unchanged` (secret chat still gated, regular
  private chat still supported, no bot association). `src/connect.rs` gains
  the driver test `bot_info_fetched_once_on_chat_open` (one `getUserFullInfo`
  recorded per chat; repeat selects do not refetch; the `@extra`-matched
  response caches the canary info). `envelope.rs` unit-tests the full-info
  and update parses. `composer.rs` unit-tests `insert_bot_command_text`
  (empty/whitespace/text/chained commands).
- **Out of this slice (→ 3.2 and beyond):** inline keyboards and
  `answerCallbackQuery`; full `/` command menu in the composer (3.3);
  inline bots / `getCommands`; bot privacy modes and group membership
  details; menu-button / web-app buttons; bot photo/avatar headers; bot
  sponsored-message rows (fetching still skips bots).

## Phase 3.2 — Inline keyboards (2026-09-26)

- **Rationale:** bot messages often end with an inline keyboard
  (`replyMarkupInlineKeyboard`) and until now those buttons were invisible —
  the messages were silently uninteractive. This slice renders the keyboard
  as a button grid under each history message that carries one (not just
  bot private chats — the parse lives on the shared message path), and
  wires the actionable button kinds to real behavior; everything else
  renders honestly disabled instead of crashing or faking it.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `replyMarkupInlineKeyboard rows:vector<vector<inlineKeyboardButton>>
    force_reply:Bool = ReplyMarkup;` (line 3855) — only this markup is
    kept on `ParsedMessage.reply_markup`; `replyMarkupShowKeyboard` and
    friends stay `None`
  - `inlineKeyboardButton text:string icon_custom_emoji_id:int64
    style:ButtonStyle type:InlineKeyboardButtonType = InlineKeyboardButton;`
    (line 3828) — `icon_custom_emoji_id` is parsed but not rendered yet
  - `buttonStyleDefault/Primary/Danger/Success/Link = ButtonStyle;`
    (lines 3696–3708); unknown styles fall back to `Default` so the
    keyboard always renders
  - `inlineKeyboardButtonTypeUrl/LoginUrl/WebApp/Callback/
    CallbackWithPassword/CallbackGame/SwitchInline/Buy/User/CopyText/
    Disabled` (lines 3774–3807) — all eleven parsed; only `Url`,
    `Callback`, `SwitchInline`, and `CopyText` are actionable. Note
    `inlineKeyboardButtonTypeCallbackWithPassword` carries `data:bytes`
    (line 3789), stored on the variant for the future password prompt
  - `targetChatCurrent/Chosen/InternalLink = TargetChat;` (lines 7476–7482)
    — no chat picker in this slice: all three insert into the current
    chat's composer, documented in the UI and the composer helper
  - `getCallbackQueryAnswer chat_id:int53 message_id:int53
    payload:CallbackQueryPayload = CallbackQueryAnswer;` (line 13138) —
    clients press callback buttons with this, NOT `answerCallbackQuery`
    (bots-only per its schema doc comment, line 13146).
    `callbackQueryPayloadData data:bytes` (line 7737); `bytes` is base64
    in the JSON interface
  - `callbackQueryAnswer text:string show_alert:Bool url:string =
    CallbackQueryAnswer;` (line 7747)
  - `updateMessageEdited chat_id:int53 message_id:int53 edit_date:int32
    reply_markup:ReplyMarkup = Update;` (line 10431) — bots edit keyboards
    this way; previously unhandled by the parser, now replaces
    `HistoryMessage.reply_markup` (null/absent `reply_markup` removes it).
    `updateMessageContent` carries no markup, so it correctly leaves the
    keyboard untouched
- **UX:**
  - Rows render as horizontal button rows under the message bubble; buttons
    stretch to share the row width (Telegram-desktop style); empty rows are
    skipped. Styles map to primary / danger / success / link / ghost
    buttons. Search hits keep their keyboard (`SearchMessageHit`).
  - `Url` buttons open in the OS browser through the same
    non-http-scheme-refusing gate as message-text links. `Callback` sends
    `getCallbackQueryAnswer` (driver-guarded: chats path active, supported
    chat, real non-pending message); the `callbackQueryAnswer` is matched
    by `@extra`, consumed by the UI on the next poll, and shown in the
    transient status line — a URL answer opens in the OS browser; TDLib
    error 502 (bot missed the query timeout) surfaces as "bot did not
    answer" instead of echoing TDLib text. `SwitchInline` inserts the query
    into the current chat's composer. `CopyText` copies to the clipboard.
    `LoginUrl` / `WebApp` / `CallbackWithPassword` / `CallbackGame` / `Buy`
    / `User` / `Disabled` / unknown render disabled with an explanatory
    tooltip.
  - Malformed rows are skipped and malformed buttons become disabled
    `Unknown` placeholders — a hostile keyboard can never crash the parse.
  - Screenshot demo: `quill --screenshot-demo ready-bot-keyboard` →
    `docs/screenshots/ready-bot-keyboard.png` (URL row, callback +
    switchInline row, copy-text + unknown/disabled row).
- **Replay proof:** `tests/replay.rs` gains
  `replay_inline_keyboard_mixed_lands_on_history` (mixed keyboard lands on
  `HistoryMessage.reply_markup` with rows, styles, and types intact),
  `replay_inline_keyboard_hostile_yields_disabled_placeholders` (unknown
  type/style, missing `type`, non-array rows → skipped or disabled
  placeholders), `replay_non_inline_markup_ignored`
  (`replyMarkupShowKeyboard` and absent `reply_markup` → `None`), and
  `replay_update_message_edited_replaces_keyboard` (edit replaces the
  keyboard; null `reply_markup` removes it) — plus the prior
  `replay_inline_keyboard_stored_from_real_json` and
  `replay_callback_query_answer_surfaced` (`@extra`-matched answer stored,
  stray answer ignored, 502 → fallback note). `envelope.rs` unit-tests the
  keyboard parse, hostile tolerance, and `callbackQueryAnswer`;
  `requests.rs` unit-tests the `getCallbackQueryAnswer` JSON shape
  (base64 `bytes`, guard against `answerCallbackQuery`); `composer.rs`
  unit-tests `insert_switch_inline_text`.
- **Out of this slice (→ 3.3 and beyond):** full `/` command menu (3.3);
  chat picker for `targetChatChosen`; opening `targetChatInternalLink`
  links; `inlineKeyboardButtonTypeLoginUrl` (`getLoginUrlInfo` flow),
  `WebApp` (`openWebApp`), `CallbackWithPassword` password prompt,
  `CallbackGame`, `Buy` payment flow, `User` mention insertion; rendering
  `icon_custom_emoji_id`; rendering `replyMarkupForceReply` /
  `replyMarkupShowKeyboard`.

## Phase 3.3 — Bot commands menu (2026-09-26)

- **Rationale:** bot private chats expose their commands through `botInfo`,
  but the composer had no way to browse them — the user had to know the
  exact command names. This slice adds the tdesktop-style `/` menu: typing
  `/` (or `/prefix`) in a bot chat pops a menu above the composer with the
  bot's commands; tapping or pressing Enter inserts the highlighted
  command. Global-scope commands (the `getCommands` result) are fetched
  too and shown below the bot-specific ones.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `botCommand command:string description:string is_ephemeral:Bool =
    BotCommand;` (line 826)
  - `botCommands bot_user_id:int53 commands:vector<botCommand> =
    BotCommands;` (line 829) — the `getCommands` response wrapper
  - `botInfo … commands:vector<botCommand> … = BotInfo;` (line 2430) —
    already cached by 3.1
  - `getCommands scope:BotCommandScope language_code:string = BotCommands;`
    (line 14953), annotated **"for bots only"**
  - `botCommandScopeDefault = BotCommandScope;` (line 10360, "a scope
    covering all users") — the default scope a null `scope` selects;
    other scopes exist at lines 10363–10378
- **The `getCommands` caveat (honest limitation):** the schema's own
  annotation says `getCommands` is for bots only — a user session gets an
  `error` answer, never a `botCommands`. The fetch is still implemented
  exactly as typed (`scope:null` selects the default scope,
  `language_code:""`), sent once per bot chat (deduped by cache +
  in-flight purpose, alongside the `getUserFullInfo` fetch). A
  successful `botCommands` is cached in `Session::bot_commands` keyed by
  bot user id; an `error` is recorded as an empty set (mirroring
  `bot_info`'s `None` negative cache) so the driver never retries.
  Either way the `/` menu degrades gracefully to
  the `botInfo` commands. A real user account therefore shows the bot's
  `botInfo` commands only — documented, not worked around (sending other
  scopes or polling would not change the bot-only restriction).
- **Menu semantics:**
  - Trigger is pure: `composer::command_menu_trigger` — the trailing
    whitespace-separated token must start with `/` at a word boundary
    (bare `/` → empty prefix = show all; `/st` → filter `st`). Mid-word
    slashes (`a/b`, `http://…`, `/a/b`) never open the menu; a trailing
    space closes it. The trailing-token convention exists because
    `TextareaState` exposes no cursor offset (no mid-text `/` menu, like
    tdesktop's full behavior — documented limitation).
  - Rows: `Session::command_menu_items` merges `botInfo` commands first,
    then cached `getCommands` results, deduped by command name
    (bot-specific description wins). Only for private chats with a
    `userTypeBot` peer; non-bot chats and empty command lists never open
    the menu.
  - Filter is case-insensitive; empty prefix matches all.
  - Open: every composer event (`Change`) + after programmatic
    `set_value` writes (bot-panel chips, switch-inline insert, demo
    seeding), which suppress `Change`. Closed: Esc (keystroke
    interceptor, runs before keymap dispatch since the `Input` context
    consumes Escape), Up/Down move the highlight (wrap), Enter picks the
    highlighted row instead of sending, tap picks via `on_click` (buttons
    avoid focus-on-mousedown so the composer keeps focus), Blur /
    outside interaction / chat switch / selection close it.
  - Pick replaces the partial token (`/st` + pick `start` → `/start`)
    through the existing 3.1 `insert_bot_command_text` helper, then
    closes the menu.
- **Rendering:** a `command-menu` popup above the composer (accent-tinted
  highlight on the selected row, `hover` tint on the rest, muted
  section header), in the same style family as the 3.1 bot panel and the
  search-result rows. A "Global" header appears only when both sections
  have rows.
- **Screenshot demo:** `quill --screenshot-demo ready-bot-command-menu` →
  `docs/screenshots/ready-bot-command-menu.png` (bot chat seeded with
  `botInfo` commands + an injected global `botCommands` response through
  the real reducer path, composer set to `/` and focused so the menu
  renders open with both sections).
- **Replay proof:** `tests/replay.rs` gains
  `replay_bot_commands_cached_from_get_commands` (`botCommands` lands in
  the cache, merges below `botInfo` rows, duplicates dropped,
  bot-specific description wins, stray response without a matching
  `@extra` ignored) and `replay_bot_commands_error_records_empty_set`
  (`error` → empty set, menu falls back to `botInfo` only).
  `connect.rs` unit-tests the fetch-once dedup, the JSON shape
  (null scope + empty language) in `requests.rs`, the `botCommands`
  parse (incl. missing `commands`) in `envelope.rs`, and `composer.rs` unit-tests the trigger boundary (bare `/`, `/prefix`,
  mid-word rejection, trailing-space close), token stripping, merge
  dedup, and case-insensitive filtering.
- **Out of this slice (→ future):** mid-text `/` menu at the cursor
  (needs a cursor-offset API on the input); per-language scopes
  (`botCommandScope…` with non-empty `language_code`); `@botname`
  namespaced commands in groups; sending `/`-commands as typed (already
  works — they are plain text); ephemeral-command rendering
  (`is_ephemeral` is not kept).

## Phase 4.1 — Rich text entities (2026-09-26)

- **Rationale:** message text and media captions arrive from TDLib as
  `formattedText` with `textEntity` spans, but Quill rendered only plain
  text (URLs got link treatment via a separate ad-hoc path). This slice
  parses the full Phase 4.1 set of styling constructors and renders them
  in the history — bold, italic, underline, strikethrough, spoiler,
  inline code, pre blocks, and pre-with-language blocks — while keeping
  the existing URL / `textEntityTypeTextUrl` link behavior.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `textEntity offset:int32 length:int32 type:TextEntityType =
    TextEntity;` (line 110) — offsets/lengths are UTF-16 code units
  - `textEntityTypeUrl = TextEntityType;` (line 5731) — autodetected URL
  - `textEntityTypeBold` (5743), `textEntityTypeItalic` (5746),
    `textEntityTypeUnderline` (5749), `textEntityTypeStrikethrough`
    (5752), `textEntityTypeSpoiler` (5755), `textEntityTypeCode` (5758),
    `textEntityTypePre` (5761), `textEntityTypePreCode language:string`
    (5764) — all supported
  - `textEntityTypeTextUrl url:string = TextEntityType;` (5773) —
    explicit link target, already supported by the URL path
  - `formattedText text:string entities:vector<textEntity> =
    FormattedText;` — unchanged; caption fields on `messagePhoto`,
    `messageDocument`, `messageAnimation`, `messageVideo`,
    `messageVoiceNote`, `messageAudio` are all `formattedText` and are
    now parsed for entities (previously captions were text-only)
- **Additive nesting rule:** entity boundaries split the text into runs;
  overlapping entities combine additively (a bold span inside an italic
  span renders bold-italic; partial overlaps get all intersecting
  styles); adjacent runs with identical style and href merge. Two links
  overlapping the same run cannot both render — the earliest (by
  offset, then length) wins for that run.
- **Malformed spans are dropped, never crash:** zero-length spans,
  spans starting past the text end, and offsets that don't land on
  UTF-16 code-unit boundaries are skipped by `parse_text_entities`.
- **Rendering (`rich_text_line`, used for message text and captions):**
  bold → `FontWeight::BOLD`; italic; underline; strikethrough via
  `line_through`; inline code → monospace chip; `pre` / `preCode` →
  full-width monospace block (pre visually wins over code, both stay
  monospace; the `preCode` language is retained in the run style but not
  yet syntax-highlighted); URLs/`textUrl` keep accent color, underline,
  and click behavior.
- **Spoiler UX (tdesktop-style tap-to-reveal):** spoiler runs render as
  an opaque chip (same-color text on a solid background) until tapped;
  tapping toggles reveal. Reveal state lives in
  `QuillApp::spoiler_revealed`, keyed by `(chat id, message id, run index,
  is_caption)` (message ids are only unique within a chat), threaded through the row render functions as a
  read-only set (rendering cannot read the app entity mid-update, so
  the set is passed down rather than re-read).
- **Screenshot:** `docs/screenshots/ready-text-entities.png` — a
  dedicated "Demo entities" chat with a mixed-entity `messageText` and
  a `messagePhoto` whose caption carries bold + link entities, driven
  by `quill --screenshot-demo ready-text-entities`.
- **Out of this slice (→ future):** custom-emoji entities
  (`textEntityTypeCustomEmoji`) and other unsupported/autodetected
  entity types (`textEntityTypeMention`, `textEntityTypeBlockQuote`,
  bank-card / phone-number / email autodetects, `textEntityTypeMediaTimestamp`
  etc. — parsed types outside 4.1 are ignored); syntax highlighting for
  `preCode` languages; entity styling in forward/quote strips (those are
  plain text today).

## Phase 4.2 — Polls (2026-09-26)

- **Rationale:** `messagePoll` is a first-class Telegram message content
  type. This slice adds poll display (question, per-option bars with
  percentages and voter counts, chosen/correct marks, open/closed and
  anonymous state), voting via `setPollAnswer`, live `updatePoll`
  refresh, and regular-poll creation from the
  composer.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `pollOption` (line 456), `inputPollOption` (462),
    `pollTypeRegular` (468), `pollTypeQuiz` (475),
    `inputPollTypeRegular` (481), `inputPollTypeQuiz` (488),
    `poll` (711), `messagePoll` (5241), `inputMessagePoll` (6193),
    `updatePoll` (11179), `updatePollAnswer` (11186),
    `setPollAnswer` (12932)
  - Critical distinction: `pollOption.id` is a **string**, while
    `setPollAnswer.option_ids` are **zero-based integer positions**
    (`vector<int32>`).
- **Model (`src/telegram/envelope.rs`):** `Poll`, `PollOption`,
  `PollType::{Regular, Quiz { correct_option_ids }}`,
  `MessageContent::Poll`; `EnvelopePayload::UpdatePoll`. Replay tests
  cover regular, quiz, closed, and `updatePoll` JSON.
- **Voting semantics (verified against TDLib `PollManager.cpp`,
  `set_poll_answer` ~lines 1233–1281):**
  - Quiz answers submit immediately on tap (single-tap, even when
    `allows_multiple_answers` is false); regular polls go through
    `poll_answer_for_tap` which toggles membership for multiple-answer
    polls and replaces for single-answer polls.
  - When `allows_revoting == false`, any tap that changes the current
    vote is a local no-op once a vote exists — including re-tapping the
    current selection (TDLib would reject with "Can't retract vote in
    the poll" / "Can't revote in a quiz").
  - Optimistic local `chosen` state, corrected by `updatePoll`
    broadcasts (`Session::apply_update_poll` scans loaded histories by
    `poll.id`).
- **Creation (`send_poll` / `PollSend`, composer Poll button → dialog):**
  regular polls only (question 1–255 chars, 2–10 non-empty options,
  anonymous / multiple-answers toggles); validated client-side before
  the `sendMessage` + `inputMessagePoll` request. Quiz creation is out
  of scope.
- **Rendering (`poll_body`, `poll_option_row`):** question, type/status
  line ("Poll · N votes · anonymous"), per-option rows with percentage
  + voter count inline ("55% · 12 votes"), fraction bar (blue fill when
  chosen), ✓ on the user's choice, green "· correct answer" suffix on
  the quiz correct option; closed polls show results without a voting
  affordance. Polls are excluded from edit-message support.
- **Screenshot:** `docs/screenshots/ready-poll.png` — a dedicated
  "Demo polls" chat with an open voted regular poll, a closed quiz
  poll (with the non-empty explanation shown after answering), and a
  membership-restricted poll with the restriction label (injected JSON
  through the real reducer), driven by
  `quill --screenshot-demo ready-poll`.
- **Out of this slice (→ future):** quiz creation with correct-option
  authoring; media polls / scheduled polls; poll editing; quiz
  explanation display; `updatePollAnswer` voter-list detail;
  `pollVoteRestrictionReason` handling (restricted polls currently
  render as votable; the vote fails server-side).

## Phase 4.3 — Location / venue / contact (2026-09-26)

- **Rationale:** `messageLocation`, `messageVenue`, and `messageContact`
  are everyday first-class content types. This slice adds typed parsing
  and display rows: coordinates with a tappable OpenStreetMap link for
  locations, title + address rows for venues, and name + phone rows for
  contacts.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `messageLocation` (line 5214), `messageVenue` (line 5217),
    `messageContact` (line 5220)
  - `location` (line 646): `latitude` / `longitude` doubles, `horizontal_accuracy` meters (0 = unknown)
  - `liveLocation` (line 653): `location` + `live_period` (int32,
    `0x7FFFFFFF` = forever), `heading` (1–360, 0 = unknown),
    `proximity_alert_radius` (0–100000 m, 0 = disabled)
  - `messageLiveLocation` (line 5211): `location:liveLocation` +
    `expires_in` (int32, 0 = can't be updated anymore)
  - `venue` (line 663): `location` + `title`, `address`, `provider`
    ("foursquare" / "gplaces"), `id`, `type`
  - `contact` (line 640): `phone_number`, `first_name`, `last_name`,
    `vcard` (0–2048 bytes), `user_id` (int53, 0 = unknown)
  - **Correction vs. the task brief:** `live_period`, `heading`, and
    `proximity_alert_radius` do **not** live on `messageLocation` —
    they belong to the separate `messageLiveLocation` /
    `liveLocation` constructors. Both constructors are parsed, with
    `LocationContent.live: None` for plain locations.
- **Model (`src/telegram/envelope.rs`):** `GeoLocation` (coordinates as
  integer microdegrees — 10⁻⁶ degrees ≈ 11 cm — so the model keeps the
  `Eq` derive; accuracy rounded to whole meters), `LiveLocationState`,
  `LocationContent { location, live }`, `VenueContent` (provider kept;
  provider `id` / `type` dropped as internal database identifiers),
  `ContactContent` (`vcard` kept verbatim but not rendered; `display_name()`
  joins first + last name), `MessageContent::{Location, Venue, Contact}`.
  **Safe rule (documented on `geo_location`):** coordinates must be
  finite with `|lat| <= 90` and `|lon| <= 180`; anything else drops the
  location (the message renders `Unsupported`) rather than clamping to
  a pole or feeding a map link corrupt data. Composer edit excludes the
  three new variants (not text-editable).
- **Rendering (`location_row`, `venue_row`, `contact_row` in
  `src/ui/mod.rs`):** static placeholder chips (no live map tiles) —
  location shows "📍 Location" / coordinates / accuracy (or the live
  status line: "Live · expires in 10:00 · heading 90° · proximity alert
  ≤ 500 m") plus a tappable "🗺 Open map" link; venue shows title,
  address, "coordinates · via provider", and the same map link; contact
  shows name + phone and a subtle "Telegram user" note when `user_id !=
  0`. Map links build `https://www.openstreetmap.org/?mlat=…&mlon=…`
  and go through `platform::open_external_url` (https scheme-gated).
  The phone number is display-only: tapping it never dials (`tel:` is
  refused by the scheme gate anyway).
- **Screenshot:** `docs/screenshots/ready-location.png` — a dedicated
  "Demo places" chat with a location, a live location, a venue, and a
  contact (injected JSON through the real reducer), driven by
  `quill --screenshot-demo ready-location`.
- **Out of this slice (→ future):** live-location re-rendering as
  `updateMessageContent` ticks arrive; live-location sharing from the
  composer; map tiles / static-map preview images; contact
  add-to-address-book; profile deep-links for `user_id`; venue deep
  links (Foursquare / Google Places URLs are not opened — provider
  `id`/`type` are dropped).

## Phase 4.4 — Dice (2026-09-26)

- **Rationale:** `messageDice` is a first-class Telegram content type —
  the 🎲 / 🎯 / 🏀 / ⚽ / 🎰 / 🎳 emoji that roll into an animated
  sticker with a value. This slice adds typed parsing and a static
  display row (large emoji glyph + rolled value); the roll animation
  itself is out of scope.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `messageDice` (line 5231): `initial_state:DiceStickers`
    `final_state:DiceStickers` `emoji:string` `value:int32`
    `success_animation_frame_number:int32`
  - `DiceStickers` (line 7362): animated stickers that "must be used for
    dice animation rendering"; `diceStickersRegular` (line 7365) and
    `diceStickersSlotMachine` (line 7373) are the two shapes
  - `messageStakeDice` (line 5249) is a **separate** constructor (casino
    stake dice with gram amounts) — not parsed in this slice; it
    renders `Unsupported`
- **Model (`src/telegram/envelope.rs`):** `DiceContent { emoji, value }`
  (`value` int32; its range depends on the emoji, e.g. 1–6 for 🎲,
  1–64 for 🎰 — the model keeps whatever TDLib sends), `face()` (empty
  emoji falls back to 🎲 so a malformed payload still renders a die),
  `label()` → `🎲 4` for previews, `MessageContent::Dice`. **Dropped
  fields (documented on the struct):** `initial_state` / `final_state`
  (`DiceStickers` roll-animation stickers) and
  `success_animation_frame_number` (only meaningful to the animated
  rendering). **Safe rule (documented on `parse_message_dice`):**
  `value` is required — a missing or non-integer `value` can't be
  displayed honestly, so the message becomes `Unsupported`
  (`messageDice`) instead of inventing a number. Composer edit excludes
  the new variant (not text-editable).
- **Rendering (`dice_row` in `src/ui/mod.rs`):** a centered card (same
  border/background as the 4.3 rows) with the emoji at 64 px and
  `Rolled {value}` underneath — the glyph stands in for the final
  animation frame, tdesktop-style.
- **Screenshot:** `docs/screenshots/ready-dice.png` — a dedicated
  "Demo dice" chat with three injected rolls (incoming 🎲 = 4, outgoing
  🎲 = 6, incoming 🎯 = 5) through the real reducer, driven by
  `quill --screenshot-demo ready-dice`.
- **Out of this slice (→ future):** the animated roll (playing the
  `DiceStickers` initial → final animation); rendering
  `messageStakeDice`; sending dice from the composer (the animated
  🎲 emoji); `updateMessageContent` live-update of a roll.

## Phase 4.5 — Fullscreen media viewer (2026-09-26)

- **Rationale:** clicking a photo/video in history opened nothing; the
  natural next step (from the photo slice's "Out of this slice" backlog)
  is a fullscreen viewer for already-fetched media, without inventing
  any new TDLib requests.
- **Schema refs:** no new TDLib constructors or fields. The viewer
  reuses the already-parsed `messagePhoto` (line 1392 area of
  `src/telegram/envelope.rs`), `messageVideo` (line 1649 area),
  `photo` / `photoSize` / `file` / `localFile` fields consumed by
  `parse_message_photo` / `parse_message_video`. The only network
  operation is the existing `downloadFile` path
  (`request_media_download`), never a new constructor.
- **Model (`src/media_viewer.rs`, pure, no GPUI):** `MediaViewerKind`
  (`Photo` / `Video`); `MediaViewerItem` (chat/message ids, kind,
  `display_file_ids` largest-first, `download_file_id`, caption/entities,
  optional duration). `MediaViewer` is an open/closed state machine:
  `open(items, index)`, `current()`, `position()`, `prev()` / `next()`
  clamped at the ends, `close()`. `collect_media_items()` filters a
  chat's ordered history to photo/video only, excluding documents,
  animations/GIFs, stickers, audio/voice, secret media, and spoiler
  media. Photos target the largest size for both display and download;
  videos show their thumbnail in the viewer (playback stays in the
  history row — `src/video.rs` frame extraction was deliberately not
  re-piped for this slice).
- **Rendering (`src/ui/mod.rs`):** `QuillApp.media_viewer` holds the
  state; history photo and video visuals get click handlers that collect
  the chat's viewer items and open on the clicked message. The overlay
  is a fullscreen absolute panel appended after the status bar: dark
  backdrop (click closes), Close button, Prev/Next buttons, `n / total`
  counter, caption via the existing `rich_text_line`, a loading/CTA
  state when no local file exists yet (the CTA and the auto-open both
  reuse `request_media_download`; the existing poll loop re-renders on
  `updateFile`). Esc closes the viewer first via the extended
  `CancelSearch` binding. Sponsored, secret, and spoiler media keep
  their old behavior (no viewer).
- **Screenshot:** `docs/screenshots/ready-media-viewer.png` — the Ready
  media session with the viewer opened on the first photo, driven by
  `quill --screenshot-demo ready-media-viewer`.
- **Out of this slice (→ future):** in-viewer video playback (full-file
  frame rendering); zoom/pan; opening documents, GIFs, stickers, or
  audio from the viewer; keyboard left/right navigation; opening the
  viewer from album mosaics.

## Phase 4.6 — Audio/voice seek bars (2026-09-26)

- **Rationale:** the audio/voice slice's "Out of this slice" backlog
  promised a seek bar. This slice gives `messageAudio` and
  `messageVoiceNote` rows tdesktop-style scrubbing: an elapsed/total
  time label and a seek bar that advances while playing, with
  click-to-seek and drag support.
- **Schema (1.8.67, verified in `schema/td_api.tl`):** no new
  constructors or fields. Durations come from the already-parsed
  `audio.duration` (line 564) and `voiceNote.duration` (line 624);
  the rows reuse `messageAudio` (line 5154) / `messageVoiceNote`
  (line 5194) parsing from the audio/voice slice.
- **Model (`src/playback.rs`, pure, no GPUI):** `PlaybackClock` is the
  seek-state machine — `base_secs` frozen offset + optional
  `started_at` while running; `elapsed_secs()` clamped to
  `[0, duration]`; `pause()` / `resume()`; `seek()` clamped to
  `[0, duration]` and keeping the playing/paused state (seek-while-
  paused just moves the frozen offset); `finished()` true only when a
  running clock reaches the duration. Unit-tested: elapsed advance,
  seek clamping, play/pause/seek transitions, finish semantics.
- **Seek mechanism:** ffplay accepts no seek commands on stdin, so a
  finished seek restarts the player with `-ss <seconds>` placed before
  the input file (input seeking — fast on local files, no re-encode).
  Tradeoffs, documented honestly: (1) seeking is not gapless — each
  seek kills and respawns ffplay, with a brief (~100–300 ms) audio gap
  on local files; (2) only the *released* position restarts the
  player — dragging fires `Change` previews (time label follows the
  thumb) and a single `-ss` restart happens on `Release`, so a drag
  never spams subprocesses; (3) seeking while paused moves the frozen
  clock position with no player restart; Play then resumes from there
  via `-ss`. The alternative (a long-lived player with IPC seeking,
  e.g. mpv `--input-ipc-server`) was rejected: it would add a new
  runtime dependency and an IPC protocol for a gap we can already hear
  is small.
- **UI (`src/ui/mod.rs`):** the active row (playing or paused) renders
  the gpui-component `Slider` bound to one `Entity<SliderState>` owned
  by `QuillApp` (min 0, max = duration, step 0.1 s); `Change` sets a
  scrub preview, `Release` applies the seek. A 250 ms GPUI timer task
  (`spawn_playback_tick`, same pattern as `spawn_voice_tick`) advances
  the clock, re-renders the bar, and auto-stops state when the clock
  reaches the duration (ffplay `-autoexit` exits on its own; the tick
  clears our side to match). The clock is pushed into the slider entity
  at the top of `render` (`sync_seek_slider`; the tick has no
  `&mut Window`, which `SliderState::set_value` requires) and skipped
  while scrubbing so a drag is never fought. Inactive rows render a
  static track + fill at their remembered position;
  `playback_positions` remembers the last position per message so a
  paused/stopped row keeps its bar and Play resumes from it. Pause now
  freezes instead of fully stopping (the old toggle-off behavior);
  full state clears when another track starts, the track finishes, or
  the chat changes.
- **Waveform:** kept as-is — it is *not* a mock. `voiceNote.waveform`
  is decoded from TDLib's real 5-bit packed bytes
  (`voice::decode_waveform_5bit`, matching tdesktop's
  `documentWaveformDecode`). True waveform decode from audio *samples*
  (for `messageAudio`, which has no waveform field) is out of scope.
- **Screenshot:** `docs/screenshots/ready-seek-bars.png` — injected
  voice note (12 s) playing from 0:05 with the seek bar advancing, plus
  the "Night Drive" track paused with a remembered 1:27 position and a
  static bar, both showing elapsed/total labels; driven by
  `quill --screenshot-demo ready-seek-bars` (playback state faked, no
  ffplay subprocess in the demo).
- **Out of this slice (→ future):** gapless seeking (mpv IPC or a
  persistent player); click-to-seek on inactive rows (today: press Play
  first, it resumes from the remembered position); per-row volume;
  playback speed; showing the seek bar for `messageVideoNote` round
  videos.

## Phase 5.1 — Forum topics (2026-09-26)

- **Rationale:** forum supergroups (groups with `is_forum`) organize
  messages into topics. This slice reads the topic list, opens a topic,
  and reads per-topic history — read-only, matching the app's
  history-first posture so far.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `messageTopicForum forum_topic_id:int32 = MessageTopic` (line 3004).
  - `forumTopics … topics:vector<forumTopic> next_offset_date:int32
    next_offset_message_id:int53 next_offset_forum_topic_id:int32`
    (line 3976); `forumTopic info:forumTopicInfo last_message:message
    order:int64 is_pinned:Bool unread_count:int32 …` (line 3968);
    `forumTopicInfo … forum_topic_id:int32 name:string
    icon:forumTopicIcon … is_general … is_closed …` (line 3953).
  - `supergroup … is_forum:Bool …` (line 2746) and
    `updateSupergroup supergroup:supergroup = Update` (line 10738);
    `getSupergroup supergroup_id:int53 = Supergroup` (line 11510).
  - `getForumTopics chat_id:int53 query:string offset_date:int32
    offset_message_id:int53 offset_forum_topic_id:int32 limit:int32 =
    ForumTopics` (line 12701).
  - `searchChatMessages chat_id:int53 topic_id:MessageTopic query:string
    sender_id:MessageSender from_message_id:int53 offset:int32 limit:int32
    filter:SearchMessagesFilter = FoundChatMessages` (line 11864).
- **Discovery decision:** `chatTypeSupergroup` carries no `is_forum`
  flag, so forum status is learned through `getSupergroup` /
  `updateSupergroup` only. Selecting a chat fires `getSupergroup`
  (unless the chat is already known to be non-forum or the request is
  in flight); a positive `is_forum` triggers `getForumTopics`.
  Re-selecting an already-open chat also runs the discovery/fetch pair
  so a race on the first select cannot leave a forum without its
  topics. `is_forum: None` = unknown, `Some(false)` = checked
  non-forum.
- **Per-topic history decision:** `getChatHistory` has no topic
  parameter, so per-topic history goes through `searchChatMessages`
  with `topic_id = messageTopicForum{forum_topic_id}` and an empty
  query — never `getChatHistory`. `RequestPurpose::{GetSupergroup,
  GetForumTopics, GetTopicHistory}` route responses into
  `forum_topics: HashMap<i64, Vec<ForumTopic>>` and
  `topic_histories: HashMap<(i64, i32), TopicHistory>`; topic history
  pages oldest-first like chat history (page size 50; the message-id
  cursor works because `searchChatMessages` returns message ids).
- **First-page tradeoff:** `getForumTopics` fetches the first page
  (limit 100) with no `query` filter and deliberately drops
  `next_offset_*` — a forum with more than 100 topics is vanishingly
  rare and a full paginated list UI is a follow-up, not this slice.
- **Dropped fields (documented, not forgotten):** from
  `forumTopic`/`forumTopicInfo` we keep `forum_topic_id`, `name`,
  `is_general`, `is_closed`, `is_pinned`, `unread_count`, `order`, and
  a cheap `last_message` preview (parsed message text only). Dropped:
  icon, creation/creator metadata, outgoing/hidden/implicit flags,
  read message ids, mention/reaction/poll-vote unread counts,
  notification settings, and drafts.
- **UI (`src/ui/mod.rs`):** opening a forum with no selected topic
  shows the topic list (rows with name, unread badge, General/Closed
  tags, last-message preview; pinned-first ordering); selecting a row
  opens the topic's history in the same history component; a strip
  above the topic view shows the topic name plus a "‹ Topics" back
  button; `load_older_action` pages the open topic's history; chat
  rows carry a "Topics" badge for forum chats. The composer is hidden
  in topic view with a read-only note — posting into a topic is out of
  scope.
- **Screenshot:** `docs/screenshots/ready-forum-topics.png` — injected
  forum supergroup with three topics (General pinned + 3 unread,
  Announcements with a preview, Random closed); driven by
  `quill --screenshot-demo ready-forum-topics`.
- **Out of this slice (→ future):** posting to a topic
  (`sendMessage` with `messageTopicForum` reply-to/topic input);
  topic creation/edit/close/pin management; paginated topic lists
  (>100 topics); General-topic special-casing (e.g. its messages also
  appearing in the main history); per-topic notification settings and
  unread-marking; draft messages in topics.

## Phase 6 — Contacts & profiles (2026-09-26)

- **Rationale:** the sidebar gains a **Contacts** tab (Telegram's people
  list), and private chats / supergroups get a side info panel (user
  profile with bio + photo, group description + member count). A known
  non-contact user can be added to contacts from their panel.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `getContacts = Users` (line 14520); `users total_count:int32
    user_ids:vector<int53> = Users` (line 2471). The response carries
    only ids — user objects arrive via `updateUser`.
  - `user … first_name:string last_name:string usernames:usernames
    phone_number:string status:UserStatus profile_photo:profilePhoto …
    is_contact:Bool … type:UserType … = User` (line 2403);
    `usernames` has `active_usernames` but no singular `username`
    (line 2372) — the first active username is kept.
  - `profilePhoto … small:file big:file … = ProfilePhoto` (line 754).
  - `updateUserStatus user_id:int53 status:UserStatus = Update`
    (line 10729); `userStatusEmpty`, `userStatusOnline expires:int32`,
    `userStatusOffline was_online:int32`, `userStatusRecently`,
    `userStatusLastWeek`, `userStatusLastMonth` (lines 6407–6422).
  - `getUserFullInfo user_id:int53 = UserFullInfo` (line 11501);
    `userFullInfo personal_photo:chatPhoto photo:chatPhoto
    public_photo:chatPhoto … bio:formattedText … bot_info:botInfo =
    UserFullInfo` (line 2468); `chatPhoto … sizes:vector<photoSize> …
    = ChatPhoto` (line 1030).
  - `getSupergroupFullInfo supergroup_id:int53 = SupergroupFullInfo`
    (line 11513); `supergroupFullInfo photo:chatPhoto …
    description:string member_count:int32 … = SupergroupFullInfo`
    (line 2792).
  - `addContact user_id:int53 contact:importedContact
    share_phone_number:Bool = Ok` (line 14513);
    `importedContact phone_number:string first_name:string
    last_name:string note:formattedText = ImportedContact` (line 7382).
- **Correlation decision:** `users`, `userFullInfo`, and
  `supergroupFullInfo` responses carry no subject id, so
  `PendingRequest` gained explicit `user_id` / `supergroup_id` fields
  (`Session::request_for_user`, `RequestPurpose::{GetContacts,
  AddContact, GetUserFullInfo, GetSupergroupFullInfo}`). The user panel
  opened from a chat header resolves the user through the private chat
  instead (`RequestPurpose::GetUserFullInfo` + `chat_id`).
- **Photo decision:** the panel photo comes from
  `userFullInfo.photo:chatPhoto` — the preferred size (`type == "m"`,
  else largest ≤ 320px wide, else smallest) is parsed, its
  `photo:file` cached in `Session::files`, and downloaded at panel-open
  priority. `user.profile_photo.small` is kept only as a download
  fallback when no full info is cached. Rationale: the task calls for
  "bio + photo via `getUserFullInfo`", and the fresh fetch beats the
  possibly stale `updateUser` photo handle. Personal/public photo
  variants and animation/sticker chat-photo extras are dropped.
- **Add-contact honesty:** `addContact` needs a known Telegram
  `user_id` — there is no phone-number-only discovery in this flow, so
  the UI only offers adding a known user (from their info panel), and
  the dialog *requires* a phone number (prefilled from the cached user
  when known) because `importedContact` needs one. Quill does not offer
  adding by bare user id. `share_phone_number` is `false` — sharing the
  user's own number is a privacy choice this minimal flow does not
  make. A successful `addContact` invalidates the contacts cache so
  the tab refetches.
- **Dropped fields (documented, not forgotten):** from `user`:
  accent/background colors, emoji status, verification status, premium,
  story state, restrictions, `added_to_attachment_menu`, language code;
  from `userFullInfo`: block list, call capabilities, story flags; from
  `supergroupFullInfo`: photo, invite link, sticker set, slow-mode,
  join settings. Status keeps a display string only (no exact
  `was_online` timestamps — "last seen recently/within a week/within a
  month" labels).
- **UI (`src/ui/mod.rs`):** sidebar **Chats | Contacts** tabs (Ready
  mode only); the Contacts tab fires `getContacts` on open (deduped by
  settled cache + in-flight purpose) and renders loading / error +
  Retry / empty / rows (initials avatar, name, status; tap → user
  panel). The conversation header title is clickable for private chats
  and supergroups/channels and opens the right-side info panel (close
  ✕); user panel shows photo-or-initials, name, status,
  @username/phone rows, bio, and **Add contact** for known non-contact
  non-bot users; group panel shows title, member count, description.
  The add-contact dialog is a centered overlay (phone + first/last
  name, prefilled) over the media-viewer-style backdrop.
- **Screenshot:** `docs/screenshots/ready-contacts.png` — injected
  contacts tab (Ada online, Noor recently, Zed last-week) with Zed's
  info panel open (bio from injected `userFullInfo`, Add contact
  button); driven by `quill --screenshot-demo ready-contacts`.
- **Out of this slice (→ future):** contact search; importing device
  contacts (`importContacts`); phone-number lookup of unknown users;
  profile-photo *setting*; per-contact notification/privacy settings;
  group member lists and admin management. `updateUserFullInfo` is
  parsed and cached, so server-pushed profile changes land in the panel
  on the next render; the panel fetch itself is cache-deduped (no
  refetch while cached).

## Phase 7 — Folders & discovery (2026-09-26)

- **Rationale:** the sidebar gains folder tabs (Telegram's chat folders)
  beyond Main/Archive, and global search gains a public-username lookup
  (`searchPublicChats`) alongside the offline known-chat search.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - There is **no `getChatFolders`** function in 1.8.67. The folder list
    arrives as `updateChatFolders
    chat_folders:vector<chatFolderInfo> main_chat_list_position:int32
    are_tags_enabled:Bool = Update` (line 10606) — pushed after
    authorization and on every change. `chatFolderInfo id:int32
    name:chatFolderName icon:chatFolderIcon color_id:int32 is_shareable:Bool
    has_my_invite_links:Bool = ChatFolderInfo` (line 3485);
    `chatFolderIcon name:string = ChatFolderIcon` (line 3453);
    `chatFolderName text:formattedText animate_custom_emoji:Bool =
    ChatFolderName` (line 3458); `formattedText text:string
    entities:vector<textEntity> = FormattedText` (line 117).
  - Folder **membership** is positional: `chatListFolder
    chat_folder_id:int32 = ChatList` (line 3524) appears in
    `chatPosition.list`, `updateChatAddedToList`/`updateChatRemovedFromList`
    `chat_list`, and the full positions set of `updateChatLastMessage`.
    `getChatListsToAddChat chat_id:int53 = ChatLists` (line 13347) is *not*
    folder membership — it lists the chat lists a chat may be added to for
    `addChatToList` — so it is not used here.
  - `getChats chat_list:ChatList limit:int32 = Chats` (line 11600) and
    `loadChats chat_list:ChatList limit:int32 = Ok` (line 11595) both accept
    a `chatListFolder`; the tab-select path uses `loadChats`.
  - `searchPublicChats query:string type_filter:SearchChatTypeFilter =
    Chats` (line 11609); `chats total_count:int32 chat_ids:vector<int53> =
    Chats` (line 3630). `type_filter` null = all chat types (same as the
    existing `searchChats` call). `SearchChatTypeFilter` has only
    `searchChatTypeFilterBot` / `searchChatTypeFilterChannel` constructors
    (lines 6350–6353), so null is the only "all" option.
- **Folder membership approach:** `ChatSummary.folder_positions:
  BTreeMap<i32, i64>` (folder id → TDLib order), maintained by the same
  reducers as Main/Archive: `updateChatPosition` (order 0 removes),
  `updateChatLastMessage` full positions set (drops folder ids no longer
  present), `updateChatAddedToList` (membership confirmed, order 0 until
  the position arrives), `updateChatRemovedFromList`. `updateChatFolders`
  replaces `Session::chat_folders` wholesale (the update carries the full
  ordered list). `Session::ordered_folder_chats(id)` sorts by folder order
  desc, then chat id desc — the same convention as main/archive.
- **Tab UX:** sidebar renders `Main` + folder tabs above the search field
  only when the account has folders. Selecting a folder tab filters the
  chat list to that folder's chats (Archive section hidden in folder
  view — it stays as-is under the main list). Tab-select also fires a
  single-shot `loadChats(chatListFolder)` (`RequestPurpose::LoadFolderChats`,
  deliberately *not* `LoadChats` so the ok-response does not re-trigger
  main-list paging). The selected tab is App view state (`folder_tab:
  Option<i32>`, `None` = Main), next to `contacts_tab_open`.
- **Public lookup:** typed global search now sends `searchChats` +
  `searchPublicChats` + `searchMessages` (new
  `RequestPurpose::SearchPublicChats`); `SearchState` gained
  `public_chat_ids` + its own done/error flags, and the Ready/Empty/Failed
  status waits for all three legs (`SearchFlight::Query` is now a
  3-tuple). Results render in their own **Public chats** section (TDLib
  excludes known chats from `searchPublicChats` results, so no merging
  with the Chats section). **Selecting a public chat opens it** — the same
  `select_search_chat` path as known chats (`addRecentlyFoundChat` +
  `openChat`); unknown chats arrive via `updateNewChat` before the `chats`
  response, with a "chat {id}" fallback row title until then.
- **Dropped fields (documented, not forgotten):** from `chatFolderInfo`:
  `is_shareable`, `has_my_invite_links` (folder sharing out of scope);
  from `chatFolderName`: `animate_custom_emoji` and all text entities
  (names may only carry CustomEmoji entities per the schema docs — dropped
  to plain text); `chatFolder` full specs (create/edit/delete/reorder) are
  not parsed — only the info list. `updateChatFolders`'s
  `main_chat_list_position` / `are_tags_enabled` are dropped (reorder and
  tags out of scope). Folder `is_pinned` is not tracked separately —
  pinned folder chats already sort first by TDLib order.
- **Screenshot:** `docs/screenshots/ready-folders.png` — injected
  `updateChatFolders` (Work / News) with the non-default **News** folder
  selected, showing only its chats; driven by
  `quill --screenshot-demo ready-folders`.
- **Out of this slice (→ future):** folder create/edit/delete/reorder
  (`createChatFolder`, `editChatFolder`, `deleteChatFolder`,
  `reorderChatFolders`, `toggleChatFolderTags`); folder icons rendered as
  images (only the icon *name* is kept); folder invite links; per-chat
  "add to folder" (`addChatToList` with `chatListFolder`); folder tags UI;
  paging folder chats beyond one `loadChats` page; `getChatListsToAddChat`
  surfacing in the archive/unarchive menu.

## Phase 7.3 — Folder management (2026-09-26)

- **Rationale:** full folder management toward Telegram parity — create,
  edit, delete, reorder folders, toggle folder tags, add/remove chats to
  and from folders, and a manage dialog in the UI. Builds on the Phase 7.1
  folder-tabs foundation.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `createChatFolder folder:chatFolder = ChatFolderInfo` (line 13358);
    `editChatFolder chat_folder_id:int32 folder:chatFolder = ChatFolderInfo`
    (line 13361); `deleteChatFolder chat_folder_id:int32 leave_chat_ids:vector<int53> = Ok`
    (line 13364); `getChatFolder chat_folder_id:int32 = ChatFolder`
    (line 13355); `reorderChatFolders chat_folder_ids:vector<int32>
    main_chat_list_position:int32 = Ok` (line 13373);
    `toggleChatFolderTags are_tags_enabled:Bool = Ok` (line 13376);
    `getChatFolderChatsToLeave chat_folder_id:int32 = Chats` (line 13367);
    `getChatListsToAddChat chat_id:int53 = ChatLists` (line 13347);
    `addChatToList chat_id:int53 chat_list:ChatList = Ok` (line 13352);
    `chatFolder` full spec (line 3476).
  - There is **no `removeChatFromList`** in 1.8.67. Removing a chat from a
    folder is done by fetching the full spec with `getChatFolder`,
    dropping the chat from `included_chat_ids`/`pinned_chat_ids` (and
    adding it to `excluded_chat_ids` when it only matched via filters),
    and sending the modified spec with `editChatFolder`.
  - `ChatFolderSpec` (`src/folders.rs::ChatFolderSpec`) mirrors the
    `chatFolder` constructor: name (1–12 chars per the schema docs),
    icon (null — custom-emoji icons out of scope), pinned/included/
    excluded chat ids, exclude_muted / exclude_read / exclude_archived,
    include_* type filters, color_id (-1), is_shareable (false).
- **Driver (`src/connect.rs`):** one method per operation
  (`create_chat_folder`, `edit_chat_folder`, `delete_chat_folder`,
  `reorder_chat_folders`, `toggle_chat_folder_tags`, `fetch_chat_folder`,
  `fetch_chat_folder_chats_to_leave`, `fetch_chat_lists_to_add_chat`,
  `add_chat_to_list`, `remove_chat_from_folder`), with
  `RequestPurpose::*` variants and per-folder request dedupe
  (`has_purpose_for_folder`). Folder-load 404s mark
  `folder_chats_exhausted`; folder paging stays eager (each `loadChats`
  ok re-enters `maybe_load_folder_chats`), the same pattern as the main
  list — there is deliberately no user-triggered "Load more".
- **State (`src/state.rs`):** `updateChatFolders` now keeps
  `are_folder_tags_enabled`; `folder_specs` caches full specs keyed by
  folder id (dropped on edit); `chat_lists_for_add` caches
  `getChatListsToAddChat` results; `folder_chats_to_leave` caches
  `getChatFolderChatsToLeave` results for the delete-confirm dialog;
  `folder_remove_queue` holds remove-from-folder edits until the
  `getChatFolder` spec arrives.
- **UI (`src/ui/mod.rs`):** a **Folders** manage dialog (open from the
  folder-tabs row "···" entry) listing folders with chat counts, ↑/↓
  reorder, Edit, Delete, a "Show folder tags" toggle
  (`toggleChatFolderTags`), and **New folder**. The create/edit form has
  a name field (schema-validated), include-type filters, included/excluded
  chat multi-select, and exclude-muted/read/archived toggles. Delete
  asks for confirmation and offers to leave the suggested chats
  (`getChatFolderChatsToLeave`). Chat rows show folder-tag chips when
  tags are enabled; the conversation header has a **Folders** picker with
  add destinations (Main / Archive / folders from
  `getChatListsToAddChat`) and remove rows for current memberships.
  Demo-local create/edit/delete/reorder helpers back the screenshot
  demo (no live Telegram).
- **Screenshot:** `docs/screenshots/ready-folders-manage.png` — the
  Folders manage dialog over demo chats; driven by
  `quill --screenshot-demo ready-folders-manage`.
- **Out of this slice (→ future):** custom-emoji folder icons;
  folder invite links (`is_shareable` / `has_my_invite_links`);
  suggested filters (`getChatFolderDefaultIconName`,
  `getChatFolderNewChats` and friends); richer create/edit parity
  (per-chat include/exclude search, drag reorder).

## Parity slice — Chat-list avatars & channel/supergroup header (2026-09-26)

- **Rationale:** toward Telegram parity for the two most-seen surfaces —
  the chat list (every row gets a recognizable avatar) and the
  channel/supergroup conversation header (photo, description snippet,
  @username, subscriber/member count, discussion-group link), while the
  heavier channel surfaces stay out of scope.
- **Schema (1.8.67, verified in `schema/td_api.tl`):**
  - `chatPhotoInfo small:file big:file ...` (line 762); `chat ...
    photo:chatPhotoInfo ...` (line 3627) — only `small` is kept and
    downloaded (the cheap 160px thumbnail; `big` is fetched on demand
    later, out of this slice).
  - `updateChatPhoto chat_id:int53 photo:chatPhotoInfo` (line 10488).
  - `usernames active_usernames:vector<string> ...` (line 2372) — the
    first active username is treated as the primary one; empty when the
    object is null.
  - `supergroup ... usernames:usernames ... member_count ...` (line
    2746); `updateSupergroupFullInfo supergroup_id:int53
    supergroup_full_info:supergroupFullInfo` (line 10750);
    `supergroupFullInfo photo:chatPhoto description:string
    member_count:int32 linked_chat_id:int53 ...` (line 2792);
    `getSupergroupFullInfo supergroup_id:int53 = SupergroupFullInfo`
    (line 11513). `linked_chat_id` is 0 when there is no / unknown
    linked discussion group (line 2756).
- **Behavior:**
  - Envelope parsing keeps `chat.photo.small` on `updateNewChat` and
    parses `updateChatPhoto`; the session stores
    `ChatSummary::photo_file_id` and remembers the file for download.
  - The driver auto-downloads chat-list photos on every ingest at thumb
    priority; `should_download` dedupes in-flight and completed files,
    so each photo is requested at most once and `updateChatPhoto`
    re-arms the new id.
  - Opening a supergroup/channel fires `getSupergroup` (header
    @username, deduped by the username cache + in-flight purpose) and
    `getSupergroupFullInfo` (description, subscriber/member count,
    `linked_chat_id`; deduped by the full-info cache).
  - The header identity (avatar + title + meta) stays clickable and
    opens the existing info panel; a **Discuss** button selects the
    linked discussion chat when `linked_chat_id` resolves to a known
    chat. Non-channels never show it.
  - Fallback avatar: deterministic colored circle (Telegram-ish palette
    keyed on chat id) with 1–2 initials. Photo removal (`photo: null`)
    falls back to initials.
  - Counts render compact ("12.3K", "1.2M"); channels say
    "subscribers", groups say "members".
- **UI proof:** `docs/screenshots/ready-chat-avatars.png` — mixed photo
  and initial avatars across private / basic-group / supergroup /
  channel rows; open channel header with photo, @username, subscriber
  count, description snippet, and the Discuss button; driven by
  `quill --screenshot-demo ready-chat-avatars`.
- **Out of this slice (→ future):** invite links / join requests, admin
  log, boosts / statistics, suggested / scheduled posts, channel comment
  threading, on-demand `big` photo fetch.

## Phase 8.1 — Desktop notifications (2026-09-26)

- **Rationale:** new incoming messages should surface an OS notification when
  the user isn't looking at the chat (app in background, or a different chat
  open); clicking it focuses the chat. No new TDLib constructors are needed.
- **Schema (1.8.67, verified in `schema/td_api.tl`) — reused, not new:**
  `updateNewMessage message:message = Update` (line 10400);
  `chatNotificationSettings … use_default_mute_for:Bool mute_for:int32 …
  use_default_show_preview:Bool show_preview:Bool … = ChatNotificationSettings`
  (line 3363; `mute_for` = seconds left before unmute, comment lines
  3347–3348). Mute evaluation reuses `ChatNotificationSettings::is_muted()`
  (`!use_default_mute_for && mute_for > 0`); scope defaults are not applied.
- **Decision (`src/notify.rs::decide_notify`, pure, unit-tested).** An
  `updateNewMessage` notifies iff: incoming (`!is_outgoing`); chat not
  exception-muted; chat known to the reducer (unknown chat → skip: no title,
  no verified mute/read state); `message.id > last_read_inbox_message_id`
  (already-read echoes never notify); and NOT (app active AND chat currently
  open). In other words: notify when the app is in the background **or** the
  chat isn't open. The title is the chat title; the body is
  `MessageContent::preview()` unless previews are hidden — user setting
  `hide_notification_previews` (default true) or per-chat
  `use_default_show_preview || show_preview` — or the preview is empty, in
  which case the body is the generic "New message".
- **Plumbing:** the reducer computes the decision in the
  `UpdateNewMessage` arm and appends to `Session::pending_notifications`
  with same-chat burst coalescing ("N new messages" once a second message
  for a chat is still queued). `Session::app_active` (default true, so no
  notifications fire before the first paint measures it) is written by the
  UI from `Window::is_window_active()` at the top of every render;
  `hide_notification_previews` lives on the session (mirrors
  `settings::Preferences`, default true — no settings UI yet). The UI
  drains the queue in `render` (the only UI path with a `&mut Window`) and
  shows each on a worker thread (`quill-notify`), capped at 8 concurrent;
  excess bursts are dropped, not stacked. Arguments are passed without a
  shell, so message text can never inject shell syntax. The reducer never
  spawns processes.
- **Linux (`notify-send`, libnotify):** `notify-send --app-name=Quill --wait
  --action=default=Open <title> <body>`; `--wait` blocks until dismissal or
  click, and a click prints `default` on stdout, which the worker thread
  maps to focusing the chat via `select_listed_chat` on the next render.
  Honest limits: `--action` support varies by notification daemon
  (GNOME/KDE honor it; some daemons ignore clicks — then the notification
  is display-only); if `notify-send` is missing the spawn fails silently.
- **macOS:** gpui-kit 0.6.1 exposes **no** NotificationCenter binding
  (verified in the vendored crate source), and inventing a native
  `UNUserNotificationCenter` binding is out of scope, so macOS uses an
  `osascript` `display notification … with title …` fallback
  (AppleScript-escaped, no shell). It is display-only: `display
  notification` offers no click callback, so click-to-focus does not work
  on macOS in this slice.
- **Out of this slice (→ future):** native macOS NotificationCenter binding
  with click-to-focus; per-mention unmute (`disable_mention_notifications`)
  and `chat.default_disable_notification`; fetching global default
  notification settings for `use_default_*` fallback; notification sounds;
  replacing an existing notification on edit; a settings UI for
  `hide_notification_previews`; notifying for unknown chats (first message
  of a chat TDLib hasn't announced yet); screenshots (OS chrome, not app
  UI — no `--screenshot-demo` marker for this slice).

## Phase 9.1 — Story viewing (2026-09-26)

- **Rationale:** Telegram's tdesktop shows an active-stories tray above the
  chat list and opens a fullscreen story viewer on tap. This slice brings
  that to Quill: tray, fullscreen viewer, read state — all read-only
  viewing. Posting stays out (per the roadmap).
- **Schema (1.8.67, verified in `schema/td_api.tl` — no invented
  constructors/fields):** `storyVideo … thumbnail:thumbnail … video:file
  = StoryVideo` (line 6633); `storyContentPhoto photo:photo = StoryContent`
  (line 6654); `storyContentVideo video:storyVideo
  alternative_video:storyVideo = StoryContent` (line 6657);
  `storyContentLive … = StoryContent` (line 6662);
  `storyContentUnsupported = StoryContent` (line 6665);
  `storyListMain = StoryList` / `storyListArchive = StoryList` (lines
  6687/6690); `story … content:StoryContent … caption:formattedText …
  = Story` (line 6742); `storyInfo story_id:int32 date:int32
  is_for_close_friends:Bool is_live:Bool = StoryInfo` (line 6773);
  `chatActiveStories chat_id:int53 list:StoryList order:int53
  can_be_archived:Bool max_read_story_id:int32 stories:vector<storyInfo>
  = ChatActiveStories` (line 6783); `updateStory story:story = Update`
  (line 10895); `updateChatActiveStories active_stories:chatActiveStories
  = Update` (line 10911); `getStory story_poster_chat_id:int53
  story_id:int32 only_local:Bool = Story` (line 13695);
  `loadActiveStories story_list:StoryList = Ok` (line 13762);
  `getChatActiveStories chat_id:int53 = ChatActiveStories` (line 13768);
  `openStory story_poster_chat_id:int53 story_id:int32 = Ok` (line 13794);
  `closeStory story_poster_chat_id:int53 story_id:int32 = Ok` (line 13799).
  There is **no** `viewStories` in this schema — viewing is
  `openStory`/`closeStory` pairs, and TDLib derives read state from
  `chatActiveStories.max_read_story_id`.
- **Kept / dropped (parser, `src/telegram/envelope.rs`).** Kept: ids, date,
  media files, caption/entities, tray order and read state. Dropped:
  `alternative_video`; story flags, privacy, reactions/interactions, repost
  info, clickable areas, and albums. Photo stories keep all `photoSize`
  files; video keeps the primary `storyVideo`, its thumbnail, duration, and
  video file; live and unknown content degrade to a placeholder.
- **Tray (`Session::story_tray`, `ordered_story_tray`).** `updateChatActiveStories`
  / `chatActiveStories` responses upsert main-list entries; archive-list or
  null-list entries remove the tray row (no archive tray UI this slice).
  Sorted by `(order, chat_id)` descending (schema comment, line 6781).
  Unread = any `story_id > max_read_story_id` (accent ring); else a muted
  ring. The tray shows all main-list posters (primarily contacts).
- **Driver (`src/connect.rs`).** `loadActiveStories(storyListMain)` once per
  Ready (feeds `updateChatActiveStories`); `getChatActiveStories(chat_id)`
  for a refresh; `getStory` with `only_local: false` for missing story
  details, deduped per `(chat_id, story_id)` in-flight
  (`PendingRequest::story_id`); `openStory`/`closeStory` are fire-and-forget
  (`ok` answers need no handling).
- **Viewer (`src/story_viewer.rs` + UI overlay).** Pure state machine
  modeled on the Phase 4.5 media viewer (`StoryViewer::open/close/prev/next`,
  clamped index, `position()`). Items are collected in
  `chatActiveStories.stories` chronological order. Photo shows the largest
  size; video shows the `storyVideo` thumbnail (the full clip is not
  renderable by the `img` element — same call as Phase 4.5; if no thumbnail
  exists, the clip downloads but the item shows a placeholder); live and
  unsupported stories stay placeholders. Tapping a tray entry prefetches
  missing `getStory` details and opens on the latest story; a click while
  the `story` response is in flight defers to `pending_story_open`,
  resolved on the next render. The fullscreen overlay is modeled on
  `media_viewer_overlay`: dark backdrop, poster name + "Story N of M",
  caption, Prev/Next/Close; Escape closes the story viewer before other
  overlays (`cancel_search`).
- **Out of this slice (→ future):** posting stories; story reactions and
  replies; joining/playing live stories; actual video playback (thumbnail
  only); story albums; story privacy/close-friends management; story
  interaction/view-count UI; the archive-list tray.

## Phase 9.2 — Story reactions, replies, and own-story deletion (2026-09-26)

- **Rationale:** extend the Phase 9.1 story viewer with the interaction
  affordances Telegram clients show under a story: emoji reactions
  (quick-react + picker), interaction counters, text replies to the
  poster, and deleting an own story. Story *posting* stays out — see the
  schema blocker below.
- **Schema blocker RETRACTED (2026-09-27 — the original "verified" claim was wrong).** The original note searched for a `sendStory` constructor and concluded story posting was blocked on a TDLib upgrade. That was the wrong name: TDLib names the function **`postStory`**, and it is present in the pinned vendored schema (`schema/td_api.tl`, 1.8.67) at **line 13715**: `postStory chat_id:int53 content:InputStoryContent areas:inputStoryAreas caption:formattedText privacy_settings:StoryPrivacySettings album_ids:vector<int32> active_period:int32 from_story_full_id:storyFullId is_posted_to_chat_page:Bool protect_content:Bool = Story;` — identical signature to current TDLib master. The full posting surface is also present: `canPostStory` (line 13702), `inputStoryContentPhoto` (line 6673), `inputStoryContentVideo` (line 6681), all four `storyPrivacySettings*` types (lines 8928–8937), `inputStoryAreas` + area types (line 6619). No TDLib upgrade is needed — posting was never blocked; the verifier assumed the constructor name instead of discovering it. Story posting becomes an implementable slice (queued after the current call work): photo/video composer → `canPostStory` eligibility → `postStory` with caption + privacy selector → honest pending/failed/succeeded states via `updateStoryPostSucceeded` (line 10901) / `updateStoryPostFailed` (line 10907).
- **Schema (1.8.67, verified in `schema/td_api.tl` — no invented
  constructors/fields):** `reactionTypeEmoji emoji:string =
  ReactionType` (line 2915); `inputMessageReplyToStory
  story_poster_chat_id:int53 story_id:int32 = InputMessageReplyTo`
  (line 3099); `storyInteractionInfo view_count:int32 forward_count:int32
  reaction_count:int32 recent_viewer_user_ids:vector<int53> =
  StoryInteractionInfo` (line 6712); `story … interaction_info:StoryInteractionInfo
  chosen_reaction_type:ReactionType … can_be_deleted:Bool …
  can_be_replied:Bool … can_get_interactions:Bool … = Story` (line 6742);
  `availableReaction type:ReactionType needs_premium:Bool =
  AvailableReaction` (line 7321); `availableReactions … =
  AvailableReactions` (line 7330); `updateStoryDeleted
  story_poster_chat_id:int53 story_id:int32 = Update` (line 10898);
  `updateStoryPostSucceeded story:story old_story_id:int32 = Update`
  (line 10901); `updateStoryPostFailed story:story error:error
  error_type:CanPostStoryResult = Update` (line 10907); `deleteStory
  story_poster_chat_id:int53 story_id:int32 = Ok` (line 13754);
  `getStoryAvailableReactions row_size:int32 = AvailableReactions`
  (line 13802); `setStoryReaction story_poster_chat_id:int53
  story_id:int32 reaction_type:ReactionType update_recent_reactions:Bool
  = Ok` (line 13809 — `reaction_type: null` removes; the schema comment
  excludes live stories); `getStoryInteractions story_poster_chat_id:int53
  story_id:int32 … = StoryInteractions` (line 13819 — detailed viewer
  list, kept out of this slice).
- **Parser (`src/telegram/envelope.rs`).** `ParsedStory` gains
  `chosen_reaction_emoji` (only `reactionTypeEmoji` with a non-empty
  emoji; custom-emoji/paid/null → `None`), `interaction_info`
  (`StoryInteractionInfoView` + `any_nonzero()`), and the
  `can_be_deleted` / `can_be_replied` / `can_get_interactions` gates.
  New payloads: `UpdateStoryDeleted`, `UpdateStoryPostSucceeded`,
  `UpdateStoryPostFailed` (diagnostic + tray refresh; kept, not dropped),
  `StoryAvailableReactions` (emoji-only options; custom-emoji rows
  dropped). `updateStory` still parses as the ordinary `Story`.
- **Requests (`src/telegram/requests.rs`).** `get_story_available_reactions`
  (`row_size` 10, inside the schema's 5–25), `set_story_reaction`
  (`reaction_type: null` for removal, `update_recent_reactions: true`),
  `delete_story`, `send_text_story_reply` — the reply is a plain
  `sendMessage` with `reply_to: inputMessageReplyToStory` and `clear_draft:
  true`, same call as replying to a message. All shapes asserted against
  1.8.67 in unit tests.
- **Driver (`src/connect.rs`).** `get_story_available_reactions`
  (cache + in-flight dedupe), `set_story_reaction` (rejects unknown or
  live stories and empty emoji),
  `delete_story` (gated on cached `can_be_deleted`), `send_story_reply`
  (gated on cached `can_be_replied`, non-empty text, supported chat).
  Reducer: delete removes the story from the cache and the poster's tray
  entry (entry dropped when it has no stories left); post-succeeded
  upserts the story and queues the poster's tray refresh
  (`Session::story_tray_refresh`, drained in the UI tick into
  `getChatActiveStories`); available reactions cached for the picker.
- **UI (`src/ui/mod.rs`).** Under the viewer caption: interaction counts
  (`👁 42 · ❤️ 7 · ↩ 3`, only when `can_get_interactions` and non-zero),
  an action row — ❤️ quick-react (toggles, shows ✓ when chosen),
  **React…** (picker fed by `getStoryAvailableReactions`), **Reply**
  (gated on `can_be_replied`, text input + Send), **Delete** (gated on
  `can_be_deleted`; the viewer closes when its story disappears from the
  cache). Escape closes the story viewer (resetting picker flags inside
  `close_story_viewer`). Screenshot proof:
  `docs/screenshots/ready-story-post.png` (`ready-story-post` demo —
  picker + reply row open on an own story; its caption previously stated
  a `sendStory` blocker — retracted 2026-09-27, the function is
  `postStory`, line 13715).
- **Out of this slice (→ future):** story posting / photo composer /
  caption + privacy selector (unblocked per the retraction above —
  shipped in Phase 9.3);
  video uploads; story albums; privacy/close-friends management beyond
  the per-story read of `can_be_*`; joining/playing live stories;
  `getStoryInteractions` detailed viewer list; the archive-list tray.

## Phase 9.3 — Story posting (2026-09-27)

- **Rationale:** with the `sendStory` blocker retracted (the constructor
  is `postStory`, 9.2 above), this slice completes the story loop: a
  composer overlay posts photo/video stories via `canPostStory`
  eligibility → `postStory` with caption + privacy, and the tray gains a
  persistent "+" tile that opens it.
- **Schema (1.8.67, verified in `schema/td_api.tl` — no invented
  constructors/fields):** `canPostStory = CanPostStoryResult` (line
  13702); `postStory chat_id:int53 content:InputStoryContent
  areas:inputStoryAreas caption:formattedText
  privacy_settings:StoryPrivacySettings album_ids:vector<int32>
  active_period:int32 from_story_full_id:storyFullId
  is_posted_to_chat_page:Bool protect_content:Bool = Story` (line
  13715); `inputFileLocal path:string = InputFile` (line 325);
  `inputStoryContentPhoto photo:InputFile
  added_sticker_file_ids:vector<int32> = InputStoryContent` (line
  6673); `inputStoryContentVideo video:InputFile
  added_sticker_file_ids:vector<int32> duration:double
  cover_frame_timestamp:double is_animation:Bool = InputStoryContent`
  (line 6681); `inputStoryAreas areas:vector<inputStoryArea> =
  InputStoryAreas` (line 6619 — sent empty, areas are out of slice);
  `canPostStoryResultOk story_count:int32` (8535),
  `canPostStoryResultPremiumNeeded` (8538), `canPostStoryResultBoostNeeded`
  (8541), `canPostStoryResultActiveStoryLimitExceeded` (8544),
  `canPostStoryResultWeeklyLimitExceeded retry_after:int32` (8547),
  `canPostStoryResultMonthlyLimitExceeded retry_after:int32` (8550),
  `canPostStoryResultLiveStoryIsActive story_id:int32` (8553);
  `storyPrivacySettingsEveryone/Contacts/CloseFriends/SelectedUsers`
  (lines 8928–8937 — SelectedUsers carries `user_ids:vector<int53>`);
  `updateStoryPostSucceeded story:story old_story_id:int32 = Update`
  (line 10901); `updateStoryPostFailed story:story error:error
  error_type:CanPostStoryResult = Update` (line 10907).
- **Composer (`src/story_composer.rs`, new).** Pure state machine (no
  GPUI): media kind (photo/video extension sniff), the 4-way privacy
  selector (Everyone / Contacts / Close friends / Selected users —
  exact TDLib `storyPrivacySettings*` JSON mapping), and selected-user
  toggle set. Unit tests cover privacy JSON, media-kind sniffing, and
  user selection.
- **Requests (`src/telegram/requests.rs`).** `can_post_story()`,
  `input_story_content(kind, path)` (`inputFileLocal`),
  `post_story(...)` — caption via `formattedText` entities, empty
  `inputStoryAreas`, empty album ids, `active_period: 86400`,
  `from_story_full_id: null`, `is_posted_to_chat_page: false`,
  `protect_content: false`. Request-shape tests for photo/video,
  caption entities, privacy, and all fixed fields.
- **Parser (`src/telegram/envelope.rs`).** `CanPostStoryResult` enum for
  all seven pinned-schema outcomes (Ok, PremiumNeeded, BoostNeeded,
  ActiveStoryLimitExceeded, WeeklyLimitExceeded, MonthlyLimitExceeded,
  LiveStoryIsActive), parsed
  into `EnvelopePayload::CanPostStoryResult`, with user-facing messages
  per variant. Parser tests cover every variant.
- **Driver (`src/connect.rs`).** `check_can_post_story()` (posts to
  `Session::my_user_id`, purpose `CheckCanPostStory`);
  `post_story(kind, path, caption, privacy, user_ids)` — validates media
  type, local file existence, and non-empty selected users before
  sending (purpose `PostStory`).
- **State (`src/state.rs`).** `Session::story_post` (`StoryPostState`):
  purpose-gated eligibility / check errors / `StoryPostOutcome`
  (none/posting/succeeded/failed). Reducers: eligibility is stored only
  for the composer check purpose; the `postStory` response captures the
  temporary story id; `updateStoryPostSucceeded`/`updateStoryPostFailed`
  drive the succeeded/failed outcomes and queue the poster's tray
  refresh; raw TDLib errors stop the spinners with sanitized messages.
- **UI (`src/ui/mod.rs`).** The tray always renders (even empty) and
  gains a persistent "+ / Post" tile that opens the composer. The
  composer overlay: path textarea (path entry is the required temporary
  picker — Quill has no native file-picker infrastructure yet), photo
  preview (`img` with contain + fallback; videos get a note tile),
  caption textarea, the 4 privacy buttons, and a contact picker for
  Selected users (reuses the G1 contact-checkbox row + the session
  contact cache). Post validates the path, then `canPostStory`; the
  render tick converts the answer into `postStory` (eligible) or a
  reason line (ineligible / check error). The status line shows local
  validation errors, Checking…, Posting…, ✓ Posted, or ✗ failed —
  honest about the pending state (pending begins when the `postStory`
  *answer lands*, not when it is sent — the Post button stays disabled
  in between). Escape closes the composer before
  the story viewer. Screenshot proof:
  `docs/screenshots/ready-story-composer.png` (`ready-story-composer`
  demo — seeded photo path, caption draft, Close friends, seeded
  `canPostStoryResultOk`).
- **Out of this slice (→ future):** native file picker (path entry is
  the required temporary UI); story areas (sent empty); expiry
  selection (fixed 86400); post-to-chat-page / protect-content (fixed
  false); story editing, covers, or privacy changes after posting;
  posting as a channel / admin-rights nuances; repost
  (`from_story_full_id` fixed null); the archive-list tray; stories in
  the in-app updater (unchanged queue).

## Phase 9.4 — Story composer options (2026-09-28)

- **Rationale:** Phase 9.3 posted with fixed `active_period` 86400, empty
  areas, and both toggles false. This slice wires the remaining
  user-facing `postStory` composer options: expiry selection, link +
  suggested-reaction story areas, "post to chat page", and "protect
  content". (`album_ids` stays fixed `[]` — story albums are a separate
  feature, out of this slice.)
- **Schema (1.8.67, verified in `schema/td_api.tl` — no invented
  constructors/fields):** `postStory chat_id:int53
  content:InputStoryContent areas:inputStoryAreas caption:formattedText
  privacy_settings:StoryPrivacySettings album_ids:vector<int32>
  active_period:int32 from_story_full_id:storyFullId
  is_posted_to_chat_page:Bool protect_content:Bool = Story` (line
  13715); parameter comments — `@active_period Period after which the
  story is moved to archive, in seconds; must be one of 6 * 3600,
  12 * 3600, 86400, or 2 * 86400 for Telegram Premium users, and 86400
  otherwise`; `@is_posted_to_chat_page Pass true to keep the story
  accessible after expiration`; `@protect_content Pass true if the
  content of the story must be protected from forwarding and
  screenshotting`; `inputStoryAreas areas:vector<inputStoryArea> =
  InputStoryAreas` (line 6619); `inputStoryArea
  position:storyAreaPosition type:InputStoryAreaType = InputStoryArea`
  (line 6610); `storyAreaPosition x_percentage:double
  y_percentage:double width_percentage:double
  height_percentage:double rotation_angle:double
  corner_radius_percentage:double = StoryAreaPosition` (line 6530);
  `inputStoryAreaTypeLink url:string = InputStoryAreaType` (line 6597 —
  comment: "An area pointing to a HTTP or tg:// link");
  `inputStoryAreaTypeSuggestedReaction reaction_type:ReactionType
  is_dark:Bool is_flipped:Bool = InputStoryAreaType` (line 6588);
  `reactionTypeEmoji emoji:string = ReactionType` (line 2915).
  Concept-level area search (case-insensitive scan of the area block,
  `td_api.tl:6572`–`td_api.tl:6619`) confirms the full posting surface:
  Location (6572), FoundVenue (6577), PreviousVenue (6582),
  SuggestedReaction (6588), Message (6593), Link (6597), Weather
  (6603), UpgradedGift (6606). Server limits are documented at
  `td_api.tl:6613`–6618 (up to 10 location/venue, up to
  `story_suggested_reaction_area_count_max` reactions, 1 message, up to
  `story_link_area_count_max` links **for Premium users**, 3 weather,
  1 gift).
- **Composer (`src/story_composer.rs`, pure).** `StoryExpiry` (6h / 12h /
  24h / 48h → 21600 / 43200 / 86400 / 172800; non-24h options labeled
  Premium per the schema comment; default 24h), `post_to_chat_page` /
  `protect_content` bools, `link_url` + `reaction_emojis` strings, and
  `areas_json()` building the `inputStoryAreas` block: one link area
  from the URL, one reaction area per space-separated emoji (UI cap 5 —
  the server enforces the real `story_suggested_reaction_area_count_max`).
  Only link + reaction types are implemented: they are the only area
  types expressible with plain text inputs in this path-entry dialog.
  `link_url_error()` enforces the schema's "HTTP or tg:// URL" prefix
  (http://, https://, tg://) locally. Areas are fixed sensible
  placements (`storyAreaPosition` percentages) — the composer has no
  media canvas for drag placement yet (`ponytail:` comment in code).
  Areas are baked into the `postStory` request JSON, so they ride the
  existing post→answer correlation (temp story id →
  `updateStoryPostSucceeded`/`updateStoryPostFailed`) with no extra
  state. Unit tests: expiry mapping, area JSON shapes, URL prefix
  validation.
- **Requests (`src/telegram/requests.rs`).** `post_story` now takes
  `areas`, `active_period`, `is_posted_to_chat_page`,
  `protect_content`; shape tests updated + a new S2 test pins the
  wired fields.
- **Driver (`src/connect.rs`).** `post_story` passes the four options
  through and rejects non-schema `active_period` values
  (`InvalidRequest` — the schema says "must be one of").
- **UI (`src/ui/mod.rs`).** Composer overlay gains: "Expires after" (4
  checkbox buttons), "Link sticker URL" + "Reaction stickers
  (emoji, space-separated)" textareas (the link field labels itself
  Premium), and the two toggles under "Options". Post syncs the area
  inputs into composer state and surfaces a bad link URL as a local
  error before the `canPostStory` check. Screenshot proof:
  `docs/screenshots/ready-story-composer.png` re-captured with the
  options seeded (48h expiry, both toggles on, link + two reactions).
- **`toggleStoryIsPostedToChatPage` — deferred, not wired.**
  `toggleStoryIsPostedToChatPage story_poster_chat_id:int53
  story_id:int32 is_posted_to_chat_page:Bool = Ok` exists (line
  13749), but it applies to already-posted stories, which lives in the
  story viewer / posted-story state — not the composer path. The
  composer covers new posts via `postStory`'s own flag; the viewer
  toggle waits for posted-story management (a future slice).
- **Out of this slice (→ future):** location / venue areas (no
  location/venue picker), message areas (no message picker), weather
  areas (no live weather data), upgraded-gift areas (no gift
  inventory); `editStory` (13732), `editStoryCover` (13738),
  `setStoryPrivacySettings` (13743); `toggleStoryIsPostedToChatPage`
  for already-posted stories (13749); posting as a channel /
  supergroup (`getChatsToPostStories` 13698, admin story rights);
  repost (`from_story_full_id` fixed null).

## Phase 9.5 — Story viewers list, report, stealth mode (2026-09-28)

- **Rationale:** complete the story viewer loop (Phase 9.1–9.4): see who
  viewed an own story, report someone else's story through the
  multi-step server flow, and activate story stealth mode.
- **Schema (1.8.67, pinned commit `d1085f9`, concept-level search on all
  three levels — never a single-name grep):**
  - (1) `schema/td_api.tl`: `getStoryInteractions story_id:int32
    query:string only_contacts:Bool prefer_forwards:Bool
    prefer_with_reaction:Bool offset:string limit:int32 =
    StoryInteractions` (:13819) — comment: "Returns interactions with a
    story. The method can be called only for stories posted on behalf of
    the current user". There is **no `getStoryViewers` constructor** (no
    `getStoryViewer*` / `storyViewer*` method anywhere in the schema;
    the viewers concept surfaces only as `getStoryInteractions` and
    `getChatStoryInteractions` — the latter is chat-admin-only, :13828).
    `story.can_get_interactions` (:6732): "True, if interactions with the
    story can be received through getStoryInteractions".
    `storyInteractions total_count:int32 total_forward_count:int32
    total_reaction_count:int32 interactions:vector<storyInteraction>
    next_offset:string = StoryInteractions` (:6811);
    `storyInteraction actor_id:MessageSender interaction_date:int32
    block_list:BlockList type:StoryInteractionType = StoryInteraction`
    (:6803) with `storyInteractionTypeView/Forward/Repost` (:6789–6795).
    `reportStory story_poster_chat_id:int53 story_id:int32 option_id:bytes
    text:string = ReportStoryResult` (:13835) — comment: "Reports a
    story to the Telegram moderators"; option_id/text empty for the
    initial request. `ReportStoryResult` (:9221–9231):
    `reportStoryResultOk`; `reportStoryResultOptionRequired title:string
    options:vector<reportOption>` ("The user must choose an option …
    and repeat request"); `reportStoryResultTextRequired option_id:bytes
    is_optional:Bool` ("The user must add additional text details").
    `activateStoryStealthMode = Ok` (:13839) — comment: "Activates
    stealth mode for stories, which hides all views of stories from the
    current user in the last \"story_stealth_mode_past_period\" seconds
    and for the next \"story_stealth_mode_future_period\" seconds; for
    Telegram Premium users only". `updateStoryStealthMode
    active_until_date:int32 cooldown_until_date:int32 = Update`
    (:10919): "Story stealth mode settings have changed … 0 if it is
    disabled / 0 if there is no active cooldown".
  - (2) raw `telegram_api.tl` (@d1085f9):
    `stories.getStoryViewsList#7ed23c57` (:5382) — the raw method behind
    `getStoryInteractions`; `stories.report#19d8eb45 peer:InputPeer
    id:Vector<int> option:bytes message:string = ReportResult` (:5389);
    `stories.activateStealthMode#57bbd166` (:5391);
    `updateStoriesStealthMode#2c084dc1` (:918);
    `storiesStealthMode#712e27fd` (:3029) with the two date fields.
  - (3) TDLib source (@d1085f9): `StoryManager::get_story_interactions`
    (`td/telegram/StoryManager.h:389`) → `GetStoryViewsListQuery`
    (`StoryManager.cpp:484`) sending
    `telegram_api::stories_getStoryViewsList`;
    `StoryManager::report_story` (`StoryManager.h:404`) →
    `ReportStoryQuery` (`StoryManager.cpp:1362`) mapping
    `reportResultReported`→Ok, `reportResultChooseOption`→OptionRequired,
    `reportResultAddComment`→TextRequired — **including the edge case
    that an empty option list maps to Ok** (:1414–1416), which the
    reducer mirrors; `StoryManager::activate_stealth_mode`
    (`StoryManager.h:448`), `on_update_story_stealth_mode` (:468),
    `get_update_story_stealth_mode` (:797) emitting the td_api update.
  - Telegram X (local source, `~/workspace/telegram-x`): stores and
    broadcasts the two stealth timestamps (`Tdlib.java:472`,
    `:8630–8635`; `StoryListener.java:13`; `TdlibListeners.java:1303`)
    but has **no** story-viewers UI and **no** stealth-activation UI
    (no `StoryViews` / `activateStoryStealthMode` references anywhere in
    the app source) — the schema is the authority for those surfaces.
- **Parser (`src/telegram/envelope.rs`).** `ReportStoryResult::{Ok,
  OptionRequired, TextRequired}`, `StoryInteractionsView` /
  `StoryInteractionView` / `StoryInteractionKind::{View, Forward,
  Repost}` (unknown interaction types are skipped, per the lenient-parse
  convention), `EnvelopePayload::{ReportStoryResult, StoryInteractions,
  UpdateStoryStealthMode}`; parser unit tests for all variants.
- **Requests (`src/telegram/requests.rs`).**
  `get_story_interactions` (defaults `query:""`,
  `only_contacts:false`, `prefer_forwards:false`,
  `prefer_with_reaction:false`, `limit:50`), `report_story`,
  `activate_story_stealth_mode`; shape test
  `s4_story_requests_match_1_8_67`.
- **Reducer (`src/state.rs`).** `RequestPurpose::{GetStoryInteractions,
  ReportStory, ActivateStoryStealthMode}`; `StoryViewersState`
  (per-story rows + `next_offset` pagination, stale-page rejection,
  error string); `StoryReportFlow` /
  `StoryReportStage::{Checking, PickOption, Sending, TextRequired,
  Reported, Failed}` (stale-story answers dropped; empty option list →
  `Reported`, mirroring TDLib); `StoryStealthMode` with
  `is_active`/`is_cooling_down` predicates; `updateStoryStealthMode`
  applies to state, refused activations surface as
  `Session::story_stealth_error`. Reducer unit tests for pagination /
  stale rejection / error, the report stage transitions, and stealth
  predicates.
- **Driver (`src/connect.rs`).** `get_story_interactions` — gated on
  the cached story's `can_get_interactions`, deduped per story while in
  flight (`Ok(None)`); `report_story` — gated on the cached story, own
  (deletable) stories rejected; `activate_story_stealth_mode` — deduped
  while in flight. Driver tests for the gates, dedupe, and request
  shapes.
- **UI (`src/ui/mod.rs`).** Viewer action row gains Viewers (own
  stories with `can_get_interactions`), Report (other people's
  stories), and Stealth (label reflects pushed
  `updateStoryStealthMode` state: "Stealth" / "Stealth on" / "Stealth
  cooling down"; the schema exposes no getter, so the button never
  claims to know more than pushed state). Viewers panel: actor names
  from cached users/chats (raw id fallback — never invented),
  reaction-emoji or kind label + relative time, "Load more" while
  `next_offset` is non-empty, honest loading/error/empty states. Report
  UI mirrors the flow stages: spinner, the server-provided reason
  picker, the details field (required or skippable per `is_optional`),
  success/failure. Demo says so honestly ("demo — … run with live
  TDLib"). Screenshot proof:
  `docs/screenshots/ready-story-viewers.png` (new `ReadyStoryViewers`
  demo: the `ReadyStoryPost` fixture plus a `storyInteractions` page
  injected through the real reducer path).
- **Out of this slice (→ future):** `getChatStoryInteractions`
  (:13828, chat-admin viewers — needs admin-state plumbing);
  viewers-list search (`query`), `only_contacts` filter, and
  `prefer_forwards`/`prefer_with_reaction` sort toggles (request
  builder takes them; the UI sends defaults); `getStoryPublicForwards`
  (:13847); `getStoryStatistics` (:15773); story albums
  (`getChatStoryAlbums` :13850, `getStoryAlbumStories` :13857);
  stealth-mode periods display (the raw `storiesStealthMode` periods
  come from `getAllStories`, not yet parsed — the button shows
  active/cooldown only).

## Parity slice — Forum-topic posting (2026-09-26)

- **Rationale:** Phase 5.1 made forum topics read-only (composer hidden
  with a read-only note). This slice completes the loop: the composer is
  live inside an open topic and sends route to the selected topic; it is
  hidden/disabled for closed topics and for chats where the user may not
  post.
- **Schema (1.8.67, verified in `schema/td_api.tl` — no invented
  constructors/fields):**
  - `messageTopicForum forum_topic_id:int32 = MessageTopic` (line 3004);
    `message.topic_id:MessageTopic` (line 3165).
  - `sendMessage chat_id:int53 topic_id:MessageTopic …` (line 12200) —
    the dedicated `topic_id` field carries the send; **not** a reply-to
    field and **not** an invented `message_thread_id` (the schema has
    none; guarded by `send_message_uses_topic_id_field_in_schema`).
  - `sendMessageAlbum … topic_id:MessageTopic …` (line 12209);
    `setChatDraftMessage … topic_id:MessageTopic …` (line 13493);
    `sendChatAction chat_id:int53 topic_id:MessageTopic …` (line 13191).
  - `chatPermissions.can_send_basic_messages` (line 1070);
    `chat.permissions` (line 3627);
    `updateChatPermissions chat_id:int53 permissions:chatPermissions =
    Update` (line 10500).
  - Management constructors exist but are not used here:
    `createForumTopic` (line 12665), `editForumTopic` (line 12674),
    `toggleForumTopicIsClosed` (line 12713),
    `toggleForumTopicIsPinned` (line 12725), `deleteForumTopic`
    (line 12736).
- **Parser (`src/telegram/envelope.rs`).** `ParsedMessage.topic_id:
  Option<i32>` — `parse_message_topic` keeps only
  `messageTopicForum.forum_topic_id`; other `MessageTopic` variants and
  null map to `None`. `UpdateNewChat` gains `can_send_basic_messages`
  (parsed from `chat.permissions`, defaulting to `true` when the block
  is absent — lenient like other permission reads); new
  `EnvelopePayload::UpdateChatPermissions`.
- **Reducer (`src/state.rs`).** `ChatSummary.can_send_basic_messages`
  (default `true` for placeholder chats), refreshed by `updateNewChat`
  and `updateChatPermissions`. `TopicHistory::{upsert, replace_id}`;
  `upsert_message` stores the message in the chat's main history as
  before and additionally in an *already-loaded* topic history when
  `topic_id` is a forum topic (never creates a history for an unloaded
  topic — paging stays fetch-owned).
  `updateMessageSendSucceeded` / `updateMessageSendFailed` replace
  pending rows in both histories, so outgoing
  pending/success/failure rows show in the topic view.
- **Requests (`src/telegram/requests.rs`).** Private
  `message_topic_value(Option<i32>)` renders
  `{"@type":"messageTopicForum","forum_topic_id":N}` or null; threaded
  through text, photo, document, video, video note, media album, voice
  note, poll, sticker, and saved-animation sends. Non-topic callers pass
  `None` (null preserved). `StickerSend` / `AnimationSend` / `PollSend`
  gain `topic_id`; voice-note params bundled into new `VoiceNoteSend`
  (keeps the constructor under clippy's argument limit; `VideoNoteSend`
  already existed for round video notes). Story replies stay null-topic
  by design. `sendChatAction` stays null-topic (typing indicator, not
  message posting).
- **Driver (`src/connect.rs`).** `send_topic(chat_id)` returns the
  selected topic only when `chat_id` is the open chat; the driver is the
  source of truth and overwrites the UI request structs' `topic_id:
  None`. `topic_send_is_closed(chat_id)` rejects sends into closed
  topics (guards a stale-snapshot race); closed checks on
  text/media/album/voice/poll/sticker/GIF paths.
- **UI (`src/ui/mod.rs`).** The composer is visible in an open forum
  topic when the chat can post, `can_send_basic_messages` is true, and
  the topic is not closed. Hidden-composer notes: closed — "This topic
  is closed — new messages are disabled."; no permission — "You don't
  have permission to post in this topic."; topic info not yet loaded —
  "Loading topic…". The Phase 5.1 read-only note is removed. The `‹
  Topics` strip, topic selection, history, and paging are untouched.
- **Screenshot:** `docs/screenshots/ready-topic-post.png` —
  `quill --screenshot-demo ready-topic-post` opens forum chat 16,
  selects the General topic, injects a two-message topic history
  carrying `messageTopicForum`, and pre-fills the composer with
  "Posting into the General topic…".
- **Out of this slice (→ future):** per-topic drafts (draft state is
  chat-keyed; clean support needs topic-keyed local + remote drafts via
  `setChatDraftMessage.topic_id`); topic management UI
  (create/edit/close/pin/delete — constructors verified above, unused);
  threading `sendChatAction` typing into the open topic; General-topic
  main-history special-casing; per-topic notification/unread behavior;
  topic-list pagination beyond 100; richer topic icons.

## Parity slice — In-viewer video playback + photo zoom/pan (2026-09-26)

- **Rationale:** The media viewer (slice 4) showed video thumbnails and
  launched an external ffplay window for playback — not "in-viewer".
  This slice renders decoded video frames inside the GPUI viewer, adds
  photo zoom/pan, left/right viewer navigation, and opens the viewer
  from album mosaics.
- **Schema (1.8.67, verified in `schema/td_api.tl` — no invented
  constructors/fields):**
  - `file` (line 311); `thumbnail` (line 375); `video` (line 606);
    `messageVideo` (line 5188).
  - Reused `messageVideo.video`, `messageVideo.start_timestamp`,
    `video.thumbnail`, `video.duration`, `video.mime_type`, and the
    existing `downloadFile` request. No new TDLib constructors.
- **Model (`src/ui/mod.rs`).** `MediaViewerItem` gains `play_file_id:
  Option<FileId>`, `duration_secs: Option<i32>`, `mime_type:
  Option<String>`, `start_timestamp: Option<i32>`. Photos set the
  playback-specific fields to `None`. Video tests verify propagation
  of file ID, duration, MIME type, and start timestamp.
- **Playback (`src/video.rs`, `src/ui/mod.rs`).** Full-clip PNG
  extraction via ffmpeg at viewer width (720 px):
  - Normal rate 8 fps; cache capped at 600 frames; clips longer than
    75 s use an adaptive lower fps (minimum 1 fps).
  - Viewer clock maps elapsed time to the displayed frame; GPUI
    refresh tick is 125 ms. The frame index is clamped, not wrapped:
    frames cover `(duration − start_timestamp)`, so the tail of the
    clock holds the last frame instead of replaying early frames.
  - `ffplay -nodisp -autoexit` provides audio only; if ffplay/audio is
    unavailable, frame playback continues silently.
  - Extraction runs async in normal use (thumbnail + "Loading video…"
    until ready). Viewer frames use a separate cache under
    `/tmp/quill-viewer-frames/{file_id}`, display-allowlisted. Stale
    cache dirs from previous runs are swept at startup
    (`sweep_stale_viewer_frame_caches`).
  - Closing or stepping the viewer stops audio, kills the running
    ffmpeg extraction (published child handle), clears frames, and
    removes the cache. A shared `AtomicBool` cancellation flag closes
    the race where the viewer closes before ffmpeg publishes its child:
    the worker checks it before spawning and after publishing, killing
    its own just-spawned child instead of orphaning it. Completions from
    killed or superseded runs are dropped by an extraction epoch —
    silently, with no error note.
  - Every start path (`maybe_autoplay_viewer_video`, the
    download-resume in `resume_pending_viewer_video`, and the
    never-started Play toggle) routes through the pure
    `decide_viewer_video_start` (`src/media_viewer.rs`, unit-tested): a
    local clip without cached frames always goes through extraction —
    never straight to playback with an empty frame cache (previously
    the resume and toggle paths played the thumbnail forever).
  - **Rendering mechanism (corrected 2026-09-26 review fix):** the
    earlier diagnosis ("GPUI `img` caches by element ID") was wrong —
    the pinned `gpui-pre-0.3.5` `src/elements/img.rs` keys its asset
    cache by resource (path), and the real failure was that path
    sources resolve through `window.use_asset::<ImgResourceLoader>`,
    which loads asynchronously and returns `None` until the fs-read +
    PNG-decode completes. At 125 ms path churn each frame needed its
    own async round trip while the tick fired independently →
    flicker/lag. The fix: frames are pre-decoded (on the background
    thread) into `Arc<RenderImage>` handles passed as
    `ImageSource::Render`, whose `use_data` returns
    `Some(Ok(data.to_owned()))` synchronously — every tick renders the
    current frame immediately. Genuine animated in-viewer playback, no
    fixed-frame special case (removed).
- **Zoom/pan (`src/ui/mod.rs`).** `ViewerZoom`: 1×–8×, factor 1.15,
  center-preserving zoom, clamped drag pan, reset/fit. Controls: mouse
  wheel zoom, drag pan while zoomed, double-click reset, `+`/`-`/Reset
  buttons, `0`/`=`/`-` keyboard shortcuts. Key handlers only stop
  propagation while the viewer is open. Opening or stepping resets
  zoom and stops existing playback.
- **Navigation/albums.** Left/Right arrows step through viewer items;
  album photo/video tiles open the media viewer; clicking a
  non-openable message no longer falls back to unrelated item zero.
- **Overlay styling.** GPUI Kit Ghost buttons rendered effectively
  black on the dark viewer overlay; added explicit white text to
  viewer zoom, Play/Pause, Download, Prev, Next controls (and story
  viewer Prev/Next).
- **Screenshot:** `docs/screenshots/ready-video-playback.png` —
  `quill --screenshot-demo ready-video-playback` opens the media
  viewer on a 12 s demo clip (message 204, file 96,
  `docs/screenshots/fixtures/demo-clip-12s.mp4`, ffmpeg `testsrc`
  320×180 30 fps), extracts + decodes 96 frames synchronously, seeks
  to 5 s, and shows the genuinely animated frame in-viewer (captured
  mid-animation — the clock keeps ticking and each 125 ms refresh
  renders the frame for the current clock position) with "❚❚ Pause",
  elapsed / 0:12, zoom controls, caption "Demo clip", and Prev/Next
  navigation. The ffplay subprocess is skipped in the demo.
- **Out of this slice (→ future):** streaming/HLS; storyboard
  scrubbing; alternative qualities; opening documents/GIFs/stickers/
  audio in the viewer; mpv IPC.

## Parity slice — Notification sounds + notification settings UI (2026-09-26)

- **Rationale:** Phase 8.1 explicitly deferred "notification sounds" and
  "a settings UI for `hide_notification_previews`". This slice closes
  both: a sound now plays alongside the existing OS-toast pipeline, and
  the header Mute panel becomes a full Notifications panel with a
  scope-defaults dialog.
- **Schema (1.8.67, verified in `schema/td_api.tl` — no invented
  constructors/fields):**
  `notificationSettingsScopePrivateChats` (:3337),
  `notificationSettingsScopeGroupChats` (:3340),
  `notificationSettingsScopeChannelChats` (:3343);
  `chatNotificationSettings … use_default_sound:Bool sound_id:int64 …
  = ChatNotificationSettings` (:3363; `@sound_id` comment: "Identifier
  of the notification sound to be played for messages; 0 if sound is
  disabled");
  `scopeNotificationSettings … sound_id:int64 … = ScopeNotificationSettings`
  (:3375; `@sound_id` comment: "0 if sound is disabled; pass -1 to use
  the app-dependent default sound");
  `notificationSound id:int64 duration:int32 date:int32 title:string
  data:string sound:file = NotificationSound` (:8857);
  `notificationSounds` (:8860; `@description` on
  `getSavedNotificationSounds`: "If a sound isn't in the list, then
  default sound needs to be used" — line 13646);
  `fileTypeNotificationSound` (:9716);
  `updateChatNotificationSettings` (:10552),
  `updateScopeNotificationSettings` (:10668),
  `updateSavedNotificationSounds` (:10947);
  `setChatNotificationSettings` (:13498),
  `getSavedNotificationSound` (:13644),
  `getSavedNotificationSounds` (:13647),
  `addSavedNotificationSound` (:13650),
  `removeSavedNotificationSound` (:13653),
  `getScopeNotificationSettings` (:13662),
  `setScopeNotificationSettings` (:13665).
- **Decision — sound resolution (`src/notify.rs::decide_notification_sound`,
  pure, unit-tested).** An incoming-message notification plays a sound
  iff: the app/window is not focused (no sound for the open chat in the
  foreground — same suppression as toasts); the chat is not muted;
  `sound_id != 0`. Resolution: chat `use_default_sound` (or an unknown
  scope) → the scope default; scope `sound_id == -1` → the app default;
  scope `sound_id == 0` → silent; a positive id → that saved sound. A
  saved id missing from the list falls back to the app default, per the
  `getSavedNotificationSounds` comment. Coalesced same-chat bursts play
  the first message's sound once.
- **App-default tone.** TDLib provides no app-default audio file, so
  Quill synthesizes one: Linux plays a 660 Hz / 0.35 s lavfi tone via
  `ffplay -nodisp -autoexit`; macOS uses `osascript -e beep`. Custom
  saved sounds play their downloaded file (ffplay on Linux, afplay on
  macOS). All players spawn without a shell; a missing/failed player
  fails silently. Sounds are capped at 2 concurrent player threads
  (`quill-sound`); toast threads stay capped at 8.
- **Model (`src/state.rs`).** The session caches the saved-sound list
  (`saved_notification_sounds` + loaded/stale flags), the three scope
  settings (`scope_notification_settings` + loading set), the sound
  file-id → local-path mapping, and pending custom-sound downloads that
  queue a playback when `updateFile` completes. `getSavedNotificationSounds`
  and all three `getScopeNotificationSettings` are fetched once after
  Ready; `updateSavedNotificationSounds` marks the list stale for
  refetch on the next ingest. `getScopeNotificationSettings` responses
  carry no scope field — the scope is correlated through
  `PendingRequest::scope`, stamped by `Session::request_for_scope`.
- **Requests (`src/connect.rs`).** `get_saved_notification_sounds`,
  `get_scope_notification_settings`, `set_scope_notification_settings`,
  plus per-chat `set_chat_sound` and `set_chat_show_preview`. TDLib has
  no partial settings setter, so per-chat edits resend the full
  `chatNotificationSettings`, preserving every untouched field
  (unit-tested).
- **UI (`src/ui/mod.rs`).** The header Mute panel is now a
  "Notifications" panel: a status line (mute · sound · previews), the
  existing mute presets, a message-preview toggle (writes
  `use_default_show_preview=false` + explicit `show_preview`), a sound
  picker (Default / None / each saved sound with title + duration and a
  ▶ preview button that plays the sound immediately — downloading it
  first when needed), and a "Defaults for all chats…" link opening the
  scope-defaults dialog. The dialog shows private chats / groups /
  channels sections, each with mute presets, a preview toggle, and an
  expandable default-sound picker (scope `-1` = default, `0` = none,
  positive id = saved sound). Live edits go through
  `setScopeNotificationSettings`; screenshot-demo edits apply locally
  through the same reducer paths.
- **Screenshot:** `docs/screenshots/ready-notification-sound.png` —
  `quill --screenshot-demo ready-notification-sound` (taller window via
  `QUILL_DEMO_WINDOW_SIZE=1200x1150` so the expanded panel, history,
  and composer all fit without scrolling): Demo chat A on a custom
  saved sound ("Ding"), the Notifications panel open with the picker
  expanded (Default / None / ✓ Ding (2s) / Chime (3s), ▶ preview
  buttons, "Defaults for all chats…" link).
- **Behavior — effective mute and preview (review fix).** The toast and
  sound decisions no longer gate on the chat's exception mute alone:
  `Session::effective_muted` also applies the scope's `mute_for` when the
  chat keeps `use_default_mute_for`, and `Session::effective_preview_allowed`
  applies the scope's `show_preview` when the chat keeps
  `use_default_show_preview` (td_api.tl :3348/:3350 — "the value for the
  relevant type of chat ... is used instead of"). While the scope fetch is
  still in flight the schema defaults apply. Previously, setting "Forever"
  under Groups changed server state but Quill kept showing toasts and
  playing sounds for default-setting chats; now both are suppressed.
- **Behavior — failed scope fetch retries (review fix).** A TDLib error on
  `getScopeNotificationSettings` now drops the scope from
  `scope_settings_loading`, so the next ingest / "Defaults for all chats…"
  open retries the fetch instead of skipping the scope forever. The
  scope-defaults apply buttons also refuse to send while a scope's settings
  are still unfetched (status note "defaults still loading…") instead of
  sending schema-defaults as current state.
- **Out of this slice (→ future):** notification exceptions beyond
  per-chat mute (per-mention unmute, `disable_mention_notifications`,
  `chat.default_disable_notification`); in-app banner previews; DND
  scheduling; badge counts; grouping; story-sound configuration /
  upload / removal (`addSavedNotificationSound` /
  `removeSavedNotificationSound` / scope `story_sound_id` — schema-clean
  but not surfaced); per-chat story settings UI
  (`use_default_mute_stories` etc.); a global
  `hide_notification_previews` settings UI (the per-chat preview
  toggle exists).

## Phase A — core-chat gap audit + slow mode (A1, 2026-09-26)

### Phase A audit: top-10 core-chat gaps, ranked by impact

Ranked against what official clients expose for everyday 1:1 / group
messaging. Schema citations are TDLib 1.8.67 (`schema/td_api.tl`); sizes
are rough (S < 1 day, M = days, L = week+).

1. **Search filtering / pagination (M).** `searchChatMessages`
   (:11864) takes a sender and `SearchMessagesFilter`, but no date range;
   `searchMessages` (:11877) adds media + chat-type filters and
   `min_date`/`max_date`. Media/filter constructors :6275–6329. Quill
   sends null sender/filter and has no date/media/sender UI and no
   complete pagination.
2. **Scheduled messages (M).** `messageSchedulingState*` (:5902–5909);
   `message.scheduling_state` (:3165); `messageSendOptions.scheduling_state`
   (:5934); `getChatScheduledMessages` (:12000). Quill has no
   scheduled-message handling or UI at all.
3. **Draft sync fidelity (S/M).** `draftMessage` (:3433),
   `draftMessageContentText` (:3404), plus rich / input-rich / video-note /
   voice-note draft forms (:3407–3424). Quill stores only plain text +
   reply message id — no rich text, voice, effects, link-preview, or
   suggested-post fidelity.
4. **Pinned-message navigation / list (S).** `getChatPinnedMessage`
   (:11545), `unpinAllChatMessages` (:13565). Quill has a single pinned
   banner plus pin/unpin, but no multi-pin navigation/list and no
   unpin-all or pin options.
5. **Edit/delete edge cases (S/M).** `editMessageCaption` (:12338) —
   Quill already edits text/captions; `deleteMessages` (:12282) — Quill
   always sends `revoke: true`. Missing: richer media editing/options,
   live property gating, and a nuanced delete-for-me vs delete-for-
   everyone UI.
6. **Auto-delete timers (S).** `messageAutoDeleteTime` (:9057);
   `chat.message_auto_delete_time` (:3627); `setChatMessageAutoDeleteTime`
   (:13454). No Quill support.
7. **Slow mode (S — this slice).** See below.
8. **Themes / wallpaper (M).** `chat.background` / `chat.theme` (:3627);
   `chatThemeEmoji` / `chatThemeGift` (:8502/:8505); `setChatTheme`
   (:13487). No Quill support.
9. **Message effects (S).** Effect constructors (:3016–3028);
   `message.effect_id` (:3165); `messageSendOptions.effect_id` (:5934);
   `getMessageEffect` (:12862). No Quill support.
10. **Chat export (M/L).** No dedicated chat-export constructor was found
    in schema 1.8.67 (grep 2026-09-26). Telegram Desktop's export is a
    client-side feature (render history to files), not a TDLib call — so
    this is implementable but entirely client work; Quill has none.

### A1 implementation: slow-mode enforcement

- **Parse.** `supergroupFullInfo.slow_mode_delay` (:2758) and
  `slow_mode_delay_expires_in` (:2759), plus `my_boost_count` /
  `unrestrict_boost_count` (:2779–2780), are kept in
  `SupergroupFullInfoData` (with `fetched_at_ms`). Own
  `chatMemberStatus*` from `supergroup.status` (:2746) is kept per
  supergroup for the bypass check; `rights.can_restrict_members` from
  own `chatMemberStatusAdministrator` (:2500/:1092) gates the admin
  control — `setChatSlowModeDelay` requires it (:13551).
- **Gate.** `Session::slow_mode_wait_secs(chat_id, now_ms)` is pure in
  the clock: non-channel supergroup + positive delay + member (or
  unknown) status without boost exemption + positive locally-decayed
  `slow_mode_delay_expires_in`. Creators/administrators bypass (tdesktop
  behavior; :2758 comment); `my_boost_count >= unrestrict_boost_count >
  0` bypasses (:2780 comment); `unrestrict_boost_count` 0 = unspecified.
  Unknown own status is conservatively gated. Ceil rounding for the
  countdown.
- **Why local decay.** The schema (:2759) warns no
  `updateSupergroupFullInfo` fires when only the expiry changes while
  old and new are non-zero — so the countdown decays locally against
  `fetched_at_ms`, and every *blocked* send re-fetches
  `getSupergroupFullInfo` for a fresh server value
  (`refresh_supergroup_full_info`, deduped in-flight like the existing
  header fetch).
- **Surfaces gated (all through one `slow_mode_blocked` helper):**
  composer text/attachment/album submit (live + demo), GIF picks,
  sticker picks, voice-note sends, poll dialog submits, and forwards
  (forwarding sends messages). Voice-note gate runs before the capture
  is consumed so a blocked recording survives. Story replies are not
  gated (different TDLib mechanism, not chat messages). Edits are not
  gated (slow mode restricts new sends).
- **UI.** Composer banner "Slow mode · wait Ns" (ticking task mirrors
  the voice-tick pattern, exits when the gate lifts); blocked sends set
  the status note "Slow mode: wait Ns before sending". Group info panel
  shows a Slow mode row (Off/5s/10s/30s/1m/5m/15m/1h — exactly the
  allowed values :13551) to creators / admins with
  `can_restrict_members`; `set_slow_mode_delay` re-checks the right
  before firing (defense in depth).
- **Screenshot:** `docs/screenshots/ready-slow-mode.png` —
  `quill --screenshot-demo ready-slow-mode`: "Slow-mode demo group" (id
  17) open as a plain member, 30s delay with ~25s countdown, composer
  banner visible.
- **Tests.** Envelope parsing (slow-mode + boost fields on
  `supergroupFullInfo` / `updateSupergroupFullInfo`; own status +
  `can_restrict_members` on `updateSupergroup` / `supergroup`,
  incl. true/false/absent rights); `setChatSlowModeDelay` request shape;
  reducer replay tests for wait/countdown rounding/expiry,
  creator+admin bypass, boost bypass (incl. `unrestrict_boost_count` 0),
  channel / zero-delay / missing-full-info ungated, unknown status
  gated, restrict-right tracking.
- **Drive-by fix (pre-existing, required to build the UI).** Main
  (469bb6b) does not compile with `--features ui --bins`:
  `Session::effective_muted` / `effective_preview_allowed` are
  `pub(crate)` in the lib but called from the `ui` binary crate
  (broken by PR #54). Widened to `pub`. (Note: default-feature clippy
  `-D warnings` was already dirty on main — dead code and
  `unnecessary_unwrap` lints in `ui/` — so the enforced gates stay the
  `--no-default-features` ones.)
- **Out of this slice (→ future):** gaps 1–6, 8–10 above; slow-mode in
  basic groups (schema limits `setChatSlowModeDelay` to supergroups);
  boost-gated send UI hints; `slow_mode_delay_expires_in` refresh while
  the composer just sits open (currently refreshes on blocked sends
  and panel open).

## Phase B1 — Secret chat lifecycle (2026-09-26)

- **Rationale.** Secret chats were gated out as unsupported since Phase 0
  ("No secret chats" in DECISIONS.md:12). They are the last chat-type
  gate in the app; enabling them closes a core-chat parity gap. TDLib
  owns the E2E cryptography — Quill only enables secret chats
  (`setTdlibParameters.use_secret_chats`, schema 1.8.67 :11298/:11305)
  and models the lifecycle + UI.
- **Schema (1.8.67, not invented):** `secretChatStatePending` (:2798 —
  "waiting for the other user to get online"), `secretChatStateReady`
  (:2801), `secretChatStateClosed` (:2804); `secretChat` (:2816)
  `id:int32 user_id:int53 state:SecretChatState is_outbound:Bool
  key_hash:bytes layer:int32`; `chatTypeSecret` (:3448);
  `updateSecretChat` (:10741) — the comment at :10740 guarantees it
  arrives *before* the secret-chat identifier is returned;
  `createNewSecretChat` (:13340), `getSecretChat` (:11516, "an offline
  method"), `closeSecretChat` (:15242).
- **Parse.** `parse_secret_chat` keeps the full record: all three states
  plus `SecretChatState::Unknown` degradation for future constructors,
  base64-decoded `key_hash` (36 bytes), `is_outbound`, `layer`. The
  full `ParsedSecretChat` is cached per `secret_chat_id` at the session
  level because `updateSecretChat` arrives before `updateNewChat`
  (:10740) — `updateNewChat` hydrates the chat summary from the cache,
  or queues an offline `getSecretChat` (driver drains the queue in
  `ingest`, deduped in-flight) when the state was never seen. `key_hash`
  is retained for the B2 key-verification UI.
- **Sending.** Secret chats are ordinary chat ids at the send layer, so
  the ordinary `sendMessage` path is reused unchanged — no secret-chat
  branch in `send_snapshot`. `ChatSummary::can_post()` gates on state:
  only `Ready` posts; Pending/Closed hide the composer, which is
  replaced by a note: "🔒 Waiting for {name} to come online…" (:2798
  semantics) or "🔒 Secret chat closed". The driver rejects stale
  snapshots into non-Ready chats with `InvalidRequest` (defense in
  depth), same as closed forum topics.
- **UI.** User profile panel: **Start secret chat** for regular users
  (not bots, not self) → `createNewSecretChat`; the new chat opens when
  its `updateNewChat` arrives. Chat list: blue 🔒 badge on secret chats
  (matches the Muted-badge pattern). Open-chat header: **Close secret
  chat** → confirm banner (closing is permanent) → `closeSecretChat`;
  the resulting `secretChatStateClosed` update hides the composer.
- **Bot note (client decision, not a schema claim).** The "Start secret
  chat" entry point and `start_secret_chat` driver validation reject
  bot users — a UX decision, not a verified schema prohibition; the
  schema line examined for `createNewSecretChat` carries no bot comment.
- **Forwarding.** No blanket claim: `messageProperties.can_be_copied_to_secret_chat`
  (:6226) governs forwardability per message; Quill's forward flow does
  not yet fetch/use that property (future work).
- **Screenshot:** `docs/screenshots/ready-secret-chat.png` —
  `quill --screenshot-demo ready-secret-chat`: Ready secret chat (id 41)
  with Zed open, 🔒 badge in the chat list, **Close secret chat** in
  the header, three injected E2E messages, composer live with typed
  text.
- **Tests.** Request shapes (`createNewSecretChat` / `getSecretChat` /
  `closeSecretChat` + `use_secret_chats: true` in TDLib parameters);
  envelope parsing of all three states + full `secretChat` fields +
  base64 `key_hash` + unknown-state degradation; replay tests for
  state-before-chat hydration, unknown-state `getSecretChat` queueing
  (deduped), Pending → Ready → Closed transitions, per-state
  `can_post()`, and a driver test proving the ordinary `sendMessage`
  request is emitted into a Ready secret chat while a Pending chat is
  rejected. The old `replay_non_bot_chat_gating_unchanged` assertion
  (secret chats unsupported) was replaced with the lifecycle version.
- **Out of this slice (→ future):** B2 key-verification UI (fingerprint
  display from retained `key_hash`); B3 self-destructing messages
  (`messageSelfDestructType`); secret-chat-specific notification
  behavior; honoring `can_be_copied_to_secret_chat` in the forward
  flow; secret-chat file-download UI differences.

## Phase B2 — Secret chat key verification UI (2026-09-26)

- **Rationale.** A secret chat's security hinges on the two devices
  agreeing on the E2E key; official clients let the user compare a
  visual fingerprint of the key. B1 retained the raw `key_hash` exactly
  for this slice.
- **Schema (1.8.67, not invented):** the `secretChat.key_hash` comment
  (:2812–2813, verified verbatim) — "36 little-endian bytes, which must
  be split into groups of 2 bits, each denoting a pixel of one of 4
  colors FFFFFF, D5E6F3, 2D5775, and 2F99C9"; "The pixels must be used
  to make a 12x12 square image filled from left to right, top to
  bottom." Constructor `secretChat` at :2816. No new constructors were
  needed — the whole slice is derived from the retained hash.
- **Mapping (pure, deterministic).** New `src/key_fingerprint.rs`:
  `key_hash_pixels(&[u8]) -> Option<[u8; 144]>` — `None` unless the
  slice is exactly 36 bytes; pixel `4i+j` = `(byte[i] >> (2*j)) & 3`
  (little-endian 2-bit groups, least-significant pair first),
  row-major left-to-right, top-to-bottom. Color table
  `[0xFFFFFF, 0xD5E6F3, 0x2D5775, 0x2F99C9]` in schema-comment order.
- **UI.** The secret chat partner's info panel (opened by clicking the
  secret chat's header title — `Session::info_panel_target_for_chat`
  now returns the partner user for `ChatKind::Secret` too) gains an
  **Encryption key** section: the 12×12 grid (18px cells) plus
  Telegram-style verification copy ("If this image matches the one on
  your contact's device, your conversation is secure."). Shown only
  when the open chat is a **Ready** secret chat with that user
  (`Session::open_ready_secret_chat_for_user`); a Ready record whose
  hash isn't 36 bytes yet renders "Encryption key · still loading…"
  instead of the grid. Non-Ready chats show no key section.
- **Security hygiene.** Key material stays in memory only: `Session`
  has no `Debug`/`serde`; `ParsedSecretChat`'s derived `Debug` was
  replaced with a hand-written impl printing only `key_hash_len` (a
  canary test asserts the byte values never appear); no logging or
  disk writes of key bytes anywhere; the UI receives only pixel
  indices/colors, never the raw bytes. E2E crypto remains inside TDLib.
- **Screenshot:** `docs/screenshots/ready-key-verification.png` —
  `quill --screenshot-demo ready-key-verification`: Ready secret chat
  (id 41) with Zed, deterministic 36-byte `key_hash` fixture, info
  panel open on the fingerprint grid + verification copy.
- **Tests.** 5 unit tests on the byte→pixel mapping (zero hash → all
  white; 0xE4 → little-endian group order [0,1,2,3]; 35/37/empty → not
  renderable; color table order; full 0x00–0x23 fixture incl. tail byte
  0x23 → [3,0,2,0]); the `Debug`-redaction canary test.
- **Out of this slice (→ future):** B3 self-destructing messages
  (`messageSelfDestructType`); secret-chat-specific notification
  behavior; honoring `can_be_copied_to_secret_chat` in the forward
  flow; secret-chat file-download UI differences; hex-format key
  display alternative (the schema comment's "alternatively" clause —
  official mobile clients show the image, so the image is the parity
  target).

## Phase B3 — Self-destructing messages (2026-09-26)

- **Rationale.** Official clients let the sender attach a self-destruct
  timer to a photo/video; the content is destroyed after the timer once
  opened. Quill needed the send path, the receive badge, and a live
  countdown — all grounded in the pinned schema, not guessed.
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `messageSelfDestructTypeTimer self_destruct_time:int32` (:5915) /
  `messageSelfDestructTypeImmediately` (:5918); `inputMessagePhoto` /
  `inputMessageVideo` carry `self_destruct_type` (:6117/:6128 —
  "private chats only" per the field comments); `message` carries
  `self_destruct_type` (:3146) and `self_destruct_in` (:3147) in its
  constructor (:3165). `inputMessageText` has **no** self-destruct
  field (:6081) — plain text cannot carry per-message self-destruction,
  so no text UI is offered (documented, not faked).
- **Compatibility finding (verified against the pinned TDLib commit
  `d1085f9`, not just master).** `MessageContent.cpp`
  `get_input_message_content` returns 400 "Messages can self-destruct
  only in private chats" unless `dialog_id.get_type() ==
  DialogType::User`; `DialogId.h` lists `User` and `SecretChat` as
  distinct `DialogType` values — so "private chats" means 1:1 **cloud**
  chats, **not** secret chats (secret chats likely handle
  self-destruction through a different mechanism — ~~unverified, no
  corresponding constructor in the pinned schema~~ **(wrong — corrected
  in Phase B4: the chat-level timer `setChatMessageAutoDeleteTime` /
  `chat.message_auto_delete_time`, schema 1.8.67 :13454 / :3616 / :3627)** —
  and are out of this slice). `MessageSelfDestructType.cpp` validates the
  timer as 1..=60 (`MAX_PRIVATE_MESSAGE_TTL = 60`, "server-side
  limit"). Consequence: this slice ships for **private chats only**;
  the original "secret chats" assumption in the task brief was wrong
  and is not implemented. The picker is gated to `ChatKind::Private`,
  and the driver strips the choice for every other chat kind
  (defense in depth — a stale snapshot can never turn a send into a
  400). The schema-valid picker offers only Off / 5s / 30s / 1m /
  View once (no `1h`/`1d` — the server would 400 them).
- **Send.** `SelfDestructSend::{Timer(i32), Immediately}` in
  `src/telegram/requests.rs` (`Copy`); `self_destruct_type_value`
  emits `messageSelfDestructTypeTimer` /
  `messageSelfDestructTypeImmediately` / null. `input_message_photo`
  and `send_photo` take `Option<SelfDestructSend>`; for video the
  choice rides on the `VideoSend` struct (keeps `send_video` at 7 args
  for the `too_many_arguments` lint). `ComposerSnapshot` gains
  `self_destruct` + `with_self_destruct`; `send_snapshot` and
  `send_album_snapshot` (connect.rs) apply the private-chat gate.
- **Receive.** `ParsedMessage` / `HistoryMessage` gain
  `self_destruct: Option<MessageSelfDestruct>` (`kind` +
  `expires_in_ms` — whole milliseconds, not `f64`, to keep the `Eq`
  derive — + `fetched_at_ms`); unknown future variants degrade to
  `None`. `remaining_secs` / `badge_label` decay the countdown locally
  ("⏱ 60s" unscheduled → "⏱ 42s left" → 0); rows disappear via the
  normal `updateDeleteMessages` path — no special deletion code.
- **UI.** `session_history_row` renders the badge under the media for
  incoming and outgoing rows; a 1-second render tick
  (`ensure_self_destruct_tick`, mirroring the Phase A1 slow-mode tick,
  guarded by `self_destruct_tick_chat`) keeps badges fresh while the
  open chat has a live timer; the tick exits when no timer is live or
  the chat changes. The composer shows a ⏱ cycle button (Off → 5s →
  30s → 1m → View once → Off) only when the open chat is private and a
  photo/video attachment is pending; the choice resets after a
  successful send.
- **Screenshot:** `docs/screenshots/ready-self-destruct.png` —
  `quill --screenshot-demo ready-self-destruct`: private chat with Zed,
  incoming photo with live ⏱ countdown badge, outgoing ⏱ view once
  photo, composer picker on 30s.
- **Tests.** Request shapes (null / timer / immediately, timer carries
  `self_destruct_time`); parse of both variants + unknown-variant
  degradation + local countdown decay + garbage `self_destruct_in`;
  replay test: self-destructing photo arrives, badge data on the row,
  `updateDeleteMessages` removes it and the tick gate goes quiet.
- **Out of this slice (→ future):** auto-delete timers for regular
  chats (`messageAutoDeleteTime`); screenshot-detection notices;
  secret-chat-specific notification behavior. ~~Secret-chat TTL UI
  (likely a per-chat timer mechanism distinct from this slice's
  per-media type — unverified, no constructor in the pinned
  schema).~~ **Correction (Phase B4, same day):** the mechanism was
  found — `setChatMessageAutoDeleteTime` (schema 1.8.67 :13454) and
  `chat.message_auto_delete_time` (:3616/:3627) — and is implemented
  in Phase B4 below.

## Phase C1 — 1-on-1 call signaling + call UI (2026-09-26)

- **Rationale.** TDLib carries call *signaling* but no audio/video
  transport; this slice implements the signaling state machine and the
  full call UI, and states the missing transport honestly everywhere —
  never faking a working call. Audio-only `createCall`; video, group
  calls/voice chats, transport, and debug/log upload are out of slice.
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `callProtocol` (:7008); `callId` (:7034);
  `callStatePending` (:7054) / `callStateExchangingKeys` (:7057) /
  `callStateReady` (:7068) / `callStateHangingUp` (:7071) /
  `callStateDiscarded` (:7078, `need_rating` field :7078) /
  `callStateError` (:7081); `call` (:7287); `updateCall` (:10816);
  `updateNewCallSignalingData` (:10862); `createCall` (:14212);
  `acceptCall` (:14215); `sendCallSignalingData` (:14218);
  `discardCall` (:14227); `sendCallRating` (:14234);
  `sendCallDebugInformation` (:14237); discard reasons :6984–6999;
  call problems :7253–7277.
- **Honesty design.** `callProtocol` claims no media transport
  (`udp_p2p: false`, `udp_reflector: false`, layer 65–92, empty
  `library_versions`) — an honest capability advertisement, not a
  fake. `updateNewCallSignalingData` chunks are queued (32-chunk cap,
  diagnostic-only overflow) and never consumed: there is no transport
  in this slice to feed them to. `sendCallSignalingData` has no request
  builder for the same reason (documented as C2). Every live-call card
  (incoming / connecting / connected) and the end screen carry the
  explicit note "Audio isn't connected — Quill's voice transport ships
  in Phase C2. This call carries no sound."
- **State model (`src/state.rs`).** `ActiveCall` (id, peer, direction,
  state, started/ready instants, signaling queue) + `CallSummary`
  (terminal reason line, duration, `need_rating`, rating state) on the
  session; `call_error` for surfaced request failures;
  `call_busy_decline_queue` for a second incoming call while one is
  active (busy-declined with plain `discardCall` once the active call
  ends — there is no `callDiscardReasonBusy` constructor in 1.8.67 —
  the driver drains it during ingest). `accept_call_update`
  starts tracking from `updateCall` or the `callId` answer, advances
  same-id states, starts the duration at `Ready`, and produces the
  summary on `Discarded`/`Error` (unknown future states stay
  nonterminal rather than dropping a possibly live call). Discard
  reasons map to human lines ("Missed call", "Call declined",
  "You were busy", "The call timed out" for the 4005000 timeout
  code); TDLib error *message text* is never stored (it can contain
  secrets) — only code/class.
- **Requests (`src/telegram/requests.rs`).** `create_call` (audio-only,
  `is_video: false`), `accept_call`, `discard_call` (connected duration
  when known), `send_call_rating` (1–5 + optional comment, problems
  list left empty for C1). Request purposes `CreateCall` / `AcceptCall`
  / `DiscardCall` / `SendCallRating`; the driver (`src/connect.rs`)
  gates `start_call` on known user, non-bot, non-self, no active call.
- **UI (`src/ui/mod.rs`).** "Call" button on user profiles; a modal
  overlay above everything: incoming ringing (avatar, name, ticking
  clock, Accept / Decline), outgoing ringing (Cancel), connecting
  ("Connecting…", Cancel), connected (state, live duration, Hang up),
  unknown-state honest label, end screen (reason line, duration, 1–5
  rating prompt when `need_rating` — `need_debug_information` /
  `need_log` are out of slice, stated on the card), and a request-error
  banner with dismiss. A 1-second tick (guarded by the
  `call_tick_active` flag) keeps clocks/durations fresh while a call is
  live.
- **Screenshot:** `docs/screenshots/ready-call-ui.png` —
  `quill --screenshot-demo ready-call`: incoming voice call from Zed,
  Accept / Decline, ticking clock, and the no-audio-transport note.
- **Tests.** Envelope unit tests (all call states, signaling bytes,
  `callId`, constructor pins); request-shape tests; replay test
  `replay_call_signaling_lifecycle`: incoming pending → busy-decline
  queue → exchanging keys → signaling queued → ready → remote hangup
  with `need_rating` → summary; rejected `createCall` surfaces
  `call_error` without TDLib message text; missed untracked call still
  records a summary; 4005000 error maps to "timed out".
- **Out of this slice (→ future):** Phase C2 — libtgvoip audio
  transport (the queued signaling data becomes the transport's input;
  outgoing `sendCallSignalingData` produced by the transport); video
  calls (`is_video` — shipped signaling-only in Phase C1b, next
  section); group calls / voice chats; `sendCallDebugInformation`
  / `need_debug_information` / `need_log` upload; richer rating
  (comment + `callProblem` checklist).

## Phase C1b — 1:1 video-call signaling (2026-09-26)

- **Rationale.** C1 shipped 1:1 call signaling audio-only; the pinned
  schema has always supported video calls via the `is_video` flag on
  `createCall` / `discardCall` / the `call` type. This slice adds video
  to the *signaling* layer only — still no media transport (Phase C2),
  still honest UI that never implies a working video call.
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `createCall user_id:int53 protocol:callProtocol is_video:Bool =
  CallId;` (:14212, "@is_video Pass true to create a video call");
  `discardCall ... is_video:Bool ...` (:14227, "@is_video Pass true if
  the call was a video call"); `call ... is_video:Bool ...` (:7287);
  `callId` (:7034 — the `createCall` answer, carries no `is_video`);
  `acceptCall` (:14215); `sendCallRating` (:14234). No new
  constructors; video reuses the exact C1 signaling surface.
- **Honesty design.** `callProtocol` still advertises no media
  transport (unchanged from C1). A connected video call shows a
  video-stage *placeholder* grid — dark remote + local tiles labeled
  "No video — transport ships in Phase C2" / "No preview — transport
  ships in Phase C2" — never a fake live picture. The card note reads
  "Video isn't connected — video transport ships in Phase C2. This
  call carries no video or audio." The Mute/Unmute toggle tracks
  local-only state (`ActiveCall.muted`) and is labeled "Muted — no
  audio to mute; voice transport ships in Phase C2."
- **State model (`src/state.rs`).** Incoming `updateCall`: `is_video`
  is taken from the `call` object (authoritative per :7287),
  including the same-id state-advance branch. Outgoing: the `callId`
  answer carries no `is_video`, so it is derived from the `createCall`
  request args stashed in `RequestPurpose::CreateCall { is_video }`
  (documented inference — the only non-authoritative read in the
  slice). `ActiveCall` gains `muted: bool` (default false; dies with
  the call). `call_busy_decline_queue` entries already carried
  `(call_id, is_video)`, so a second incoming video call is
  busy-declined with `is_video: true`.
- **Requests (`src/telegram/requests.rs`).** `create_call` already
  took `is_video`; the driver (`src/connect.rs`) now threads it
  through (`start_call(user_id, is_video)`). `discard_call` sends the
  tracked call's actual `is_video` (C1 already plumbed it; now it can
  be true). `acceptCall` / `sendCallRating` unchanged — the protocol
  negotiation is kind-agnostic.
- **UI (`src/ui/mod.rs`).** User profiles gain a "🎥 Video call"
  button next to "Call" (same gating: non-bot users, not yourself).
  Incoming video calls show a "📹 Video call" kind line; Accept flows
  through `acceptCall` unchanged. The connected video card shows the
  video-stage grid, duration clock, Mute / Hang up, and the honest
  notes; the Mute toggle also appears on connected voice calls.
- **Screenshot:** `docs/screenshots/ready-call-video.png` —
  `quill --screenshot-demo ready-call-video`: connected incoming
  video call from Zed (pending → exchanging keys → ready, injected),
  video-stage grid, 📹 kind line, ticking clock, Mute / Hang up, and
  the no-video-transport note.
- **Tests.** Envelope unit tests (new `is_video: true` `updateCall`
  parse case); request-shape tests (`createCall` / `discardCall`
  with `is_video: true`); replay test `replay_video_call_signaling`:
  outgoing video create → exchanging keys → ready → discard keeps
  `is_video: true` on the summary; incoming video accept; second
  incoming video call while active queued for busy-decline with
  `is_video: true`.
- **Out of this slice (→ future):** Phase C2 — real audio/video
  transport (libtgvoip spike), camera capture, video rendering,
  device selection; Phase C3 — group calls / voice chats. The UI must
  never imply a working video call until C2 lands.

## Phase B4 — Chat-level auto-delete / self-destruct timer (2026-09-26)

- **Rationale.** Official clients let a user set a chat-wide timer: in
  secret chats every message self-destructs N seconds after being
  viewed; in other chats every message is deleted N days after sending.
  Phase B3 had concluded the secret-chat mechanism was unverified —
  that was wrong: the mechanism is chat-level, not per-media, and is
  fully in the pinned schema. This slice implements it.
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `setChatMessageAutoDeleteTime chat_id:int53 message_auto_delete_time:int32`
  (:13454) — secret chats accept arbitrary non-negative seconds;
  non-secret chats accept 0–365 days and nonzero values must be
  divisible by 86400; groups/channels require `change_info`
  (`canChangeInfo`). `chat.message_auto_delete_time` (:3616/:3627 —
  countdown from view in secret chats, from send date otherwise).
  `updateChatMessageAutoDeleteTime` (:10549). The timer-change service
  message `messageChatSetMessageAutoDeleteTime` (:5387). Per-message
  `message.auto_delete_in` (:3148, `double` seconds, in the message
  constructor at :3165).
- **Scope decision.** Secret-chat timers are fully implemented: send
  path, receive state, live updates, picker UI, service rows,
  countdown chips. Non-secret (regular) chat timers are implemented
  through the **state/receive/driver** layers (`ChatSummary`
  gains `message_auto_delete_time` + `ttl_status_line()` for every
  chat; the driver validates day-multiples and accepts
  `updateChatMessageAutoDeleteTime` for any chat) but the **picker
  UI is secret-chat-only for now**: TDLib requires `change_info`
  rights in groups/channels and Quill does not yet know whether the
  viewer holds them. Wiring the regular-chat picker is the immediate
  follow-up (rights-aware gating), documented rather than half-built.
- **Send.** `requests::set_chat_message_auto_delete_time(extra,
  chat_id, seconds)` (shape test); driver method
  `set_chat_message_auto_delete_time` validates: negative → error;
  secret chats accept arbitrary non-negative seconds; non-secret
  chats accept 0 or day-multiples up to 365 days (else error);
  unknown chat → error. Registers
  `RequestPurpose::SetChatMessageAutoDeleteTime` and drops the pending
  request on send failure. Group/channel `change_info` rights gating
  is not implemented (deferred — see scope decision).
- **Receive.** `EnvelopePayload::UpdateNewChat` parses
  `message_auto_delete_time` (default 0 when absent);
  `EnvelopePayload::UpdateChatMessageAutoDeleteTime` applies live
  updates. `MessageContent::ChatTtlChanged { secs }` parses
  `messageChatSetMessageAutoDeleteTime` (`from_user_id` is dropped —
  the service row needs no attribution). `ParsedMessage.auto_delete`
  / `HistoryMessage.auto_delete` carry `MessageAutoDelete {
  expires_in_ms, fetched_at_ms }` (whole milliseconds, keeps `Eq`);
  unknown future content variants degrade to `None`. Countdown decay
  mirrors Phase B3's `MessageSelfDestruct` (`remaining_secs`,
  `div_ceil` round-up, `auto_delete_chip()` label `🗑 … left`).
  Rows leave via the normal `updateDeleteMessages` path — no special
  deletion code. Local search propagation included
  (`SearchMessageHit.auto_delete`).
- **Formatting.** `format_countdown_secs` (div_ceil); `format_ttl_setting`
  picks the largest exactly divisible unit (90 s stays `90s`, 604800 →
  `7d`). `chat_ttl_service_label`: secret → `Self-destruct timer set
  to 1h`; regular → `Auto-delete timer set to 1d`; 0 → `… timer turned
  off`. Chat previews stay neutral (no chat-kind context there).
- **UI.** Open-chat header shows the timer status line (`Self-destruct:
  1h` / `Auto-delete: 1d`) in all three identity-layout branches;
  Ready secret chats get a header ⏱ button (`⏱ 1h`) opening a picker
  panel (Off / 5s / 30s / 1m / 1h / 1d / 1w; current value shown).
  `ChatTtlChanged` rows render as centered neutral service notices
  before normal bubble controls. The countdown chip shares the
  existing 1-second self-destruct render tick (the tick predicate now
  also detects live auto-delete countdowns). The picker closes on
  chat switch and Escape. Demo-mode `apply_chat_ttl` mutates state
  directly (no network).
- **Screenshot:** `docs/screenshots/ready-chat-ttl.png` —
  `quill --screenshot-demo ready-chat-ttl`: Ready secret chat with
  Zed, `message_auto_delete_time` 3600, a message with a live
  `auto_delete_in` countdown chip, the timer-change service row, and
  the picker expanded.
- **Tests.** Envelope: initial chat timer, missing default 0, live
  update, service-message content, `auto_delete_in` parse,
  formatting + wording. Driver: validation for secret (arbitrary
  seconds) and regular (day-multiples, reject 90s/negative/over-365d)
  plus unknown chat. Replay: initial + live update (incl. regular
  chat 1d), service row in history, `auto_delete_in` on the row and
  normal `updateDeleteMessages` removal.
- **Out of this slice (→ future):** regular-chat picker UI with
  `change_info` rights gating; screenshot-detection notices;
  secret-chat-specific notification behavior.

## Phase E — RTL / bidi support (planned 2026-09-26, queued after Phases A–D)

**Why a new phase.** Idan's zapfast repo (`~/workspace/zapfast`, egui/epaint-based)
needed a ~1500-line `src/bidi.rs` post-layout pass (`unicode-bidi` 0.3.18 +
`icu_properties` 2.3) because egui had no bidi: visual reordering of glyph
rows, logical↔visual caret mapping, bracket mirroring, multi-line cluster
rebasing, ligature-continuation handling, RTL-first message right-alignment,
plus an `rtl-self` demo page and row-by-row painted-bubble tests. Key zapfast
commits: `ee08279` (original Hebrew fix), `2e7813e` (carets/bubble alignment),
`eac9c1f` (Arabic ligatures), `9f7c93f` (numbers in RTL), `53d8b09` (Arabic
sizing), `9e73b65`+`fb77503` (font fallback). Mine, don't copy blindly.

**Quill's situation is different.** GPUI on Linux lays out text via
cosmic-text 0.19.0 (pinned in Cargo.lock), which implements the Unicode
bidi algorithm's visual reordering itself. So we almost certainly do NOT
need zapfast's giant reorder pass — we need an empirical audit plus
targeted fixes. Portable zapfast logic worth reusing directly:
`message_rtl(text)` (first strong character via `unicode_bidi::get_base_direction_full`
→ alignment side, `src/bidi.rs:105`) and `base_rtl(text)` (first paragraph
level, `:95`); both are small, pure, and testable against `unicode-bidi`.

**Slices (audit first; pull forward only if a slice turns out to be a quick win):**
- **E1 — Empirical RTL audit.** New `rtl` screenshot-demo mode mirroring
  zapfast's `rtl-self` page: Hebrew, Arabic, mixed Hebrew+Latin, numbers
  inside RTL, mirrored brackets, niqqud, Arabic-Indic digits, wrapped
  multi-line RTL. Screenshot on Linux and verify visual order empirically.
  Deliverable is the demo + a findings list driving E2–E4.
- **E2 — Bubble alignment.** Right-align messages whose first strong
  character is RTL (port zapfast's `message_rtl` logic), matching official
  Telegram. Pure function + unit tests against `unicode-bidi`.
- **E3 — Composer caret/selection/hit-testing.** Verify GPUI's input maps
  logical↔visual caret positions correctly with RTL text; fix if broken
  (this is where zapfast spent `2e7813e` — do not assume GPUI is correct,
  verify empirically).
- **E4 — Font fallback.** Hebrew/Arabic glyph coverage on Linux (port
  zapfast's font lessons: search distro font folders, Arabic sizing parity
  with Latin — `53d8b09`); macOS/Windows coverage as follow-ups.
- **E5 — Regression tests.** Unit tests for direction/alignment logic
  against `unicode-bidi` + RTL screenshot goldens from the E1 demo.

**Out of phase:** full UI mirroring (RTL layout direction for the whole
chrome — official Telegram desktop does not mirror the full UI either;
revisit only if evidence shows otherwise), vertical text, complex-script
shaping beyond what cosmic-text/harfbuzz already do.

## Phase C — expanded call parity plan (2026-09-26, per Idan: calls = 1:1 AND group, audio AND video)

**Schema audit of TDLib 1.8.67 (all lines verified verbatim):**
- 1:1: `createCall user_id protocol is_video` (L14212) — video flag is
  signaling-supported; C1 shipped audio-only, so 1:1 video signaling is a
  schema-supported gap. `acceptCall`/`discardCall`/`sendCallSignalingData`/
  `sendCallRating` all present. `discardCall` takes `is_video` + `invite_link`
  (upgrade to group call).
- Group: `createGroupCall` (L14259), `joinGroupCall` (L14285),
  `leaveGroupCall`/`endGroupCall` (L14458/14461), `discardGroupCall` implied
  via `endGroupCall`; participants: `getGroupCallParticipants` (L14449),
  `loadGroupCallParticipants` (L14455), `updateGroupCallParticipant` (L10824),
  `updateGroupCallParticipants` (L10830); speaking: `is_speaking` on
  `groupCallParticipant`, `recent_speakers` on `groupCall`; mute:
  `toggleGroupCallParticipantIsMuted` (L14431), hand raise (L14444); video:
  `toggleGroupCallIsMyVideoPaused`/`IsMyVideoEnabled` (L14411/14414),
  `groupCallParticipantVideoInfo` (source_groups, endpoint_id, is_paused);
  screen sharing signaling: `startGroupCallScreenSharing` (L14303),
  `toggleGroupCallScreenSharingIsPaused`, `endGroupCallScreenSharing`;
  recording: `startGroupCallRecording`/`endGroupCallRecording` (L14405/14408);
  RTMP: `getVideoChatRtmpUrl` (L14262); invite links (L14395); titles (L14312);
  verification: `updateGroupCallVerificationState` (L10836, E2E emoji check);
  reconnection: `need_rejoin` on `groupCall`, `updateGroupCall` (L10819).
- **Genuinely unsupported by TDLib (not a schema gap — needs building):**
  media transport itself. `joinGroupCall` needs `groupCallJoinParameters`
  (`audio_source_id`, `payload`, `is_muted`, `is_my_video_enabled`, L7089)
  and returns `GroupCallInfo` (`group_call_id`, `join_payload`, L14244ff) —
  those payloads are SDP-like blobs the app's media engine produces/consumes
  (libtgvoip or equivalent). Device enumeration/selection has NO TDLib API
  at all — microphones/cameras/speakers come from the platform media stack
  (PipeWire/PulseAudio on Linux, CoreAudio/AVFoundation on macOS), so device
  selection is part of the transport work, not a TDLib-driven feature.
  Screen sharing signaling exists but capture/encoding is app work.

**Expanded slices:**
- C1 (done, PR #59): 1:1 audio signaling + call UI, honest no-transport.
- C1b: 1:1 video-call signaling (`is_video` on create/accept/discard),
  video-call UI states (camera-on placeholder grid, still no transport).
- C2: media transport spike + real 1:1 audio (libtgvoip or equivalent):
  produce/consume signaling payloads, platform device enumeration +
  selection UI, mute/speaker routing. This unblocks real sound for C1/C1b.
  (→ spike outcome 2026-09-26: engine = **tgcalls** (LGPLv3), NOT libtgvoip —
  program split into C2a–C2d; see "Phase C2 — media transport spike" below.)
- C3: group calls — signaling surface first (create/join/leave/end,
  participant grid with `is_speaking`/`recent_speakers` indicators,
  mute/unmute self + admin, hand raise, invite links, titles, `need_rejoin`
  handling, verification emojis), then real group audio on the C2 transport,
  then group video + screen-sharing rendering.
- C4: call recording UI state, RTMP display, `discardCall` invite-link
  upgrade path (1:1 → group call), rating comments/problems.
- Out: nothing dropped — the original "honest no-media" stance stands until
  C2 lands; no UI may imply working audio/video before its transport exists.

## Phase C2 — media transport spike (2026-09-26)

**Verdict: real 1:1 audio is a multi-slice program (C2a–C2d), not one
slice.** This slice is the spike only — no app code changed, no transport
built. The C1/C1b honest no-transport UI stays exactly as-is; its "voice
transport ships in Phase C2" notes now refer to the C2 transport program
below.

**What the official clients actually use (verified from source, not
lore).** The slice brief assumed libtgvoip ("used by all official
clients"). That is outdated:
- Telegram Desktop (`calls/calls_call.cpp`, dev branch, fetched
  2026-09-26) drives 1:1 calls through **tgcalls**: a
  `tgcalls::Descriptor` wires `signalingDataEmitted` →
  `MTPphone_SendSignalingData`, and inbound
  `MTPDupdatePhoneCallSignalingData` → `_instance->receiveSignalingData`.
- Telegram Android's JNI tree contains `TMessagesProj/jni/voip/tgcalls/`
  (`Instance.h`, `v2/InstanceV2ReferenceImpl.cpp`, …) — tgcalls, not the
  old `VoIPController` JNI.
- iOS ships the same core as its `TgVoipWebrtc` module (name visible in
  the tgcalls repo's own testbench paths).
- Industry consensus: libtgvoip is deprecated — e.g. wzgram's voice-call
  docs note "pylibtgvoip is outdated: the Telegram VoIP library underneath
  it was deprecated."

**Option A — libtgvoip via FFI: REJECTED (technically unviable).**
- License is actually FINE — the brief's "libtgvoip is GPL" premise is
  wrong: `UNLICENSE` plus every source header in grishka/libtgvoip say
  "libtgvoip is free and unencumbered public domain software." No
  copyleft issue for this MIT repo. (Telegram Desktop as a whole is GPL;
  the library itself is not.)
- But the library is abandoned: last commit 2019-06-30 (`6c82c9d`).
- Fatal: **no signaling-data API at all** — zero hits for
  `receiveSignalingData` / `signalingDataEmitted` / any `*ignaling*`
  method in `VoIPController.h` or the whole tree. It cannot produce the
  `sendCallSignalingData` payloads or consume `updateNewCallSignalingData`,
  so it cannot complete the current endpoint-exchange handshake or
  interoperate with current clients (whose protocol layers it also
  predates).
- Its build would be light (C++14; OpenSSL + Opus via configure; g++ and
  cmake are present in the sandbox) — irrelevant given the above.

**Option B — tgcalls via FFI: CHOSEN as the engine, but NOT one slice.**
`TelegramMessenger/tgcalls`, the current official stack:
- Public API (`tgcalls/Instance.h`, read verbatim) fits Quill's needs
  exactly: `Meta::Create(version, Descriptor)`; `Descriptor{ config,
  endpoints, encryptionKey, mediaDevicesConfig, stateUpdated,
  signalingDataEmitted, createAudioDeviceModule, … }`;
  `Instance::receiveSignalingData(bytes)` ← `updateNewCallSignalingData`
  (:10862); `signalingDataEmitted` → `sendCallSignalingData` (:14218);
  `setMuteMicrophone`, `setAudioInputDevice` / `setAudioOutputDevice`,
  `setInputVolume` / `setOutputVolume`; `Meta::Versions()` →
  `["7.0.0","8.0.0","9.0.0","12.0.0","13.0.0"]` (current master also
  lists `"14.0.0"` in the v2 compat impl; read the list at runtime) and
  `Meta::MaxLayer()` →
  `92` (matches the `max_layer: 92` real peers advertise — the honest
  `callProtocol` (:7008) values fall straight out: `udp_p2p` /
  `udp_reflector` per engine config, `min_layer` 65 / `max_layer` 92,
  `library_versions` from `Versions()`). `EncryptionKey` is exactly the
  256-byte `callStateReady.encryption_key` (:7068) + `isOutgoing` —
  TDLib already did the DH; the app does no crypto itself.
- License: **LGPLv3** (LICENSE + README). Compatible with this MIT repo
  via dynamic loading: a sidecar worker `dlopen`s `libtgcalls.so` (no
  static link into the Quill binary), plus attribution in
  `THIRD_PARTY.md` and the LGPL source offer for the library itself.
  (Static linking would instead trigger LGPL relink obligations —
  avoided by design.)
- **Build is the blocker.** tgcalls is 86 C++ files but needs a large
  WebRTC subset (`rtc_base`, `api`, `pc`, `media/base`), abseil, libyuv,
  and boringssl/OpenSSL. Known-good builds: tdesktop's
  `ThirdParty/tgcalls` CMake against `desktop-app/lib_webrtc` (custom
  CMake-ified WebRTC static lib — clang, hours-long build, no published
  prebuilts), or the repo's own Bazel testbench (Bazel 8.4.2 + full
  submodule tree + meson/ninja/nasm/autoconf toolchain). This sandbox
  has no clang, no WebRTC sources, no abseil, no Bazel — engine
  procurement alone is a full slice, likely on a beefier builder or
  Idan's machine.

**Option C — pure-Rust reimplementation: REJECTED.**
Telegram 1:1 media is a custom UDP protocol (DH-derived keys, custom
framing, Opus, jitter/congestion control, reflector relays) — NOT
WebRTC/RTP/SDP, so `str0m` / `webrtc-rs` don't speak it. Reimplementing
it is man-months of reverse engineering. No Rust crate speaks it today
(`grammers` has no call media; a crates.io `tgcalls` crate mentioned in
ferogram's README could not be verified — crates.io API unreachable from
the sandbox). The closest prior art, crossgram's `voice-worker`,
reimplements only the DH/signaling scaffolding in Rust and still defers
media to a native tgcalls backend seam — even the ambitious Rust projects
conclude the media engine must be tgcalls.

**Sliced plan (C2a → C2d).**
- **C2a — engine procurement.** Reproducibly build `libtgcalls.so`
  (Linux x86_64 first): WebRTC subset (lib_webrtc path or Bazel
  testbench path), abseil, libyuv, boringssl; validate with the repo's
  `tgcalls_cli` (`--mode p2p`, `--mode reflector`); record LGPL
  attribution in `THIRD_PARTY.md`; document the build. Needs a builder
  with clang + WebRTC sources — beyond this sandbox.
- **C2b — engine boundary.** Rust `CallEngine` trait (a mock keeps every
  existing test green — no real network/audio in tests) + C-ABI shim over
  `tgcalls::Meta` / `Instance`, loaded at runtime via `dlopen` from a
  sidecar worker (crossgram `voice-worker` pattern: framed IPC, fake
  backend for tests, unavailable-fallback so the seam alone can't fake
  media). Wire `sendCallSignalingData` ← `signalingDataEmitted` and
  `updateNewCallSignalingData` → `receiveSignalingData`; advertise the
  honest `callProtocol` from `Meta` (min 65 / max 92 / versions /
  udp_p2p+udp_reflector per config).
- **C2c — real audio I/O.** WebRTC `AudioDeviceModule` for Linux
  (PipeWire/PulseAudio; tgcalls' own CLI uses `FakeAudioDeviceModule` —
  sine/noop — which is test-only) or the callback-audio path; platform
  device enumeration + mic/speaker pickers in the call UI; real
  mute/unmute (`setMuteMicrophone`), volumes, AEC/NS/AGC config;
  screenshot of device selection.
- **C2d — hardening.** Reconnects, `setNetworkType`, stats/debug
  surface, `sendCallDebugInformation` upload, E2E test against a real
  peer, docs.
- Out of the program: video transport/encoding (placeholder grid stays),
  group calls (C3), screen sharing, call recording.

**Sandbox evidence (2026-09-26).** g++/cc/cmake/pkg-config present, no
clang++; OpenSSL 3.0.13 dev headers present; NO Opus/PulseAudio/PipeWire
dev headers, no audio daemons; cargo registry cache holds no
str0m/webrtc/grammers/opus/cpal sources. Even Option A's light build
couldn't fully link here (no Opus), and Option B's WebRTC requirement is
orders of magnitude beyond it.

## Phase C2a — engine procurement (2026-09-26)

**Rationale.** The C2 spike's blocker ("needs a clang+WebRTC builder —
beyond this sandbox") is dissolved: `pytgcalls/ntgcalls` v3.0.0 (released
2026-09-25) publishes prebuilt shared libraries, including
`ntgcalls.linux-x86_64-shared_libs.zip` with `lib/libntgcalls.so` (125MB,
Linux x86_64) + `include/ntgcalls.h` (C API). The engine question is
settled by procurement, not construction — this slice vendors the
prebuilt engine and writes the FFI crate. Same release ships
`macos-arm64` and `windows-x86_64` shared_libs zips for the
cross-platform story (Linux x86_64 only in this slice).

**What was vendored and verified (human-direct, not delegated).**
- `scripts/vendor-ntgcalls.sh`: downloads the pinned release asset,
  verifies SHA256
  `b28f99eec39ae62a9c612da1e16b2884c5662f32c52effc0d985a6918f2831f0`
  (no checksums are published on the release page, so this is the
  implementer-observed hash; the script fails loudly on mismatch), and
  extracts into `vendor/ntgcalls/` (git-ignored — the 125MB `.so` is
  NEVER committed). Re-runnable.
- Verified against the artifact itself: `nm -D` shows 76 defined
  `ntg_*` symbols with proper `visibility("default")`; dlopen-able.
  `ntg_get_version()` returns `"3.0.0"` via the loader smoke test.
- `crates/ntgcalls-sys/`: hand-written `extern "C"` declarations for all
  76 exported functions + every type/callback in `ntgcalls.h`, each
  signature checked verbatim against the vendored header. Notable
  correction vs the brief: there is **no `ntg_destroy`** — instance
  lifecycle is `ntg_create_p2p_call`/`ntg_create_call`/`ntg_init_conference`
  on a `ntg_instance_create` handle, ended with `ntg_stop` +
  `ntg_instance_destroy`. Pattern studied from
  `YouKnow-sys/ntgcalls-rs` (`libntgcalls-sys`), but deliberately
  **not** their link-time approach: Quill's crate resolves everything at
  runtime via `libloading` (NO link-time dependency on the `.so`).
- `Loader` (in the `-sys` crate): `load(path)` / `load_default()` (env
  `QUILL_NTGCALLS_LIB` → exe-ancestor `vendor/ntgcalls/lib/libntgcalls.so`
  → system `libntgcalls.so`); fails with a clear diagnostic
  (`LoadError::LibraryMissing` naming every searched location, or
  `LoadError::SymbolMissing` naming the symbol) when the sidecar is
  absent — the call UI keeps its honest "no audio yet" stance until C2b
  wires this.
- Smoke test (`tests/dlopen_smoke.rs`): skips gracefully when the `.so`
  is not vendored; otherwise asserts the key roadmap symbols resolve
  (lifecycle, signaling bridge, mute/pause/resume, `ntg_get_media_devices`,
  E2E fingerprint, video, presentation, `ntg_get_protocol`) and that
  `ntg_get_version()` is a non-empty C string. No network, no audio.
- **License: LGPLv3 confirmed** — full license text fetched from the
  repo's `master` LICENSE (opens "GNU LESSER GENERAL PUBLIC LICENSE,
  Version 3") and GitHub API reports `LGPL-3.0`. Compliance via the
  dlopen sidecar already reasoned in the C2 spike: dynamic loading, no
  static link, attribution + source-offer wording in `THIRD_PARTY.md`
  (source: https://github.com/pytgcalls/ntgcalls, tag v3.0.0,
  unmodified).
- **License chain verified 2026-09-26 (Codex, primary sources).**
  ntgcalls repo = LGPL-3.0
  (https://github.com/pytgcalls/ntgcalls/blob/master/LICENSE); tgcalls =
  LGPL-3.0 (https://github.com/TelegramMessenger/tgcalls/blob/master/LICENSE).
  Dynamic loading (dlopen, no static linking) from a closed-source app
  is LGPL-compliant — this confirms the C2 spike's dlopen-sidecar
  decision. Distribution obligations to keep: ship the LGPL
  notices/license text with Quill, make the library's corresponding
  source available, and allow users to swap in a modified
  `libntgcalls.so` (the sidecar design already permits this: Quill
  loads it at runtime from a path the user can replace).
- **Future: building tgcalls from source.** If we ever want an
  in-house build instead of the prebuilt `libntgcalls.so`, the path is:
  clang toolchain + WebRTC sources via `depot_tools`/`gclient` (20–40GB
  checkout), tgcalls itself (LGPLv3), reference build = tdesktop's
  CMake/GN setup driving tgcalls. This sandbox cannot do it (7.5GB
  disk / 2.4GB RAM / 2 cores — verified 2026-09-26). Revisit only if a
  beefier builder is available and we have a reason to diverge from
  the prebuilt releases.

**Out of this slice (→ C2b/C2c).** Wiring signaling to TDLib
(`sendCallSignalingData` ← `signalingDataEmitted`,
`updateNewCallSignalingData` → `receiveSignalingData`); the `CallEngine`
trait (a mock keeps tests green until then); real audio I/O; device UI;
video frames; group calls on the engine. **Explicit C2b gate:** verify
ntgcalls 3.0.0 speaks the call-protocol version TDLib 1.8.67 negotiates
(min layer 65 / max from `ntg_get_protocol`, `library_versions`) before
shipping real audio.

## Phase C2b — call engine wiring (signaling bridge, no media) (2026-09-27)

Goal: wire the ntgcalls v3.0.0 sidecar into Quill's call flow with a clean
engine-trait boundary — TDLib signaling data reaches the engine, and the
engine's emitted signaling returns as `sendCallSignalingData` — while
keeping audio/video transport disabled pending Phase C2c.

### Protocol gate (verified by the human, 2026-09-27 — not by the model)
- Vendored artifact `vendor/ntgcalls/` (checksum-pinned zip from the C2a
  procurement step; ignored by git) probed directly: `ntg_get_version()` =
  `"3.0.0"`; `ntg_get_protocol()` = `{min_layer 92, max_layer 92,
  udp_p2p true, udp_reflector true, library_versions
  ["8.0.0","9.0.0","12.0.0","13.0.0"]}` — matching the upstream
  `NTgCalls::get_protocol()` source (`{92, 92, true, true,
  Signaling::supported_versions()}`).
- TDLib 1.8.67 schema (`schema/td_api.tl`, human-verified lines):
  - :7005 "use 65" (min supported API layer), :7006 "use 92" (max),
    :7007 `library_versions` is the supported tgcalls-version list,
    :7008 `callProtocol udp_p2p:Bool udp_reflector:Bool min_layer:int32
    max_layer:int32 library_versions:vector<string> = CallProtocol;`
  - :10862 `updateNewCallSignalingData call_id:int32 data:bytes = Update;`
  - :14212 `createCall user_id:int53 protocol:callProtocol is_video:Bool
    = CallId;`
  - :14215 `acceptCall call_id:int32 protocol:callProtocol = Ok;`
  - :14218 `sendCallSignalingData call_id:int32 data:bytes = Ok;`
- Gate verdict: ntgcalls 3.0.0 speaks layer 92, the top of TDLib's
  negotiated 65–92 range, and both report UDP P2P + reflector support, so
  1:1 signaling interop is plausible. Its `min_layer = 92` (not 65) means
  it requires layer-92 peers — current official clients qualify. This is
  *capability* evidence only; no real audio path has been exercised.
- Advertised protocol: engine present AND available →
  `udp_p2p:true udp_reflector:true min_layer:92 max_layer:92
  library_versions:["8.0.0","9.0.0","12.0.0","13.0.0"]` (the ENGINE's own
  reported values). Engine absent OR installed-but-unavailable → the C1
  signaling-only shape (`false,false,65,92,[]`) (reviewer fix; advertisement
  now keys on `is_available()`, not merely `Some(engine)`).
  - Why min 92, not 65: the schema's 65 is TDLib's floor; the engine does
    not claim to speak 65, so advertising it would be dishonest.
  - Why not `library_versions:["3.0.0"]`: :7007 defines that field as the
    supported *tgcalls* versions, and "3.0.0" is the ntgcalls package
    version — different thing.

### Architecture (what was built)
- `src/calls/mod.rs` + `src/calls/engine.rs`: `CallEngine` trait
  (start/accept, `send_signaling_data` app→engine,
  `receive_signaling_data` engine→app, emitted-signaling hook,
  `hangup` idempotent, `set_muted`, `media_devices`, `protocol`,
  `is_available`), `MockEngine` (Arc/Mutex-shared, observable, test-only),
  `NtgcallsEngine` (dlopen sidecar; real, driver-thread-only). Runtime
  `dlopen` is preserved — no link-time ntgcalls dependency.
- `NtgcallsEngine` lifecycle this slice: instance create + signaling
  callback registration + `ntg_create_p2p_call` on call start +
  `ntg_stop` on hangup/teardown. The callback is a C trampoline through
  an `Arc<CallbackShared>`; emitted bytes are queued into a
  `Mutex<VecDeque>` and drained on the driver thread. `Drop` unregisters
  the callback first, keeps the shared state alive while destroying the
  instance, and only then clears call maps — no callback can reach the
  shared state after destruction.
- DELIBERATELY NOT called: `ntg_skip_exchange` / `ntg_connect_p2p`.
  Human-verified reason: `P2PCall::connect()` adds mic/camera/screen
  capture+playback tracks — potentially starting real audio I/O on the
  user's default devices. Without an E2E audio test, that side effect is
  not verifiable, so C2b ships the bridge only. Transport connect,
  stream sources, and real mute are Phase C2c. The UI keeps its honest
  "No audio yet" wording; mute is still local-only.
- `ConnectDriver` owns the engine (`Box<dyn CallEngine>`, default None)
  and drains the outbox in `ingest()`; `pump_call_engine` maps TDLib
  `updateCall` lifecycle (Pending→start, terminal→hangup) and
  `updateNewCallSignalingData` → `engine.send_signaling_data`, gated on
  the reducer-tracked call id (unknown or ended calls never reach the
  engine — reviewer fix). Engine-emitted bytes leave via the outbox as
  `sendCallSignalingData`; a failed send removes the allocated request
  bookkeeping and requeues the bytes at the head of the outbox for a
  later ingest to retry (reviewer fix). Engine errors at the bridge are
  ignored — TDLib remains the source of truth and the session signaling
  queue keeps the honest record.
- New TDLib request: `send_call_signaling_data` (:14218) with base64
  `data`; `RequestPurpose::SendCallSignalingData` added.
- 17 unit tests for the slice (mock protocol advertisement incl.
  unavailable-engine fallback, engine-derived createCall/acceptCall
  protocol, absent-engine fallback, inbound-signaling bridge + unknown-call
  gating, engine lifecycle bridge, outbox send-failure cleanup/requeue,
  protocol_json shape, protocol probe).

### What still needs C2c (not this slice)
- `ntg_skip_exchange` (TDLib precomputed key) + `ntg_connect_p2p`
  (servers/config/`callStateReady` parsing), audio/video device sources,
  real mute wiring, E2E audio validation against a real peer, group calls.

## Phase C3a — group-call signaling surface (2026-09-26)

**Scope: chat-bound voice chats, signaling only.** No audio/video
transport (Phase C2), so the UI always carries the exact honest note
"No audio yet — voice transport ships in Phase C2." (Mute is labeled
local-only.)

**Schema corrections (verified directly against `schema/td_api.tl`,
TDLib 1.8.67 — the original brief had these wrong):**
- Chat-bound voice/video chats are created with `createVideoChat
  chat_id:int53 title:string start_date:int32 is_rtmp_stream:Bool =
  GroupCallId` (line 14256), NOT `createGroupCall`.
- `createGroupCall` (line 14259) creates a group call **not bound to a
  chat**; this slice doesn't expose it (driver has the builder only).
- Chat-bound joining uses `joinVideoChat` (line 14292); the
  non-chat-bound join is `joinGroupCall` (line 14285).
- The exact mute-new-participants method is
  `toggleVideoChatMuteNewParticipants` (line 14317) — there is no
  `toggleGroupCallMuteNewParticipants`.

**Shipped:** `createVideoChat` / `joinVideoChat` (honest no-device
join params: `audio_source_id` 0, empty payload) / `leaveGroupCall` /
`endGroupCall` / self-video enable+pause toggles / participant
mute+hand toggles / `toggleVideoChatMuteNewParticipants` /
`setVideoChatTitle` / `getVideoChatInviteLink` / `loadGroupCallParticipants`
/ `declineGroupCallInvitation` (builder only); full
`updateGroupCall` + `updateGroupCallParticipant` +
`updateGroupCallParticipants` + `updateGroupCallVerificationState` +
`updateChatVideoChat` parsing and state (participant ordering: recent
speakers first, then `order` desc lexicographic; `need_rejoin` →
reconnect banner → rejoin; `!is_active` → clear; join `Text` payload
stored, never consumed; invite-link `HttpUrl` stored); header
voice-chat affordance ("Start voice chat" / "🔊 Voice chat") for
groups/channels; overlay with participant grid (speaking / muted /
hand-raised / video / screen-share badges), E2E verification emojis,
self controls, admin controls gated on the actual flags
(`can_be_managed`, `can_toggle_mute_new_participants`), invite-link
display, rename dialog; `tests/replay.rs::replay_group_call_signaling`
covers create → join → participants → speaking → mute/hand →
verification → reconnect → leave/end (injected; live needs tdjson).

**Out of this slice:** non-chat-bound group calls (`createGroupCall` /
`joinGroupCall` UI), actual audio/video transport (Phase C2),
`sendGroupCallDebugInformation`, call recording / RTMP / scheduled
voice chats.

## Phase E — emoji × all languages (folded in 2026-09-26, per Idan)

Emoji must play nice with every language, not just RTL. The E1 audit
corpus is extended with: emoji adjacent to Hebrew/Arabic (both sides),
emoji inside RTL paragraphs, ZWJ sequences and skin-tone modifiers in
mixed-direction text, emoji at paragraph boundaries, emoji + numbers in
RTL. zapfast's `bidi.rs` kept emoji in the text so character offsets match
the buffer — Quill must verify the same invariant in its own text
pipeline: formatted-text entity offsets, composer caret mapping,
selection/hit-testing with emoji present (emoji are multi-code-unit in
UTF-16 and multi-scalar in general — a classic offset-corruption source).
Failing cases become regression tests/screenshots in E5.

## Phase D2 — channel author signatures + channel statistics view (2026-09-26)

- **Rationale.** Two channel-parity gaps: (1) channel posts signed by
  an admin show the author's signature under the post in official
  clients; (2) channel/group admins get a statistics view
  (`getChatStatistics`). This slice implements both, gated exactly as
  TDLib gates them.
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `message.author_signature` (:3165, comment :3155);
  `messageOriginChat.author_signature` (:2893) and
  `messageOriginChannel.author_signature` (:2899) for forwarded
  attribution; `supergroupFullInfo.can_get_statistics` (:2792);
  `getChatStatistics chat_id:int53 is_dark:Bool = ChatStatistics`
  (:15760); `dateRange` (:10135); `statisticalValue` (:10139);
  `statisticalGraphData` / `statisticalGraphAsync` /
  `statisticalGraphError` (:10145/:10148/:10151);
  `chatStatisticsObjectTypeMessage` / `Story` (:10157/:10160);
  interaction/sender/admin/inviter wrappers
  (:10168/:10174/:10181/:10186); `chatStatisticsSupergroup` (:10208,
  8 graph fields); `chatStatisticsChannel` (:10233, 12 graph fields).
  All graph fields are required in both constructors — no optional
  flags — so a null/absent graph is a parse error, never a silent
  empty graph. The same applies to every `statisticalValue` field
  (`value` / `previous_value` / `growth_rate_percentage` all required,
  :10139) and the `dateRange` period (`start_date` / `end_date`
  required, :10135): a null, absent, or mistyped one is a parse error,
  never fabricated zeros. Unknown future `ChatStatistics`
  constructors fail parsing rather than fabricating data — including
  at the envelope level, where the `Unknown` catch-all explicitly
  rejects any `@type` starting with `chatStatistics` so a future
  statistics constructor surfaces as an error instead of silently
  dropping the response.
- **Scope decision.** Signatures parse into `ParsedMessage` /
  `HistoryMessage` / `SearchMessageHit` and render as a small muted
  line under the channel post. The line is suppressed when the message
  carries `forward_info`: forwarded headers already include the
  origin's author signature, and showing both would duplicate
  attribution. `getMessageStatistics`, story statistics, revenue/star
  statistics, and invite-link/admin-management gaps are out of this
  slice (→ D3/future).
- **Send.** `requests::get_chat_statistics(extra, chat_id, is_dark)`
  (shape test against :15760). Driver `fetch_chat_statistics` gates
  strictly on `SupergroupFullInfoData.can_get_statistics` — the entry
  point is never exposed when false/absent, because TDLib errors the
  request anyway. Dedupes `Loading`/`Loaded` and in-flight requests;
  `refresh_chat_statistics` clears the cache and refetches. `is_dark`
  is passed as `false`: it only tints server-rendered graph images,
  and Quill draws its own sparklines from `json_data` client-side (the
  app has no dark-mode concept to report).
- **Receive.** `EnvelopePayload::ChatStatistics { statistics }` — the
  response carries no chat id, so `Session::apply` correlates it
  through the pending `GetChatStatistics` request's chat id
  (`chat_statistics: HashMap<i64, ChatStatisticsFetch>` with
  `Loading` / `Loaded` / `Failed`). A TDLib `error` for the request
  lands `Failed` with an honest message instead of spinning forever.
  `ChatStatistics` / `ChatStatisticsFetch` box their large variants
  (clippy `large_enum_variant`, `-D warnings`).
- **UI.** `InfoPanelTarget::Statistics(chat_id)` renders a statistics
  panel: period label, value rows (`12.4K (+5.6%)` growth vs previous),
  notifications-enabled percentage (channel), Unicode sparklines
  (`▁▂▃▄▅▆▇█`) from the first non-`x` numeric column of each
  `statisticalGraphData` `json_data`, recent interactions (channel) /
  top senders · administrators · inviters with resolved display names
  (supergroup). `Async` graphs render "Still processing — check back
  later"; `Error` graphs render the server text inline; graphs with no
  usable series are omitted — no placeholder numbers anywhere. The
  info panel gains a `Statistics` button only when
  `can_get_statistics` is true. Refresh button included.
- **Screenshot:** `docs/screenshots/ready-channel-stats.png` —
  `quill --screenshot-demo ready-channel-stats`: demo channel 13 with
  `can_get_statistics: true` and a loaded `chatStatisticsChannel`
  fixture (real graph JSON, Async + Error graph variants, recent
  interactions), stats panel open directly. The channel fixtures'
  posts also carry `author_signature` ("Demo Admin" / "News Desk"),
  so `ready-channels.png`, `ready-channels-admin.png`, and
  `ready-sponsored.png` now show the signature line and were
  recaptured.
- **Tests.** Request shape (`getChatStatistics` vs :15760). Envelope:
  `author_signature` present/absent/empty, `can_get_statistics`
  true/absent, full `chatStatisticsChannel` (values, Data/Async/Error
  graphs, message+story interactions), null graph → parse error,
  `chatStatisticsSupergroup` top lists, unknown constructor →
  parse error. Replay: request → `Loaded` correlated via pending
  request; TDLib error → `Failed`.
- **Out of this slice (→ future):** `getMessageStatistics` /
  `getStoryStatistics` and revenue/star statistics; invite-link and
  admin-management gaps; server-rendered graph images (`zoom_token`
  drill-down); dark-theme reporting for `is_dark` if the app ever
  gains a dark mode.

## Phase D3a — channel/supergroup invite links + join-request approval (2026-09-26)

- **Rationale.** Invite-link management and join-request approval are
  core admin workflows in official clients (D2 explicitly deferred
  them: "invite-link and admin-management gaps"). This slice
  implements the full loop for channels and supergroups: list / create
  / revoke invite links, list / approve / decline join requests, with
  live `updateNewChatJoinRequest` / `updateChatPendingJoinRequests`
  updates. All actions are gated on the viewer actually holding the
  `can_invite_users` admin right (or creator status).
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `chatAdministratorRights.can_invite_users` (:1092);
  `chatInviteLink` (:2627, all 14 fields incl. `subscription_pricing`,
  `member_limit`, `pending_join_request_count`, `creates_join_request`,
  `is_primary`, `is_revoked`); `chatInviteLinks` (:2630);
  `chatJoinRequest` (:2688); `chatJoinRequests` (:2691);
  `chatJoinRequestsInfo` (:2694);
  `updateChatPendingJoinRequests` (:10555);
  `updateNewChatJoinRequest` (:11210, incl. `query_id:int64`);
  `createChatInviteLink` (:14097); `editChatInviteLink` (:14115);
  `getChatInviteLinks` (:14138); `revokeChatInviteLink` (:14152);
  `getChatJoinRequests` (:14174); `processChatJoinRequest` (:14177).
  `checkChatInviteLink` (:14163) verified but unused.
- **Revocation is the only removal path.** Pinned TDLib 1.8.67 has **no
  `deleteChatInviteLink` constructor** (verified by a schema-pin test
  asserting its absence), so the UI offers Revoke, not Delete, and the
  docs say so. There is also no separate edit dialog in this slice:
  `editChatInviteLink` is implemented in requests/state (upsert path)
  but the UI create dialog is create-only; editing stays out of the
  slice.
- **Rights gating, two paths.** Channels: own membership probed via
  `getChatMember` → `ChatSummary.my_member_status` +
  `my_admin_can_invite_users` (parsed from
  `rights.can_invite_users`). Non-channel supergroups: own admin
  rights arrive on the `updateSupergroup` / `getSupergroup` status
  block → new `Session.supergroup_invite_right` map, mirroring the
  Phase A1 `supergroup_restrict_right` pattern. The single gate is
  `Session::chat_can_invite_users(chat_id)`: creator → always allowed;
  administrator → requires the explicit right; absent rights → denied
  (never fabricated). Driver fetch/create/revoke/process methods and
  both info-panel sections gate on it; the driver returns `Ok(None)`
  (no request sent) when the gate is closed.
- **Fetch / cache / dedupe.** Invite links: first page, limit 100, all
  creators, active links only (`getChatInviteLinks` defaults).
  Join requests: all invite links, empty search query, limit 50.
  In-flight dedupe by `RequestPurpose` per chat plus a `Loading` cache
  state; Refresh buttons bypass via `refresh_*` driver methods. List
  responses replace the cache; create/edit responses upsert (create
  bumps `total_count` only for a genuinely new link);
  `processChatJoinRequest` success drops the request from the cached
  list (count decremented); `updateNewChatJoinRequest` prepends +
  bumps (deduped by `user_id`); `updateChatPendingJoinRequests` is the
  authoritative pending-count badge source.
- **Honest UI.** Info-panel sections render loading / failed-with-retry
  / loaded / empty states — never placeholder numbers. Expiry shows
  "Never expires" / relative text from the real `expiration_date`.
  Join-request rows show the requester's name when known, otherwise
  "User \<id\>"; bios shown when present. The create dialog validates
  expiration-days and member-limit as non-negative numbers with
  `status_note` errors. Element ids are namespaced
  (`invite-link-*`, `join-request-*`, `invite-link-dialog-*`).
- **Screenshot.** `quill --screenshot-demo ready-invite-links`:
  demo channel 13 (viewer 777 admin with `can_invite_users`), a
  `chatInviteLinks` response (primary + named expiring limited +
  join-request link with 2 pending) and a `chatJoinRequests` response
  (2 requests with `updateUser` names) through the real reducer paths,
  info panel open → `docs/screenshots/ready-invite-links.png`.
- **Tests.** Request JSON shapes vs schema; envelope: link/list/
  request/updates parse, `deleteChatInviteLink` absence pin,
  administrator-rights parse; supergroup `can_invite_users` parse on
  both `updateSupergroup` and `supergroup`; replay (9): fetch →
  `Loaded` via `@extra` correlation, wrong-`@extra` ignored, create
  upsert bumps total, edit replaces in place, revoke replaces list,
  join-request fetch, update prepend + dedupe + total bump,
  pending-count update, process-ok drops request; gate unit test
  (channel creator/admin-with/without right, group creator/admin/member).
- **Out of this slice (→ D3b/future):** `getChatEventLog` (admin
  activity log); promote/demote admins and other admin-right
  management; message statistics (`getMessageStatistics`,
  `getStoryStatistics`); invite-link editing UI; subscription-pricing
  (Stars) link display beyond parsing; `checkChatInviteLink` usage.

## Phase D3b — channel/supergroup admin management (2026-09-26)

- **Rationale.** D3a closed invite links and join requests but explicitly
  deferred promote/demote and admin-right management. This slice
  implements the full admin-management loop for channels and
  supergroups: administrator list, promote a member, edit an
  administrator's rights, demote to member. All actions are gated on the
  viewer being the creator or holding the `can_promote_members` admin
  right (deny-by-default).
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `chatAdministratorRights` (:1092, all 18 fields in order:
  `can_manage_chat`, `can_change_info`, `can_post_messages`,
  `can_edit_messages`, `can_delete_messages`, `can_invite_users`,
  `can_restrict_members`, `can_pin_messages`, `can_manage_topics`,
  `can_promote_members`, `can_manage_video_chats`, `can_post_stories`,
  `can_edit_stories`, `can_delete_stories`, `can_manage_direct_messages`,
  `can_manage_tags`, `can_send_welcome_messages`, `is_anonymous`);
  `chatAdministrator` (:2482, `user_id:int53 custom_title:string
  is_owner:Bool can_be_edited:Bool` — note: **no rights block** on this
  constructor, so the list shows names/titles only and per-admin rights
  are loaded on demand via `getChatMember`); `chatAdministrators`
  (:2485); `chatMemberStatusCreator` (:2493);
  `chatMemberStatusAdministrator` (:2500,
  `can_be_edited:Bool rights:chatAdministratorRights`);
  `chatMemberStatusMember` (:2504, `member_until_date:int32`);
  `chatMember` (:2526); `chatMembers` (:2529);
  `supergroupMembersFilterRecent` (:2559);
  `supergroupMembersFilterAdministrators` (:2565, verified but unused —
  the promote picker needs *members*, not admins);
  `supergroupMembersFilterSearch` (:2568, `query:string`);
  `messageSenderUser` (:2831); `updateChatMember` (:11202);
  `setChatMemberStatus` (:13592,
  `chat_id:int53 member_id:MessageSender status:ChatMemberStatus`);
  `getChatMember` (:13622); `getChatAdministrators` (:13632);
  `getSupergroupMembers` (:15238,
  `supergroup_id:int53 filter:SupergroupMembersFilter offset:int32 limit:int32`).
- **No custom-title editing in TDLib 1.8.67.** There is **no**
  `setChatAdministratorCustomTitle` constructor in the pinned schema
  (verified by absence), so custom titles are display-only: the
  administrator list renders `custom_title` (or "Admin" / "👑 Owner"),
  and the promote/edit dialogs do not offer a title field. If a future
  TDLib adds the setter, it slots into `edit_admin_rights`' dialog.
- **Promote / edit / demote via one function.**
  `setChatMemberStatus` takes the full `ChatMemberStatus` value:
  promote and edit-rights both send
  `chatMemberStatusAdministrator(can_be_edited=true, rights=<18-field
  block>)`; demote sends `chatMemberStatusMember`. The driver exposes
  `promote_chat_member` / `edit_admin_rights` / `demote_chat_member`
  over a shared `send_set_chat_member_status`, deduped per
  (chat, user, kind) via `RequestPurpose::SetChatMemberStatus { user_id,
  kind }` (`MemberStatusChange::{Promote, EditRights, Demote}`).
- **Rights gating, same two paths as D3a.** Channels: own membership
  from `getChatMember` / `updateChatMember` →
  `ChatSummary.my_member_status` + new
  `ChatSummary.my_admin_can_promote_members` (parsed from
  `rights.can_promote_members`; absent rights → `None` → denied).
  Supergroups incl. channels: own rights from the `updateSupergroup` /
  `getSupergroup` status block → new `Session.supergroup_promote_right`
  map, mirroring the A1/D3a right-map pattern. The single gate is
  `Session::chat_can_manage_admins(chat_id)`: creator → always allowed;
  administrator → requires the explicit right; anything else (incl.
  unknown) → denied. All driver methods and the info-panel section
  gate on it; the driver returns `Ok(None)` (no request sent) when the
  gate is closed. The per-row Edit/Remove buttons additionally require
  the entry's `can_be_edited` and non-owner (TDLib rejects edits to the
  creator and to admins granted by someone else).
- **Fetch / cache / dedupe / invalidation.** Admin list:
  `getChatAdministrators` cached per chat (`AdminListFetch`), Refresh
  bypasses. Per-admin rights: `getChatMember` cached per (chat, user)
  (`AdminRightsFetch`) backing the edit dialog. Member picker:
  `getSupergroupMembers` (recent filter, empty query; search filter
  with the query text; offset 0, limit 200) cached per chat
  (`SupergroupMembersFetch`). In-flight dedupe by `RequestPurpose` per
  chat plus `Loading` cache states, same as D3a. Invalidation: a
  successful `setChatMemberStatus` (`ok`) and *any* `updateChatMember`
  drop the cached admin list so the panel refetches; `updateChatMember`
  for the viewer refreshes their own rights (gate follows revocation
  immediately). Errors record `Failed` with the shared
  `call_request_error_line` label ("Could not load administrators" /
  "Could not update member status" / "Could not load members" /
  "Could not load admin rights").
- **Honest UI.** The info-panel **Administrators** section renders
  loading / failed-with-retry / loaded / empty states; rows show the
  admin's display name (or "User \<id\>"), custom title or 👑 Owner
  badge, and Edit/Remove only for editable non-owner entries. The
  promote dialog shows the member picker (search + up to 30 rows,
  already-admin members excluded, click-to-select), the 18 rights
  checkboxes (all enabled by default, mirroring the official clients),
  and Promote/Cancel. The edit-rights dialog shows a loading row until
  the `getChatMember` lookup lands, then the 18 checkboxes bound to a
  staged copy (toggles never mutate the session cache); Save sends
  `setChatMemberStatus`. Demote asks for confirmation. Element ids are
  namespaced (`admin-*`, `admin-promote-*`, `admin-right-*`,
  `admin-edit-right-*`, `admin-demote-*`).
- **Screenshot.** `quill --screenshot-demo ready-admin-management`:
  demo channel 13 (viewer 777 admin with `can_promote_members`), a
  `chatAdministrators` response (owner "Founder" + two editable
  admins) and a `chatMembers` response (2 members) plus `updateUser`
  names through the real reducer paths, info panel open and the
  promote dialog open with a member selected →
  `docs/screenshots/ready-admin-management.png`.
- **Tests.** Request JSON shapes vs schema (admin list, promote with
  all 18 rights fields asserted against the schema order, demote,
  member search); envelope: `chatAdministrators` / `chatMembers` /
  rights round-trip, missing-rights → deny-by-default, exact schema
  pins, `can_promote_members` parse on both `updateSupergroup` and
  `supergroup`; state unit: gate matrix (creator / admin with /
  without / unknown right, channel + group, member, non-admin),
  list/rights/member caching, invalidation on `ok` and on
  `updateChatMember`, own-right refresh; replay (4): admin list load
  + gate via `updateSupergroup`, `ok` invalidates; `updateChatMember`
  invalidates + own gate opens/closes on right grant/revoke; per-admin
  rights lookup + demote `ok` invalidates; member picker cache +
  error marks `Failed`.
## Phase D3c — channel/supergroup admin activity log (2026-09-27)

- **Rationale.** D3b closed admin management but the natural D3c was
  the admin activity log: who did what and when in a channel or
  supergroup. This slice adds `getChatEventLog` end to end — request
  builder, `chatEvent`/`chatEvents` parsing, the deny-by-default
  admin gate, paging, and a "Recent actions" section in the
  channel/group info panel. All constructors and fields were verified
  verbatim against `schema/td_api.tl` (TDLib 1.8.67); anything not
  explicitly handled renders an honest generic row — never fabricated
  details.
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl`):**
  `chatEvent` (:7935, `id:int64 date:int32 member_id:MessageSender
  action:ChatEventAction`); `chatEvents` (:7938);
  `chatEventLogFilters` (:7956, all 15 fields:
  `message_edits message_deletions message_pins member_joins
  member_leaves member_invites member_promotions member_restrictions
  member_tag_changes info_changes setting_changes invite_link_changes
  video_chat_changes forum_changes subscription_extensions`);
  `getChatEventLog` (:15252,
  `chat_id:int53 query:string from_event_id:int64 limit:int32
  filters:chatEventLogFilters user_ids:vector<int53> = ChatEvents` —
  "available only in supergroups and channels; requires administrator
  rights; last 48 hours; reverse chronological / decreasing event id;
  limit up to 100; pass null filters for all types").
  Handled actions: `chatEventMessageEdited` (:7764),
  `chatEventMessageDeleted` (:7767),
  `chatEventMessagePinned` (:7770),
  `chatEventMessageUnpinned` (:7773),
  `chatEventMemberJoined` (:7779),
  `chatEventMemberJoinedByInviteLink` (:7782),
  `chatEventMemberJoinedByRequest` (:7785),
  `chatEventMemberInvited` (:7788),
  `chatEventMemberPromoted` (:7794),
  `chatEventMemberRestricted` (:7797),
  `chatEventDescriptionChanged` (:7812),
  `chatEventPhotoChanged` (:7830),
  `chatEventTitleChanged` (:7842),
  `chatEventInviteLinkEdited` (:7886),
  `chatEventInviteLinkRevoked` (:7889),
  `chatEventInviteLinkDeleted` (:7892).
- **No `chatEventInviteLinkCreated` in 1.8.67.** Link *creation* is
  not a constructor in the pinned schema (verified by absence —
  `grep` returns 0 hits), so the parser handles edited/revoked/deleted
  only; a unit test asserts this absence so a future schema bump with
  the constructor fails loudly instead of silently falling through to
  the generic row.
- **Request.** `get_chat_event_log` (`src/telegram/requests.rs`)
  sends `from_event_id` (0 = latest), `limit` 100, `filters: null`
  (all event types — Quill does not filter in this slice), empty
  `query`, empty `user_ids`. A `ChatEventLogFilterSet` mirrors all 15
  schema fields in order (JSON-shape test asserts every field exists
  by reading the pinned schema) for future filtering. The JSON-shape
  test covers the initial all-events request and a paged filtered
  request.
- **Parsing.** `ChatEventAction` / `ParsedChatEvent` /
  `EnvelopePayload::ChatEvents` (`src/telegram/envelope.rs`).
  Promotion vs demotion and restriction vs ban/unban are derived by
  comparing old/new `ChatMemberStatus` constructor names (both
  `chatMemberStatusAdministrator` → "changed admin rights"; to
  `chatMemberStatusMember` → demoted; to
  `chatMemberStatusRestricted`/`Banned` → restricted/banned; away from
  those → "removed restrictions"/"unbanned"). Message rows use the
  new message's `messageText` excerpt capped at 80 chars; invite-link
  rows show the link name or raw URL. Events whose actor `member_id`
  cannot be parsed are dropped (never misattributed); every other
  unhandled constructor becomes `Unsupported { type_name }` and
  renders "performed an action". Tests cover all 16 handled
  constructors, the status-transition distinctions, unsupported
  constructors, and actorless events.
- **Gate, same two paths as D3b.** `Session::chat_can_view_event_log`
  is deny-by-default: unknown chat → false; non-supergroup → false.
  Channels probe via the `ChatSummary` path (`getChatMember` → new
  `ChatSummary::is_admin_or_creator`); supergroups via the
  `updateSupergroup`/`getSupergroup` status block. Unlike D3b (needs
  `can_promote_members`), *any* administrator or the creator qualifies
  — the schema only requires "administrator rights". The driver
  returns `Ok(None)` (no request sent) when the gate is closed, and
  the info-panel section stays hidden.
- **Fetch / cache / paging / dedupe.** `RequestPurpose::GetChatEventLog
  { from_event_id }`; `Session.event_logs: HashMap<i64,
  ChatEventLogFetch>` (`Loading` / `Loaded(ChatEventLogPage)` /
  `Failed`). Driver: `fetch_chat_event_log` (first page, idempotent),
  `refresh_chat_event_log` (clears cache, re-sends), and
  `fetch_chat_event_log_more` — cursor is the oldest cached event id;
  only fires when the loaded page is full (`has_more`: 100 events
  received). In-flight dedupe across cursors via
  `RequestRegistry::has_event_log_in_flight`. Reducer: cursor 0
  replaces; nonzero cursor appends, dedupes by event id, sorts
  decreasing; first-page failure → visible `Failed` ("Could not load
  recent actions"); load-more failure keeps the loaded page.
- **Honest UI.** The info-panel **Recent actions** section renders
  loading / failed-with-retry / loaded / empty states plus Refresh
  and (when `has_more`) Load more. Rows show the actor's display name
  (or "User \<id\>"), the action description, and a relative
  timestamp ("just now" / "Nm ago" / "Nh ago" / "Nd ago" — the log only
  covers 48h). Unhandled actions read "performed an action". Element
  ids namespaced (`event-log-*`, `("event-log-row", event_id)`).
  The section auto-loads when the info panel opens for a supergroup.
- **Screenshot.** `quill --screenshot-demo ready-admin-log`: demo
  channel 13 (viewer 777 administrator) with a `chatEvents` fixture
  covering the handled action types plus one unhandled
  (`chatEventMemberLeft`) as a generic row, through the real reducer
  paths, info panel open →
  `docs/screenshots/ready-admin-log.png`.
- **Tests.** Request JSON shapes vs schema (initial + paged/filtered,
  all 15 filter fields asserted from the schema); envelope: all 16
  handled constructors, promote/demote and restrict/ban/unban
  distinctions, unsupported constructors, actorless/invalid-actor
  events dropped, exact schema pins, `chatEventInviteLinkCreated`
  absence; state unit: gate matrix (unknown / non-supergroup /
  member / admin / creator × channel and supergroup paths),
  first-page replace, load-more append/dedupe/order, refresh replace,
  short page clears `has_more`, initial vs load-more failure states,
  relative-time buckets; driver replay: inactive path rejected,
  non-admin/unknown no-ops, request shape (null filters, limit 100,
  cursor 0), in-flight dedupe, older-page cursor, refresh re-send.
- **Out of this slice (→ future):** the ~40 deferred action
  constructors, all rendering as generic rows today —
  `chatEventPollStopped` (:7776), `chatEventMemberLeft` (:7791),
  `chatEventMemberTagChanged` (:7800),
  `chatEventMemberSubscriptionExtended` (:7803),
  `chatEventAvailableReactionsChanged` (:7806),
  `chatEventBackgroundChanged` (:7809),
  `chatEventEmojiStatusChanged` (:7815),
  `chatEventLinkedChatChanged` (:7818),
  `chatEventLocationChanged` (:7821),
  `chatEventMessageAutoDeleteTimeChanged` (:7824),
  `chatEventPermissionsChanged` (:7827),
  `chatEventSlowModeDelayChanged` (:7833),
  `chatEventStickerSetChanged` (:7836),
  `chatEventCustomEmojiStickerSetChanged` (:7839),
  `chatEventUsernameChanged` (:7845),
  `chatEventActiveUsernamesChanged` (:7848),
  `chatEventAccentColorChanged` (:7855),
  `chatEventProfileAccentColorChanged` (:7862),
  `chatEventHasProtectedContentToggled` (:7865),
  `chatEventInvitesToggled` (:7868),
  `chatEventIsAllHistoryAvailableToggled` (:7871),
  `chatEventHasAggressiveAntiSpamEnabledToggled` (:7874),
  `chatEventSignMessagesToggled` (:7877),
  `chatEventShowMessageSenderToggled` (:7880),
  `chatEventAutomaticTranslationToggled` (:7883),
  `chatEventVideoChatCreated` (:7895),
  `chatEventVideoChatEnded` (:7898),
  `chatEventVideoChatMuteNewParticipantsToggled` (:7901),
  `chatEventVideoChatParticipantIsMutedToggled` (:7904),
  `chatEventVideoChatParticipantVolumeLevelChanged` (:7907),
  `chatEventIsForumToggled` (:7910),
  `chatEventForumTopicCreated` (:7913),
  `chatEventForumTopicEdited` (:7916),
  `chatEventForumTopicToggleIsClosed` (:7919),
  `chatEventForumTopicToggleIsHidden` (:7922),
  `chatEventForumTopicDeleted` (:7925),
  `chatEventForumTopicPinned` (:7928); event-log filtering/search UI
  (the request builder already supports filters/query/user_ids);
  channel title/description/photo editing (deferred from D3b).

## Phase C2c - real call audio transport (2026-09-26)

Goal: real P2P audio for 1:1 calls — `ntg_skip_exchange` +
`ntg_connect_p2p` with microphone-capture / speaker-playback stream
sources, real mute through the native engine, and real device
enumeration with mic/speaker pickers. No fabricated devices: an empty
device list renders honestly.

### Connect sequence
- The driver connects the native transport exactly once per call,
  gated on the reducer-tracked `ActiveCall.transport` being `None`
  (no separate attempted set): `updateCall` → `callStateReady`
  (schema 1.8.67 :7068) with `call.ready` parsed → `ConnectParams`
  → `engine.connect(call_id, &params)` → `TransportState::Connecting`.
- Native order inside the engine (`NtgcallsEngine::connect`):
  `ntg_skip_exchange` (encryption key + reflector/WebRTC servers),
  then `ntg_connect_p2p` with null custom parameters, then
  `ntg_set_stream_sources` with the selected mic/speaker inputs.
  Custom-parameters passthrough is deferred (out of slice).
- A Ready call without a decodable encryption key cannot build the
  native encryption parameters: the driver sets
  `transport = Failed` with the exact error
  `call became ready without an encryption key`. TDLib still owns
  signaling, so the call stays up but carries no sound.
- The device list is refreshed at connect (best effort), and a
  pre-existing mute applies via `engine.set_muted(call_id, true)`
  once the transport exists (failure there is non-fatal).
- Transport states arrive from the engine's worker thread into the
  driver's transport outbox and drain into
  `ActiveCall.transport` / `transport_error` on the next pump.

### TDLib → native server mapping (schema 1.8.67)
- `callProtocol` (:7008), `callServerTypeTelegramReflector` (:7014),
  `callServerTypeWebrtc` (:7021), `callServer` (:7030),
  `callStateReady` (:7068), `updateCall` (:10816). TDLib `bytes`
  fields (encryption key, peer tag) are base64, decoded with
  `base64::engine::general_purpose::STANDARD`.
- Reflector: empty username/password, `turn=true`, `stun=false`,
  `tcp=is_tcp`, base64-decoded `peer_tag`. WebRTC: supplied
  username/password, `supports_turn`/`supports_stun` flags,
  `tcp=false`, empty `peer_tag`.

### Real mute
- `CallEngine::set_muted` → `ntg_mute` / `ntg_unmute`. The driver
  calls the engine first when an available engine is installed and a
  transport exists; an engine error propagates via
  `EngineError::NoActiveCall` / `EngineError` without flipping the
  session `muted` flag. Without a transport yet, the flag is stored
  and applied on connect.

### Device selection
- `ConnectDriver` keeps `call_devices_cache` and
  `selected_devices: (Option<String>, Option<String>)`, defaulting
  to `(None, None)` = engine default. Nothing is auto-selected from
  enumeration and no devices are fabricated; an empty cache renders
  `No audio devices found - the engine reported none.`, and a
  missing engine renders `No audio - call engine unavailable.`
- `select_call_devices` always stores the pair and forwards to
  `engine.select_devices` only when a transport is connected; a
  failed forward propagates before the stored selection changes.

### Honest state
- `CallState::Ready` stays a unit tag; parsed transport material
  lives in `ParsedCall.ready: Option<ReadyParams>`, copied to
  `ActiveCall.ready` by the reducer.
- UI transport text: `Connecting audio...` (none/connecting),
  `Connected` / `Muted - microphone off`, `Couldn't start audio:
  {error}. The call is up but carries no sound.` (failed),
  `Audio disconnected.` (closed). The call-end summary reports
  `Audio was connected for this call.` only when the transport
  actually reached `Connected`.

### Input semantics assumption
- Stream sources are registered as `NTG_STREAM_DEVICE` at 48 kHz
  mono with nullable input ids (engine default when `None`) and
  `keep_open=false`. The v3.0.0 C API exposes no device-type
  distinction beyond the input id, so mic-vs-speaker routing relies
  on the engine interpreting each id; device kinds come only from
  `ntg_get_media_devices` enumeration.

### AEC/NS/AGC
- Minimal by necessity: the v3.0.0 C API exposes no AEC/NS/AGC
  knobs — verified by direct inspection of
  `vendor/ntgcalls/include/ntgcalls.h` (80 `NTG_C_EXPORT`
  declarations, zero matches for echo/noise/gain/AEC/AGC). The
  engine's internal WebRTC audio-processing defaults apply; no
  unsupported controls were invented.

### Tests (all hardware-free, mock engine only)
- Engine: connect records exact params incl. mapped server data;
  unknown call → `NoSuchCall`; unavailable → `Unavailable`; device
  selection recording/errors; transport-state delivery; newest
  callback wins for both hooks.
- Envelope: Ready parsing for reflector + WebRTC (key, peer tag,
  TURN/STUN/TCP, `allow_p2p`).
- Driver: Ready connects exactly once; transport callback updates
  the session; real mute updates engine and state; failed mute
  leaves state unchanged; device selection before connect is stored
  only, after connect invokes the engine; empty key fails honestly.

### Out of this slice (→ future)
- Video transport (later slice; video calls carry audio only, noted
  honestly in the UI); group calls; screen sharing; call recording;
  custom-parameters passthrough to `ntg_connect_p2p`;
  reconnect/backoff on transport failure; stats/debug surface (C2d).
## Phase C2d — call hardening (2026-09-27)

- **Built:** failed 1:1 audio transports retry the same retained
  `ConnectParams` at most three times. Each native `Failed`/timeout callback
  drives one retry through the existing connect path
  (`ntg_skip_exchange` + `ntg_connect_p2p` + stream sources); there are no
  timers or background retry loop. `Connected` and a new initial connect reset
  the counter. Missing engine/parameters, exhausted attempts, and engine errors
  all remain visible failures. Whether ntgcalls accepts `ntg_connect_p2p` again
  for a failed live-peer call is unverifiable without a live peer; any rejected
  reconnect surfaces the engine error honestly.
- **Built:** the call-end card can upload a compact JSON diagnostic payload made
  only from real app/OS, engine protocol, ended-call, reconnect, mute, and
  selected-device state. TDLib 1.8.67 defines
  `sendCallDebugInformation` at `schema/td_api.tl:14237`; its discarded-call
  identifier is `inputCallDiscarded` at `schema/td_api.tl:7043`.
- **Deferred — network type:** the pinned ntgcalls v3.0.0 C header was extracted
  from `ntgcalls.linux-x86_64-shared_libs.zip` (SHA-256
  `b28f99eec39ae62a9c612da1e16b2884c5662f32c52effc0d985a6918f2831f0`).
  `grep -in 'network\|stats' include/ntgcalls.h` returned no matches, so the C
  API exposes no network-type operation. OS network detection would have no
  engine consumer and is not built.
- **Deferred — transport stats/debug surface:** the same pinned-header check
  exposes no per-call stats API; `ntg_connection_info` contains only `state`
  and `kind`. Quill does not invent bitrate, RTT, jitter, or loss, and there are
  no tests for mappings that do not exist.
- **Out of this slice:** E2E testing against a real peer (CI has no live
  Telegram), video transport, group calls, screen sharing, call recording, and
  custom-parameters passthrough.

## Phase C2e — 1:1 video transport, session 1: engine layer (2026-09-27)

- **Built:** the 1:1 video transport ENGINE layer (`src/calls/engine.rs`,
  `src/connect.rs`), no UI yet. New types: `RemoteVideoState`
  {Inactive, Paused, Active} mirroring Telegram X's `VideoState`
  annotation (verified INACTIVE=0, PAUSED=1, ACTIVE=2 in
  `~/workspace/telegram-x/.../voip/annotation/VideoState.java`);
  `VideoFrame` {seq, width, height, rgba, is_local} — RGBA8 row-major with
  rotation already applied; `VideoFrameCallback` /
  `RemoteVideoStateCallback` hooks. `CallEngine` gains
  `set_video_frame_callback`, `set_remote_video_state_callback`, and
  `set_camera_enabled(call_id, enabled, camera)` (camera doubles as
  selection, `None` = default), implemented for both `NtgcallsEngine` and
  `MockEngine`. `ConnectParams` gains `video_enabled` and
  `camera_input`; the driver keeps the honest default (off, None) until
  session 2 opts in.
- **Built:** `NtgcallsEngine` retains per-call `CallMediaConfig`
  {mic, speaker, camera_enabled, camera} (populated in `connect()`,
  updated in `select_devices()` / `set_camera_enabled()`, removed in
  `hangup()`); `set_audio_sources` became `set_media_sources`, re-issuing
  `ntg_set_stream_sources` for CAPTURE (mic + camera description when
  enabled, NULL camera removes the reader and ntgcalls signals the peer
  `video_stopped`) and PLAYBACK (speaker). Camera description is
  `NTG_MEDIA_SOURCE_DEVICE`, 640x480@30 — the conventional tgvoip P2P
  default. `ntg_on_frames_callback` and
  `ntg_on_remote_source_change_callback` register in `ensure_instance()`
  with the same failure-cleanup pattern as the existing registrations and
  unregister in `Drop`.
- **Built:** pure, unit-tested `i420_to_rgba` (I420 planar -> RGBA8,
  full-range BT.601, alpha 255, returns `None` on bad size/zero dims),
  `rotate_rgba` for 90/180/270 (dimension swap for 90/270), and
  `remote_video_state_from` (ACTIVE->Active, PAUSED->Paused,
  IDLING/other->Inactive). The frames trampoline takes the LAST frame of
  each batch, classifies CAPTURE+CAMERA as local preview and
  PLAYBACK+CAMERA as peer, ignores everything else (screen = later
  slice), copies the bytes off the C thread, and sequences frames from a
  shared `AtomicU64`. The remote-source trampoline ignores non-camera
  devices; the state is passed by value per the C typedef. `video_wanted`
  is a pure driver helper: a video call is wanted only when a camera
  exists, returning the first camera id, else (false, None).
- **Key decisions:** `ntg_add_incoming_video` is GROUP-ONLY (throws on a
  P2P call via the GroupCall cast) and is NOT used — peer frames arrive
  unsolicited through the frames callback for 1:1. Full-range BT.601 with
  no limited-range scaling: neutral chroma passes Y straight through
  (Y=235 -> ~white, Y=16 -> ~black, +-2 rounding). Hook delivery happens
  outside the mutex locks to avoid deadlock with UI callbacks.
- **Not verifiable without a live peer:** the native trampoline paths
  (frame delivery from real ntgcalls, rotation handedness against a real
  camera, peer MediaState -> stream status). Tested instead: pure
  conversion/rotation/mapping (7 unit tests) plus the mock engine's
  toggle contract, frame hook plumbing with replacement semantics, and
  state-emission ordering (8 new tests total, all green).
- **Out of this slice (session 2):** driver wiring
  (`ConnectDriver::call_connect_params` opts into video via
  `video_wanted`; camera toggle control), UI (local preview tile +
  peer video tile rendering `VideoFrame`s, camera device selection),
  docs, and a milestone screenshot. Session 2 can then check
  `parity:calls-start-video`, `parity:calls-camera-preview`,
  `parity:calls-remote-video`, `parity:calls-camera-switch`, and
  `parity:calls-camera-select` — no README boxes touched in this slice.

## Phase C2e — 1:1 video transport, session 2: driver + UI (2026-09-27)

- **Built:** driver wiring (`src/connect.rs`). `ConnectDriver` registers
  the session-1 `VideoFrameCallback` / `RemoteVideoStateCallback` hooks in
  `set_call_engine`, draining peer camera states into
  `ActiveCall::remote_video` in `pump_call_engine` behind the same
  active-call gate as transport, and keeping only the latest frame per
  (call id, is_local) in `video_frame_slots` (cleared when the tracked
  call ends, so the UI can never render a stale picture). The connect
  path already refreshes the device cache before `call_connect_params`,
  which now opts into video via the session-1 pure helper
  `video_wanted`: `video_enabled` + `camera_input` from the first
  enumerated camera, overridden by the user's `selected_camera`. New
  driver controls: `set_call_camera(call_id, enabled)` (engine-first
  error contract mirroring `set_call_muted` — a failed native call never
  flips the flag), `select_call_camera` (stores the pick and re-applies
  the camera on the active call), `selected_call_camera`,
  `latest_video_frame(call_id, is_local)`, and the honest
  `call_video_ready()` (active video call + available engine + a camera
  exists) that the UI uses for disabled states instead of guessing.
  `ActiveCall` gains `camera_on` (initialized from `is_video` at both
  reducer construction sites) and `remote_video` (default `Inactive`).
- **Built:** UI (`src/ui/mod.rs`). `call_video_stage` renders the peer's
  camera as the main tile and the local preview as a 160x120 PiP anchored
  bottom-right (absolute positioning, mirroring the existing
  `#call-backdrop` usage). Frames decode through `video_render_image`
  (`RgbaImage::from_raw` -> `image::Frame::new` -> `RenderImage::new`,
  exactly the `decode_viewer_frames` pattern; malformed bytes -> `None`,
  never garbage) and are cached by frame seq so re-renders don't
  re-decode. Tiles degrade honestly: peer "Connecting video…" /
  "Video paused by peer" / "Peer's camera is off" (name + initials
  placeholder), local "Starting camera…" / "Camera off". The Ready
  video-call button row gains the camera on/off toggle — shown only when
  `call_video_ready()` (live) or in demo mode, otherwise the muted text
  "No camera available". The device pickers gain a Camera row with radio
  selection for video calls ("No camera found." when the engine reported
  other devices but no camera); demo mode drives the same tiles from
  injected synthetic frames. The call tick drops to 100ms while a video
  call is Ready with a live feed (peer streaming or camera effectively
  on), 1s otherwise. `call_overlay` / `call_card` / `call_active_card`
  changed `&self` -> `&mut self` for the image cache; verified the call
  sites sit inside render methods.
- **Schema citations (no new TDLib calls):** `call.is_video` at
  `schema/td_api.tl:7287`; `callStateReady` at `:7068`; `updateCall` at
  `:10816`; `updateNewCallSignalingData` at `:10862`;
  `createCall`/`sendCallSignalingData` at `:14211`/`:14218`. A
  concept-level search (td_api.tl + raw telegram_api.tl + TDLib source)
  found no video-frame/state constructors beyond `is_video` — established
  in session 1, unchanged in session 2.
- **`ntg_add_incoming_video` negative claim (carried from session 1):**
  search strategy was C++ source inspection of the pinned ntgcalls
  v3.0.0 tree: the method is declared only on `GroupCall`
  (`ntgcalls/include/ntgcalls/instances/group_call.hpp:32-34`); the public
  `NTgCalls::add_incoming_video` casts via `safe_call` to `GroupCall`,
  which throws on a P2P call; `P2PCall::connect` auto-adds the incoming
  camera track, so 1:1 peer frames arrive unsolicited through
  `ntg_on_frames_callback` (mode=PLAYBACK, device=CAMERA). Never called
  on this path.
- **References:** peer camera state mirrors Telegram X's `VideoState`
  annotation (`INACTIVE=0, PAUSED=1, ACTIVE=2`, verified at
  `~/workspace/telegram-x/app/src/main/java/org/thunderdog/challegram/voip/annotation/VideoState.java`);
  frames are I420 planar converted to RGBA8 in session 1 (full-range
  BT.601, rotation already applied); camera toggle re-issues CAPTURE
  sources with the camera description present/absent (`ntg_pause` /
  `ntg_resume` are global and never used for the camera); the camera
  description is `NTG_MEDIA_SOURCE_DEVICE` 640x480@30, the conventional
  tgvoip P2P default.
- **Deferred:** group-call video, screen sharing, camera selection
  persistence across calls, external frame injection. 1:1 verification
  emojis are still parsed and shown only for group calls, so
  `parity:calls-verify-emoji` stays unchecked.
- **Not verifiable without a live peer:** the full native path — real
  camera enumeration on the user's machine, `ntg_set_stream_sources`
  accepting the camera description, peer MediaState -> stream status,
  actual frame delivery and rotation handedness against a real camera.
  Tested instead: driver outbox/slot plumbing against `MockEngine` (7 new
  tests, all green), pure `video_wanted` (session 1), and the
  synthetic-frame screenshot below
  (`docs/screenshots/ready-call-video.png`, frames generated in code —
  NOT a real camera).
- **Out of this slice:** E2E testing against a real peer (CI has no live
  Telegram), 1:1 verification-emoji UI, screen sharing, group-call video,
  call recording, camera selection persistence across calls, per-peer
  video quality controls.
## Phase C2f — group voice-chat participant management (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `inviteGroupCallParticipant group_call_id:int32 user_id:int53 is_video:Bool = InviteGroupCallParticipantResult;` (:14375). The result is NOT `Ok` — four variants at lines 7216–7227: `inviteGroupCallParticipantResultUserPrivacyRestricted`, `...UserAlreadyParticipant`, `...UserWasBanned`, and `inviteGroupCallParticipantResultSuccess chat_id:int53 message_id:int53`. All four are parsed; non-success results surface as the group-call error line.
  - `declineGroupCallInvitation chat_id:int53 message_id:int53 = Ok;` (:14380).
  - `banGroupCallParticipants group_call_id:int32 user_ids:vector<int64> = Ok;` (:14385) — plural constructor, vector of *user* ids (channel senders / `messageSenderChat` cannot be banned; the UI shows no Ban button for them). Requires `groupCall.is_owned` — the owner can ban; `can_be_managed` is "for video chats and live stories only" and does NOT grant ban rights in a voice chat. Caveat: the method is documented for calls "not bound to a chat" while Quill's voice chats are chat-bound — live server behavior unverified.
  - `setGroupCallParticipantVolumeLevel group_call_id:int32 participant_id:MessageSender volume_level:int32 = Ok;` (:14438) — volume is 1–20000 (hundreds of percents); the driver clamps before sending.
  - `messageGroupCall unique_id:int64 is_active:Bool was_missed:Bool is_video:Bool duration:int32 other_participant_ids:vector<MessageSender> = MessageContent;` (line 5288). Schema note: incoming, inactive, non-missed messages show an incoming screen — accept via `joinGroupCall` (`inputGroupCallMessage`), decline via `declineGroupCallInvitation`. The history row follows exactly that rule; sent/missed/active variants render as a neutral service notice.
- **Built:** request builders (`invite_group_call_participant`, `ban_group_call_participants`, `set_group_call_participant_volume_level`) + driver methods with gates (no active call / not owner → `InvalidRequest`; accept refuses while a 1:1 call is active). `ActiveGroupCall.rejoin_attempts` + auto-rejoin on `need_rejoin`: one attempt per ingest tick, `joinVideoChat` with the honest no-device params and retained self-mute, max 3 attempts (the C2d discipline), manual Rejoin resets the counter, a clean joined `updateGroupCall` resets attempts + banner + stale error. UI: invite picker (loaded contacts, in-call users excluded), per-row volume stepper (−/+ 10%, shows current %), owner-gated Ban button, incoming-invitation Accept/Decline row, and the previously write-only `group_call_error` now renders on the overlay with a dismiss. New screenshot demos `ready-group-call-invite` and `ready-group-call-invitation`.
- **Key decisions:** reuse `MessageSenderRef` and the existing group-call request/driver patterns — no new picker abstraction; the volume UI is a stepper, not a slider entity (the slice says "slider/control"); invite `is_video` follows the tracked call; ban is user-only per the schema; a failed rejoin re-arms `reconnecting` so ingest retries, and exhaustion writes "Reconnect attempts exhausted." while keeping manual Rejoin available.
- **Not verifiable without a live Telegram group call:** the actual TDLib request/response round-trips (`inviteGroupCallParticipant` result variants, ban, volume, accept/decline, rejoin `joinVideoChat` acceptance), `updateGroupCallParticipants` behavior after a ban, and the incoming `messageGroupCall` → `joinGroupCall` accept flow against a real active call. Tested instead: request shapes + gates + volume clamping (driver unit tests), the full auto-rejoin discipline incl. exhaustion and reset (driver unit tests with injected `error` envelopes), and envelope parsing of `messageGroupCall` + all four invite-result variants.
- **Postscript (same day):** the `joinGroupCall` success shape was missing — `groupCallInfo` (`group_call_id:int32 join_payload:string`, schema :7190) had no parser, so invitation acceptance sent the request but dropped the answer. Added `EnvelopePayload::GroupCallInfo` + a reducer arm mirroring the `GroupCallId`/`createVideoChat` pattern: for `JoinGroupCallInvitation` it queues a `getGroupCall` fetch (tracking starts even if `updateGroupCall` lags), stores the tgcalls join payload, and clears the error. `updateGroupCall` remains the source of truth for `is_joined`.
- **Out of this slice:** live group video frames, screen-share start, recording/RTMP, title/schedule/invite-link UI, in-call group messages.

## Phase C2g — group video tiles + group-call screen sharing (2026-09-27)

- **Built:** TDLib parsing (`src/telegram/envelope.rs`).
  `GroupCallVideoSourceGroup { semantics, source_ids }` and
  `GroupCallVideoInfo { endpoint_id, is_paused, source_groups }` parse
  `groupCallParticipantVideoInfo`; `ParsedGroupCallParticipant` retains
  `video_info` / `screen_sharing_video_info` alongside the existing
  `video_enabled` / `screen_sharing_enabled` booleans.
- **Built:** engine API (`src/calls/engine.rs`). `VideoFrame` gains
  `participant_user_id: Option<i64>` + `is_screen: bool`; new
  `GroupVideoSource { user_id, endpoint, ssrc_groups }` /
  `GroupVideoSourceGroup { semantics, ssrcs }`; pure
  `group_offer_audio_source_id` (first `a=ssrc:` line of the audio `m=`
  section, 0 when absent — the join is never blocked). `CallEngine`
  gains `create_group_call` / `connect_group_call` / `sync_group_video`
  / `set_group_camera` / `start_screen_share` / `connect_screen_share`
  / `stop_screen_share` / `leave_group_call`; `MockEngine` records all
  of them. The native `NtgcallsEngine` keys group state by TDLib group
  call id (internally the ntgcalls chat id): `ntg_create_call`,
  `ntg_connect(..., false)`, `ntg_add_incoming_video` /
  `ntg_remove_incoming_video` (the returned sink is deliberately
  ignored — routing is ssrc-keyed, removal is endpoint-keyed via the
  endpoints map, so one endpoint's removal never drops another endpoint
  of the same user), `ntg_init_presentation` +
  `ntg_connect(..., true)` + `ntg_set_stream_sources` with
  `NTG_MEDIA_SOURCE_DESKTOP` (NULL input = default display) for screen
  sharing, `ntg_stop_presentation`, `ntg_stop`. Group camera capture
  re-issues capture sources like the 1:1 path. The frame callback routes
  group frames (native chat id -> group call id, frame ssrc -> user id,
  `NTG_STREAM_DEVICE_SCREEN` -> `is_screen`) into the driver's group
  slots; P2P routing is untouched.
- **Built:** driver (`src/connect.rs`). `join_video_chat` /
  `rejoin_group_call` share `group_join_params`, which resolves the chat
  id through `chat.video_chat.group_call_id` (the schema's `groupCall`
  carries no chat id), creates the native context first, and sends the
  real offer + parsed audio SSRC — degrading to the honest no-device
  params when no engine is available or the offer fails (the join is
  never blocked). `pump_group_call_transport` runs after every reducer
  update: finishes `ntg_connect` when the `joinVideoChat` `Text` answer
  lands (exactly once; `transport_ready`), re-issues the camera on flag
  changes, reconciles incoming subscriptions from the participants'
  video info on every pump (the engine diffs add/remove), finishes the
  presentation handshake when the `startGroupCallScreenSharing` answer
  lands, and tears the transport + frame slots down when the tracked
  call ends or is replaced. Frames live in `group_video_frame_slots`
  keyed `(group_call_id, user_id, is_screen)`; `latest_group_video_frame`
  serves the UI; leave/end clear eagerly. `ActiveGroupCall` gains
  `transport_ready` / `transport_error` / `screen_share_pending` /
  `screen_sharing` / `screen_share_answer`; new `RequestPurpose`s
  `StartGroupCallScreenSharing` / `EndGroupCallScreenSharing` with the
  `Text`/`Ok` reducer handling (failures clear the flags and surface an
  honest `group_call_error`). `toggle_group_call_screen_share` is gated
  on an enumerated `MediaDeviceKind::Screen` device (refreshed on group
  connect) — without one the toggle is rejected and the UI shows "No
  screen source available." instead of a dead button.
- **Built:** UI (`src/ui/mod.rs`). Participant tiles render the latest
  retained frame (screen share preferred when sharing, else camera)
  through the existing `video_render_image` path with a per-slot
  `(call_id, user_id, is_screen)` seq cache; the avatar placeholder
  stays when no frame exists. "Share screen" / "Stop sharing" sits next
  to the group video controls (gated on the screen-source check).
  `group_call_overlay` / `group_call_card` / `group_call_joined_card` /
  `group_call_participant_tile` changed `&self` -> `&mut self` for the
  image cache. The ready-group-call demo fixture gives Zed (41) a camera
  `video_info` and Mia (42) a `screen_sharing_video_info` plus synthetic
  test-pattern frames (injected demo data, labeled as such).
- **Schema citations (TDLib 1.8.67, verified in `schema/td_api.tl`):**
  `groupCallJoinParameters.audio_source_id` "received from tgcalls" at
  `:7085`; `groupCallVideoSourceGroup` at `:7157`;
  `groupCallParticipantVideoInfo` at `:7163`; participant camera +
  screen-sharing info fields at `:7184`; `startGroupCallScreenSharing`
  at `:14303`; `endGroupCallScreenSharing` at `:14309`;
  `joinVideoChat` returning `Text` (unchanged from C3a).
- **ntgcalls findings (v3.0.0 bindings, compile-time only):**
  `create(chatId)` returns the WebRTC SDP offer, `connect(..., false)`
  consumes Telegram's normal answer, and `init_presentation` +
  `connect(..., true)` is the documented screen-sharing sequence
  (corroborated by the ntgcalls N-API docs: `api-reference.md`,
  `quick-start.md`). The C++ source confirms `add_incoming_video` is a
  `GroupCall`-only method (see the C2e-session-2 note above) — the
  per-endpoint sink it returns is deliberately ignored: routing is
  ssrc-keyed and removal is endpoint-keyed via the endpoints map, so one
  endpoint's removal never drops another endpoint of the same user.
- **Not verifiable without a live group call:** the full native path —
  real `ntg_create_call` offer generation, `ntg_connect` against
  Telegram's answer, `ntg_add_incoming_video` subscription behavior,
  actual frame delivery/ssrc attribution, desktop capture availability
  on a real display. The vendored `libntgcalls.so` is present and loads
  on this VM, so even the bindings are exercised only against the mock
  here — end-to-end issuance needs real hardware. Tested instead: SDP SSRC
  parsing (pure), video-info parsing, the offer/answer/subscription/
  frame-slot/presentation lifecycle against `MockEngine` (8 new driver
  tests + 2 engine tests + 1 envelope test, all green), and the
  synthetic-frame screenshot below (`docs/screenshots/ready-group-call.png`
  — generated test-pattern pixels, NOT real video).
- **Review fixes (same slice):** `sync_group_video` bookkeeping
  hardened — the endpoints map now stores the full `GroupVideoSource`
  (was endpoint->user only), so removing one endpoint drops only its
  own ssrcs (a camera unpublish no longer kills the same user's screen
  share), changed ssrc groups re-subscribe instead of going stale, and
  the callback ssrc-map prune keeps other group calls' entries
  (keys are `(chat_id, ssrc)`). The ignored `ntg_add_incoming_video`
  sink is deliberate: routing is ssrc-keyed and removal is
  endpoint-keyed; no binding consumes the sink. The driver pump now
  reconciles the native presentation against the tracked flags via
  `presentation_active`, so a failed `startGroupCallScreenSharing`
  request or a bad answer can't leave a stray initialized presentation.
  The toggle is gated on an enumerated `MediaDeviceKind::Screen` device
  ("No screen source available." when absent) — end-to-end feasibility
  of real desktop capture is unverifiable without a real display
  (the vendored `libntgcalls.so` is present and loads).
- **Review fixes (second pass, 2026-09-27):**
  1. Participant tile rendered the initials avatar twice (once above the
     video/avatar area, once as the avatar fallback) — the redundant
     outer avatar is deleted; `docs/screenshots/ready-group-call.png`
     was regenerated after the fix.
  2. `rejoin_group_call` replaced the native context via
     `create_group_call` but never reset `transport_ready`, so the pump's
     join-answer filter dropped the new `joinVideoChat` answer and the
     transport never reconnected. The rejoin now resets
     `transport_ready`/`join_payload`, clears the screen-share flags,
     and stops an orphaned presentation before the media entry is
     replaced. Covered by
     `connect::tests::group_rejoin_answer_reconnects_native_transport`
     (verified to fail without the reset).
  3. `NtgcallsEngine::leave_group_call` called `ntg_stop` but never
     `ntg_stop_presentation` — a live presentation now stops first
     (privacy: capture ends before the call). Mock mirrors it via
     `screen_share_stops`; the leave driver test asserts it.
  4. The joined-card transport note claimed voice ("Voice connected.")
     while this slice carries no audio (mic/speaker sources are null) —
     reworded to video/transport copy ("Video connected." /
     "Connecting…" / "Video failed to connect."), and "Join to connect
     voice." became "Join to connect video." "Voice chat" as the chat
     entity name is kept.
  Nits: `stop_screen_share` now uses `ensure_instance()`; `sink`
  findings wording corrected (ignored, not fed into the callback map).
- **`calls-group-video-pause` gap:** `group_video_sources` skips
  `is_paused` endpoints entirely, so the schema's "ignore `is_paused`
  if new video frames are received" (td_api.tl:7162) can't be honored —
  a paused endpoint never subscribes, so frames can't arrive to
  override it. Kept as a gap.
- **Out of this slice:** recording/RTMP, video-chat title/schedule/
  invite-link UI, in-call group messages, 1:1 screen sharing
  (`parity:calls-screen-share` stays unchecked — group screen sharing is
  a separate new item below), per-participant volume, group-call
  invites/bans, the local-user camera preview tile (frames are dropped
  at the native callback; see `parity:calls-group-video-self`), E2E
  testing against a live group call.
## Phase C2h — video-chat management: title, schedule, invite link, recording, RTMP, in-call chat (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `createVideoChat chat_id:int53 title:string start_date:int32 is_rtmp_stream:Bool = GroupCallId;` (:14256). `//@title Group call title; if empty, chat title will be used`. `//@start_date Point in time (Unix timestamp) when the group call is expected to be started by an administrator; 0 to start the video chat immediately. The date must be at least 10 seconds and at most 8 days in the future`. `startScheduledVideoChat group_call_id:int32 = Ok;` (:14277). `//@description Starts a scheduled video chat` — the earlier "no startGroupCall exists in the schema" claim was wrong (the `sendStory`/`postStory` failure mode): the constructor exists and admins (`can_be_managed`) can start a scheduled call early via the scheduled card's Start-now button; otherwise the call goes live via `updateGroupCall`/`updateNewVideoChat`. `toggleVideoChatEnabledStartNotification group_call_id:int32 enabled_start_notification:Bool = Ok;` (:14280, "Toggles whether the current user will receive a notification when the video chat starts; for scheduled video chats only") is OUT OF THIS SLICE.
  - `setVideoChatTitle group_call_id:int32 title:string = Ok;` (:14312). `//@description Sets title of a video chat; requires groupCall.can_be_managed right ... @title New group call title; 1-64 characters`.
  - `getVideoChatInviteLink group_call_id:int32 can_self_unmute:Bool = HttpUrl;` (:14395). `//@description Returns invite link to a video chat in a public chat ... //@can_self_unmute Pass true if the invite link needs to contain an invite hash, passing which to joinVideoChat would allow the invited user to unmute themselves. Requires groupCall.can_be_managed right`.
  - `revokeGroupCallInviteLink group_call_id:int32 = Ok;` (:14398). `//@description Revokes invite link for a group call. Requires groupCall.can_be_managed right for video chats or groupCall.is_owned otherwise`.
  - `startGroupCallRecording group_call_id:int32 title:string record_video:Bool use_portrait_orientation:Bool = Ok;` (:14405). `//@description Starts recording of an active group call; for video chats only. Requires groupCall.can_be_managed right ... //@title Group call recording title; 0-64 characters ... //@record_video Pass true to record a video file instead of an audio file`. `endGroupCallRecording group_call_id:int32 = Ok;` (:14408, same gate).
  - `getVideoChatRtmpUrl chat_id:int53 = RtmpUrl;` (:14261). `//@description Returns RTMP URL for streaming to the video chat of a chat; requires can_manage_video_chats administrator right`. `replaceVideoChatRtmpUrl chat_id:int53 = RtmpUrl;` (:14264). `//@description Replaces the current RTMP URL for streaming to the video chat of a chat; requires owner privileges in the chat`. `rtmpUrl url:string stream_key:string = RtmpUrl;` (:7113). Regenerate is owner-gated; fetch is admin-gated (closest tracked flag: `groupCall.can_be_managed`; a 403 surfaces honestly via the group-call error line).
  - `sendGroupCallMessage group_call_id:int32 text:formattedText paid_message_star_count:int53 = Ok;` (:14341). `//@description Sends a message to other participants of a group call. Requires groupCall.can_send_messages right ... //@text Text of the message to send; 1-getOption("group_call_message_text_length_max") characters for non-live-stories ... //@paid_message_star_count The number of Telegram Stars the user agreed to pay to send the message; for live stories only` (always 0 in voice/video chats). `toggleGroupCallAreMessagesAllowed group_call_id:int32 are_messages_allowed:Bool = Ok;` (:14322). `//@description Toggles whether participants of a group call can send messages there. Requires groupCall.can_toggle_are_messages_allowed right`.
  - `groupCallMessage message_id:int32 sender_id:MessageSender date:int32 text:formattedText paid_message_star_count:int53 is_from_owner:Bool can_be_deleted:Bool = GroupCallMessage;` (:7200). Updates: `updateNewGroupCallMessage group_call_id:int32 message:groupCallMessage = Update;` (:10839), `updateGroupCallMessageSendFailed group_call_id:int32 message_id:int32 error:error = Update;` (:10851), `updateGroupCallMessagesDeleted group_call_id:int32 message_ids:vector<int32> = Update;` (:10856).
  - `groupCall` flags (:7154, docs :7147–7152): `//@can_send_messages True, if the current user can send messages to the group call`; `//@are_messages_allowed True, if sending of messages is allowed in the group call`; `//@can_toggle_are_messages_allowed True, if the current user can enable or disable sending of messages in the group call`; `//@can_delete_messages True, if the user can delete messages in the group call`; `//@record_duration Duration of the ongoing group call recording, in seconds; 0 if none. An updateGroupCall update is not triggered when value of this field changes, but the same recording goes on`; `//@is_video_recorded True, if a video file is being recorded for the call`; `//@scheduled_start_date Point in time (Unix timestamp) when the group call is expected to be started by an administrator; 0 if it is already active or was ended; for video chats only`.
  - **Negative claim (concept-level, per schema discipline):** there is NO group-call message history getter, verified on all three levels at the pinned commit `d1085f9` (TDLib 1.8.67): (1) `td_api.tl` concept search over every `group_call` × `message` method found only `sendGroupCallMessage` (send), `deleteGroupCallMessages` / `deleteGroupCallMessagesBySender` (delete — both documented "for live story calls only", :14359/:14365), `toggleGroupCallAreMessagesAllowed`, `setGroupCallPaidMessageStarCount` (:14419), and the live updates `updateNewGroupCallMessage` / `updateGroupCallMessageSendFailed` / `updateGroupCallMessagesDeleted`; (2) raw layer `td/generate/scheme/telegram_api.tl` at `d1085f9`: the full `phone.*` method list (:2993–:3035) contains only `phone.sendGroupCallMessage` (:3030), `phone.sendGroupCallEncryptedMessage` (:3031), `phone.deleteGroupCallMessages` (:3032), `phone.deleteGroupCallParticipantMessages` (:3033) — no getter; messages arrive solely via `updateGroupCallMessage` (:490), `updateGroupCallEncryptedMessage` (:491), `updateDeleteGroupCallMessages` (:494); (3) TDLib source `td/telegram/GroupCallManager.cpp` at `d1085f9`: the inner `GroupCallMessages` class (:1629) is a local in-memory dedup/tracking map (`server_ids_`, `random_ids_`, `message_info_`) for messages arriving via updates — the manager emits only `updateNewGroupCallMessage` (:3628), `updateGroupCallMessagesDeleted` (:3662), `updateGroupCallMessageSendFailed` (:3695) and issues no history-fetch request. Telegram X (local TGX-Android source, `~/workspace/telegram-x`): `GroupCallListener.java:23–26` forwards exactly the three live updates and the app has no voice-chat message-history UI or getter — consistent with the schema. Backfill is therefore impossible through TDLib; the in-call chat is necessarily a live feed.
- **Built:** request builders (`revoke_group_call_invite_link`, `start_group_call_recording`, `end_group_call_recording`, `start_scheduled_video_chat`, `get_video_chat_rtmp_url`, `replace_video_chat_rtmp_url`, `send_group_call_message`, `toggle_group_call_are_messages_allowed`; `create_video_chat` now forwards `start_date`) + envelope parsers (`RtmpUrl`, `ParsedGroupCallMessage`, the three message updates, message/recording/schedule fields on `ParsedGroupCall`) + driver methods with flag gates (`start_video_chat` validates title ≤64 chars and the 10s–8d schedule window; `start_scheduled_video_chat` gated on `can_be_managed` for still-scheduled calls; recording/revoke gated on `can_be_managed` for video chats; send gated on `can_send_messages && are_messages_allowed`; toggle gated on `can_toggle_are_messages_allowed`; RTMP fetch admin-gated, regenerate owner-gated). `ActiveGroupCall` gained `scheduled_start_date`, `rtmp_url`/`rtmp_stream_key`, the message flags, a capped (200) live `messages` feed with dedup and delete handling, and recording fields. UI: Start/Schedule dialog (now, +1h/+3h/+12h/+24h presets with human "Starts in …" labels), scheduled-call card (start time + admin-only Start-now button; Join appears once TDLib activates the call), admin row (Record/Stop, Stream key, Chat on/off), invite link row with Copy/Revoke, recording indicator ("● Recording (video) 2:05"), RTMP panel with Copy + owner-only Regenerate, and the in-call chat section (live messages with sender names, composer gated on the flags, honest empty/disabled states). New screenshot demo `ready-group-call-manage`.
- **Key decisions:** reuse the existing request/driver/state/UI patterns — no new abstractions; `record_video: true` (video file) is the recording default — the start button just says "● Record" and the in-call indicator shows "● Recording (video)"/"(audio)" once the recording is live, so there is no video-labeled start affordance; the 4096-char client-side message cap is a sanity guard only (the true cap is `getOption("group_call_message_text_length_max")`, server-enforced); the scheduled card offers Start now to admins (`startScheduledVideoChat`) and Join once TDLib activates the call; recording duration display may go stale because the schema states `updateGroupCall` is not triggered when `record_duration` changes mid-recording.
- **Not verifiable without a live Telegram group call:** every actual TDLib round-trip (create/schedule, set title, invite link fetch/revoke, recording start/stop, RTMP fetch/regenerate, message send + echo/delivery), scheduled-call activation via `updateNewVideoChat`/`updateGroupCall`, and admin/peer-visible effects (invite link works for others, recording actually records, RTMP stream is ingestible). Tested instead: request shapes + gates + validation ranges (driver unit tests), message routing/dedup/deletion + scheduled tracking + RTMP caching (state unit tests), envelope parsing of `rtmpUrl` / message updates / recording fields, and the full UI surface in the `ready-group-call-manage` screenshot demo (injected, no live Telegram).
- **Out of this slice:** live group video frames, screen-share start, message deletion in calls (schema restricts `deleteGroupCallMessages` to live stories), paid-message star counts, RTMP streaming from Quill itself, `toggleVideoChatEnabledStartNotification` (:14280, notify-me-when-a-scheduled-video-chat-starts toggle).

## Phase C2i — call history + call settings (2026-09-27)

Schema discipline: every TDLib request below was verified
concept-level against the pinned `schema/td_api.tl` (1.8.67); Telegram
X (`~/workspace/telegram-x`) was the behavior reference for call
labels (short `TD.getCallName` variants: outgoing-missed →
"Cancelled", incoming-missed → "Missed", outgoing-declined → "Busy",
incoming-declined → "Declined") and for listing both call privacy and
P2P privacy in the privacy settings.

- **Recent calls are server-backed.** `searchCallMessages
  offset:string limit:int32 only_missed:Bool = FoundMessages;`
  (schema/td_api.tl:11903) searches call and group-call messages,
  newest first; the result is `foundMessages total_count:int32
  messages:vector<message> next_offset:string = FoundMessages;`
  (:3172). No local fake call log — the Calls tab's first page is
  `searchCallMessages("", 40, false)`, with "Load more" continuing
  from the returned `next_offset`. Reducer appends pages to
  `Session::recent_calls` (request-shape tests cover the wire format).
- **Call messages.** `messageCall unique_id:int64 is_video:Bool
  discard_reason:CallDiscardReason duration:int32 = MessageContent;`
  (:5277); reasons `callDiscardReasonMissed` (:6987),
  `callDiscardReasonDeclined` (:6990),
  `callDiscardReasonDisconnected` (:6993), `callDiscardReasonHungUp`
  (:6996) (plus Empty :6984 / Upgrade-to-group :6999). In-chat
  `messageCall` rows render the reason-aware label with duration and
  a "Call again" button for 1:1 chats (group calls have no single
  peer — the button is hidden).
- **Rating detail.** `sendCallRating call_id:InputCall rating:int32
  comment:string problems:vector<CallProblem> = Ok;` (:14234).
  Problems: `callProblemEcho` (:7253), `callProblemNoise` (:7256),
  `callProblemInterruptions` (:7259),
  `callProblemDistortedSpeech` (:7262), `callProblemSilentLocal`
  (:7265), `callProblemSilentRemote` (:7268), `callProblemDropped`
  (:7271), `callProblemDistortedVideo` (:7274),
  `callProblemPixelatedVideo` (:7277). The star tap no longer sends
  immediately — it opens the detail editor (problems chips +
  optional comment); Submit sends the full request. The old
  stars-send-immediately flow is replaced.
- **Call log upload.** `sendCallLog call_id:InputCall
  log_file:InputFile = Ok;` (:14240). The schema accepts only
  `inputFileLocal` / `inputFileGenerated` for the log, so the driver
  writes the honest local diagnostics payload to the account exports
  dir and sends it as `inputFileLocal` (`inputFileLocal path:string
  = InputFile;` at :325). The end screen shows an "Upload
  call log" button when `need_log` is set, with success/error state.
  (Existing `sendCallDebugInformation` inline-text upload is kept for
  `need_debug_information`.)
- **Call privacy.** `userPrivacySettingAllowCalls` (:9006) and
  `userPrivacySettingAllowPeerToPeerCalls` (:9009) via
  `getUserPrivacySettingRules setting:UserPrivacySetting =
  UserPrivacySettingRules;` (:15620) and `setUserPrivacySettingRules
  setting:UserPrivacySetting rules:userPrivacySettingRules = Ok;`
  (:15617). Rules map to Everybody → `userPrivacySettingRuleAllowAll`
  (:8943), My Contacts → `userPrivacySettingRuleAllowContacts`
  (:8946), Nobody → `userPrivacySettingRuleRestrictAll` (:8961).
  Sets are applied optimistically; on error the optimistic value is
  cleared (`None`) and the section shows "Couldn't load privacy
  settings" until the tab refetches — the reducer's
  `SetCallPrivacyRules` error arm clears the optimistic value and
  flags `call_privacy_error`, and the next successful
  `getUserPrivacySettingRules` restores the truth.
- **Confirm before calling.** Persisted per-account
  (`call_prefs.json`): the preference-aware `start_call_for_user`
  stashes the pending call and shows a confirm dialog; the actual
  `startCall` only fires on Confirm. Works from profiles, history
  rows, and the end screen's "Call again".
- **Less data for calls.** Persisted the same way, but the toggle is
  explicitly labeled "saved here — the call engine doesn't support
  it yet": the current native layer exposes no data-saving control
  (Telegram X's less-data is engine config, not a TDLib setting).
  README stays unchecked.
- **Busy-call honesty.** No TDLib hold/swap request exists
  (concept-level schema search; native `ntg_pause` /
  `ntg_resume` only pause local media — not a Telegram-level swap).
  Incoming calls during an active call are still auto-declined via
  `discardCall`, but now recorded in
  `Session::call_busy_declined` and surfaced as a dismissible banner
  ("Missed calls from X — declined because another call was
  active."). `parity:calls-swap-prompt` stays unchecked.
- **Out of this slice:** proxy-for-calls (schema exposes
  add/edit/enable/disable proxy but no per-call field; Telegram X
  feeds it to the native VoIP engine client-side — our native layer
  has no such binding), echo-cancellation / noise-suppression
  toggles (`ntgcalls-sys` exposes none), call verification emojis
  for 1:1 calls (already parsed for group calls).
## Phase S1 — SECRET-CHAT PARITY (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `messageScreenshotTaken = MessageContent;` (:5375). `//@description A screenshot of a message in the chat has been taken` (:5374).
  - `viewMessages chat_id:int53 message_ids:vector<int53> source:MessageSource force_read:Bool = Ok;` (:13230). `messageSourceScreenshot = MessageSource;` (:3233–3234, `//@description The message was screenshotted; the source must be used only if the message content was visible during the screenshot`).
  - `linkPreviewOptions` (:2237) and `inputMessageText ... link_preview_options:linkPreviewOptions ...` (:6081).
  - `toggleSessionCanAcceptSecretChats session_id:int64 can_accept_secret_chats:Bool = Ok;` (:15117).
  - `secretChat id:int32 user_id:int53 state:SecretChatState is_outbound:Bool key_hash:bytes layer:int32` (:2816).
  - `createNewSecretChat user_id:int53 = Chat;` (:13340). `closeSecretChat secret_chat_id:int32 = Ok;` (:15242).
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`, strings at `app/src/main/res/values/strings.xml`):** `AwaitingEncryption` "Waiting for %1$s to get online…" (:943); `YouTookAScreenshot` "You took a screenshot" / `XTookAScreenshot` "%1$s took a screenshot" (:1157–1158); `EncryptionKeyDescription` "This image and text were derived from the encryption key for this secret chat with %1$s.\n\nIf they look the same on %1$s's device, end-to-end encryption is guaranteed." (:1707); `SecretChatFeatureUnsupported` "%1$s's Telegram client doesn't support this feature. They need to install an update first." (:2544); `NotificationChannelSecretChat` "Custom notification settings for the Secret Chat with %1$s." (:2695); `DeleteSecretChatPendingConfirm` "Are you sure you want to cancel the secret chat with %1$s?" (:3018), `DeleteSecretChatClosedConfirm` "…delete the secret chat with %1$s? This action cannot be undone." (:3019), `DeleteSecretChatConfirm` "…delete the secret chat with %1$s? All chat history will be deleted forever. This action cannot be undone." (:3020); `SecretPasscodeInfo` (:3081); `SecretChatForwardError` "This message cannot be forwarded to secret chats." (:3729). TGX's screenshot-send mechanism (`Tdlib.java:4775`) is `client().send(new TdApi.ViewMessages(chatId, messageIds, new TdApi.MessageSourceScreenshot(), false), ...)` — there is no dedicated screenshot-send function in td_api.tl (concept search for `screenshot` finds only the three received content/source types above, zero functions). TGX gates round video notes on `secretChat.layer >= 66` (`chatSupportsRoundVideos`, `RecordAudioVideoController.java`). TGX's session toggle is per-session ("Session Accepts → Secret Chats", `EditSessionController`), not a general phone-number privacy toggle. The E2E explainer (`MessagesHolder` TYPE_SECRET_CHAT_INFO) is "Secret Chats" + four bullets: "Use end-to-end encryption", "Leave no trace on our servers", "Have a self-destruct timer", "Do not allow forwarding".
- **Built:** `messageScreenshotTaken` parse → `MessageContent::ScreenshotTaken` (envelope.rs), chat-list preview "Took a screenshot", centered service row "You took a screenshot" (own) / "{name} took a screenshot" (peer, TGX strings verbatim, ui/mod.rs), excluded from composer edit; secret chats send with explicit `linkPreviewOptions{is_disabled:true}` (with `@type`, which TDLib's JSON interface requires on every object) while ordinary chats send `null` (requests.rs, connect.rs detects `ChatKind::Secret`); "🔒 New secret chat" sidebar entry + eligible-contact picker (non-bot, non-self, shared `can_start_secret_chat_with` gate with the profile button); close-confirm copy per state (pending/closed/ready, TGX strings verbatim with the peer's name); "Waiting for {name} to get online…" header subtitle for pending chats (plus the existing composer note); TGX `EncryptionKeyDescription` verbatim under the fingerprint grid; real app-rendered E2E explainer for empty secret chats (the demo's fake injected explainer messages were removed — `apply_ready_secret_chat` now injects no messages); two-sided forward gate — destination side with TGX's `SecretChatForwardError` string (picker stays open), source side hiding the Forward/Select buttons on secret-chat rows (TGX's `canBeForwarded` gate, `MessagesController.java:5287`); video-note send gated on `secretChat.layer >= 66` with the `SecretChatFeatureUnsupported` string; "Custom notification settings for the secret chat with {name}." label in the existing per-chat notification panel (which already applies to secret chats); per-chat passcode hint (`SecretPasscodeInfo` verbatim) in the user info panel; `toggle_session_can_accept_secret_chats` request builder (request layer only — no sessions UI exists).
- **Key decisions:** ponytail throughout — reuse existing `Button`, contacts-list row styling, service-row pattern, and status-note error line; no new abstractions. The picker is a sidebar-expanding panel (Quill has no main menu) with toggle-button close, consistent with the forward picker's existing UX (no dedicated Escape). `can_start_secret_chat_with` is a shared associated fn so profile and picker can't drift. The forward gate is two-sided, matching TGX exactly: destination-side `ShareController.java:1896–1900` blocks forwarding TO secret chats (`SecretChatForwardError`, verbatim) gated on the source message's `canBeCopiedToSecretChat`; source-side `MessagesController.java:5287` only offers Forward when `messageProperties.canBeForwarded` (false for secret-chat messages). Quill doesn't parse `messageProperties`, so the source gate uses the chat kind — the Forward/Select buttons are hidden on secret-chat rows. (An earlier draft of this section claimed forwarding out was allowed because "the wire copy is local" — wrong: TGX gates both sides, and so does Quill.)
- **Concept-level blockers (verified, not single-grep claims):**
  - `secret-screenshot-send`: the TDLib API EXISTS — TGX's mechanism is `viewMessages` with `messageSourceScreenshot` (td_api.tl:13230/:3234). The blocker is OS-level: Linux desktop (X11/Wayland) provides no screenshot-detection API to trigger it (no equivalent of Android's MediaStore observer that feeds TGX's `onScreenshotTaken`). Unchecked.
  - `secret-screenshot-block`: no OS-level screen-capture prevention API on Linux — no `FLAG_SECURE` equivalent on X11/Wayland (verified via web research 2026-09-27; Windows has `WDA_EXCLUDEFROMCAPTURE`, macOS has `NSWindow.sharingType`, Linux has nothing). Unchecked.
  - `secret-bot-alert`: Quill has no inline-query flow — `insert_switch_inline_query` only types "@bot query" into the composer; no `getInlineQueryResults`/results path exists to gate, so TGX's `SecretChatContextBotAlert` has no trigger point. Unchecked, out of slice.
  - `secret-session-accept`: request builder exists; Quill has no sessions UI (no `getActiveSessions` surface), so TGX's per-session "Session Accepts → Secret Chats" toggle has no parent screen. Unchecked.
  - `secret-storage-category`: Quill has no storage-usage surface at all, so the TGX `SecretFiles` category has no parent screen. Unchecked, out of slice.
  - `secret-session-terminate`: no sessions UI, so TGX's `ClosingXSecretChats` strings have no parent screen. Unchecked, out of slice.
  - Lock-screen notification privacy (TGX `HideSecret`/`ShowSecretOn`): out of slice — Linux desktop exposes no lock-screen notification API to the app; documented as a limitation.
  - Per-chat passcode setting: hint only; a real per-chat passcode needs a full app passcode feature — out of slice.
  - Link-preview opt-in alert (TGX `SecretLinkPreviewAlert`): deferred — default-off is the S1 scope.
- **Not verifiable without a live peer:** screenshot service rows arriving via real `updateNewMessage` (parsed from fixture-shaped JSON instead), `linkPreviewOptions{is_disabled:true}` round-trip through TDLib, `createNewSecretChat`/`closeSecretChat` effects, `toggleSessionCanAcceptSecretChats` acceptance, pending→ready state transitions from real `updateSecretChat`. Tested instead: envelope parsing of `messageScreenshotTaken`, request shapes (`send_text` secret vs ordinary, `toggle_session_can_accept_secret_chats`), and the full UI surface in the `ready-secret-chat` and `ready-secret-picker` screenshot demos (injected, no live Telegram).
- **Out of this slice:** everything listed under Concept-level blockers, link-preview opt-in alert, per-chat passcode setting, lock-screen notification privacy, E2E testing against a live secret-chat peer.

## Phase S2 — SECRET-CHAT REMAINDERS (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `getStorageStatistics chat_limit:int32 = StorageStatistics;` (:15781). `//@description Returns storage usage statistics. Can be called before authorization` (:15779).
  - `storageStatistics size:int53 count:int32 by_chat:vector<storageStatisticsByChat> = StorageStatistics;` (:9793). `//@description Contains the exact storage usage statistics split by chats and file type` (:9789).
  - `storageStatisticsByChat chat_id:int53 size:int53 count:int32 by_file_type:vector<storageStatisticsByFileType> = StorageStatisticsByChat;` (:9787).
  - `storageStatisticsByFileType file_type:FileType size:int53 count:int32 = StorageStatisticsByFileType;` (:9780).
  - `fileTypeSecret = FileType;` (:9728). `//@description The file was sent to a secret chat (the file type is not known to the server)` (:9727).
  - `fileTypeSecretThumbnail = FileType;` (:9731). `//@description The file is a thumbnail of a file from a secret chat` (:9730).
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):** `SecretChatContextBotAlert` "Please note that inline bots are provided by third-party developers. For the bot to work, the symbols you type after the bot\\'s username are sent to the respective developer." (`app/src/main/res/values/strings.xml:2638`); trigger at `helper/InlineSearchContext.java:792-800` — inline search with an empty query in a secret chat (`ChatId.isSecret`), one-time tutorial (`TUTORIAL_INLINE_SEARCH_SECRECY`, persisted in `Settings`), single `Confirm` button, `ALERT_NO_CANCEL | ALERT_NO_CANCELABLE`, title `AppName`. `SecretFiles` "Secret media and files" (`strings.xml:1679`); `ui/SettingsCacheController.java:612-616` maps `TGStorageStats.FILE_TYPE_SECRET` ← `TdApi.FileTypeSecret` to it; `data/TGStorageStats.java:107-123` maps every `fileType` constructor, with `FileTypeSecretThumbnail` going to the internal database bucket (not the Secret category).
- **Built:**
  - `secret-bot-alert`: a `SwitchInline` button press is Quill's only inline-bot invocation point (no `getInlineQueryResults`/`sendInlineQueryResultMessage` path exists), so the TGX alert gates `insert_switch_inline_query` — the single call site, so all callers route through it. In a secret chat the first press stashes the query and shows the warning banner above the composer (TGX copy verbatim, single Confirm, no Cancel); Confirm inserts the query and sets the once-per-session flag. Non-secret chats are unaffected (TGX's `isSecret` check).
  - `secret-storage-category`: full minimal pipeline — `get_storage_statistics` request builder (requests.rs, `chat_limit` 0 since the overlay aggregates by file type), `RequestPurpose::GetStorageStatistics`, `storageStatistics` envelope parse aggregating `by_chat[].by_file_type[]` into per-`fileType` totals (saturating, skips typeless entries), `Session::storage_stats` + `storage_stats_loading` cache (purpose-matched by `@extra`, like `NotificationSounds`), `ConnectDriver::maybe_fetch_storage_statistics` (guarded: once per session unless refreshed) + `refresh_storage_statistics`. New "💾 Storage usage" sidebar entry opens an overlay dialog (backdrop + Close + Refresh) listing the total and TGX-ordered categories with counts and sizes, including "Secret media and files" for `fileTypeSecret`; unknown types and `fileTypeSecretThumbnail` fold into "Other" (TGX puts secret thumbnails in its internal database bucket — "Other" is the honest minimal equivalent). Zero-size categories are skipped, as TGX skips them.
- **Key decisions:** ponytail throughout — the alert reuses the existing composer banner pattern (Quill's other secret-chat confirms are banners, not TGX's modal; both block before proceeding); the storage overlay reuses the `notification_defaults_overlay` dialog chrome and the existing `format_bytes` helper; category order/labels are one const + one match, no new abstractions. The one-time alert flag is per app session while TGX persists it — documented divergence (Quill has no tutorial-flag store). TGX's dialog title is the app name ("Telegram"); Quill's is "Quill".
- **Not verifiable without a live peer:** the real `getStorageStatistics` round trip (parsed from fixture-shaped JSON instead; the reducer test pins purpose-matching and the secret-category aggregation), the `SwitchInline` press in a live secret chat (the gate is UI-only; screenshotted in the injected `ready-secret-bot-alert` demo). Tested instead: request shape (`get_storage_statistics`), envelope parse + schema-pin tests (verbatim constructor lines), reducer cache behavior incl. stray-answer rejection, and both new UI surfaces in injected screenshot demos (no live Telegram).
- **Out of this slice:** `secret-screenshot-send` / `secret-screenshot-block` (unchanged blockers), `secret-session-accept` / `secret-session-terminate` (no sessions UI — future auth-sessions slice), everything else.
## Phase M1 — MESSAGING CORE: compose formatting + message actions (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `textEntityTypeBold` (:5743); `textEntityTypeItalic` (:5746); `textEntityTypeUnderline` (:5749); `textEntityTypeStrikethrough` (:5752); `textEntityTypeSpoiler` (:5755); `textEntityTypeCode` (:5758); `textEntityTypePre` (:5761); `textEntityTypePreCode language:string` (:5764); `textEntityTypeBlockQuote` (:5767); `textEntityTypeExpandableBlockQuote` (:5770); `textEntityTypeTextUrl url:string` (:5773).
  - `messageSchedulingStateSendAtDate send_date:int32 repeat_period:int32 = MessageSchedulingState;` (:5902); `messageSchedulingStateSendWhenOnline` (:5905–5906).
  - `messageSendOptions suggested_post_info:inputSuggestedPostInfo disable_notification:Bool from_background:Bool protect_content:Bool allow_paid_broadcast:Bool paid_message_star_count:int53 update_order_of_installed_sticker_sets:Bool scheduling_state:MessageSchedulingState effect_id:int64 sending_id:int32 only_preview:Bool = MessageSendOptions;` (:5934).
  - `sendMessage chat_id:int53 topic_id:MessageTopic reply_to:InputMessageReplyTo options:messageSendOptions reply_markup:ReplyMarkup input_message_content:InputMessageContent = Message;` (:12200).
  - `linkPreviewOptions is_disabled:Bool url:string force_small_media:Bool force_large_media:Bool show_above_text:Bool = LinkPreviewOptions;` (:2237); `inputMessageText text:formattedText link_preview_options:linkPreviewOptions clear_draft:Bool = InputMessageContent;` (:6081).
  - `forwardMessages chat_id:int53 topic_id:MessageTopic from_chat_id:int53 message_ids:vector<int53> options:messageSendOptions send_copy:Bool remove_caption:Bool = Messages;` (:12237).
  - `deleteMessages chat_id:int53 message_ids:vector<int53> revoke:Bool = Ok;` (:12282); capability comments `//@can_be_deleted_only_for_self` (:6228), `//@can_be_deleted_for_all_users` (:6229).
  - `pinChatMessage chat_id:int53 message_id:int53 disable_notification:Bool only_for_self:Bool = Ok;` (:13559); `unpinChatMessage` (:13562); `unpinAllChatMessages chat_id:int53 = Ok;` (:13565).
  - `getMessageLink chat_id:int53 message_id:int53 media_timestamp:int32 checklist_task_id:int32 poll_option_id:string for_album:Bool in_message_thread:Bool = MessageLink;` (:12064); `messageLink link:string is_public:Bool` (:9666).
  - `resendMessages chat_id:int53 message_ids:vector<int53> quote:inputTextQuote paid_message_star_count:int53 = Messages;` (:12251); `//@can_retry True, if the message can be re-sent using resendMessages` (:3038).
  - `getChatScheduledMessages chat_id:int53 = Messages;` (:12000). Scheduled messages are `Messages`; editing them is the same `editMessageText`/`editMessageCaption` request as normal messages (schema declares no sent-only restriction on those functions), validated against the scheduled list.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):** `InputView.java` (~:273–301) exposes clear, bold, italic, spoiler, underline, strikethrough, monospace, link, and quote controls in the composer input row — the reference for the toolbar button set. Clear-format availability handled at ~:314.
- **Built:**
  - Visible-markup composer formatting (`composer.rs`): `**bold**`, `*italic*`/`_italic_`, `__underline__`, `~~strikethrough~~`, `` `code` ``, fenced ```pre/pre-code (optional language after the opening fence), `||spoiler||`, `> quote` lines, `[label](url)`. Toolbar (bold, italic, underline, strike, code, pre, spoiler, quote, link, clear) wraps the selection / inserts at the cursor; Ctrl-B / Ctrl-I / Ctrl-U shortcuts; clear-format strips all markup. Markup stays visible in the composer and is converted to TDLib UTF-16-offset entities only at send/edit time (`send_text`, `edit_message_text` in `requests.rs`). Single-pass, non-nesting parser by design (ponytail: no hidden entity-range bookkeeping).
  - Send options (`SendOptions` + `ComposerScheduling`): silent send (`disable_notification`), schedule at date / when online (`messageSchedulingStateSendAtDate` / `messageSchedulingStateSendWhenOnline`), link-preview disable (`linkPreviewOptions.is_disabled`). `sendMessage` now always sends a non-null `messageSendOptions` (schema declares the field non-nullable). Toggles are session-only (reset on restart — nothing is written to prefs); scheduling is one-shot and resets after a successful send. Secret chats force previews off (S1 behavior, now flowing through the same options path).
  - Scheduled messages: `getChatScheduledMessages` request (`RequestPurpose::ScheduledList`), UTC-labeled dialog with per-row Delete and Edit (edit routes through the existing `ComposerEdit` composer flow with `scheduled: true`, validated against `session.scheduled_messages`, then the same `editMessageText`/`editMessageCaption` request). Delete uses `deleteMessages` with `revoke: false` on the scheduled list. Schedule popup: +1h / +8h / +24h, send-when-online, clear schedule, open scheduled list.
  - Delete: `DeleteConfirm` gained `revoke` + `can_revoke`; incoming messages are now deletable for the current user (`revoke: false`); the banner toggles "for me / for everyone" and only offers "for everyone" when `can_revoke`. Revoke is only ever sent for outgoing messages.
  - Forward: `ForwardDraft` gained `send_copy` + `remove_caption` (remove-caption only enabled with send-copy), wired into `forwardMessages`.
  - Pin: `pinChatMessage` `disable_notification` follows the silent-send toggle; "Unpin all" button in the pinned banner when more than one loaded history row is pinned (`unpinAllChatMessages`).
  - Message actions: right-click context menu (Reply, Copy, Forward, Pin/Unpin, Share link, Retry failed send, Delete); leftward drag > 24px starts a reply (swipe-to-reply); copy supports text and captions; `getMessageLink` results go to the clipboard; failed sends (`updateMessageSendFailed`) are marked red with a retry hint and retry via `resendMessages`.
- **Key decisions:** ponytail — visible markup instead of invisible entity ranges (no offset bookkeeping across edits); one `scheduled: bool` flag on `ComposerEdit` reuses the entire edit flow instead of a second edit path; `messageLink` result drains to clipboard through the existing live event pump; the silent-send toolbar toggle doubles as the silent-pin toggle (both are `disable_notification` on the wire).
- **Not verifiable without live Telegram:** formatting entities round-trip, silent/scheduled sends, link-preview toggle, delete for-me vs everyone, forward send-copy/remove-caption, silent pin, unpin-all, share-link clipboard flow, resendMessages retry, scheduled list/edit/delete, swipe-to-reply and right-click menus (all UI-state and request-shape tests pass; the wire beyond that is live-only).
- **Out of this slice:** quote-in-reply (`inputTextQuote` still `null` on sends; the README box stays unchecked), hidden inline formatting while typing (markup stays visible by design), per-language code-block picker (language is typed after the opening fence), link URL dialog (URL is typed into the `[]()` scaffold), scheduled-message caption editing is supported via the same edit flow for caption kinds where applicable.

## Slice G1 — GROUPS & CHANNELS CORE (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `createNewBasicGroupChat user_ids:vector<int53> title:string = CreatedBasicGroupChat;` (:13327); `createNewSupergroupChat title:string is_forum:Bool is_channel:Bool description:string location:chatLocation message_auto_delete_time:int32 for_import:Bool = Chat;` (:13337 — takes no initial member IDs; members are added after creation with `addChatMembers`).
  - `toggleSupergroupIsBroadcastGroup supergroup_id:int53 = Ok;` (:15221) — one-way upgrade, no boolean; description: "Upgrades supergroup to a broadcast group; requires owner privileges". CAUTION: `ChatManager::can_convert_channel_to_gigagroup` (tdlib/td `td/telegram/ChatManager.cpp:3925` — verified 2026-09-27 against tdlib/td master, where the function and the cited line numbers hold verbatim) is self-contradictory — its body returns true on every blocking condition (unknown channel, non-megagroup, non-creator, already gigagroup, default permissions not fully restricted) while its name and the `if (!can_...) return error` call site (`ChatManager.cpp:3668`) assume the opposite polarity; sibling `can_*` predicates in the same file use the sane convention. Do not cite its exact precondition list. Reliable signals: the schema (owner privileges) and TGX `Tdlib.suggestConvertToBroadcastGroup` (non-channel supergroup, not already broadcast, creator, plus the server-sent `suggestedActionConvertToBroadcastGroup`). The driver gates on owner + not-already-broadcast and rolls the optimistic flag back on any TDLib error. Known limitation: Telegram X additionally gates the offer on the server-sent `suggestedActionConvertToBroadcastGroup` (near-capacity eligibility); Quill offers conversion to every eligible owner and relies on the server to reject ineligible ones, rolling the optimistic flag back on error.
  - `addChatMember chat_id:int53 user_id:int53 forward_limit:int32 = FailedToAddMembers;` (:13578 — basic groups, one request per user, each answering `failedToAddMembers` with 0 or 1 failures); `addChatMembers chat_id:int53 user_ids:vector<int53> = FailedToAddMembers;` (:13584 — supergroups/channels; `failedToAddMembers failed_to_add_members:vector<failedToAddMember> = FailedToAddMembers` :3640 — the client counts `failed_to_add_members.len()`; the single bulk response replaces the count while per-user basic-group responses accumulate under a distinct request purpose).
  - `getBasicGroupFullInfo basic_group_id:int53 = BasicGroupFullInfo;` (:11507); `basicGroupFullInfo photo:chatPhoto description:string creator_user_id:int53 members:vector<chatMember> invite_link:chatInviteLink bot_commands:vector<botCommands> = BasicGroupFullInfo;` (:2714 — full member list, no filters).
  - `getSupergroupMembers supergroup_id:int53 filter:SupergroupMembersFilter offset:int32 limit:int32 = ChatMembers;` (:15238); filters `supergroupMembersFilterRecent` (:2559), `supergroupMembersFilterSearch` (:2568), `supergroupMembersFilterAdministrators` (:2565), `supergroupMembersFilterRestricted` (:2571), `supergroupMembersFilterBanned` (:2574) — real server-side pages (search tab = All+query per TGX).
  - `setChatMemberStatus chat_id:int53 member_id:MessageSender status:ChatMemberStatus = Ok;` (:13592); `chatMemberStatusRestricted` (:2510), `chatMemberStatusBanned` (:2517).
  - `setChatMemberTag chat_id:int53 user_id:int53 tag:string = Ok;` (:13598) — "Changes the tag or custom title of a chat member; requires can_manage_tags administrator right to change tag of other users; for basic groups and supergroups only". Tag: 0-16 characters, no emoji (:13597). **This is the admin custom-title setter** (verified against Telegram X `EditRightsController`, which drives the "Custom title" field through `TdApi.SetChatMemberTag`; channels return false in `canViewOrEditCustomTitle`). `chatMember tag:string` (:2526); `chatAdministrator custom_title` (:2482).
  - `setChatPermissions chat_id:int53 permissions:chatPermissions = Ok;` (:13464); `chatPermissions` is 16 fields (:1070).
  - `replacePrimaryChatInviteLink chat_id:int53 = ChatInviteLink;` (:14089); `toggleSupergroupJoinByRequest supergroup_id:int53 join_by_request:Bool = Ok;` (:15188).
  - `setSupergroupUsername supergroup_id:int53 username:string = Ok;` (:15136 — owner only; empty clears).
  - `deleteChat chat_id:int53 = Ok;` (:11850), gated by `chat.can_be_deleted_for_all_users`; leave = `leaveChat` for basic groups, supergroups, and channels (schema :13572 — "Removes the current user from chat members").
  - `inputTextQuote text:formattedText position:int32 = InputTextQuote;` (:3056); `inputMessageReplyToMessage message_id:int53 quote:inputTextQuote checklist_task_id:int32 poll_option_id:string = InputMessageReplyTo;` (:3086); quote positions are UTF-16 code units.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):** `ProfileController.java` `convertToBroadcastGroup` confirms the one-way broadcast upgrade (red destructive confirm → `ToggleSupergroupIsBroadcastGroup` → success alert → refresh); `Tdlib.java` `deleteOrLeaveChat` confirms leave semantics (basic/supergroup → `SetChatMemberStatus` left + `DeleteChatHistory`; channel → `LeaveChat`); `EditRightsController.java` confirms custom titles go through `SetChatMemberTag` with 16-char/emoji client validation and are hidden for channels.
- **Built:**
  - Creation: "New group" / "New supergroup" / "New channel" sidebar entries → dialog (title + contact picker + description for supergroups/channels + forum toggle for supergroups) → `createNewBasicGroupChat` / `createNewSupergroupChat` → supergroup members added after creation via `addChatMembers`; partial add failures surface `failedToAddMembers` counts.
  - Members: info-panel "Manage members" (supergroups/channels) / basic-group info panel (basic groups) — real lists: `getBasicGroupFullInfo` (all members) and `getSupergroupMembers` with All/Administrators/Restricted/Banned tabs + search; add-members picker (`addChatMember` for basic groups, `addChatMembers` bulk for supergroups/channels) with privacy-failure notice; restrict/ban/unban via `setChatMemberStatus` with the Restricted/Banned dialogs (duration cycler); custom titles via `setChatMemberTag` (16-char/emoji client validation, shown next to admin rows).
  - Permissions: default-permissions editor — 16 `ChatPermissions` checkboxes → `setChatPermissions`, gated on `chat.can_edit_permissions`.
  - Invite links & join requests: primary invite link displayed + replace (`replacePrimaryChatInviteLink`); "Approve new members" toggle (`toggleSupergroupJoinByRequest`).
  - Usernames: public username editor (`setSupergroupUsername`, owner only, empty clears).
  - Broadcast: owner-only "Convert to broadcast group" with TGX-style destructive confirm; one-way (schema has no reverse; TGX has none either).
  - Delete/leave: "Leave"/"Delete" with confirm; delete gated by `can_be_deleted_for_all_users`.
  - Partial quotes: message menu → "Quote reply" on text/caption messages → dialog to pick the quoted part → validated against the live message text (UTF-16 offset via `quote_position`) → `ComposerReplyTo.quote` → `SendReply` → `inputMessageReplyToMessage.quote = inputTextQuote` on every send path (text, media, albums, voice notes, GIFs/stickers, polls). Reply banner shows ❝quote❞. Drafts round-trip quotes: `ChatDraft.quote: Option<(String, i32)>` (text + UTF-16 position) parsed from `inputTextQuote`, restored into `ComposerReplyTo::with_quote` on draft restore; draft save/flush paths carry the full `SendReply`. Evidence: raw schema `draftMessage#60fe3294` carries `reply_to:flags.4?InputReplyTo` (:897) and `inputReplyToMessage#3bd4b7c2` carries `quote_text`/`quote_entities`/`quote_offset` (:1760); TDLib `MessagesManager::create_message_input_reply_to` keeps `MessageQuote{td_, quote_}` for `inputMessageReplyToMessage` when `for_draft=true`; TGX `MessagesController.updateDraftMessage` restores `replyToMessage.quote` into `showReply`, and its draft save stores `replyTo.toInputMessageReply()` (quote included) in the `DraftMessage`.
  - Non-editable administrators: `chatMemberStatusAdministrator.can_be_edited` is parsed; member Restrict/Ban controls are hidden for the current user, creators, and non-editable administrators (TGX `ProfileController`/`TD.canPromoteAdmin` treat those targets as view-only).
  - Error honesty: failed `setChatMemberTag`/`setChatMemberStatus` record a dialog-visible `Session::member_action_error` (the member dialog reads member caches, not `admin_lists`, where the error used to go); basic-group per-user `addChatMember` errors count into `add_members_failed` (reset on each new add, cleared on dialog close); optimistic `setChatPermissions` / `toggleSupergroupJoinByRequest` / `setSupergroupUsername` carry their pre-request values on `PendingRequest::rollback` and restore them on a TDLib error; failed broadcast upgrades drop the optimistic flag; failed primary-link replacement marks the invite-link fetch failed.
- **Key decisions:** ponytail — one generic `TextPromptKind` dialog for username + custom title instead of two dialog structs; `SendReply` (message_id + optional quote) replaces bare `MessageId` at send boundaries; basic groups reuse the supergroup member-fetch state type (`SupergroupMembersFetch`) rather than a second cache type; `addChatMember` added alongside `addChatMembers` because the bulk variant is supergroup/channel-only.
- **Not verifiable without live Telegram:** every TDLib round-trip (creation, member add/remove, restrict/ban/unban, permissions, invite-link replace, join toggle, username set, custom title, broadcast upgrade, delete/leave, quote-carrying sends, draft quote save/restore) — request shapes and state transitions are unit-tested; the wire beyond that is live-only.
- **Out of this slice:** event-log filters, author-signature toggle, forum topic management, channel comments, anti-spam, boosts (per G1 scope); deleting chat history when leaving a group (TGX `deleteOrLeaveChat` clears history for basic groups/supergroups — this client leaves membership only).

## Slice G2 — GROUPS & CHANNELS REMAINDER + WELCOME MESSAGES (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - Event log: `chatEventLogFilters` (:7956); `getChatEventLog chat_id:int53 query:string from_event_id:int64 limit:int32 filters:chatEventLogFilters user_ids:vector<int53> = ChatEvents;` (:15252). Note the schema's `user_ids` is the server-side per-admin filter — Quill does NOT use it; see the client-side decision below.
  - Channel signatures: `toggleSupergroupSignMessages supergroup_id:int53 sign_messages:Bool show_message_sender:Bool = Ok;` (:15175).
  - Anti-spam: `toggleSupergroupHasAggressiveAntiSpamEnabled supergroup_id:int53 has_aggressive_anti_spam_enabled:Bool = Ok;` (:15212); capability/state in `supergroupFullInfo` (:2792).
  - Forum: `createForumTopic` (:12665) answers `forumTopicInfo`; `editForumTopic` (:12674); `toggleForumTopicIsClosed` (:12713) — close AND reopen are the same constructor with the boolean; `toggleGeneralForumTopicIsHidden` (:12718); `toggleForumTopicIsPinned` (:12725) — pin and unpin are the same constructor; `deleteForumTopic` (:12736). `forumTopicInfo.is_hidden` (:3953) is parsed for the General topic's hidden state.
  - Comments: `getMessageThread chat_id:int53 message_id:int53 = MessageThreadInfo;` (:11566); `getMessageThreadHistory chat_id:int53 message_id:int53 from_message_id:int53 offset:int32 limit:int32 = Messages;` (:11839).
  - Boosts: `getAvailableChatBoostSlots = ChatBoostSlots;` (:13914); `getChatBoostStatus chat_id:int53 = ChatBoostStatus;` (:13917); `boostChat chat_id:int53 slot_ids:vector<int32> = ChatBoostSlots;` (:13922) — answers `chatBoostSlots`, NOT `ok` (this was a real bug in the first implementation, which dropped the cached status under the `Ok` payload arm; fixed 2026-09-27).
  - Welcome messages: `welcomeMessage` (:6839); `loadChatWelcomeMessages chat_id:int53 = Ok;` (:12632); `addChatWelcomeMessage` (:12639), `editChatWelcomeMessage` (:12646), `deleteChatWelcomeMessage` (:12651), `deleteAllChatWelcomeMessages` (:12654); updates `updateChatWelcomeMessages` (:10649) / `updateChatHasWelcomeMessages` (:10600) — NOT rendered joiner-side in this slice; admin right `can_send_welcome_messages` (:1090/1092).
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`, bounded search 2026-09-27):** `ProfileController.java` exposes two channel toggles — "Sign messages" and "Show message authors" — and sends `ToggleSupergroupSignMessages(supergroup_id, signMessages, signMessages && showAuthors)`, gated to channels with `can_change_info`; disabling signatures forces show-authors off, which Quill mirrors. Anti-spam appears in TGX only when full info permits toggling. `MessagesController` uses actual thread context for comments. `EditRightsController` shows the welcome-message admin right. No complete forum-management or welcome-editor UI was found in the bounded search; the pinned TDLib schema is authoritative there.
- **Built:**
  - Event log: query text search + `chatEventLogFilters` category chips sent through `getChatEventLog`; per-admin filter chips filter the loaded page client-side (see decision); deduped `ChatEventLogPage::admin_user_ids()` helper (first-seen order, skips `MessageSender::Chat`) with a pure unit test.
  - Channel settings: "Sign messages" + "Show message authors" toggles (`toggleSupergroupSignMessages`, disabling sign forces show off per TGX); "Aggressive anti-spam" toggle for groups (`toggleSupergroupHasAggressiveAntiSpamEnabled`), gated on the full-info capability.
  - Forum management: dialog with create (default blue `0x6FB9F0` icon — custom icons out of scope), rename (edit renames only), close/reopen, pin/unpin, delete, General hide/show.
  - Channel comments: message-menu "View comments" → `getMessageThreadHistory` dialog; success/failure ingestion tested at driver level.
  - Boosts: status section (`getChatBoostStatus` → level/count) + "Boost" action that chains `getAvailableChatBoostSlots` → `boostChat` with the first slot id, then refetches status.
  - Welcome messages: admin pack editor — load/add/edit/delete, text only (`ParsedWelcomeMessage` keeps text).
- **Key decisions:**
  - Per-admin event filtering is client-side: `getChatEventLog` does accept a `user_ids:vector<int53>` server filter, but the current implementation filters the loaded page per-admin client-side instead — sufficient for the box; server-side filtering is a possible later improvement.
  - Confirmed-success refetch (fixed 2026-09-27): the UI used to call `refresh_forum_topics` immediately after sending a mutation — pre-confirmation, racing the server. Now the driver captures the mutation purpose before `apply` takes the pending request and refetches after `apply` confirms: the dropped cache is the success signal, so a failed mutation (cache kept) refetches nothing. Same for welcome packs and boost status. `Session::requests.get()` (non-destructive pending view) was added for this.
  - `boostChat` answers `chatBoostSlots`, not `ok`: the status drop lives in the `ChatBoostSlots` payload arm keyed on `RequestPurpose::BoostChat`; the old `Ok`-arm branch was dead and deleted. Driver test covers slots → boost → `chatBoostSlots` → status invalidation → `getChatBoostStatus` refetch.
  - Text-only welcome support; forum custom icons out of scope; comment rows show only content/outgoing/signature (no invented sender/date).
- **Not verifiable without live Telegram:** every TDLib round-trip (filters, signature toggles, anti-spam, all forum mutations, thread history, boost chain, welcome load/mutations) — request shapes, reducer transitions, rollback, and cache invalidation are unit- and driver-tested; the wire beyond that is live-only.
- **Out of this slice:** `parity:groups-welcome-view` — joiner-side rendering of `updateChatWelcomeMessages` is not implemented (admin pack management is done); forum custom topic icons; forum editing beyond rename.

## Slice groups-welcome-view — JOINER-SIDE WELCOME RENDERING (2026-09-28)

Concept-level investigation (td_api.tl + raw telegram_api.tl + TDLib C++ source at the pinned 1.8.67 commit `d1085f9`, + Telegram X) showed the README item as framed describes a mechanism that does not exist for joiners — no new code was needed:

- **Pinned schema (`schema/td_api.tl`):** `welcomeMessage` (:6839); `updateChatWelcomeMessages` (:10649, "The list of welcome messages of a chat has changed"); `updateChatHasWelcomeMessages` (:10600); `chat.has_welcome_messages` (:3627 — "for chat administrators with can_change_info administrator right only"); `loadChatWelcomeMessages` (:12632 — "requires can_send_welcome_messages administrator right in the chat"); mutations (:12639/12646/12651/12654, all require the right). The `message` type (:3165) has NO welcome flag.
- **Raw layer (`telegram_api.tl` at pinned commit):** `ephemeralMessage#dd27bee9` carries `welcome_template:flags.5?true` (:2260); `ephemeral.sendMessage` has `welcome:flags.7?true` + `receiver_id` (:3134 — the admin's client stores a template with `welcome=true`, `receiver=inputUserEmpty`); `ephemeral.getWelcomeMessages` (:3141).
- **TDLib source at pinned commit:** `UpdatesManager.cpp:3793` routes raw `updateNewEphemeralMessage` with `welcome_template_=true` to `WelcomeMessageManager::on_new_welcome_message`, everything else to `MessagesManager::on_new_ephemeral_message`. `WelcomeMessageManager.cpp:477-478` DROPS template pushes unless the pack was loaded (`loaded_welcome_messages_`), and loading is server-gated on `can_send_welcome_messages` — so a plain joiner never receives `updateChatWelcomeMessages`. Non-template ephemeral messages go through `MessagesManager::on_new_ephemeral_message` (:4289) → `on_get_message` → a regular td_api `updateNewMessage` in the dialog.
- **Telegram X:** `Tdlib.java:7537` `updateChatWelcomeMessages` handler is literally `// TODO?` — the reference client renders no pack UI joiner-side (or anywhere); the only welcome UI is the admin right in `EditRightsController`.
- **Decision:** joiner-side "rendering" is the existing message pipeline — the server delivers welcome content to a new joiner as `updateNewEphemeralMessage` (`welcome_template=false`, per-user so it is visible only to the joiner), TDLib surfaces it as `updateNewMessage`, and Quill renders it like any message (text/media/entities/reply markup all preserved through `parse_ephemeral_message`). Building a pack banner out of `updateChatWelcomeMessages` would be wrong: joiners never receive it. README box checked with this corrected framing; nothing to screenshot (no new UI).
- **Not verifiable without live Telegram:** the server actually pushing the per-joiner ephemeral updates on `joinChat` — server behavior, identical for all TDLib clients; the 13th-anniversary announcement confirms joiners see the messages.

## Slice M2 — ANNIVERSARY MESSAGING (2026-09-27)

Telegram's 13th-anniversary post (2026-08-25, official blog) added: rich
formatting in welcome messages, in-message buttons pairing with ephemeral
content for automated flows, inline documents/files/music inside text, and a
rich editor that opens via ⛶ after typing more than 3 lines. All six boxes
are TDLib 1.8.67 schema-native — verified concept-level per the schema
discipline (td_api.tl + raw telegram_api.tl + TDLib source), not single-name
greps.

- **Schema evidence (pinned TDLib 1.8.67, `schema/td_api.tl`):**
  - `richMessage` (:123) — `is_full`, `is_rtl`, `blocks:vector<pageBlock>`;
    partial messages carry `is_full == false`.
  - `inputRichMessage` (:149); `richMessageSourceBlocks` / `richMessageSourceRawText` sources.
  - `ephemeralMessageContent` (:3115) — `content:messageContent`,
    `reply_markup:replyMarkup`; `message.ephemeral_content` (:3161/:3165).
  - `richTextButton` (:4111) — inline button inside rich text; `inlineButton`
    (:4030) with `buttonStyle*` and `inlineKeyboardButtonType*`.
  - `pageBlockDocument` (:4272); `pageBlockButtonRow` (:4364).
  - `messageRichMessage` (:5143); `inputPageBlockDocument` (:6023);
    `inputPageBlockButtonRow` (:6071); `inputMessageRichMessage` (:6084).
  - `getFullRichMessage chat_id:int53 message_id:int53 = RichMessage`
    (:11554) — fetches the full blocks of a partial rich message.
- **Official-blog behavior (2026-08-25 post):** rich formatting works in
  welcome messages; buttons can appear inside messages and pair with
  ephemeral content for automated flows; inline documents/files/music can
  appear within text; the rich editor opens via ⛶ after typing more than
  3 lines.
- **Built (`src/rich.rs`, new):**
  - Incoming: `RichText` flattening (nested styled runs → plain text +
    M1 `TextEntity` spans, `richTextButton` → `InlineKeyboardButton`);
    `pageBlock*` → `RichBlock` (headings, paragraphs, lists incl.
    checklists, collapsible, documents, tables, button rows, dividers;
    anchors/unknown blocks parse to `Empty`/`Unsupported` — rendered as
    nothing, never fake content).
  - Outgoing: M1 markup/entities → `RichText` per block (`block_rich_text`
    reuses `parse_format_markup`, so the format toolbar stays meaningful);
    `RichBlock` → `inputPageBlock*`; `input_rich_message` builds
    `inputRichMessage` (`detect_automatic_blocks: true`).
  - Editor markup: `#`/`##`/`###` headings, `-`/`*`/`•` and `1.` lists,
    `[]`/`[x]` checklists, `>>` collapsible, `---` dividers
    (`markup_to_blocks`).
  - `inlineButton.text` is a `RichText`, unlike
    `inlineKeyboardButton.text` (plain string): `parse_inline_button`
    flattens the label first, then reuses the existing style/type parser.
- **Driver (`connect.rs`, `requests.rs`, `state.rs`, `envelope.rs`):**
  - `send_rich_message` / `get_full_rich_message` request builders with
    exact-shape unit tests (`inputMessageRichMessage` +
    `clear_draft:true`; `getFullRichMessage` ids).
  - `send_rich_snapshot` (same channel-post + closed-topic gates as
    `send_snapshot`); honest failure — the composer keeps the draft and
    shows the error, never maps refusal to success; no optimistic local
    row (M1's optimistic send is text-only).
  - `fetch_full_rich_message` + `RequestPurpose::GetFullRichMessage`;
    the driver captures the `richMessage` answer before `apply` and
    replaces only the matching `RichMessage` row's blocks in history.
  - `messageRichMessage` / `message.ephemeral_content` parse;
    `effective_content` prefers ephemeral content; the row renders the
    ephemeral `reply_markup` instead of the message's own.
  - `MessageContent::preview` returns the first text-ish block
    ("Rich message" fallback); rich messages are excluded from the legacy
    text/caption edit path (edit resend goes through the normal text
    flow — rich re-edit is out of slice).
- **UI (`ui/mod.rs`):**
  - Rich editor: ⛶ button appears in the composer after 3+ lines
    (official behavior); rich mode reuses the textarea + format toolbar,
    adds block buttons (H1/H2/list/checklist/collapsible/divider append
    markup templates) and a live block preview; send routes
    `inputMessageRichMessage` via `send_rich_snapshot`; editor state is
    retained on send failure. Inline documents attach through the existing
    explicit local-file picker — never a TDLib-provided `local.path`.
  - Renderer: `message_rich_block` stacks headings/lists/collapsible/
    documents/tables/dividers; styled text reuses `rich_text_line`;
    paragraph buttons and `pageBlockButtonRow` reuse
    `inline_keyboard_button`, so URL/callback/switchInline/copy taps keep
    their existing honest behavior (unknown types render disabled).
  - Screenshot demos: `ReadyRichMessage` (blocks + document + buttons +
    ephemeral override), `ReadyRichEditor` (editor open with preview).
- **Key decisions:**
  - `inputPageBlockSectionHeading.size` is schema-documented (:5978): 1-6,
    1 is the largest — the editor's H1/H2/H3 map to sizes 1/2/3, and incoming
    `pageBlockSectionHeading` (:4213) sizes clamp to levels 1-3.
  - `is_rtl` is a known simplification: `parse_rich_message` ignores the
    `richMessage.is_rtl` flag and `input_rich_message` always sends `false`
    — Hebrew/Arabic rich messages render LTR. RTL layout is out of slice.
  - Collapsible blocks render expanded with an indented body; no
    collapse toggle in this slice (queued).
  - The editor is markup-source based (the composer textarea is the
    source of truth); a WYSIWYG block editor is out of slice.
  - The editor stores markup text in blocks and the send path re-parses
    it (`block_rich_text`); the live preview resolves markup separately
    (`preview_blocks` → clean text + `TextEntity`, UTF-16→UTF-8 converted).
    Paragraphs/headings preview styled; list items and collapsible
    header/body preview marker-stripped plain — matching how Quill
    renders those blocks everywhere (incoming `pageBlockListItem.label`
    is a plain string, schema :4143).
  - Ephemeral content overrides for render, copy-source selection, and
    reply markup — the regular content is still stored (history/search
    keep working on it).
- **Not verifiable without live Telegram:** every TDLib round-trip
  (rich send, `getFullRichMessage` fetch, callback taps on rich buttons,
  ephemeral delivery) — request shapes, reducer transitions, and the
  no-optimistic-row / retain-on-failure behavior are unit- and
  driver-tested; the wire beyond that is live-only.
- **Out of this slice:** collapsible toggle; rich-message re-edit;
  WYSIWYG block editing; ephemeral countdown/expiry UI; RTL layout for rich
  messages (`is_rtl` renders LTR, see above).

## Slice MED2 — MEDIA RECORDING & VOICE (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `videoNote.speech_recognition_result` (:614-616) and `voiceNote.speech_recognition_result` (:622-624) carry `speechRecognitionResult` (:7390-7399): `speechRecognitionResultPending partial_text`, `speechRecognitionResultText text`, `speechRecognitionResultError error`. Parsed into `envelope::SpeechRecognition`; null/unknown stay `None`.
  - `recognizeSpeech chat_id:int53 message_id:int53 = Ok` (:12181); `messageProperties.can_recognize_speech` (:6254) gates eligibility (parsed but the UI shows the Transcribe button regardless — a refusal is surfaced in the status note, never a faked transcript). Request optimistically shows "transcription requested"; a TDLib refusal (`error` envelope on the request) lands in `Session::recognize_speech_error` and is drained into the status note (MED2 fix-up — it was previously swallowed).
  - `chatActionRecordingVoiceNote` (:6368) / `chatActionRecordingVideoNote` (:6392) — the driver sends the matching action while the record bar is active (Unigram/TGX pattern).
  - Transcript delivery is `updateMessageContent` on the existing message — no new update plumbing; the reducer's existing content replacement picks it up.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - Exact strings `HoldToAudio`/`HoldToVideo` ("Hold to record audio. Tap to switch to video." / vice versa, `strings.xml`:2538-2539); `VoiceVideoButtonView.onTouchEvent` (:255-318) — press/hold starts recording in the preferred mode, movement distinguishes up-lock from left-cancel, short tap switches mode.
  - `RecordLockView` exists and its click finishes a released/locked recording (`RecordAudioVideoController.java`:379-390) — Quill's Lock button is the desktop mapping.
  - HQ round video: `MAX_ROUND_RESOLUTION` 280, `MAX_HQ_ROUND_RESOLUTION` 480 (`RecordAudioVideoController.java`:99-100,1593); persisted setting `needHqRoundVideos` ("Record HQ Round Videos").
- **Built:**
  - Record mode toggle: right-click the record button flips audio/video (TGX tap-to-switch, desktop-mapped — a touch hold has no honest mouse equivalent); button label + tooltip show the current mode and hint; `MediaPrefs.prefer_video_mode` persisted in `media_prefs.json`.
  - Round video-note capture: `video::VideoNoteCapture` — ffmpeg V4L2 from `/dev/video0` (same pattern as voice's ffmpeg capture), graceful SIGINT stop, center-crop square + scale to 280/480 per the HQ pref, validated duration (1–60s) and square dimensions; `connect::send_recorded_video_note` sends `inputMessageVideoNote` with the probed duration/length. Missing camera or ffmpeg is an honest status-note error, never a fake file.
  - Lock-to-record + discard confirmation: record bar gains Lock/Unlock (locked ignores Esc), Cancel and Esc on an unlocked recording open a "Discard this recording?" confirm row (Esc with the row open dismisses it and keeps recording). Chat switches/panel toggles still force-cancel (internal teardown, not user cancellation).
  - Transcription: `connect::recognize_speech` driver method (real, non-pending messages only; refused requests are errors); rows under voice and video notes show Transcribe / "Transcribing… {partial}" / the transcript / "Transcription failed: …" + Retry.
  - "Record HQ round videos" toggle in the settings Media section, persisted via the existing `set_media_pref` path.
- **Key decisions:** ponytail — the record action sync was parametrized (`sync_record_action(active, now_ms, action)`) instead of a second timer; the video-note send mirrors `send_voice_note` (slow-mode gate, reply threading via a shared `recording_send_reply` helper, demo routing); `VideoNoteCapture` reuses the `VoiceCapture` lifecycle shape (start/discard/finish + `Drop` kills the child); no slide-to-cancel / hold-to-record gesture plumbing — touch gestures have no mouse equivalent, and the Lock button + confirm row cover the same intent on desktop.
- **Not verifiable without live Telegram:** the `recognizeSpeech` round-trip and transcript delivery via `updateMessageContent`; the live camera path (`/dev/video0` absent on this VM — synthetic ffmpeg transcode and the no-camera error are test-covered); actual V4L2 encoder behavior.
- **Out of this slice:** touch-hold to record / swipe-up to lock / slide-to-cancel (TGX gestures; desktop-mapped as click / Lock button / confirm row — the README boxes note this); in-call "video messages" (a different TGX feature, not round video notes).

## Slice MED1 — MEDIA VIEWER & PLAYBACK (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `pinChatMessage chat_id:int53 message_id:int53 disable_notification:Bool only_for_self:Bool = Ok;` (:13559); `unpinChatMessage chat_id:int53 message_id:int53 = Ok;` (:13562). There is NO album-level pin constructor — album pin is one `pinChatMessage` per member (matches TGX `MessagePinAlbum`, which loops `PinChatMessage` over the album's message IDs).
  - Rights: `chatPermissions.can_pin_messages` (:1070), `chatAdministratorRights.can_pin_messages` (:1092), `messageProperties.can_be_pinned` (:6234). Quill gates album-pin on `ChatSummary::can_pin_messages`: private/secret chats allow; creators allow; admins require `rights.can_pin_messages`; ordinary groups use `chat.permissions.can_pin_messages` (same derivation as the existing message-menu pin gate).
  - Negative claim (concept-level, per schema discipline): no `always_on_top` / z-order / picture-in-picture constructor exists in `td_api.tl` (irrelevant anyway — PiP is a window-manager concern, and GPUI 0.3.5's `WindowOptions` has no always-on-top field; see below).
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `PlaybackSpeedLayout.java`: the speed set is 0.5x, 0.7x, 1x, 1.2x, 1.5x, 2x (also `TGPlayerController` normal/higher constants). Quill's speed button cycles exactly this set (single-tap cycle instead of TGX's long-press dial — a dial needs press-and-hold gesture plumbing this stack doesn't have; the speeds and the 0.5–2.0 span are identical).
  - `Settings.java` persists `rememberAlbumSetting` via `FLAG_OTHER_REMEMBER_ALBUM_SETTING`; `MessageView.java` labels the album actions `MessagePinAlbum`/`MessageUnpinAlbum` and unpins when `pinnedCount > 0`, else pins — Quill copies both semantics.
  - `MediaViewController.java` distinguishes `GifPlaybackUnsupported`/`VideoPlaybackUnsupported` from `GifPlaybackError`/`VideoPlaybackError` — Quill maps the extractor's `"unsupported video"` error to "video format not supported" and everything else to "couldn't play this video".
- **Built:**
  - Viewer Rotate (photos): `rotate_rgba_quarter_turns` (pure pixel math, no image crate — covered by the no-default-features tests) decoded eagerly into a cached `RenderImage`; per-item, reset on open/step; 0 → 90 → 180 → 270 cycle.
  - Viewer Share: closes the viewer, opens the existing forward picker with the message selected (`begin_forward_one`, reused).
  - Viewer Save: copies the largest local photo size / full local video clip to the downloads folder (`XDG_DOWNLOAD_DIR` → `UserDirs` → `~/Downloads`), ` (n)` de-dup like the desktop clients; honest "download the media first" when nothing is local (no thumbnail fallback — a video never saves its thumbnail).
  - Viewer "Show in chat": closes the viewer, jumps to the source message via the existing reply-jump machinery.
  - Viewer seek: scrub slider under the transport (the history-row `SliderState`/`SliderEvent` pattern) — drag previews the position in the elapsed label, release seeks the `PlaybackClock` and restarts ffplay at `-ss` when playing; the tick syncs the thumb (skipped while scrubbing).
  - Playback speed: one shared `playback_speed` (0.5–2.0) applied to voice/audio rows and the viewer video — clock rate moves the playhead, ffplay restarts with `-af atempo=` so audio stays in sync; `PlaybackClock::set_rate` clamps and never jumps the playhead.
  - Viewer volume/mute: `playback_volume` → ffplay `-volume`; slider applies on release (no per-tick restart storm), mute remembers and restores the previous level; voice/audio rows get the same mute toggle.
  - Playback errors: ffplay spawn failure, unsupported video format, and generic extraction failure all surface as a red error line in the transport / active row instead of the old silent stall ("(no audio player)" status notes kept).
  - Album viewer: album tiles already opened the fullscreen viewer (the README note claiming otherwise was stale — corrected); the viewer header now also shows Pin/Unpin album when the item is in an album and the user may pin there.
  - Album grouping setting: composer "Grouped ✓ / Ungrouped" toggle for 2+ photos/videos + "Remember: on/off"; persisted as `settings::MediaPrefs` in `media_prefs.json` (loaded at connect like `CallPrefs`); when remembering is off the toggle follows the default (grouped). Ungrouped sends go out as separate messages with the caption on the first (demo session mirrors this).
- **Key decisions:** ponytail — every viewer action reuses an existing flow (forward picker, reply-jump, `pinChatMessage` per member) instead of new machinery; seek/volume sliders copy the history-row `SliderState` pattern verbatim; one shared speed/volume pair for voice+audio+viewer instead of per-track state; the save helper splits dir-resolution from the copy so tests pass a temp dir without env mutation; JPEG support added to the `image` crate features (photos are usually JPEG) and nothing else.
- **Not verifiable without live Telegram:** `pinChatMessage`/`unpinChatMessage` round-trips and their error notes; ffplay audio actually starting (no audio device in CI); the volume/atempo flags' audible effect; save-to-Downloads in a real desktop session (unit-tested against a temp dir).
- **Out of this slice:** picture-in-picture — blocked, not deferred: GPUI 0.3.5 `WindowOptions` has no always-on-top field (verified 2026-09-27 against the pinned gpui source), and this repo has no multi-window plumbing, so a second window would be an ordinary window, not an honest PiP. The README box stays unchecked with this note. A long-press speed dial (TGX's popup) — the cycle button covers the same speeds; a dial needs press-and-hold gesture support this stack lacks.

## Slice MED3 — DOWNLOADS: progress, cancel, retry, manager, auto-download (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - Progress: `localFile` (:292) carries `downloaded_size` whose doc says it is for calculating progress; Quill's `ParsedFile::download_progress()` is `downloaded_size / size` when `size > 0`, `None` otherwise. `updateFile` (:10767) is the progress channel.
  - One-shot download: `downloadFile file_id:int32 priority:int32 offset:int53 limit:int53 synchronous:Bool = File` (:13979-13985); the doc block states progress/completion arrive through `updateFile`. It has **no pause parameter**.
  - Cancel: `cancelDownloadFile file_id:int32 only_if_pending:Bool = Ok` (:13990-13991) — "Stops the downloading of a file"; `only_if_pending:false` stops an active download, `true` only stops one not yet sent. Quill always passes `false` (explicit user cancel).
  - Pause — negative claim with concept-level evidence (per schema discipline):
    1. `td_api.tl` concept search for `pause`: the constructor name `pauseDownload` does not exist; pause lives **only** in the file-download-list API: `toggleDownloadIsPaused file_id:int32 is_paused:Bool` (:14041-14044), `toggleAllDownloadsArePaused` (:14046-14047), plus `addFileToDownloads` (:14036-14040), `removeFileFromDownloads` (:14049-14050), `removeAllFilesFromDownloads` (:14052-14056), `searchFileDownloads` (:14058-14063), `updateFileDownload` (:10793-10795), `fileDownload.is_paused` (:3318-3319).
    2. TDLib source (upstream tdlib/td, `td/telegram/DownloadManager.cpp`, read 2026-09-27): `toggle_is_paused` resolves the file through `get_file_info_ptr(file_id)` against the DownloadManager's own `files_` map — populated only by `add_file`, i.e. the `addFileToDownloads` implementation — and fails the promise for anything not in the list; `toggle_all_is_paused` iterates the same map; pause state persists to the binlog (`dlds#` keys), confirming it is list state, not a per-request flag. Pausing itself is `callback_->pause_file(...)`: stopping the in-flight `upload.getFile` parts client-side.
    3. Raw MTProto layer: not vendored in this repo, but (2) shows there is no protocol primitive involved — pause is TDLib client-side state over ordinary offset/limit `upload.getFile` part fetching.
    Conclusion: pause/resume applies to files in the persistent download list, not to one-shot `downloadFile` requests, which have no pause parameter.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `TdlibFilesManager.java`: auto-download is one `settings_autodownload` bitmask — media flags photo `0x01`, voice `0x02`, video `0x04`, file `0x08`, music `0x10`, GIF `0x20`, video-note `0x40`; per-chat-category shifts private `<<8`, group `<<16`, channel `<<24`. Defaults are photo|voice|GIF|video-note (`AUTO_DOWNLOAD_DEFAULT = 0x63`), **off** for video/file/music.
  - `settings_datasaver` carries global/mobile/roaming bits that suppress auto-downloads entirely.
  - Inline cancellation calls `CancelDownloadFile(fileId, weak)`; progress is `local.downloadedSize / file.expectedSize` — Quill mirrors both exactly.
- **Built:**
  - Document chips: ready → **Open** (system app via the existing OS-open mechanism — `open`/`xdg-open`, no shell) + **Show in folder** secondary (`open -R` macOS, `explorer /select,` Windows, parent dir via `xdg-open` on Linux); downloading → real percent + determinate progress bar + **Cancel**; failed → **download failed — Retry**; failed+unknown → **Retry**.
  - Media viewer loading status shows the real percent.
  - `connect::cancel_download` sends `cancelDownloadFile(file_id, only_if_pending:false)` after checking `session.downloading` contains the id — it returns `Ok(false)` for any id not tracked there, and never consults `local.is_downloading_active`.
  - State: `Session::user_downloads` (explicit UI downloads only — auto thumbs/avatars/sounds never enter), `completed_downloads` (capped at 50), `failed_downloads`. `updateFile` with `is_downloading_completed` moves user downloads into the recent list; `downloadFile` errors land in `failed_downloads`; `begin_download` clears failure (retry path); a completed non-user file unsticks silently.
  - Downloads-manager panel beside the conversation (sidebar **Downloads** toggle): active user downloads with percent + Cancel; recent completed with Open / Show in folder; failed user downloads with Retry.
  - Auto-download: TGX-compatible bit model (`MediaPrefs.auto_download_{private,groups,channels}`, serde-defaulted, old `media_prefs.json` files load fine) + `data_saver` master switch; settings grid renders per-chat-kind × media-type with the data-saver row. Two ingest hooks: `thumb_file_ids_to_download` (thumbnails — ungated, matching TGX `MediaWrapper.pickDisplaySize` where the preview size always downloads for display) and `auto_download_media_file_ids` (full media — photo/voice/video/file/music/GIF/video-note — each gated on its own flag, secret/spoiler never auto-downloaded, files known to exceed `AUTO_DOWNLOAD_MAX_BYTES` (50 MiB, TGX `canAutomaticallyDownload` WiFi default) skipped), driven by `maybe_download_open_chat_media` at priority 4 (above thumbs, far below explicit user downloads at 32) with `user_initiated: false` so auto downloads never enter the manager's user lists. Chat-list avatars remain display chrome gated only on data saver.
- **Key decisions (ponytail):**
  - Desktop has no mobile/wifi/roaming distinction, so TGX's per-connection grids collapse into one per-chat-kind grid plus the global data saver — Telegram Desktop's own dialog is the same grid. `data_saver` maps to TGX's global data-saver bit.
  - MED3's manager is the lightweight recent-downloads panel (`updateFile` + user tracking), **not** `searchFileDownloads`: the persistent list API requires `addFileToDownloads` per file (an extra request round-trip per download) and MED3's scope is progress/cancel/retry/recent — one-shot `downloadFile` already covers it. Pause/resume stays out of this slice honestly: it only exists for the list API.
  - The README `media-download-cancel` box previously said "Pause / cancel"; it is now cancel-only with pause moved to an out-of-slice box (schema shows pause is a list-API feature).
  - No speculative partial-download streaming: `downloadFile` offset/limit stay 0 (full file), and `getSuggestedFileName` is unused — `download_display_name` uses the document `file_name` / local path.
- **Not verifiable without live Telegram:** real `downloadFile` progress cadence and the 42% fixture's live equivalent; `cancelDownloadFile` racing an in-flight server transfer; the send-failure → failed state path; auto-download triggering against real message flow (the driver hooks are covered by seeded logic tests only).
- **Out of this slice:** persistent downloads-list API (`addFileToDownloads` / `toggleDownloadIsPaused` / `toggleAllDownloadsArePaused` / `searchFileDownloads` / `removeFileFromDownloads` — pause/resume and list-wide management, README box `media-downloads-pause`); `downloadFile` offset/limit partial streaming; pause-all on data saver for *active user* downloads (data saver only gates *new* automatic downloads, like TGX); per-type auto-download size caps (TGX limits auto-download by file size per connection type; Quill auto-downloads any size the flags allow).

## Slice MED4 — LINK PREVIEWS + CAPTIONS (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `linkPreviewOptions` (:2237): `is_disabled`, `url`, `force_small_media`, `force_large_media`, `show_above_text`. Send-time wiring: `inputMessageText.link_preview_options` (:6081). Quill implements **disable only** (the composer toggle); force small/large and show-above-text are not exposed — no `getLinkPreview` prefetch exists, so there is no local preview to resize/reposition honestly.
  - `getLinkPreview text:string link_preview_options:linkPreviewOptions = LinkPreview` (:14792) — **not called**. Compose-time prefetch is out of this slice (see below).
  - `getWebPageInstantView url:string only_local:Bool = WebPageInstantView` (:14794-14797); `webPageInstantView` (:4377) carries `page_blocks`. Quill passes `only_local:false` and reuses the M2 `messageRichMessage` page-block renderer for the reader.
  - `linkPreview.instant_view_version` (:4570): "Version of the instant view... 0 if none". Quill treats `> 0` as IV-available (the card tap attempts IV; 0 goes straight to the browser).
  - Embedded players: `linkPreviewTypeEmbeddedVideoPlayer` (:4434), `...EmbeddedAudioPlayer` (:4443), `...EmbeddedAnimationPlayer` (:4452). Album: `linkPreviewTypeAlbum` (:4392) with `linkPreviewAlbumMediaPhoto` (:4383) / `...Video` (:4386).
  - Caption position: `messagePhoto.show_caption_above_media` (:6117), `messageVideo` (:6128), `messageAnimation` (:6091). `sendMessageAlbum` (:12208) requires all album members to share the position — Quill's album builder applies the snapshot's single flag to every member.
  - `editMessageCaption ... show_caption_above_media:Bool` (:12338) — the edit path preserves the message's position and the composer toggle can flip it.
  - `forwardMessages.remove_caption` (:12237) — gated on `send_copy`, matching TGX.
  - Caption limit: `inputMessagePhoto` doc (:6088) points at `getOption("message_caption_length_max")`; `updateOption` (:10926) + `optionValueInteger` (:8895). Quill reads the runtime option (default 1024), counts Rust `char`s (Unicode scalar values — TDLib's exact counting unit was not source-verified; documented as an assumption), refuses overlong captions in `send_snapshot`/`send_album_snapshot`/`edit_snapshot` with `ConnectSendError::CaptionTooLong { limit }`, and shows a live `n / max` counter.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `MessagesController.MessageInputContext` tracks found URLs + preview cache + selected URL + force-small/large — the full compose-time preview pipeline Quill does **not** replicate (see out-of-slice).
  - `TdlibUi` calls `GetLinkPreview`; Instant View is attempted when the card offers it and falls back externally on error/bad/unsupported IV — Quill mirrors this exactly (`instant_view_version > 0` → `getWebPageInstantView` → reader; TDLib error → browser fallback via `Session::instant_view_fallback_url`).
  - IV settings None / Telegram-internal / All (`Settings.java` ~796-798, `SettingsThemeController` ~1722) → Quill's `InstantViewMode::{Off, Telegram, All}` (default `Telegram`), cycled in Media settings. `Telegram` mode attempts IV only for cards with `instant_view_version > 0`; `All` attempts it for any card link; `Off` goes straight to the browser.
  - `MediaBottomGalleryController.allowShowCaptionAboveMedia()` allows caption-above for attachment sends except secret chats — Quill's toggle is photo/video-only (schema fields exist only there) **and hidden in secret chats** (`open_chat_is_secret()`); the send path forces `caption_above=false` there too.
  - `ShareController` ~3317 shows "Remove Captions" only for copied messages; `TD.forwardMessages` passes `sendCopy` + `removeCaption` — Quill's forward sheet matches (checkbox disabled unless send_copy).
- **Built:**
  - Composer: detected-URL chip (first `http(s)://` URL via stdlib `find_urls`) + the existing preview on/off toggle; caption bar ("Add a caption…" hint, caption-above toggle for photo/video, live `n / max` counter) shown while attachments are pending or a caption is edited; `with_caption_above_media` rides every snapshot path (single/album/ungrouped); toggle resets after send; overlong captions refuse with a named-limit status note (never a silent send).
  - Received: caption renders above or below the media per `show_caption_above_media` (photo/video/animation; documents/audio/voice always below — no schema field); link-preview cards gain embedded-player badges (▶/♪ + `m:ss`) and album thumbnail strips (up to 4); card tap opens the embed URL for players, else attempts Instant View when offered, else the browser.
  - Instant View: `open_instant_view` sends `getWebPageInstantView` (tracked by request id); success renders page blocks in a reader overlay (reuses `message_rich_block`); TDLib error stashes the URL for browser fallback. A refusal is never rendered as a reader.
  - Settings: Instant View mode cycler (Off → Telegram links → All links) in Media settings, persisted in `MediaPrefs`.
- **Key decisions (ponytail):**
  - No `getLinkPreview` prefetch: the server already attaches previews to sent messages, and a prefetch would need debounce/cache/pending/error UI for zero user-visible gain in this slice. The composer chip shows the detected URL + toggle state instead.
  - No inline media player for embeds: the badge + tap-to-open-embed-URL is the honest scope; inline playback is a media-viewer slice.
  - Documents/audio/voice captions stay below: the schema has no `show_caption_above_media` for them.
  - `RequestPurpose::GetWebPageInstantView` is a unit variant (not carrying the URL) to preserve the enum's `Copy`; URL correlation uses `Session::instant_view_urls: HashMap<RequestId, String>`.
- **Not verifiable without live Telegram:** `getWebPageInstantView` round-trip (success page blocks, 404 → browser fallback); `message_caption_length_max` arriving via `updateOption` (default 1024 assumed until the option arrives); the exact TDLib caption-counting unit (chars vs UTF-16 code units); `instant_view_version > 0` on real cards.
- **Out of this slice:** compose-time `getLinkPreview` prefetch with debounce/cache/pending/error card; `linkPreviewOptions.force_small_media` / `force_large_media` / `show_above_text` / URL selection; inline embedded-player playback; shared-media gallery + empty states (chat-info/profile work, README boxes `media-shared-gallery`, `media-shared-gallery-empty` stay unchecked).

## Slice MED4b — LINK PREVIEW SEND OPTIONS (2026-09-28)

Completes README `parity:media-link-preview-send-options` (MED4 left it partial: disable toggle + detected-URL chip only).

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `linkPreviewOptions is_disabled:Bool url:string force_small_media:Bool force_large_media:Bool show_above_text:Bool = LinkPreviewOptions;` (:2237). Comments: `force_small_media` "True, if shown media preview must be small; ignored in secret chats or if the URL isn't explicitly specified" (:2234); `force_large_media` same for large (:2235); `show_above_text` "True, if link preview must be shown above message text; otherwise, the link preview will be shown below the message text; ignored in secret chats" (:2236); `url` "If empty, then the first URL found in the message text will be used" (:2233).
  - `getLinkPreview text:formattedText link_preview_options:linkPreviewOptions = LinkPreview;` (:14792) — "Returns a link preview by the text of a message. Do not call this function too often. Returns a 404 error if the text has no link preview".
  - `inputMessageText text:formattedText link_preview_options:linkPreviewOptions clear_draft:Bool = InputMessageContent;` (:6081) — the ONLY `inputMessage*` constructor with `link_preview_options` (verified: `inputMessagePhoto` :6117, `inputMessageVideo` :6128, `inputMessageDocument` :6101, `inputMessageAudio` :6096, `inputMessageAnimation` :6091 have no such field), so media-caption sends have no equivalent — options wire through text sends only.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `MessagesController.MessageInputContext` (`ui/MessagesController.java` ~7016-7115): per-URL `LinkPreview` cache, `takeOutputLinkPreviewOptions` copies `forceLargeMedia`/`forceSmallMedia` into the options and **sets `options.url` explicitly when forcing** (schema requirement).
  - `helper/LinkPreview.java`: prefetch is `GetLinkPreview(FormattedText(url), null)` with a 400ms rate limiter; `toggleLargeMedia()` flips force-small/force-large relative to the current effective size and no-ops when the preview has no media or no large variant; `getForcedTitle()` falls back description → title/siteName.
  - `MessagesController.onRequestToggleLargeMedia` (~8595) + `onRequestToggleShowAbove` (~8612): separate controls; both no-op when the preview is disabled; hint strings `LinkPreviewShowAbove`/`LinkPreviewShowBelow` ("Link preview will appear above/below the text"), `LinkPreviewEnlarged`/`LinkPreviewMinimized`.
- **Built:**
  - Composer chip: detected URL + prefetched preview line (title — description, "Getting link info…" while loading, "No preview for this link" on 404) + Preview on/off + "Media: large/small" toggle (only when the prefetched preview has `has_large_media` and actual media — TGX gate) + "Above text"/"Below text" toggle (hidden while preview is off, like TGX's no-op). Toggles set `status_note` hints matching TGX's strings.
  - `getLinkPreview` prefetch: `ConnectDriver::request_composer_link_preview` (`RequestPurpose::GetLinkPreview`, URL keyed by `RequestId` in `Session::composer_preview_urls` — the `Copy`-purpose pattern from MED4); UI debounces 500ms after the URL settles (schema "too often" guidance, TGX 400ms); success parses via a new `EnvelopePayload::LinkPreview` arm reusing `parse_link_preview`; 404 → `Some(None)` ("no link info", never a card); late answers for superseded URLs are dropped.
  - Send path: `SendOptions` gains `link_preview_above_text: bool` + `link_preview_media: PreviewMediaSize::{Auto, ForceSmall, ForceLarge}` (enum makes the invalid both-true wire state unrepresentable; `toggle()` mirrors TGX's flip-relative-to-effective). `send_text` emits the full `linkPreviewOptions` object (with the detected first URL set explicitly, per schema/TGX) whenever above-text or a force flag is set; disabled and default behaviors unchanged.
  - `LinkPreview` gains parsed `has_large_media` (was dropped; needed for the TGX size-toggle gate).
- **Key decisions (ponytail):**
  - No thumbnail downloads for the composer prefetch: `parse_link_preview`'s `ParsedFile`s are dropped in the `"linkPreview"` payload arm; the chip shows a 🖼 glyph + text only. Downloading transient preview files is out of slice.
  - No `url` selector for multiple URLs: the first detected URL is used (same as MED4's chip and the schema default).
  - New options persist across sends like the existing disable toggle (TGX-equivalent); secret chats still force `is_disabled: true` driver-side, which also nulls the new flags (schema ignores them there anyway).
  - Prefetch fires only in live sessions; the screenshot demo injects a fake preview into `demo_session`.
- **Not verifiable without live Telegram:** the `getLinkPreview` round-trip against a real server (success payload shape, 404 behavior); the exact server treatment of `force_small_media`/`force_large_media` on exotic preview types.
- **Out of this slice:** per-URL preview selection when several URLs are typed; preview thumbnails in the chip; `show_above_text` for secret chats (schema-ignored); embedded-player inline playback (still MED4-out).
## Slice CL1 — CHAT LIST: ROW MENU, PIN, READ/UNREAD, MUTE, CLEAR/DELETE (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `toggleChatIsPinned chat_list:chatList chat_id:int53 is_pinned:Bool = Ok` (:13678); `setPinnedChats` (:13681, right after) is the reorder primitive — CL1 wires only the toggle; reorder is CL2.
  - `toggleChatIsMarkedAsUnread chat_id:int53 is_marked_as_unread:Bool = Ok` (:13519); `updateChatIsMarkedAsUnread` (:10588); `chat.is_marked_as_unread` and the delete-capability fields `can_be_deleted_only_for_self` / `can_be_deleted_for_all_users` (:3600/:3627).
  - `deleteChatHistory chat_id:int53 remove_from_chat_list:Bool revoke:Bool = Ok` (:11845) vs destructive `deleteChat chat_id:int53 = Ok` (:11850): clearing history is `remove_from_chat_list:false`; chat-list removal is `remove_from_chat_list:true` — **never** the destructive constructor.
  - `viewMessages chat_id:int53 message_ids:vector<int53> source:messageSource force_read:Bool = Ok` (:13230); `messageSourceChatList` (:3222).
  - `readChatList chat_list:chatList = Ok` (:13684) exists but is unused — mark-all-read is CL2.
  - Pin limits arrive as runtime options `pinned_chat_count_max` / `pinned_archived_chat_count_max` (via `updateOption`, :10926); the schema documents separate main/archive and secret/non-secret limits. **Negative-claim discipline:** the raw `telegram_api.tl` layer and TDLib source (`Requests.cpp` / managers) were not exhaustively searched for this slice, so no "not in schema" claims are made — only positive evidence is stated.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - Pin limits: `strings.xml` :3144-3145 ("you can pin up to N chats and N secret chats at once"), :4354-4355 (main-list limit text recommending the archive); `TD.java` :2550 maps the maximum-pinned error; `ChatsController.java` :1410-1430 pre-checks secret/non-secret counts separately against main/archive limits.
  - Mark read: `Tdlib.markChatAsRead` (:3963-3986) clears the manual flag with `ToggleChatIsMarkedAsUnread(false)` and sends `ViewMessages(chat.lastMessage.id, MessageSourceChatList, force_read=true)`; `ChatsController.java` :1849 passes `MessageSourceChatList`; mark-all-read (:1850-1873) clears mentions separately.
  - Delete vs clear: `Tdlib.deleteChat` (:5336-5370) removes private chats from the list via `DeleteChatHistory(chatId, removeFromChatList=true, revoke)`; `TdlibUi` :4960-4992 clears history with `DeleteChatHistory(chatId, false, revoke)`; :5023+ routes chat deletion UI through `tdlib.deleteChat` (the removal path, not destructive TDLib `deleteChat`).
- **Built:**
  - Row context menu (right-click on a chat row): Pin/Unpin, Mark as read/unread, Mute/Unmute, Archive/Unarchive, Clear history (+ "for everyone" when the chat allows), Delete chat. Destructive entries use the existing confirm-modal pattern; async errors surface in the status note; `Ok(None)` confirmations are reported as "request already in flight", never as success.
  - Pin/unpin: optimistic `toggleChatIsPinned` with rollback on failure; TGX-style client-side pre-check using `pinned_chat_count_max` / `pinned_archived_chat_count_max` (defaults 5/100 until `updateOption` arrives); TGX limit message; numeric TDLib errors surface in the status note.
  - Per-chat read/unread: Mark as read sends `viewMessages` over the newest known message with `messageSourceChatList` + `force_read:true` (TGX semantics) and clears the manual flag; Mark as unread sends `toggleChatIsMarkedAsUnread(true)`. Marked-as-unread rows render a dot badge; `updateChatIsMarkedAsUnread` updates the model live.
  - Mute/unmute from the list reuses the existing `toggleChatsNotificationSettings` driver path (same action as the open-chat header bar).
  - Clear history: `deleteChatHistory(remove_from_chat_list:false, revoke:…)` gated on `can_be_deleted_only_for_self` / `can_be_deleted_for_all_users` (menu items hidden when not allowed; driver refuses with `None`).
  - Delete chat from the list: `deleteChatHistory(remove_from_chat_list:true, revoke:false)` — the pre-existing destructive `deleteChat` builder is left wired only to the group-panel "Delete group" behavior it already served.
  - Request-shape tests pin every outbound payload (`toggleChatIsPinned`, `toggleChatIsMarkedAsUnread`, `viewMessages` with `messageSourceChatList`, `deleteChatHistory` flags), plus driver tests: mark-read sends view+untoggle, mark-read is a noop when nothing is unread, remove-from-list sends `deleteChatHistory` (and never the destructive constructor); state tests cover pin/unpin rollback, marked-unread update+rollback, clear-history error surfacing, and pin-limit option tracking.
  - Screenshot fixture `quill --screenshot-demo ready-chat-list`: chat 11 pinned, chat 12 archived, chat 13 marked unread, full row menu open (`docs/screenshots/ready-chat-list.png`).
- **Key decisions (ponytail):**
  - `mark_chat_as_read` does not use `readChatList` — TGX's per-chat path (`viewMessages` + untoggle) is the honest per-row semantic; `readChatList` is reserved for the CL2 mark-all-read entry.
  - No `chat.last_message` ID is retained in `ChatSummary`, so mark-read uses the newest cached message ID; when a chat has unread messages but no cached history and no manual flag, the driver returns `None` (documented below) rather than guessing.
  - `chatlist-delete-chat` previously noted "driver has a deleteChat request builder" — the slice deliberately avoids destructive `deleteChat` for list removal, following TGX's `DeleteChatHistory(removeFromChatList=true)` mapping.
- **Not verifiable without live Telegram:** real `updateOption` pin limits (defaults 5/100 assumed); real `updateChatIsMarkedAsUnread` traffic; numeric pin-limit error text from the server; `force_read:true` clearing server-side unread on a live chat; `mark_chat_as_read` for a chat whose only unread state is server-side with no cached history (returns `None` — no message ID to view).
- **Out of this slice (CL2):** pinned drag reorder (`setPinnedChats`), mark-all-read (`readChatList`), Saved Messages row, collapsible/archive settings + `setArchiveChatListSettings`, category chips (unread/archived filters), clear recent searches, search no-results state. **(CL3):** chat preview on hover/long-press, mention/reaction badges on the unread badge, multi-select (Select… / Select unread), report/block from the list, app badge counter preferences, list style settings.

## Slice CL2 — CHAT LIST: PIN REORDER, MARK-ALL-READ, SAVED MESSAGES, ARCHIVE COLLAPSE/SETTINGS, CATEGORY CHIPS, CLEAR RECENTS, SEARCH EMPTY (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `setPinnedChats chat_list:ChatList chat_ids:vector<int53> = Ok` (:13681); `readChatList chat_list:ChatList = Ok` (:13684); `clearRecentlyFoundChats = Ok` (:11671).
  - `getArchiveChatListSettings = ArchiveChatListSettings` (:13421); `setArchiveChatListSettings settings:ArchiveChatListSettings = Ok` (:13424); the type's three fields — `archive_and_mute_new_chats_from_unknown_users`, `keep_unmuted_chats_archived`, `keep_chats_from_folders_archived` — at :3512.
  - `createPrivateChat user_id:int53 force:Bool = Chat` (:13312); the schema doc at :9590: "Call createPrivateChat with getOption(\"my_id\") and open the chat" — Saved Messages is the own private chat.
  - **Negative-claim discipline:** no "not in schema" claims are made for this slice. The one live-settings surface TGX's controller subscribes to (`onArchiveChatListSettingsChanged`, TGX `SettingsArchiveChatListController`) has no dedicated schema update in the pinned `td_api.tl` search, but per standing discipline that single-name search is not a claim — the slice does not depend on it (fetch-on-open + optimistic set with rollback instead).
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - Pin reorder: `ChatsAdapter.java` :540-595 — drag only in the unfiltered list with ≥2 pinned header chats; drops send the complete reordered pinned-id list.
  - Clear recents: `SearchManager.java` :788-796 — clears local recents, then sends `ClearRecentlyFoundChats`.
  - Archive settings: `SettingsArchiveChatListController.java` :56-150 — fetches `getArchiveChatListSettings` on open, exposes the three schema-backed toggles; labels taken verbatim from `strings.xml` :4757-4759 (unknown users: "Archive and Mute" / "Automatically archive and mute new chats, groups and channels from non-contacts."), :5464-5466 (unmuted chats: "Always keep archived" / "Keep archived chats in the Archive even if they are unmuted and get a new message."), :5468-5470 (folder chats: same pattern). The controller's fourth "archive as folder" appearance toggle is a client-side display preference — out of scope.
  - Unread filter: `ChatFilter.java` :166 — unread is `unreadCount > 0 || isMarkedAsUnread`; `MainController.java` :1520+ — archive pager category and unread filter.
  - Archive collapse: TGX's `archiveCollapsed` setting maps to the per-session collapsed header.
- **Built:**
  - Pinned drag reorder: `PinnedChatDrag` payload (typed, renders its own ghost chip), drop onto another pinned row moves to that slot; driver `set_pinned_chat_order` sends the full reordered ID list with optimistic `reorder_pinned_chats` + rollback on refusal; ID-set mismatch is a no-op (cross-list drops). Enabled only in unfiltered All/Archived views with ≥2 pinned chats (TGX parity).
  - Mark all read: `readChatList` for main (`✓ Mark all read` on the list) and archive (archive header button); badges are never faked locally — no-op when nothing is unread.
  - Saved Messages: always-visible `💾 Saved Messages` entry; opens the listed self chat directly, otherwise `createPrivateChat(my_user_id, force:false)`; the bare `chat` answer parses as `UpdateNewChat` (`parse_new_chat`) and the driver opens it through the normal `select_chat` flow (`openChat` + history) when the `@extra` matches the in-flight `CreatePrivateChat` request — never a bare `open_chat` field set. A refused request surfaces "could not open Saved Messages (error …)" and opens nothing. Known nuance: the async open bypasses the UI's draft-restore pass (the leaving composer's draft is flushed first); the self chat's own saved draft restores on the next manual selection.
  - Archive: clickable ▸/▾ collapse header with count (session `archive_collapsed`); archive header buttons for mark-read + ⚙ settings; `Archive settings` dialog with the three schema-backed toggles, fetch-on-open, loading/error/retry states, optimistic toggle + rollback on refusal (TGX labels).
  - Category chips: Unread / Archived beside the folder tabs (the folder/Main selection itself is the All view); folder selection resets the filter; Archived forces the archive section open.
  - Clear recents: `Clear` button on the Recent heading; `clearRecentlyFoundChats` with optimistic local clear (TGX `SearchManager` parity); refusal surfaces as a status note.
  - Search no-results: the existing `No chats or messages match "…"` state is pinned by a dedicated fixture + screenshot.
  - Request-shape tests for every new constructor (`setPinnedChats` main/archive, `readChatList` main/archive, `clearRecentlyFoundChats`, archive settings get/set with all three fields, `createPrivateChat` with own ID + `force:false`); driver tests for reorder/refusal/mismatch, mark-all-read no-op, clear-recents optimistic + refusal, archive settings fetch/set/refusal rollback, create-private-chat success/refusal; state tests for pinned reorder + rollback and archive settings rollback.
  - Screenshot fixtures `quill --screenshot-demo ready-chat-list-2` (folders/tabs, category chips, mark-all-read, Saved Messages, pinned + expanded archive), `ready-chat-list-archive` (archive settings dialog), `ready-chat-list-search` (search empty state) — `docs/screenshots/ready-chat-list-2.png`, `ready-chat-list-archive.png`, `ready-chat-list-search.png`.
- **Key decisions (ponytail):**
  - Drag/drop uses GPUI's built-in `on_drag`/`on_drop` on the existing row `Div` (now `impl IntoElement` since `.id()` yields `Stateful<Div>`); no custom drag infrastructure.
  - No `chat.last_message` retention was added — mark-all-read needs no message IDs (`readChatList` takes only the list).
  - The archive settings dialog fetches on every open (TGX behavior) rather than caching across opens.
  - Pre-compaction UI edits had wrongly changed `message_menu_overlay`/`statistics_body` return types to `Div`/`impl IntoElement`; both reverted to their HEAD types to keep the ui build green.
- **Not verifiable without live Telegram:** real `setPinnedChats` round-trip and server reorder conflicts; real `readChatList` badge clearing; real `clearRecentlyFoundChats` server acceptance; real `get/setArchiveChatListSettings` traffic (defaults assumed until fetch); real `createPrivateChat` answer for the own user; `updateChatFolders`-driven folder tabs beyond the injected fixture.
- **Out of this slice (CL3):** chat preview on hover/long-press, mention/reaction badges on the unread badge, multi-select (Select… / Select unread), report/block from the list, app badge counter preferences, chat list style preferences.

## Slice CL3 — CHAT LIST: MENTION/REACTION BADGES, MULTI-SELECT, REPORT/BLOCK (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `chat.unread_mention_count` / `chat.unread_reaction_count` (:3611-3612), `chat.can_be_reported` (:3606), `chat.block_list` (:3627; the initial `blocked` state).
  - `updateChatUnreadMentionCount` (:10567), `updateChatUnreadReactionCount` (:10570), `updateChatBlockList chat_id:int53 block_list:BlockList = Update` (:10594).
  - `reportChat chat_id:int53 option_id:bytes message_ids:vector<int53> text:string = ReportChatResult` (:15693); the schema doc at :3667 explicitly permits the simple spam flow with empty `option_id` and `message_ids`. Result variants :9210-9219 (`reportChatResultOk`, `reportChatResultOptionRequired`, `reportChatResultTextRequired`, `reportChatResultMessagesRequired`) — non-OK variants collapse to an honest "more info required" note, never success.
  - `setMessageSenderBlockList sender_id:MessageSender block_list:BlockList = Ok` (:14492); `blockListMain` (:9692); the schema doc at :3674 shows `setMessageSenderBlockList` accepts secret-chat senders; null `block_list` unblocks.
  - **Negative-claim discipline:** no "not in schema" claims are made for this slice; every constructor above was read verbatim from the pinned schema.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - Badges: `TGChat.java` tracks mention/reaction counters separately; `ChatView.java` :655+ draws distinct mention (@ glyph) and reaction (heart) badges; `TGChat.setCounter` suppresses the ordinary unread count when there is exactly one unread and a mention badge is present (`hasMentions && unreadCount == 1 ? 0 : unreadCount`) — Quill replicates this suppression.
  - Report/block: `Tdlib.blockSender` sends `SetMessageSenderBlockList` with `blockListMain`; `Tdlib.unblockSender` sends null `block_list`.
  - Multi-select: `ChatsController` select mode — checked chats, selection count in the header, bulk pin/unpin, mute/unmute, delete (`createSimpleChatActions`, :2227).
  - Chat preview (NOT built — see below): `ForceTouchView.java` — long-press/3D-touch peek showing a floating card with the chat's recent message history plus quick actions.
  - List style (NOT built — see below): `Settings.java` :1262-1264 (`CHAT_MODE_2LINE/3LINE/3LINE_BIG`); `ChatView.java` `getViewHeight`/`getAvatarSizeDp` — these are Android density metrics (72/78/82dp rows), not portable content settings.
- **Built:**
  - Mention/reaction badges: `@` badge (blue circle) when `unread_mention_count > 0`, `♥` badge (blue, dimmed when muted) when `unread_reaction_count > 0`, rendered right-to-left like TGX (reactions, mentions, then the count); the main counter hides when mentions exist and `unread_count == 1`.
  - Multi-select: row-menu "Select" enters the mode with the chat checked; row clicks toggle the check (the last uncheck exits); select bar with count + Pin / Read / Mute / Archive / Select unread / Delete / ✕ — all reusing the existing per-chat paths (`toggle_chat_pin`, `toggle_chat_marked_as_unread`'s mark-read flow, `apply_chat_mute`, `toggle_archive`, `remove_chat_from_list`); bulk delete confirms first via the existing `GroupConfirmDialog`.
  - Report: row-menu "Report" gated on `chat.can_be_reported`; driver `report_chat` sends the simple spam report, de-duped; `ReportChatResult` surfaces via `Session::report_chat_outcome` → status note; refusals surface as errors, never success.
  - Block/Unblock: row-menu item for private/secret peer users (never the own chat); driver `set_chat_user_blocked` sends `blockListMain` or null, de-duped; state tracks `updateChatBlockList`.
  - Drive-by (verified against CL1 commit `5cd5c25`): the same dark-panel `.text_color(rgb(0xe6edf3))` fix applied to the message context menu's ghost buttons — the chat-row menu got it in CL1, the message menu was missed.
  - Tests: request-shape tests for `reportChat` and `setMessageSenderBlockList` (block + unblock); reducer tests for mention/reaction parse + updates, report result ok/more-info, `updateChatBlockList`, and refused report/block errors.
  - Screenshot fixture `quill --screenshot-demo ready-chat-list-3`: select mode with two chats checked, @ badge on chat 11, ♥ badge on chat 12, the select bar, and the row menu open showing Report / Block user — `docs/screenshots/ready-chat-list-3.png`.
- **Key decisions (ponytail):**
  - No new settings surface was built: every bulk action reuses an existing per-chat driver; the select bar is a plain div, not a new component.
  - The report flow is the simple spam report only — option/text/message flows need picker UI and stay future work; the non-OK result says so honestly instead of faking success.
  - `ReportChatOutcome` collapses the three "required" variants — the chat-list context can't satisfy any of them, so distinguishing them has no user-visible effect.
- **Not verifiable without live Telegram:** real `reportChat` round-trip and server result variants; real `setMessageSenderBlockList` acceptance and `updateChatBlockList` delivery; real `updateChatUnreadMentionCount`/`updateChatUnreadReactionCount` traffic.
- **Out of this slice (left unchecked with evidence):**
  - `parity:chatlist-chat-preview` — TGX's reference is `ForceTouchView`: long-press peek rendering a floating card with the chat's recent message history + quick actions. A faithful port needs long-press gesture detection (no GPUI 0.3.5 primitive), async `getChatHistory` for unopened chats, and a floating scrollable message-list overlay reusing bubble rendering — a full slice on its own, not a CL3-sized addition. No TDLib/schema blocker; queued as future UI work.
  - `parity:chatlist-badge-settings` — framework-blocked: the app icon badge these settings would control doesn't exist in GPUI 0.3.5. Concept-level check of the pinned framework source: `gpui-pre-0.3.5/src/platform.rs` `Platform` trait (:182-404) exposes `set_dock_menu` and nothing badge-related — no `set_badge`, dock-tile count, or taskbar overlay API anywhere in `src/` (grep for "badge" returns zero hits). Settings for "include muted/archived, messages vs chats" would be inert preferences with no effect to observe; per standing rules, no inert prefs are added.
  - `parity:chatlist-list-style` — TGX's `CHAT_MODE_2LINE/3LINE/3LINE_BIG` are Android density metrics (72/78/82dp row heights, avatar sizes in `ChatView.getViewHeight`/`getAvatarSizeDp`), not portable content settings; the 3-line content mode needs last-message sender tracking that `ChatSummary` doesn't retain, and a density toggle needs a settings surface that doesn't exist in Quill yet. Media icons and text formatting in previews are separable future slices. No speculative preferences were added.

## Slice A1 — LOGIN-FLOW COMPLETION: CODE RESEND + QR LOGIN (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim — concept-level, never single-grep):**
  - `resendAuthenticationCode reason:ResendCodeReason = Ok` (:11334). Doc: "Works only when the current authorization state is authorizationStateWaitCode, the next_code_type of the result is not null and the server-specified timeout has passed, or when the current authorization state is authorizationStateWaitEmailCode"; reason "pass null if unknown".
  - `resendCodeReasonUserRequest = ResendCodeReason` (:6974); `resendCodeReasonVerificationFailed error_message:string = ResendCodeReason` (:6978) is the device-verification variant, NOT used here.
  - `resendPhoneNumberCode reason:ResendCodeReason = AuthenticationCodeInfo` (:14888) belongs to the phone-number-verification flow (`requestPhoneNumberVerificationCode` doc :9545) — NOT the auth flow. The README's old parenthetical naming it was corrected.
  - `requestQrCodeAuthentication other_user_ids:vector<int53> = Ok` (:11346). Doc: works in `authorizationStateWaitPhoneNumber` (plus no-pending-query other states); `other_user_ids` = "List of user identifiers of other users currently using the application" — empty here (no logged-in user to hint at).
  - `authorizationStateWaitOtherDeviceConfirmation link:string = AuthorizationState` (:227) — the QR payload arrives in `link`.
  - `confirmQrCodeAuthentication link:string = Session` (:11414) is the *logged-in* client's "link desktop device" flow — NOT this slice (backlog).
- **Telegram X reference:** the local TGX checkout (`~/workspace/telegram-x`) returned no source hits for `resendAuthenticationCode` / `requestQrCodeAuthentication` (its auth-manager layer is not in the checked-out tree), so TGX behavior was not verified from source this slice. The flow follows the schema docs plus official-client convention: QR entry on the phone screen, resend affordance on the code screen, server-driven cooldown.
- **Built:**
  - Request builders `resend_authentication_code` (reason `resendCodeReasonUserRequest`) and `request_qr_code_authentication` (empty `other_user_ids`) in `telegram/requests.rs`, with shape tests.
  - Driver `resend_code()` (gated to `WaitCode`) and `request_qr_login()` (gated to `WaitPhoneNumber`) in `connect.rs`; new `RequestPurpose::{ResendAuthenticationCode, RequestQrCodeAuthentication}` folded into `is_auth_submit` so `ok` clears / `error` classifies via `last_auth_error` → status note. New `user_message` lines: invalid → "couldn't resend the code", flood → "too many resends — wait and try again".
  - Envelope: `AuthorizationState::WaitOtherDeviceConfirmation { link }` now carries the QR payload (was dropped); the link is never logged (asserted in the driver test via `sink.rendered()`).
  - UI (`ui/mod.rs` auth section only): "Resend code" button beside "Submit code" on the code screen; "Sign in with QR code" button on the phone screen; the QR renders as a real bitmap from the state link via the small `qrcode` crate (optional dep, `ui` feature only) → `RenderImage` → `img()`, cached by link; honest "Waiting for the QR payload from Telegram…" fallback when the link is empty.
  - Honest cooldown: NO local countdown is invented. A too-early resend fails server-side (429) and the flood-wait surfaces through the existing auth-error path.
  - Screenshot demo `quill --screenshot-demo wait-qr` (injected `WaitOtherDeviceConfirmation` with a fake `tg://login/?token=…` link, no live Telegram) → `docs/screenshots/ready-auth-qr.png`; `wait-code` (now showing the Resend button) → `docs/screenshots/ready-auth-resend.png`.
  - Tests: builder shapes (`requests.rs`), driver gates + shapes + link-carry + never-logged (`connect.rs`), `view_for` QR mapping (`auth.rs`).
- **Key decisions (ponytail):**
  - `qrcode` chosen over hand-rolled rendering and over the copyable-link fallback: pure Rust, one small dep, feature-gated to `ui`; its `image`-feature renderer feeds the existing `RenderImage` path (~20 lines).
  - No new components: both buttons reuse the existing ghost `Button`, the QR is a plain `img` in the auth section; `sidebar` went `&self` → `&mut self` for the QR cache.
- **Not verifiable without live Telegram:** real `resendAuthenticationCode` round-trip and server 429 timing; real `requestQrCodeAuthentication` → `updateAuthorizationState` link delivery; QR scan + confirm from a real second device.
- **Out of this slice (left unchecked with evidence):**
  - `parity:auth-qr-authorize-other` — "Link desktop device" on a logged-in client (`confirmQrCodeAuthentication link:string = Session` :11414) is the reverse flow: Quill would *display its own* QR/session link for another device to scan. Needs session-link UI plus scan handling; separate slice.
  - `parity:auth-email-login`, `parity:auth-registration` — still explicit `UnsupportedHalt`; `resendAuthenticationCode` in `WaitEmailCode` is intentionally not wired (gated to `WaitCode` only).
  - 2FA manage, sessions, password recovery — untouched, still backlog.
## Slice A2 — TWO-STEP VERIFICATION MANAGEMENT (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim — concept-level, never single-grep):**
  - `getPasswordState = PasswordState` (:11426) — returns the authoritative state object.
  - `setPassword old_password:string new_password:string new_hint:string set_recovery_email_address:Bool new_recovery_email_address:string = PasswordState` (:11434). Doc: enable/change/disable in one call — an *empty `new_password`* turns the password off; `set_recovery_email_address` true sends the recovery email in the same call on first-time enable.
  - `setRecoveryEmailAddress password:string email_address:string = PasswordState` (:11458); `resendRecoveryEmailAddressCode = PasswordState` (:11464); `cancelRecoveryEmailAddressVerification = PasswordState` (:11467); `checkRecoveryEmailAddressCode code:string = PasswordState` (:11461) — NOT wired this slice (email-code entry is out of scope).
  - `passwordState has_password:bool password_hint:string has_recovery_email_address:bool has_passport_data:bool recovery_email_address_code_info:emailAddressAuthenticationCodeInfo login_email_address_pattern:string pending_reset_date:int32 = PasswordState` (:273). `emailAddressAuthenticationCodeInfo email_address_pattern:string length:int32 = EmailAddressAuthenticationCodeInfo` (:83) — the pattern is a masked string (e.g. `n***@example.com`); TDLib never exposes the real address.
- **Telegram X reference (checked-out TGX-Android tree, 2026-09-28):** `TwoStepVerification` = "Two-Step Verification"; `SetAdditionalPassword` = "Set additional password"; `ChangePassword` = "Change Password"; `SetRecoveryEmail` = "Set Recovery Email"; `ChangeRecoveryEmail` = "Change Recovery Email"; `PendingEmailText` = "Your recovery email %1$s is not yet active and pending confirmation."; `AbortRecoveryEmail` = "Abort recovery email setup". TGX's `PasswordController.java` sends `SetPassword` and `SetRecoveryEmailAddress` and consumes the returned `PasswordState` on success — Quill does the same (no optimistic mutation). Note: the README's old parenthetical named `AbortPasswordSetup`; the TGX string is `AbortRecoveryEmail` and that verbatim wording ships.
- **Built:**
  - Envelope: `PasswordState` model (nullable `recovery_email_address_code_info` handled) + parser + verbatim-constructor tests (`envelope.rs`).
  - Request builders (`requests.rs`): `get_password_state`, `set_password` (old/new/hint/email-in-same-call), `set_recovery_email_address`, `resend_recovery_email_address_code`, `cancel_recovery_email_address_verification`; one shape test covering fetch, enable with optional recovery email, disable via empty new password, email set/change, resend, abort. Passwords ride outbound JSON only, never logged.
  - Session (`state.rs`): `RequestPurpose::PasswordStateOp { op }` with `PasswordOp::{Fetch, SetPassword, DisablePassword, SetRecoveryEmail, ResendCode, AbortEmailSetup}`; cached `password_state`, `password_state_loading`, `password_op_error`. The success reducer accepts the returned `PasswordState` only for a matching pending op, replaces the cache, clears loading/error — no optimistic mutation. Errors classify honestly: flood wait → "too many attempts — wait and try again", unauthorized → "session is no longer authorized", 400 on password ops → "wrong password — try again", 400 on email ops → "the email was rejected — check the address", resend/abort rejection → "Telegram refused the request — try again". Stray `passwordState` (no matching purpose) is ignored.
  - Driver (`connect.rs`): `fetch_password_state()` (guarded: cached state reused, in-flight fetch deduped, `Ok(None)` = no request), `set_two_step_password()` (enable/change/disable), `set_recovery_email()`, `resend_recovery_email_code()` (pending-gated), `cancel_recovery_email_setup()` (pending-gated). Shared `password_op_send`: chats-path gate, one-op-in-flight gate, clears a stale error before send, rolls back pending/loading if `send_json` fails. Driver tests: gates, JSON shapes, full enable→answer→email→pending→resend→abort round-trip through `ingest`, and passwords/emails never reaching diagnostics.
  - UI (`ui/mod.rs`): "🔐 Two-step verification" sidebar entry next to "💾 Storage usage" (Quill has no settings screen); modal overlay with backdrop. Status screen renders the cached state: On/Off, hint, recovery-email Set/Not set, the pending card with "Resend code" + "Abort recovery email setup" (TGX verbatim), and the TGX-verbatim action labels ("Set additional password", "Change Password", "Set Recovery Email"/"Change Recovery Email"). Forms: enable (new + hint + optional recovery email), change (current + new + hint), disable (current only → empty new password), email (current + new address). Submit clears the inputs immediately and `zeroize()`s the password strings (same as the login password flow); "Sent to TDLib only — never logged" captions under password fields. Loading shows an honest "Working…"; errors show the classified line.
  - Screenshot demos `quill --screenshot-demo ready-2fa-manage` (password on, hint, recovery email set) and `ready-recovery-email` (pending card + resend/abort), both injected fixtures → `docs/screenshots/ready-2fa-manage.png`, `docs/screenshots/ready-recovery-email.png`.
- **Key decisions (ponytail):**
  - One shared send gate (`password_op_send`) instead of five ad-hoc send sites; the driver validates only doomed cases (disable without old, email without password/address) and lets TDLib be the authority for the rest — matching the storage-stats "no invented cooldown" precedent (resend has no local countdown; TDLib's 429 surfaces honestly).
  - The email-code *entry* field (`checkRecoveryEmailAddressCode`) is deliberately not in the overlay: the slice's recovery-email contract is set/change + pending state + resend/abort. Added to the backlog below.
  - UI lives entirely in `ui/mod.rs` following the storage-overlay pattern; no new components.
- **Not verifiable without live Telegram:** real `getPasswordState`/`setPassword`/`setRecoveryEmailAddress` round-trips and server-side 429 timing on resend; whether TDLib rejects `setRecoveryEmailAddress` when no password exists (driver doesn't gate it — TDLib is authoritative).
- **Out of this slice (left unchecked with evidence):**
  - `parity:auth-password-recovery` — password recovery via 6-digit email code (`checkRecoveryEmailAddressCode` :11461) on the login screen; still the "available in the official client" hint.
  - Email-code entry inside this overlay (confirming the pending recovery email from the manage screen) — same constructor, same reason it's out: code-entry UX is a separate small slice.
  - `parity:auth-sessions-list`, `parity:auth-sessions-incomplete`, `parity:auth-session-terminate-one`, `parity:auth-sessions-terminate-all`, `parity:auth-session-toggles`, `parity:auth-web-sessions` — sessions management, untouched.
  - Logout warning (e.g. "you'll lose access without your password") — no TGX source wording verified this slice; parked.
## Slice B1 — BOT KEYBOARDS: INLINE BUTTONS, CUSTOM KEYBOARDS, FORCE-REPLY (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - Inline button types: `inlineKeyboardButtonTypeLoginUrl` (:3780), `inlineKeyboardButtonTypeWebApp` (:3783), `inlineKeyboardButtonTypeCallbackWithPassword` (:3789), `inlineKeyboardButtonTypeCallbackGame` (:3792), `inlineKeyboardButtonTypeUser` (:3801). Callback payloads: `callbackQueryPayloadData` (:7737), `callbackQueryPayloadDataWithPassword` (:7740), `callbackQueryPayloadGame` (:7743). `messageGame` (:5234) with game `short_name` (:673).
  - Custom keyboards: `keyboardButton` types :3714–3760, `keyboardButton` :3768; markups `replyMarkupRemoveKeyboard` (:3835), `replyMarkupForceReply` (:3840), `replyMarkupShowKeyboard` (:3850), `replyMarkupInlineKeyboard` (:3855).
  - Requests: `getLoginUrlInfo` (:12985), `getLoginUrl` (:12993), `getCallbackQueryAnswer` (:13138), `deleteChatReplyMarkup chat_id:int53 message_id:int53 = Ok` (:13183).
  - **Negative-claim discipline:** no `sendGame` constructor exists in the pinned schema (concept-level check: `game` in td_api.tl shows only `callbackQueryPayloadGame` + `getCallbackQueryAnswer`; raw telegram_api.tl likewise; TDLib `Requests.cpp` exposes game launch only through the callback-answer path). Games launch with `getCallbackQueryAnswer` + `callbackQueryPayloadGame`, never a dedicated send.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `TGInlineKeyboard.java:1135` — password buttons use `CallbackQueryPayloadDataWithPassword`.
  - `:1193–1207` — game buttons read `messageGame.game.shortName` and send the game payload.
  - `:1209+` — user buttons open the private profile/chat.
  - `:1239–1258` — login URL buttons resolve via `GetLoginUrlInfo`.
- **Built:**
  - Login URL: `getLoginUrlInfo` on press (schema:12985); `loginUrlInfoOpen` opens the URL in the OS browser, `loginUrlInfoRequestConfirmation` shows the TDLib-reported domain for consent and — on consent — `getLoginUrl` (schema:12993) fetches the authorized `httpUrl` (schema:7458), which opens in the OS browser; failures degrade to the raw URL (schema:12993 doc: "the button must be handled as an ordinary URL button"). TGX `TGInlineKeyboard.getLoginCallback` does exactly this consent→`GetLoginUrl` flow; the dialog also notes when the bot requests write access (`request_write_access`, passed through as `allow_write_access`).
  - Web App: honest browser fallback (no in-app web view); README marks partial.
  - Password callbacks: modal prompts for the 2-step password, sends `callbackQueryPayloadDataWithPassword`; TDLib error 400 surfaces as "wrong 2-step verification password".
  - Game buttons: `getCallbackQueryAnswer` with `callbackQueryPayloadGame` carrying the `messageGame` short name; answer URL opens in the browser (games UI beyond launch is out of slice).
  - User buttons: open (or create) the private chat with the user.
  - Custom keyboards: rendered above the composer; text buttons send via the normal path; contact/location/poll/user/chat/managed-bot render disabled with honest tooltips; one-time keyboards hide locally on tap and send `deleteChatReplyMarkup`; resize/persistent minimally honored.
  - Force-reply: incoming `replyMarkupForceReply` arms `pending_force_reply`; the UI drains it into a composer reply-to + focus.
  - Tests: envelope parse tests (show-keyboard, force-reply/remove, game, login-url-info, `reply_markup_demands_reply`), request-shape tests (password/game/login-url-info/delete-reply-markup), state tests (force-reply arming, wrong-password error, login-url fallback, keyboard show/remove/dismiss rules).
  - Screenshot fixture `quill --screenshot-demo ready-bot-keyboard`: inline buttons, game message, custom keyboard, force-reply — `docs/screenshots/ready-bot-keyboards.png`.
- **Key decisions (ponytail):**
  - No in-app web view was built — the OS browser is the honest fallback for web apps and login URLs, matching the schema's client latitude.
  - `active_custom_keyboard` is a pure function in `state.rs` (testable under the no-default-features gate); the UI only renders what it returns.
  - Passwords are not zeroized — the string is dropped after the single TDLib send; speculative zeroization was removed.
- **Not verifiable without live Telegram:** real `getLoginUrlInfo` round-trips and consent variants; real password-callback acceptance/rejection; real game-launch answers; real `deleteChatReplyMarkup` acceptance.
- **Known gap (pre-existing, not B1):** `updateChatReplyMarkup` is unhandled — per the schema doc on `replyMarkupRemoveKeyboard` (:3833), server-driven keyboard removal arrives via that update with null markup, not an inline message. The B1 panel clears on an inline `replyMarkupRemoveKeyboard` message and on one-time use; a stale panel can linger until the bot sends a newer message.
- **Out of this slice (left unchecked with evidence):**
  - `parity:bots-inline-buy` — payments/buy buttons: invoice flow, payment UI, and receipt handling are a full payments slice, not a keyboard addition. Buttons render disabled with an honest tooltip.
  - Games UI beyond launch — scoreboards, game messages rendering, and in-app game surfaces; only the TDLib launch flow (callback answer + URL) is built.
  - Inline mode (`inlineKeyboardButtonTypeSwitchInline` query flows are pre-existing; full inline-result browsing/picking is separate).
## Slice calls-remainder-1 (2026-09-27)

**Scope:** three small, unchecked calls-lane items — scheduled-video-chat
start notification, 1:1 call verification emojis, group video paused
indicator. Signaling/UI only; no transport changes.

**Schema verification (concept-level, pinned TDLib 1.8.67
`schema/td_api.tl`):**
- `toggleVideoChatEnabledStartNotification group_call_id:int32
  enabled_start_notification:Bool = Ok;` — verbatim at :14282; the
  `groupCall.enabled_start_notification:Bool` field at :7154 ("True, if
  the group call is scheduled and the current user will receive a
  notification when the group call starts; for video chats only").
  Pinned in `telegram::envelope` tests verbatim (schema pin test).
- 1:1 call verification emojis: `callStateReady ... emojis:vector<string>
  ...` at :7068 — the 4-emoji E2E fingerprint rides the `Call.state`
  itself (no separate update, unlike group calls'
  `updateGroupCallVerificationState` :10836). Parsed into
  `ReadyParams.emojis`, already threaded to `ActiveCall.ready`.
- Group video pause: `groupCallParticipantVideoInfo ...
  is_paused:Bool` at :7163 ("True, if the video is paused. This flag
  needs to be ignored, if new video frames are received"). Already
  parsed into `GroupCallVideoInfo.is_paused`; TDLib clears the flag
  when new frames arrive, so the tile badge reads it directly.

**TGX evidence:** the checked-out Telegram X tree does not implement
the scheduled-chat notify toggle or the paused-video tile badge (no
`EnabledStartNotification` constructor usage; the `isPaused` hits are
the Tdlib client lifecycle, not video info). Behavior target is
Telegram Desktop's scheduled voice-chat surface ("Notify me when it
starts" toggle on the scheduled-chat info) and its group video tiles
(pause indicator on the tile when the peer's video is paused).

**Built:**
- `toggle_video_chat_enabled_start_notification` request builder +
  JSON shape test; `Connect::toggle_video_chat_start_notification`
  driver method gated on the tracked call still being scheduled
  (`scheduled_start_date > 0`; any viewer may set it — no admin right
  per schema); `RequestPurpose::ToggleVideoChatEnabledStartNotification`
  wired into the group-call error path; the driver sends the flipped
  flag and the honest value arrives back as `updateGroupCall`, which
  the reducer already stores (`ParsedGroupCall` /
  `ActiveGroupCall.enabled_start_notification`).
- UI: the scheduled-chat card (Phase C2h surface) gains a
  "🔕 Notify me when it starts" / "🔔 Notifying — tap to turn off"
  ghost button next to the admin-only "Start now".
- 1:1 call card: "End-to-end verification:" + the 4 emojis from
  `callStateReady`, rendered exactly like the group-call card; shown
  only when the call is Ready and the fingerprint is non-empty.
- Group video tiles: "⏸ paused" badge alongside the existing
  speaking/muted/hand-raised/video/sharing badges when
  `video_info.is_paused`.
- Screenshot fixtures: `ReadyCallVideo` fixture now injects
  `callStateReady.emojis`; `ReadyGroupCall` fixture gives Raj a paused
  camera; new `quill --screenshot-demo ready-group-call-scheduled`
  demo + `ReadyGroupCallScheduled` variant for the scheduled card
  (`docs/screenshots/ready-group-call-scheduled.png`).
- Tests: driver gating tests (scheduled-only, flip on/off,
  refuses on active call), reducer parse test for
  `enabled_start_notification`, `ReadyParams.emojis` parse test, JSON
  shape test. All gates pass: `cargo fmt`, clippy `-D warnings`,
  `cargo test --no-default-features --locked` (835 + 59), `cargo
  build --features ui`.

**Key decisions (ponytail):**
- No separate "notification state" tracking: the toggle sends the
  flipped tracked flag; TDLib's `updateGroupCall` is the source of
  truth (same as every other video-chat admin action in C2h).
- The paused badge reads `video_info.is_paused` only — no frame-age
  heuristic in the tile (the schema note says TDLib clears the flag
  when frames arrive; duplicating that client-side is speculative).
- No 1:1-call info dialog for the emojis: the overlay card shows them
  inline, matching the group-call rendering one screen over.

**Not verifiable without live Telegram:** the real
`toggleVideoChatEnabledStartNotification` round-trip (server
acceptance + `updateGroupCall` echo), the actual push notification at
start time, real `callStateReady.emojis` from a live call, real
`is_paused` traffic.

**Out of this slice:** nothing from the three items was deferred —
`parity:calls-schedule-notify`, `parity:calls-verify-emoji`, and
`parity:calls-group-video-pause` are all checked.

## Dev-profile iteration experiment (2026-09-28, build-infra)

`Cargo.toml`: `[profile.dev] debug = "line-tables-only"`,
`[profile.dev.package."*"] debug = false`, plus `[profile.debugging]
inherits = "dev" debug = true` as the full-debuginfo escape hatch.
Release profile untouched.

Measured on this box (touch `src/ui/mod.rs` + `build --features ui`,
cargo-reported, single measurement — preliminary): warm rebuild 21.07s →
13.73s (~35% faster). One-time
cost: profile change invalidates all fingerprints, so the first build
after the switch recompiles everything (10m42s cargo-reported here).
Safe because: release profile is byte-identical, dev binaries still get
line tables (backtraces usable), and `--profile debugging` restores full
debuginfo when needed.

## Slice B2 — BOT PROFILE ACTIONS: START, RESTART, SHARE, BLOCK, MENU BUTTON, PRIVACY, SIMILAR BOTS (2026-09-27)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - Deep links: `internalLinkTypeBotStart bot_username:string start_parameter:string autostart:Bool = InternalLinkType` (:9399) — "Call searchPublicChat with the given bot username, check that the user is a bot, show START button in the chat with the bot, and then call sendBotStartMessage with the given start parameter after the button is pressed".
  - `sendBotStartMessage bot_user_id:int53 chat_id:int53 parameter:string = Message;` (:12216).
  - Block/unblock: `setMessageSenderBlockList sender_id:MessageSender block_list:BlockList = Ok;` (:14492) — "pass null to unblock" (reused the CL3 plumbing; no separate toggle constructor exists).
  - Menu button: `botMenuButton text:string url:string = BotMenuButton;` (:834), surfaced as `botInfo.menu_button` (line 2430); the schema's `webAppOpenParameters` path (:13103) is the in-app web-app launch — we take the honest browser fallback like B1's WebApp button.
  - Privacy: `botInfo.privacy_policy_url` (:2414) — "The HTTP link to the privacy policy of the bot. If empty, then /privacy command must be used if supported by the bot. If the command isn't supported, then https://telegram.org/privacy-tpa must be opened".
  - Similar bots: `getBotSimilarBots bot_user_id:int53 = Users;` (:11640).
  - History clear: `deleteChatHistory chat_id:int53 remove_from_chat_list:Bool revoke:Bool = Ok;` (:11845).
  - **Negative-claim discipline:** no client-side bot privacy *setting* exists — concept-level search of td_api.tl for bot∩privacy intersections shows only `botInfo.privacy_policy_url` and the bot-owner constructors (`setBotName`, `setBotProfilePhoto`, `setBotInfoDescription`, …:13950–15080, all for bot owners editing their own bot, none a privacy toggle). Telegram X's checkout likewise has no bot-privacy surface. The profile therefore shows privacy read-only: the URL button, else a `/privacy` insert button when the bot lists that command, else the schema's `telegram.org/privacy-tpa` fallback note.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `MessagesController.java:6024` — `showActionBotButton(argument)` shows the START action button for bot chats; `:2704–2710` — a deep-link `TGBotStart` with `useDeepLinking()` either auto-calls `sendBotStartMessage` or shows the button with the argument; `:6180–6194` — press unblocks the bot first when fully blocked, then `tdlib.sendBotStartMessage(userId, chat.id, botStartArgument)`, then hides the button.
  - `Tdlib.java:5243–5248` — `blockSender(sender, blockList)` / `unblockSender(sender)` are `SetMessageSenderBlockList(sender, blockList/null)`.
  - `ProfileController.java:632–635, 738–757` — profile overflow menu: BlockBot/UnblockBot with a confirm dialog; `Tdlib.blockSender(tdlib.sender(chat.id), new TdApi.BlockListMain(), …)`.
  - `ProfileController.java:4639` — share builds the `t.me/<username>` link (`tdlib.tMeUrl(username)`).
  - `SharedChatsController.java:112` + `ProfileController.java:5908` — `Mode.SIMILAR_BOTS` backed by `TdApi.GetBotSimilarBots(chatId)`, rendered as a profile tab.
  - Divergence noted: TGX's "Restart" (`R.string.RestartBot` = "Restart") exists only in the unblock slot for blocked bot chats and does **not** clear history. The B2 profile action instead clears the chat and re-sends /start per the slice contract — documented here so nobody "simplifies" it back to the TGX variant.
- **Built:**
  - `BotInfo` gains `menu_button: Option<BotMenuButton>` and `privacy_policy_url: String` (envelope.rs; null/absent → None/"").
  - Start flow: pure `parse_bot_start_link` (state.rs) parses `t.me/<bot>?start=<param>` (t.me/telegram.me, http/https/bare) into the two `internalLinkTypeBotStart` pieces — unit-tested; `Session::bot_start_params` holds the armed parameter per chat; the panel shows a START button while armed; press → `sendBotStartMessage` via the new `RequestPurpose::SendBotStartMessage`, then the param clears (TGX's `hideActionButton`).
  - Restart bot: confirm-gated `GroupConfirmAction::RestartBot` → `ConnectDriver::restart_bot` = `deleteChatHistory(revoke:false, kept in list)` + `sendBotStartMessage` with an empty parameter.
  - Share: copies `https://t.me/<username>` to the clipboard (desktop's copy-link affordance; shown only when the bot has a username).
  - Block/Unblock: reuses `set_chat_user_blocked` + the CL3 `BlockUser` confirm; the label follows `chat.blocked` (refreshed by `updateChatBlockList`).
  - Menu button: renders `botInfo.menu_button` in the profile panel; opens the URL in the OS browser (no in-app web view — same honest fallback as B1).
  - Similar bots: `getBotSimilarBots` (deduped by `Session::similar_bots` + in-flight purpose; the `users` answer keys by the pending request's `user_id`, names resolve via `Session::users`), rendered as a button row that opens each bot's chat via the B1 `open_user_chat` path.
  - Tests: request-shape tests (sendBotStartMessage / getBotSimilarBots), deep-link parser tests, reducer tests (users→similar_bots keyed by pending user), error-surface tests.
  - Screenshot fixture `quill --screenshot-demo ready-bot-profile`: armed START button, menu button, privacy-policy link, Restart/Share/Block-Unblock row, loaded similar bots — `docs/screenshots/ready-bot-profile.png`.
- **Key decisions (ponytail):**
  - No OS deep-link intake was built — Quill registers no URL scheme, so the deep link arrives only via `parse_bot_start_link` + the armed-param state (the demo/screenshot path exercises it); a real intake is an "Out of this slice" item.
  - No separate block-confirm strings for bots — the shared CL3 dialog ("Block user? …") is honest and reuses one code path.
  - Share is copy-to-clipboard, not a share sheet — Linux desktops have no system share sheet; copy-link is the task's named alternative.
- **Not verifiable without live Telegram:** real `sendBotStartMessage` round-trips (start with parameter, restart sequence), real `getBotSimilarBots` answers, real block-state propagation via `updateChatBlockList`.
- **Out of this slice (left unchecked with evidence):**
  - OS-level deep-link intake (registering `t.me`/`tg:` handling so a real clicked link arms the START button).
  - In-app web view for the menu button (browser fallback is the honest behavior; same as B1).
  - Bot "privacy mode" (Bot API `privacy_mode`) — a bot-side setting with no TDLib client API; correctly not client-configurable.

## Slice B4 — POLLS MANAGEMENT: VOTERS, STOP, QUIZ EXPLANATION, RESTRICTIONS, PERMISSIONS, SERVICE MESSAGES (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `getPollVoters chat_id:int53 message_id:int53 option_id:int32 offset:int32 limit:int32 = PollVoters;` (:12941). Doc (:12934): "Returns message senders voted for the specified option in a poll; use poll.can_get_voters to check whether the method can be used." `option_id` is a 0-based option index (like `setPollAnswer`, line 12932); `limit` ≤ 50.
  - `stopPoll chat_id:int53 message_id:int53 reply_markup:ReplyMarkup = Ok;` (:12953). Doc: "Stops a poll … Use messageProperties.can_be_edited to check whether the poll can be stopped" and "`reply_markup` … pass null if none; for bots only" — the human client always sends null. The poll closes via `updatePoll`; `ok` carries no payload.
  - `pollVoters total_count:int32 voters:vector<pollVoter> = PollVoters;` (:2854).
  - `poll` fields (:711): `can_get_voters` ("True, if the current user can get voters in the poll using getPollVoters", :698) and `vote_restriction_reason:PollVoteRestrictionReason` ("The reason describing, why the current user can't vote in the poll; may be null if the user can vote in the poll", :710).
  - Restriction constructors (:494–:510): `pollVoteRestrictionReasonClosed`, `YetUnsent`, `Scheduled`, `CountryRestricted country_code:string`, `MembershipRequired chat_id:int53`, `Other`.
  - `pollTypeQuiz … explanation:formattedText …` (:475): "Text that is shown when the user chooses an incorrect answer or taps on the lamp icon; empty for a yet unanswered poll".
  - `chatPermissions … can_send_polls:Bool …` (:1070) — gates the poll composer entry.
  - `messageProperties … can_be_edited:Bool …` (:6262) — the server gate for stopping polls.
  - `chatEventPollStopped message:message = ChatEventAction;` (:7776) — the service/event constructor (no standalone `messagePollStopped` MessageContent constructor exists; concept-level search of td_api.tl for `PollStopped` finds only the event constructor, and TDLib's `ChatEventManager` builds it from stopped-poll log entries).
- **Telegram X evidence (TGX-Android source):**
  - `MessageView.java` (~707): Stop Poll/Quiz menu items offered only on open polls with `canBeEdited`.
  - `MessagesController.java:5587–5595`: destructive confirm then `TdApi.StopPoll`; warning copy says stopping prevents further voting and cannot be undone.
  - `PollResultsController.java:130`: voter pagination with offset = loaded count, page size 50.
  - `TGMessageService.java:1740–1751`: separate event-log text for a stopped poll vs a stopped quiz (`EventLogPollStopped` / `EventLogQuizStopped`).
- **Built:**
  - `requests.rs`: `get_poll_voters` and `stop_poll` with request-shape tests pinning the verbatim constructors via `include_str!` of the pinned schema.
  - `envelope.rs`: `Poll.can_get_voters`, `Poll.vote_restriction_reason`, `PollType::Quiz { correct_option_ids, explanation }`, `EnvelopePayload::PollVoters`, `ChatEventAction::PollStopped { is_quiz }` (quiz detected from the embedded poll content).
  - `state.rs`: `Session::poll_voters` keyed by `(chat_id, message_id, option_id)` with `Loading`/`Loaded`/`Failed`; first page replaces, later pages append and dedupe by sender.
  - `connect.rs`: `fetch_poll_voters` / `load_more_poll_voters` (50 per page) and `stop_poll`, guarded on chats-path-active, supported chats, real loaded poll messages, `can_get_voters`, and in-flight dedupe.
  - `poll.rs` (pure logic, unit-tested): `poll_vote_restriction_label`, `quiz_explanation` (shown after answering, per schema doc), `can_stop_poll`, `can_view_poll_voters`, `chat_allows_polls` (absent permissions = unknown = allowed); `Poll::can_vote` now also rejects non-null restriction reasons.
  - UI: restriction reason rendered on the poll card; quiz explanation appears after answering; per-option "View voters" dialog with paginated voter rows (names resolved from session users/chats, Load more at 50/page); "Stop poll"/"Stop quiz" message-menu item → red confirm banner with the TGX warning copy; demo stop flips the poll closed locally; poll composer entry hidden when `can_send_polls` is false with a "Polls restricted in this chat" notice; event log renders "stopped the poll" / "stopped the quiz".
  - Tests: pure-logic tests (restriction labels, explanation conditions, stop gating, voter affordance, composer permission), request-shape tests, reducer tests (pollVoters replace/append/dedupe), parse tests (`pollVoters`, `chatEventPollStopped` with quiz/non-quiz content), schema-pin tests.
  - **Screenshot:** `docs/screenshots/ready-poll.png` refreshed — the demo chat now also shows the quiz with a non-empty explanation (rendered after answering) and a membership-restricted poll with the restriction label, both injected through the real reducer; driven by `quill --screenshot-demo ready-poll`.
- **Key decisions (ponytail):**
  - Voters live in one `PollVotersDialog` on the G1 modal shell with a single `G1DialogClose::PollVoters` variant — no new modal infrastructure.
  - No optimistic poll closure on stop: the server gate (`can_be_edited`) and the real close (`updatePoll`) are authoritative; the demo path flips `is_closed` locally only.
  - The "View voters" affordance is per-poll (not per-option) opening the dialog at option 0 — one button, one dialog, per-option switching inside.
- **Review fixups (dedicated reviewer, 2026-09-28, verdict: approve-with-fixups — all applied):**
  - F1: "Load more" could stick forever on an approximate `total_count` after a short final page — now the reducer clamps `total_count` to the actually-held count on a short page (< 50), so the UI's `voters.len() >= total_count` check hides the button.
  - F2: quiz explanation now auto-shows only after an **incorrect** answer (TGX `TGMessagePoll.java:1085-1086`); the lamp-icon on-demand reveal for correct answers is **not** implemented — documented deviation, README box reworded.
  - F3: `can_stop_poll` gates on `is_outgoing` because Quill parses only `can_get_link` from `MessageProperties` — the schema-prescribed `can_be_edited` gate would also allow channel/group admins to stop others' polls. Documented known approximation (not a hidden limitation).
  - F4: `stop_poll` driver now also checks `can_stop_poll` (ownership + open) instead of only `!is_closed` — defense in depth behind the UI menu gate.
  - F5: `poll_answer_for_tap` returns `None` when `vote_restriction_reason` is set, keeping the demo tap path in sync with the live `send_poll_answer` gate.
  - N1: stop warning copy uses `\n\n` like TGX `StopPollWarn`/`StopQuizWarn`. N2: `apply_update_poll` invalidates cached voter pages for touched messages.
- **Not verifiable without live Telegram:** real `getPollVoters` pages, real `stopPoll` → `updatePoll` propagation, real `chatEventPollStopped` in the event log.
- **Out of this slice (left unchecked with evidence):**
  - `parity:bots-poll-show-voters` — the dedicated "show voters" poll-list surface (per B3's annotation, TGX treats show-voters as inverse anonymity; Quill's in-dialog viewer is the forward path, not this separate surface).
  - Poll media (`explanation_media`, `pollMedia*` on messages).
  - In-app updater (Loop 4 slice, still queued per Idan's 2026-09-27 standing decision — listed here only as the slice's out-of-scope items).

## Slice calls-remainder-2 (2026-09-28)

**Scope:** the last two unchecked calls-lane items —
`parity:calls-audio-fx` (echo cancellation / noise suppression
toggles) and `parity:calls-proxy` ("Use proxy for calls" setting).
**Both stay unchecked** — neither the TDLib schema nor the native
call engine exposes the controls; the evidence is documented here
instead of faking the features.

**Schema investigation (concept-level, pinned TDLib 1.8.67
`schema/td_api.tl`; raw `telegram_api.tl` is not vendored in this
repo):**
- The complete proxy API surface is client-connection scope only:
  `addProxy` (:16206), `enableProxy` (:16216), `disableProxy`
  (:16219), `removeProxy` (:16222), `getProxies` (:16225),
  `pingProxy` (:16229). No call-specific proxy
  function, constructor, or `CallServer`-level proxy field exists
  anywhere in the schema.
- The only "use-for-calls" mention is in the internal-link routing
  docs at :9276 (`"proxy/add-proxy", "proxy/share-list",
  "proxy/use-for-calls"`) — a client-side proxy-screen route, not a
  TDLib function. Call media does **not** inherit the client proxy:
  nothing in `callProtocol` / `CallServer` / `callStateReady`
  carries proxy configuration.
- Audio FX: no TDLib constructor controls echo cancellation, noise
  suppression, or AGC — these are engine-side, not protocol-side.

**Native engine investigation (`crates/ntgcalls-sys`, pytgcalls
ntgcalls v3.0.0 prebuilt lib, bindings verified against
`vendor/ntgcalls/include/ntgcalls.h`):**
- The full 76-function declared surface contains **zero**
  echo/noise/AGC functions and **zero** proxy/SOCKS functions
  (case-insensitive search of `lib.rs` for
  echo/noise/agc/aec/vad/proxy/socks: no hits).
- `ntg_audio_description` / `ntg_media_description` carry only
  device/source descriptors — no audio-FX fields.
- `ntg_connect_p2p` / `ntg_create_call` / `ntg_init_exchange` take no
  proxy parameters; Quill passes NULL for `custom_parameters`
  (`src/calls/engine.rs:1460`) and there is no documented channel to
  inject audio-FX or proxy config through it.

**Telegram X evidence (`~/workspace/telegram-x`):**
- `CallConfiguration.java` — the native VoIP stack receives
  `enableAcousticEchoCanceler`, `enableNoiseSuppressor`,
  `enableAutomaticGainControl` as **call-creation-time engine
  config** (`VoIP.java:399-412`, derived as
  `!preferSystemAcousticEchoCanceler` / `!preferSystemNoiseSuppressor`
  and adaptive `echoCancellationStrength = isHeadsetPlugged || var ?
  0 : 1` in `TGCallService.java:267`). These are not user-facing
  toggles in TGX settings — and Quill's engine exposes no such
  config path at all.
- "Use proxy for calls" is **client-side**: `SettingsProxyController`
  has a `btn_useProxyForCalls` radio toggle; `Settings.java:4515`
  `getEffectiveCallsProxyId()` returns the enabled proxy id only when
  both `PROXY_FLAG_ENABLED` and `PROXY_FLAG_USE_FOR_CALLS` are set,
  and `VoIP.java:365,399` hands the enabled proxy as
  `@Nullable Socks5Proxy` to the **native** VoIP stack at call start
  ("Proxy servers may degrade the quality of your calls."
  `strings.xml:2026-2027`). Because Quill's ntgcalls C API cannot
  accept a proxy, this client-side plumbing has nothing to hand the
  proxy to — implementing the toggle alone would be a dead setting.
  (Quill also has no proxy support at all yet — no `addProxy` /
  `enableProxy` calls anywhere in `src/` — so the pref would have no
  backing proxy to reference. Not added: YAGNI.)

**Key decisions (ponytail):**
- No persisted `CallPrefs` for either item: a toggle that cannot
  reach the engine is a fake feature, and the task explicitly
  sanctions "unchecked with evidence" for this case.
- Both README boxes stay unchecked with their existing partial
  notes; nothing here regresses the parity percentage.
- The unblocking condition for both is a future ntgcalls C API
  revision exposing audio-FX and SOCKS5-proxy configuration; at that
  point the TGX pattern (call-start config + `CallPrefs` persistence)
  is the ready-made shape.
## Slice A3 — ACTIVE SESSIONS: LIST, INCOMPLETE ATTEMPTS, TERMINATE ONE / ALL OTHERS (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim — concept-level, never single-grep):**
  - `session id:int64 is_current:Bool is_password_pending:Bool is_unconfirmed:Bool can_accept_secret_chats:Bool can_accept_calls:Bool device_type:SessionDeviceType api_id:int32 application_name:string application_version:string is_official_application:Bool device_model:string platform:string system_version:string log_in_date:int32 last_active_date:int32 ip_address:string location:string = Session;` (:9144). Doc :9128: `is_password_pending` — "True, if a 2-step verification password is needed to complete authorization of the session".
  - `sessions sessions:vector<session> inactive_session_ttl_days:int32 = Sessions;` (:9147).
  - `getActiveSessions = Sessions;` (:15102) — the ONLY function in the whole file returning `Sessions` (grep for `= Sessions;` over the full schema: only :9147, :15102).
  - `terminateSession session_id:int64 = Ok;` (:15105); `terminateAllOtherSessions = Ok;` (:15108).
  - **Negative claim (concept-level, with search strategy):** there is NO separate "incomplete login attempts" constructor. Searched (1) `td_api.tl` for `incomplete` / `login_attempt` / `= Sessions;` — only the `session.is_password_pending` field and `getActiveSessions`; (2) the raw MTProto layer — `authorization#ad01d61d` carries `password_pending:flags.2?true` and `account.getAuthorizations` returns them all in one `account.Authorizations` list; (3) TDLib source — `td/telegram/AccountManager.cpp`, `GetAuthorizationsQuery` converts every authorization (including password-pending ones) into `td_api::sessions` entries and sorts current → password-pending → rest by last-active. TGX's `SessionsInfo` split on `isPasswordPending` is therefore the correct client-side model: incomplete attempts ARE `getActiveSessions` entries with `is_password_pending=true`.
- **Telegram X reference (strings, verbatim):** `SessionsTitle` "Active Sessions"; `SessionsIncompleteTitle` "Incomplete Login Attempts"; `SessionsIncompleteInfo` "The devices above have no access to your messages. The code was entered correctly, but no correct password was given."; `TerminateSessionQuestion` "Terminate this session?"; `TerminateIncompleteSessionQuestion` "Terminate this login attempt?"; `TerminateAllSessions` "Terminate All Other Sessions"; `ThisDevice` "This Device"; `AreYouSureSessions` "Are you sure you want to terminate all other sessions?"; `SessionLastActiveDate` "Last active: %1$s". All confirm wordings copied verbatim.
- **Built:**
  - Envelope: `ParsedSession` + `EnvelopePayload::Sessions { sessions }` parser (schema-pin tests for current / ordinary / password-pending sessions).
  - Requests: `get_active_sessions`, `terminate_session`, `terminate_all_other_sessions` (JSON-shape test).
  - State: `RequestPurpose::{GetActiveSessions, TerminateSession, TerminateAllOtherSessions}`; fields `sessions` / `sessions_loading` / `sessions_mutating` / `sessions_error` / `sessions_stale`; authoritative `sessions` reducer (extra-matched, account-generation-guarded); terminate-`ok` keeps the old cache and marks `sessions_stale` so the refetched list replaces it — never an optimistic delete; classified terminate errors (`sessions_error_line`).
  - Driver: `maybe_fetch_active_sessions` (fetch-once-per-account-generation guard), `refresh_active_sessions_if_stale` (runs on every `ingest` when stale and no in-flight fetch), `terminate_session` (rejects only unknown session ids and concurrent mutations; the current session's id passes the guard, goes to TDLib, and its 400 refusal surfaces via the classified error line), `terminate_all_other_sessions`.
  - UI (`ui/mod.rs`): sidebar "📱 Active sessions" entry; overlay with Current-session card (+ "This Device" chip), the Incomplete Login Attempts section (TGX info copy verbatim), other sessions sorted last-active-desc, per-row Terminate (current device has none), "Terminate All Other Sessions" (disabled when only the current session exists or a mutation is in flight), confirm banner with TGX's verbatim questions, honest loading/error lines, Esc cancels confirm then closes. Refresh drops the cache and refetches.
  - Screenshot demo `quill --screenshot-demo ready-sessions` (injected fixture: current device + two other sessions + one incomplete attempt, no live Telegram) → `docs/screenshots/ready-active-sessions.png`.
- **Key decisions (ponytail):**
  - No optimistic deletion: the terminated row stays visible with `sessions_stale` until the authoritative refetch lands — the cheapest honest state machine, one flag, zero invented rows.
  - Section split happens client-side on `is_password_pending` (TGX `SessionsInfo` precedent); `is_unconfirmed` sessions stay in the ordinary list (TGX does not separate them; the schema doc for TDLib's sort puts them after password-pending, which the last-active sort already approximates).
  - Relative "last active" only (just now / Nm ago / Nh ago / Nd ago) — no timezone math, the `format_starts_in` precedent.
- **Not verifiable without live Telegram:** real `getActiveSessions` round-trip, real terminate `ok` → refetch sequencing, real 24h `FRESH_RESET_AUTHORISATION_FORBIDDEN` on terminate, real incomplete-attempt payloads.
- **Out of this slice (left unchecked with evidence):** `parity:auth-session-websites` — connected websites (`getConnectedWebsites`/`disconnectWebsite`) is a different constructor family; `parity:auth-session-toggles` — per-session `can_accept_secret_chats`/`can_accept_calls` toggles (constructors exist: `toggleSessionCanAcceptSecretChats` (`td_api.tl:15117`) and `toggleSessionCanAcceptCalls` (`td_api.tl:15114`), and `toggle_session_can_accept_secret_chats` is already shipped in `requests.rs:1671`; separate per-row toggle surface, left for A4).

## Slice CL — CHAT LIST: PEEK PREVIEW (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `getChatHistory chat_id:int53 from_message_id:int53 offset:int32 limit:int32 only_local:Bool = Messages` (:11829) — the only history constructor the preview needs. The preview deliberately never calls `openChat` / `viewMessages`, so nothing is marked read.
  - **Negative-claim discipline:** no "not in schema" claims are made; the single constructor above was read verbatim from the pinned schema.
- **Reference-client evidence:**
  - TGX-Android (`~/workspace/telegram-x`, current source): `ChatsController.java` :2241 — long-press on a chat row calls `showChatOptions` (the options menu), not a message preview. TGX's actual peek is `ForceTouchView` (3D-touch/force-press) showing a floating card with recent message history + quick actions — a mobile force gesture with no desktop GPUI primitive.
  - tdesktop (secondary behavioral target): the chat-preview popup constructs its own `HistoryView::ListWidget`, separate from the normal chat view; the desktop trigger is hovering the chat list.
  - Quill's adaptation: desktop long-press — 600 ms press-and-hold built from GPUI `on_mouse_down`/`on_mouse_up` + a background timer. Hover was rejected: it fires while the user aims for the right-click row menu, folder tabs, or pin-drag handles, and a hover panel would fight those gestures. Long-press matches the TGX mobile gesture and is unambiguous on desktop.
- **Built:**
  - Driver `fetch_chat_preview_history(chat_id)` (`src/connect.rs`): one-shot `getChatHistory(chat_id, from_message_id=0, offset=0, limit=10, only_local=false)`, de-duped per chat while in flight, purpose `RequestPurpose::GetChatPreview`.
  - Reducer (`src/state.rs`): the `messages` answer lands in `Session::chat_preview_fetch` — it never merges into the open chat's history (the normal `GetHistory` branch drops non-open answers). Errors set `failed` so the panel shows an error, not a spinner.
  - UI (`src/ui/mod.rs`): `on_mouse_down(Left)` on chat-list rows starts the 600 ms timer (skipped in multi-select mode); `on_mouse_up` / `on_mouse_up_out` cancel it, so a quick release stays a plain click. On expiry the floating panel renders beside the row: chat title + up to 10 read-only sender/body rows using the same one-line `MessageContent::preview()` the chat list shows (media falls back to its type label). Already-loaded messages render immediately; the live fetch fills unopened chats. A full-window catcher closes the preview on click or on the release that ends the long press (its `stop_propagation` keeps the release from clicking through to the rows below); Escape closes it first in `cancel_search`.
  - Tests: reducer test `cl_chat_preview_cached_for_unopened_chat` — success is retained for a non-open chat, nothing merges into that chat's history, and a failure marks the error line.
  - Screenshot fixture `quill --screenshot-demo ready-chat-preview`: preview open on "Demo chat B" with injected messages, chat 11 still the open chat — `docs/screenshots/ready-chat-preview.png`.
- **Key decisions (ponytail):**
  - No hover trigger (see reference evidence above).
  - No interactive rows — sender/body text only. The full history renderer was rejected: its per-message controls would violate the read-only preview contract.
  - No dimmed backdrop: the panel sits beside the row and a release dismisses it; dimming would block reading the list the preview floats over.
  - The preview never calls `openChat` — unread state is untouched by construction, not by a flag.
- **Not verifiable without live Telegram:** a real `getChatHistory` round-trip for an unopened chat; the 600 ms hold timing on real hardware.
- **Out of this slice (left unchecked with evidence):**
  - Quick actions on the preview (TGX `ForceTouchView` offers mute/pin/etc.) — explicit slice exclusion (no message actions, no composer).
  - Pointer-leave dismissal: the panel is adjacent to the row, so leaving the row to reach the panel must not kill it; release / click-anywhere / Escape dismiss instead. Revisit only if a hover trigger is ever added.
## Slice B3 — POLL CREATION OPTIONS: QUIZ, DESCRIPTION, DURATION, REVOTING, SHUFFLE, COUNTRIES, DISCARD CONFIRM (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `inputMessagePoll` (:6193): `question options description media is_anonymous allows_multiple_answers allows_revoting members_only country_codes shuffle_options hide_results_until_closes type open_period close_date is_closed`.
  - `inputPollTypeQuiz` (:488): `correct_option_ids:vector<int32> explanation:formattedText explanation_media:InputPollMedia`; "Increasing list of 0-based identifiers of the correct answer options; must be non-empty"; "explanation … 0-200 characters with at most 2 line feeds".
  - `inputPollTypeRegular` (:481): `allow_adding_options:Bool`.
  - `open_period`: "0-getOption(\"poll_open_period_max\"); pass 0 if not specified" (:6190); `close_date` likewise (:6191). Country codes: "two-letter ISO 3166-1 alpha-2 … up to getOption(\"poll_country_count_max\")" (:6188).
  - **Negative-claim discipline:** there is NO show-voters field on `inputMessagePoll` (concept-level check: `voter` in td_api.tl shows only `recent_voter_ids`/`can_get_voters` on the server-side `poll` object (:711) and `getPollVoters` (:12941); no creation-time flag anywhere). Voter visibility at creation is exactly `is_anonymous`.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `CreatePollController.java:887` — the "Show voters" setting maps to `isAnonymous = !(showVoters)` on `InputMessagePoll`; it is the inverse of the anonymous toggle, not a separate field.
  - `:571–576` — quiz toggle forces revoting off only (`settingRevoting.setBoolValue(!value)`); TGX does NOT force single-answer off and supports multi-correct quizzes when multi-answers is on — Quill's single-answer forcing in quiz mode is Quill's own stricter choice (single-correct-option radio UI). Quiz sends `InputPollTypeQuiz(correctOptionIds, explanation, null)` (:876–880).
  - `:442–444` — closing with unsent input shows a discard prompt (`PollDiscardPrompt` / `QuizDiscardPrompt`).
  - `:318–319, :505` — duration picker is a `TODO` in TGX (never implemented); `:867–868` — `openPeriod`/`closeDate` left 0.
  - `:480` — country picker capped at `pollCountryCountMax` (server option).
- **Built:**
  - `PollDraft` (`poll.rs`): `description`, `is_quiz`, `quiz_correct` (usable-option index), `quiz_explanation`, `allows_revoting`, `shuffle_options`, `duration_hours` (raw text), `country_codes` (parsed, uppercased). `validate()` adds: quiz requires a correct option, explanation ≤200 chars / ≤2 line feeds, duration 1–24h when set, country codes 2-letter uppercase. `open_period_secs()` converts hours→seconds.
  - `send_poll` (`telegram/requests.rs`): new `PollTypeSend::{Regular, Quiz}` enum; quiz emits `inputPollTypeQuiz` verbatim per :488 (explanation as `formattedText`, `explanation_media` null); `description` sent as `formattedText` when non-empty else null; `allows_revoting`, `shuffle_options`, `country_codes`, `open_period` now flow through instead of hardcoded.
  - `send_poll_draft` (`connect.rs`): maps the draft; quiz mode normalizes to single-answer + no revoting (revoting-off matches TGX; single-answer is Quill's own stricter choice); stale "quiz stays out of this slice" comments updated.
  - Dialog (`ui/mod.rs`): description field above the question; quiz toggle with per-option correct-answer radios and an explanation field; duration + countries text fields; revoting/shuffle toggles; inline "Discard this poll? / Discard / Keep editing" confirmation on Cancel and Esc when the dialog is dirty.
  - Tests: draft validation (quiz correct-option, explanation limits, duration bounds, country format), request-shape tests pinning `inputPollTypeQuiz` and the full field set against the schema, driver tests for the quiz normalization.
- **Key decisions (ponytail):**
  - Duration is a plain "hours" text field (1–24), not a date picker: TGX never built one, and the schema bound is a server option Quill doesn't cache — a `ponytail:` comment in `poll.rs` names the upgrade path (cache `getOption("poll_open_period_max")`).
  - Country restriction is a comma-separated text field, format-validated only; the count cap stays server-enforced (no invented constant).
  - Description/explanation go out as plain `formattedText` with empty entities (the question already does); no markup parsing added.
  - `members_only`, `hide_results_until_closes`, poll media, explanation media untouched (still zero/null).
- **Not verifiable without live Telegram:** server acceptance of `open_period` values, country-restricted polls (channel-only), quiz sends.
- **Out of this slice (left unchecked with evidence):**
  - `parity:bots-poll-show-voters` — no such creation field exists; TGX implements it as the inverse of `is_anonymous` (already have the Anonymous toggle). Voter-list display is `getPollVoters` = `parity:bots-poll-voters` (separate box, still unchecked).
  - Poll media (`InputPollMedia`), `members_only`, `hide_results_until_closes`, explanation media, voter list, stop poll, quiz-explanation display, vote-restriction reasons, `can_send_polls` gating, stopped service message.

## Slice A4 — CONNECTED WEBSITES + PER-SESSION SECRET-CHAT / CALL TOGGLES (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim — concept-level, never single-grep):**
  - `session` constructor (:9144) carries `can_accept_secret_chats:Bool can_accept_calls:Bool` — the two toggles' target state; `ParsedSession` parses both.
  - `connectedWebsite id:int64 domain_name:string bot_user_id:int53 browser:string platform:string log_in_date:int32 last_active_date:int32 ip_address:string location:string = ConnectedWebsite;` (:9168).
  - `connectedWebsites websites:vector<connectedWebsite> = ConnectedWebsites;` (:9171).
  - `getConnectedWebsites = ConnectedWebsites;` (:15124); `disconnectWebsite website_id:int64 = Ok;` (:15127); `disconnectAllWebsites = Ok;` (:15130).
  - `toggleSessionCanAcceptSecretChats session_id:int64 can_accept_secret_chats:Bool = Ok;` (:15117); `toggleSessionCanAcceptCalls session_id:int64 can_accept_calls:Bool = Ok;` (:15114).
  - **Correction to A3's "Out of this slice" note:** it claimed no TDLib mutator exposes per-session accept flags — wrong. `toggleSessionCanAcceptSecretChats`/`toggleSessionCanAcceptCalls` are in the pinned schema (and a `toggle_session_can_accept_secret_chats` request builder already existed from S1, request-layer-only). This slice wires both into A3's session rows.
- **Telegram X reference (strings + behavior, verified verbatim):**
  - Websites (`SettingsWebsitesController.java`, `strings.xml:2728-2738`): screen title `WebSessionsTitle` "Logged In with Telegram"; red `TerminateAllWebSessions` "Disconnect All Websites" row + `ClearOtherWebSessionsHelp` "You can log in on websites that support signing in with Telegram."; section header `OtherWebSessions` "Connected Websites"; per-row: domain title, subtext "bot username, browser, platform", location line `concatIpLocation(ip, location)`, last-active time; footer `ConnectedWebsitesDesc` "Tap to disconnect from your Telegram account."; row click → options `DisconnectWebsiteAction` "Disconnect Website" → confirm screen "Disconnect %1$s?" (`TerminateWebSessionQuestion`) with red `DisconnectWebsite` "Disconnect" button + optional "Block %1$s" checkbox (`DisconnectWebsiteBan`); disconnect-all → options "Are you sure you want to disconnect all websites?" (`DisconnectAllWebsitesHint`) with red "Disconnect All Websites" button; empty state `NoActiveLogins` "**No active logins**\n\nYou can log in on websites that support signing in with Telegram."
  - Toggles (`EditSessionController.java`, `strings.xml:5166-5171`): editor rows `SessionSecretChats` "Secret Chats" / `SessionAcceptsCalls` "Calls" with `SessionAccept` "Accept" / `SessionReject` "Reject" radio values; toggling is DIRECT — no confirmation; on Done the client sends `ToggleSessionCanAcceptSecretChats`/`ToggleSessionCanAcceptCalls` with the new values and updates the session from the server answer (`sendAll` callback, then listener propagation). Shown for the current session too (`SettingsSessionsController` opens the editor for `R.id.btn_currentSession` as well); the `SessionAccepts` section is gated only on `!isPasswordPending` (`EditSessionController.java:225`).
- **Built:**
  - Requests: `get_connected_websites`, `disconnect_website`, `disconnect_all_websites`, `toggle_session_can_accept_calls` (the secret-chats twin already existed) — JSON-shape test.
  - Envelope: `ParsedWebsite` + `EnvelopePayload::ConnectedWebsites { websites }` parser; `ParsedSession` gains `can_accept_secret_chats` / `can_accept_calls`.
  - State: `RequestPurpose::{GetConnectedWebsites, DisconnectWebsite, DisconnectAllWebsites, ToggleSessionSecretChats, ToggleSessionCalls}`; fields `connected_websites` / `connected_websites_loading` / `websites_mutating` / `websites_error` / `websites_stale`; authoritative `connectedWebsites` reducer (extra-matched); disconnect-`ok` keeps the old cache and marks `websites_stale` so the refetched list replaces it — never optimistic; toggle-`ok` marks A3's `sessions_stale` (the toggled value arrives in the refetch, never flipped optimistically); classified errors via the existing `sessions_error_line` classifier.
  - Driver: `maybe_fetch_connected_websites` (fetch-once guard), `refresh_connected_websites_if_stale` (on every ingest, next to A3's), `disconnect_website` (rejects unknown ids), `disconnect_all_websites`, `toggle_session_can_accept_secret_chats` / `toggle_session_can_accept_calls` sharing one `send_session_toggle` path (two-case enum; the toggled value negates the cached flag).
  - UI (`ui/mod.rs`): sidebar "🌐 Connected websites" entry; "Logged In with Telegram" overlay with the red "Disconnect All Websites" action, "Connected Websites" section, per-row Disconnect (confirmation banner with TGX's verbatim questions), honest loading/error/empty lines, Refresh, Esc-equivalent backdrop close; session rows in the Current and Others sections gain "Secret Chats: Accept/Reject" and "Calls: Accept/Reject" direct toggles (disabled while mutating; TGX's `EditSessionController` shows them for the current session too).
  - Screenshot demos: `quill --screenshot-demo ready-web-sessions` (three-website fixture) → `docs/screenshots/ready-web-sessions.png`; the `ready-sessions` demo now renders the toggle buttons → `docs/screenshots/ready-session-toggles.png`.
- **Key decisions (ponytail):**
  - One shared `send_session_toggle` with a two-case kind enum instead of two near-duplicate driver methods; follow-up `7a1bfce` changed the S1 `toggle_session_can_accept_secret_chats` request builder to send the raw i64 `session_id` (numeric in the JSON body — asserted by `toggle_session_can_accept_secret_chats_shape_matches_1_8_67`, matching A3's `terminateSession` shape).
  - The bot-username subtext (TGX shows the login bot's username) needs a users-cache lookup — dropped to a sub "browser · platform" line with login date on the meta line; no new cache plumbing.
  - Disconnect confirmations use Quill's existing confirm-banner pattern (inside the overlay) rather than TGX's bottom-sheet options — one established pattern, same confirmation guarantee.
- **Not verifiable without live Telegram:** real `getConnectedWebsites` round-trip, real disconnect `ok` → refetch sequencing, real toggle `ok` → authoritative flag values, real bot-user avatar/username resolution.
- **Out of this slice (left unchecked with evidence):**
  - The "Block %1$s" checkbox on the website-disconnect confirm (`DisconnectWebsiteBan`; TGX chains `blockSender(MessageSenderUser(botUserId), BlockListMain)`) — needs the block-sender call plus a checkbox in the banner; backlog.
  - "Open Chat" from a website row (TGX `openPrivateChat(botUserId)`) — needs a users-cache lookup to open the bot chat; backlog.
  - Website avatar / bot username subtext (needs users cache for `bot_user_id`); empty-state bold header styling; TGX's "Disconnect All Websites" as a settings row with inline progress instead of a red button — cosmetic divergences, noted.
  - Per-website in-progress spinners (TGX animates a progress view per disconnecting row); Quill disables the buttons via `websites_mutating` instead — same guarantee, less chrome.
## Slice A5 — PROFILE MANAGEMENT: EDIT NAME / BIO, USERNAME CHECK + MULTI-USERNAME LIST, SET / REMOVE PHOTO (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `setName first_name:string last_name:string = Ok;` (:14823).
  - `setBio bio:string = Ok;` (:14826) — "0-getOption(\"bio_length_max\") characters without line feeds".
  - `setUsername username:string = Ok;` (:14830) — "Changes the editable username of the current user"; "Use an empty string to remove the username. The username can't be completely removed if there is another active or disabled username".
  - `toggleUsernameIsActive username:string is_active:Bool = Ok;` (:14835) — "The editable username can't be disabled".
  - `reorderActiveUsernames usernames:vector<string> = Ok;` (:14838) — "All currently active usernames must be specified".
  - `checkChatUsername chat_id:int53 username:string = CheckChatUsernameResult;` (:11677) — documented as the check for "a private chat with self", i.e. the sanctioned self-username check.
  - `checkChatUsernameResultOk / UsernameInvalid / UsernameOccupied / UsernamePurchasable / PublicChatsTooMany / PublicGroupsUnavailable` (:8580–8598) — all six verdicts parsed.
  - `usernames` (:2368–2371): ordered `active_usernames`, `disabled_usernames`, `editable_username` — now retained on `ParsedUser`.
  - `setProfilePhoto photo:InputChatPhoto is_public:Bool = Ok;` (:14803); `inputChatPhotoStatic photo:InputFile` (:1042, only `inputFileLocal`/`inputFileGenerated` allowed); `inputFileLocal path:string = InputFile;` (:325).
  - `deleteProfilePhoto profile_photo_id:int64 = Ok;` (:14806); `chatPhoto id:int64 ...` (:1030) — the id now retained via `UserFullInfo.photo_id`.
  - **Negative-claim discipline:** concept-level search for `check`+`username` in td_api.tl finds only `checkChatUsername` (:11677) and `checkBotUsername` (:15020) — there is no standalone `checkUsername` constructor in the TDLib surface. (Raw MTProto does have `account.checkUsername#2714d86c` per core.telegram.org/schema — irrelevant here because Quill speaks TDLib, and TGX itself uses `CheckChatUsername` for self.)
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `EditUsernameController.checkUsernameInternal` sends `CheckChatUsername(tdlib.selfChatId(), username)` — Quill does the same with the private-chat-with-self id.
  - `AvatarPickerManager` sets the photo via `InputChatPhotoStatic(inputFile)` — Quill mirrors this with `inputFileLocal`.
  - Main profile photo is the private one: Quill sends `is_public = false` (the earlier `true` was wrong; TGX passes a contextual flag and the main avatar is not the public one).
- **Built:**
  - Requests + driver (`telegram/requests.rs`, `connect.rs`): `set_name`, `set_bio`, `set_username`, `check_chat_username`, `reorder_active_usernames`, `toggle_username_is_active`, `set_profile_photo` (static + `inputFileLocal`, `is_public=false`), `delete_profile_photo`. Request-shape tests pin all eight shapes against the schema.
  - State (`state.rs`, `envelope.rs`): `ParsedUser` keeps `active_usernames`/`disabled_usernames`/`editable_username`; `UserFullInfoData.photo_id` retained; `username_check` + `username_check_pending` + `profile_edit_error` session fields; classified async error handling (never raw TDLib text).
  - UI (`ui/mod.rs`): "Edit profile" button on the own info panel; `EditProfileDialog` with Photo / Name / Username / Bio sections in one grouped dialog (TGX has no single edit-profile screen — its editors are separate) — save name, save bio, Check username with all six verdicts, set username (empty clears it per schema), up/down reorder, activate/deactivate (no Deactivate on the editable username per schema), set photo from a local path, remove photo via the retained `chatPhoto.id`.
  - Screenshot fixtures `ready-profile-edit` / `ready-username` (`docs/screenshots/ready-profile-edit.png`, `docs/screenshots/ready-username.png`) — injected user 777 with two active + one disabled username, bio, photo id; the username demo seeds a `checkChatUsernameResultOk` verdict.
- **Key decisions (ponytail):**
  - No local username-format validation: the server verdict (`UsernameInvalid`) covers it; TGX's client-side length/charset rules are left out deliberately.
  - No native file picker: photo upload takes a local path, same as the story composer.
  - No optimistic UI: each section sends one request and reports classified errors inline.
- **Not verifiable without live Telegram:** server acceptance of the sends, the check verdict round-trip, photo upload progress.
- **Out of this slice (left unchecked with evidence):**
  - `parity:auth-qr-authorize-other`, `parity:auth-registration`, `parity:auth-email-login`, `parity:auth-premium-login`, `parity:auth-password-recovery`, `parity:auth-session-toggles`, `parity:auth-web-sessions`, `parity:auth-logout-warning` — other auth boxes, untouched.
  - Local username-format pre-validation (TGX `EditUsernameController` length/charset rules) — server verdict suffices for now.
  - Animated/sticker profile photos (`inputChatPhotoAnimation`, `inputChatPhotoSticker`), public profile photos (`is_public=true` contexts), photo crop/rotate UI.
  - Bio line-feed enforcement client-side (server-enforced "without line feeds").
  - Profile photo history / "suggested photos" from recent pictures.



## Phase 9.5 — Manage posted stories (2026-09-28)

- **Rationale:** Phases 9.1–9.4 made stories viewable, reactable, and
  postable. Posted stories still couldn't be managed: no edit, no cover
  change for videos, no privacy change, no posting as a channel, and no
  repost. This slice wires the management constructors against the
  story's own capability flags (the schema gates edit/privacy on
  `story.can_be_edited` / `story.can_set_privacy_settings` — see below),
  so the viewer only offers actions the server will accept.
- **Schema (1.8.67, verified verbatim in `schema/td_api.tl` — no invented
  constructors/fields):**
  - `getChatsToPostStories = Chats;` (line 13698);
    `canPostStory chat_id:int53 = CanPostStoryResult;` (line 13702).
  - `postStory chat_id:int53 content:InputStoryContent
    areas:inputStoryAreas caption:formattedText
    privacy_settings:StoryPrivacySettings album_ids:vector<int32>
    active_period:int32 from_story_full_id:storyFullId
    is_posted_to_chat_page:Bool protect_content:Bool = Story;` (line
    13715) — the repost mechanism is `from_story_full_id` ("Full
    identifier of the original story, which content was used to create
    the story; pass null if the story isn't repost of another story",
    line 13713; the `storyFullId` type is at line 6766).
  - `editStory story_poster_chat_id:int53 story_id:int32
    content:InputStoryContent areas:inputStoryAreas
    caption:formattedText = Ok;` (line 13732); comments: "Changes
    content and caption of a story. Can be called only if
    story.can_be_edited == true" (line 13726); "@content New content of
    the story; pass null to keep the current content" (line 13729);
    "@areas New clickable rectangle areas to be shown on the story
    media; pass null to keep the current areas. Areas can't be edited if
    story content isn't changed" (line 13730); "@caption New story
    caption; pass null to keep the current caption" (line 13731).
  - `editStoryCover story_poster_chat_id:int53 story_id:int32
    cover_frame_timestamp:double = Ok;` (line 13738); comment: "New
    timestamp of the frame, which will be used as video thumbnail" (line
    13737).
  - `setStoryPrivacySettings story_id:int32
    privacy_settings:StoryPrivacySettings = Ok;` (line 13743); comment:
    "Changes privacy settings of a story. The method can be called only
    for stories posted on behalf of the current user and if
    story.can_set_privacy_settings == true" (lines 13740–13741).
  - Story fields (line 6742) — the management gates are parsed from the
    `story` object itself: `is_edited:Bool`, `can_be_edited:Bool`,
    `can_be_forwarded:Bool`, `can_set_privacy_settings:Bool`,
    `repost_info:storyRepostInfo`, `privacy_settings:StoryPrivacySettings`,
    `areas:vector<storyArea>`, `caption:formattedText` (full line 6742).
  - `storyRepostInfo origin:StoryOrigin is_content_modified:Bool =
    StoryRepostInfo;` (line 6705);
    `storyOriginPublicStory chat_id:int53 story_id:int32 = StoryOrigin;`
    (line 6696); `storyOriginHiddenUser poster_name:string =
    StoryOrigin;` (line 6699);
    `storyInteractionTypeRepost story:story = StoryInteractionType;`
    (line 6795).
  - Area types for edit prefill (same as the 9.4 posting surface):
    `storyArea position:storyAreaPosition type:StoryAreaType = StoryArea;`
    (line 6566);
    `storyAreaTypeSuggestedReaction reaction_type:ReactionType
    total_count:int32 is_dark:Bool is_flipped:Bool = StoryAreaType;`
    (line 6546); `storyAreaTypeLink url:string = StoryAreaType;` (line
    6552). Only link + suggested-reaction areas are prefilled/edited —
    they are the only area types the composer can express with plain
    text inputs (same restriction as 9.4).
  - Admin rights (Telegram X reference, `RightStories` /
    `RightStoriesPost` / `RightStoriesEdit` / `RightStoriesDelete` —
    chat rights `canPostStories` / `canEditStories` /
    `canDeleteStories` map to TDLib `chatPermissions`): Quill's
    existing admin-rights UI already exposes and serializes these three
    story rights; this slice does not duplicate them. Posting/editing
    *as* a channel is additionally gated at request time by
    `canPostStory` for the target chat (line 13702).
- **Parser (`src/telegram/envelope.rs`).** `ParsedStory` gains
  `can_be_edited`, `can_set_privacy_settings`, `can_be_forwarded`,
  `is_edited`, `repost_info: Option<StoryRepostInfoView>`
  (`StoryOriginView::PublicStory { chat_id, story_id }` /
  `HiddenUser { poster_name }`), `privacy_settings` (raw JSON block for
  round-tripping into the composer), `area_link_url` and
  `area_reaction_emojis` (first supported link/reaction areas only —
  location/venue/message/weather/gift areas are intentionally ignored).
  Unit tests: defaults, gates + public origin, hidden-user + unsupported
  origins, link/reaction prefill.
- **Composer (`src/story_composer.rs`).** Gains edit/repost/post-as
  state: `edit_target: Option<(ChatId, i32)>`, `repost_source:
  Option<(i64, i32)>`, `as_chat_id: Option<ChatId>`, `save_sent: bool`
  (the edit→privacy two-step). `open_edit` prefills caption/link/
  reaction from the cached story and leaves the path empty (null content
  → keep content, per the schema); `open_repost` records the source for
  `from_story_full_id`; `StoryPrivacy::from_settings_json` round-trips
  the raw `privacy_settings` block back into the four privacy modes
  (unknown → Everyone). Unit tests: privacy round-trip, mode shapes.
- **Requests (`src/telegram/requests.rs`).** New builders
  `get_chats_to_post_stories`, `edit_story` (null content when the path
  is empty → keep content; areas sent only when replacement content is
  supplied, per "Areas can't be edited if story content isn't changed"),
  `edit_story_cover`, `set_story_privacy_settings`; `post_story` gains
  optional `from_story_full_id` (repost). Shape tests pin all five.
- **Reducer (`src/state.rs`).** New `RequestPurpose::{EditStory,
  EditStoryCover, SetStoryPrivacySettings, GetChatsToPostStories}` and
  `StoryManageState { pending, error }` with a single management op in
  flight; `GetChatsToPostStories` fills `Session::story_post_as_chats`.
  `ok` clears pending; `error` clears pending and surfaces a sanitized
  failure string (never raw JSON). Reducer test: success/failure
  transitions + post-as chats.
- **Driver (`src/connect.rs`).** `check_can_post_story` /
  `post_story` take the target `ChatId`; new `get_chats_to_post_stories`,
  `edit_story`, `edit_story_cover`, `set_story_privacy_settings`.
  Edit/cover/privacy are **gated on the cached authoritative story
  flags** (`can_be_edited`, `can_set_privacy_settings`) and return
  `InvalidRequest` when the story is unknown or the flag is false — the
  UI only shows the buttons when the flags are true, so this is a
  second line of defense, not the primary gate. Mocked-sender driver
  test: gating + request output.
- **UI (`src/ui/mod.rs`).** Viewer gains flag-gated buttons for own
  manageable stories: **Edit** (`can_be_edited`), **Cover** (video +
  `can_be_edited`), **Privacy** (`can_set_privacy_settings`), **Repost**
  (`can_be_forwarded` — forwards via `from_story_full_id`). Edit reuses
  the posting composer (`edit_target` set; title "Edit story"; path
  empty with "Leave empty to keep the current media"); the
  **post-as picker** lists `story_post_as_chats` (from
  `getChatsToPostStories`, refreshed when the composer opens) and the
  privacy selector hides for channel/supergroup destinations (TDLib
  ignores it there). Cover editor is a timestamp (seconds) input for
  video stories; the privacy editor reuses the four privacy modes +
  contact picker and round-trips the story's current settings. Pending
  state and sanitized errors surface in the composer/viewer. Repost
  opens the composer with the source attached; posting it calls
  `postStory` with `from_story_full_id`. The viewer also shows the
  repost origin ("Reposted from …" / hidden-user name) and an
  "edited" marker. Screenshot proof:
  `docs/screenshots/ready-story-edit.png` (`quill --screenshot-demo
  ready-story-edit` — seeded editable own story, composer in edit mode
  with caption + link + reaction prefilled).
- **Not verifiable without live Telegram:** real edit/cover/privacy
  round-trips against server validation (e.g. area edits rejected when
  content isn't changed — we follow the schema but the server is the
  arbiter); real `getChatsToPostStories` channel list; real repost
  rendering on other clients.
- **Out of this slice (→ future):** story albums (`album_ids` fixed
  `[]`); archive UI (`storyListArchive`, line 6690 —
  `getChatArchivedStories`); pinned stories on the chat page
  (`toggleStoryIsPostedToChatPage` for already-posted stories, line
  13749; `setChatPinnedStories`); story notification settings (the
  `chatNotificationSettings` story fields — `mute_stories`,
  `story_sound_id`, `show_story_poster`, lines 3354–3358); restriction
  notices (story restriction reasons on content); live stories
  (`storyInfo.is_live`, line 6772 — posting and viewing); stealth mode
  (`activateStoryStealthMode`, line 13839, `premiumStoryFeatureStealthMode`,
  line 8205, `updateStoryStealthMode`, lines 10917–10919); viewers list
  (`getStoryInteractions`); report story (`reportStory`, line 13835).

## Slice media-shared-gallery — SHARED-MEDIA GALLERY + PER-TAB EMPTY STATES (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim — concept-level across all filter constructors, never a single-name grep):**
  - `searchChatMessages chat_id:int53 topic_id:MessageTopic query:string sender_id:MessageSender from_message_id:int53 offset:int32 limit:int32 filter:SearchMessagesFilter = FoundChatMessages` (:11864). The media filters are all supported here (the unsupported list at :11872 names only mention/unread-mention/unread-reaction/unread-poll-vote/failed-to-send/pinned, and only for `searchMessages`); limit ≤ 100 (:11862).
  - `searchMessagesFilterPhotoAndVideo` (:6299), `searchMessagesFilterDocument` (:6284), `searchMessagesFilterAudio` (:6281), `searchMessagesFilterUrl` (:6302), `searchMessagesFilterVoiceNote` (:6296), `searchMessagesFilterAnimation` (:6278) — all nullary constructors, all present in the pinned schema. The other 12 `searchMessagesFilter*` constructors (:6275–:6326) were read to confirm the tab→constructor mapping is exhaustive for gallery purposes.
  - Pre-existing claim correction: the README's "(partial: `searchMessagesFilter*` schema constructors exist, no UI)" was aspirational — the *schema* constructors existed, but no request code built the filter JSONs and no UI existed. This slice adds both.
- **Reference-client evidence (TGX first, tdesktop secondary):**
  - `~/workspace/telegram-x/app/src/main/java/org/thunderdog/challegram/widget/EmptySmartView.java`: one shared empty view parameterized by mode — `MODE_EMPTY_MEDIA/PHOTO/VIDEO/FILES/MUSIC/LINKS/VOICE/VIDEO_MESSAGES/GIFS`, each with title + description + icon (`setMode(mode, isChannel, arg)`).
  - `~/workspace/telegram-x/app/src/main/res/values/strings.xml`: titles "No media to show" / "No photos to show" / "No videos to show" / "No documents to show" / "No music to show" / "No links to show" / "No voice messages to show" / "No GIFs to show" (:1601–1609); chat descriptions "Share photos and videos in this chat and\naccess them on any of your devices." etc. (:1610–1618); channel descriptions "Published photos and videos\nwill be shown here." etc. (:1619–1627). Quill copies both wordings verbatim, selecting the channel variant when the chat is a broadcast channel — exactly the `isChannel` branch TGX takes.
  - Quill's tab set (Media/Files/Music/Links/Voice/GIFs) matches the README's prescribed tabs; TGX's Photo+Video split is collapsed into one Media tab (`searchMessagesFilterPhotoAndVideo`), which is what tdesktop's shared-media "Media" section does.
- **Built:**
  - State (`src/state.rs`): `SharedMediaTab` (label / filter constructor / glyph / TGX title / TGX hint incl. channel variants), `SharedMediaTabStatus` (`Idle/Loading/Ready/Empty/Failed` — the renderer keys "still loading" vs "empty" vs "failed" off this, never off the item list), `SharedMediaItem` (message id + glyph + label from caption → file name → kind fallback), `SharedMediaState` (open/chat/active-tab/per-tab states/generation; late answers drop on chat/tab/generation mismatch). `RequestPurpose::GetSharedMedia { tab, generation }`; `foundChatMessages` apply arm and the request-error arm route into `accept`/`fail`.
  - Requests (`src/telegram/requests.rs`): `search_chat_messages` takes `filter: Option<Value>` (existing callers pass `None`); `search_messages_filter_json(constructor)` builds the nullary `{"@type": …}` filter.
  - Driver (`src/connect.rs`): `open_shared_media` (open chat + fetch active tab), `select_shared_media_tab` (fetch only never-fetched tabs), `fetch_shared_media` (one page, limit 50, `SHARED_MEDIA_PAGE_SIZE`), `close_shared_media`, `jump_to_shared_media_item` (closes the gallery, reuses the in-chat-search history-around jump pipeline like `jump_to_replied_message`).
  - UI (`src/ui/mod.rs`): "Media" ghost button in the conversation header (live sessions only — demo has no TDLib; the fixture seeds state directly); the gallery side panel (320px, beside the conversation like the downloads panel) with a 6-tab bar (active tab shows `Label · total_count`), per-tab content, and `shared_media_empty_state` — ONE shared empty-state renderer parameterized per tab (glyph + TGX title + TGX description), in the app's muted-centered empty-state style. Failed tabs show "Couldn't load {tab}." + the server error + Retry.
  - Screenshot fixture `quill --screenshot-demo ready-shared-media`: gallery open on chat 11, Media tab injected empty + Files tab two injected documents through the real reducer — `docs/screenshots/ready-shared-media.png`.
  - Tests: `search_chat_messages_filter_carries_tab_constructor` (all six tab→constructor mappings + filter placement in the JSON); fixture asserts Media→Empty / Files→Ready with 2 items through the real apply path.
- **Key decisions (ponytail):**
  - One shared empty-state renderer, not six panels — TGX does the same (`EmptySmartView.setMode`).
  - Each tab caches its first page; switching back never refetches. No pagination: `next_from_message_id` is destructured and dropped in the `foundChatMessages` apply path (it never reaches `SharedMediaState::accept`), and no "load more" is wired — see below.
  - Gallery rows are glyph + text label rows, not thumbnails — full media grids are a separate slice.
  - `searchChatMessages` (not `getChatHistory`): the only per-chat constructor that takes a media filter.
- **Out of this slice (left unchecked with evidence):**
  - Pagination of tab pages (`next_from_message_id` → follow-up `searchChatMessages` with `from_message_id`); the first page is capped at 50.
  - Per-tab counts via `getChatMessageCount` (schema:11967) — the tab bar shows the page's `total_count` only when the tab is Ready.
  - Thumbnails / media grids for the Ready state.
  - The two README boxes stay checked only for what this slice delivers: the gallery UI with tabs, per-tab fetch, and per-tab empty/loading/failed states. Pagination and thumbnails, when built, extend — not reopen — this entry.

## Slice P1 — PAYMENTS: INVOICE RENDERING, BUY BUTTONS, CHECKOUT FLOW, RECEIPTS (2026-09-28)

- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, verified verbatim):**
  - `messageInvoice` (:5270): `product_info currency total_amount start_parameter is_test need_shipping_address receipt_message_id paid_media paid_media_caption`.
  - `messagePaymentSuccessful` (:5436): `invoice_chat_id invoice_message_id currency total_amount subscription_until_date is_recurring is_first_recurring invoice_name`.
  - `messagePaymentSuccessfulBot` (:5449): `currency total_amount subscription_until_date is_recurring is_first_recurring invoice_payload order_info telegram_payment_charge_id provider_payment_charge_id` — minimal buyer-side row only.
  - `paymentForm` (:4734): `id:int64 type:PaymentFormType seller_bot_user_id product_info`.
  - `paymentFormTypeRegular` (:4720): `invoice payment_provider_user_id payment_provider additional_payment_options saved_order_info saved_credentials can_save_credentials need_password` — \"True, if the user will be able to save credentials, if sets up a 2-step verification password\" (:4719), i.e. `need_password` gates *saving* credentials, not paying.
  - `paymentFormTypeStars` (:4723) / `paymentFormTypeStarSubscription` — parsed so the dialog can decline them honestly.
  - `inputCredentialsNew` (:4680): `data allow_save`; `inputCredentialsSaved` (:4677): `saved_credentials_id`.
  - `getPaymentForm` (:15262): `input_invoice:InputInvoice theme:themeParameters` — Quill sends a real `themeParameters` object, not null.
  - `validateOrderInfo` (:15268): `input_invoice order_info allow_save` → `validatedOrderInfo order_info_id shipping_options` (:4737).
  - `sendPaymentForm` (:15277): `input_invoice payment_form_id order_info_id shipping_option_id credentials tip_amount` → `paymentResult success verification_url` (:4740).
  - `getPaymentReceipt` (:15280): `chat_id message_id` → `paymentReceipt` (:4765).
  - `invoice` (:4655): `currency price_parts max_tip_amount suggested_tip_amounts recurring_payment_terms_of_service_url terms_of_service_url is_test need_name need_phone_number need_email_address need_shipping_address send_phone_number_to_provider send_email_address_to_provider is_flexible`.
  - `inlineKeyboardButtonTypeBuy` (:3798) — attached only to `messageInvoice`.
- **Telegram X evidence (local TGX-Android source, `~/workspace/telegram-x`):**
  - `TGMessageInvoice.java` exists but is mostly skeletal (fields parsed, no rich rendering).
  - `TGInlineKeyboard.java` decorates Buy buttons with the invoice currency.
  - The Buy button press handler is still a `TODO` in TGX (~line 1105) — TGX has no Buy-button checkout behavior to copy; Quill's checkout dialog is built from the schema alone.
- **Built:**
  - `telegram/envelope.rs`: `MessageContent::{Invoice, PaymentSuccessful, PaymentReceived}` + `InvoiceContent`, `PaymentSuccessContent`, `PaymentReceivedContent`, `PaymentFormData`/`PaymentFormTypeData::{Regular, Stars, StarSubscription}` (`Regular` payload is `Box<PaymentFormRegular>` — clippy `large_enum_variant`), `InvoiceForm`, `PaymentProviderKind::{Web, Token}`, `PaymentOption`, `SavedCredential`, `OrderInfoData`, `AddressData`, `ShippingOptionData`, `ValidatedOrderInfoData`, `PaymentResultData`, `PaymentReceiptData`; `EnvelopePayload::{PaymentForm, ValidatedOrderInfo, PaymentResult, PaymentReceipt}`; chat-list previews for invoices/payment notices; parser tests.
  - `telegram/requests.rs`: `get_payment_form` (real theme params), `validate_order_info`, `send_payment_form`, `get_payment_receipt`, `input_credentials_new`, `input_credentials_saved`, `order_info_json`; request-shape tests.
  - `connect.rs`: `send_payment_form_request`, `validate_payment_order_info`, `submit_payment_form`, `fetch_payment_receipt` — all keep a `PaymentRequest` (chat_id/message_id) and use the existing callback/message gating.
  - `state.rs`: `RequestPurpose::{GetPaymentForm, ValidateOrderInfo, SendPaymentForm, GetPaymentReceipt}`; session fields `payment_request`, `payment_form`, `payment_form_loading`, `payment_note`, `payment_validated`, `payment_shipping_id`, `payment_receipt`, `payment_receipt_open`, `payment_verification_url`; reducer answers only own requests (matched by `@extra`); reducer unit tests.
  - `ui/mod.rs`: invoice card (title, description, total, TEST badge; paid invoices get \"View receipt\"); `messagePaymentSuccessful`/`messagePaymentSuccessfulBot` compact rows; Buy button wired to `getPaymentForm` (no more \"not supported\" tooltip); checkout dialog — product + price parts, provider section (Web → OS-browser button; Token → provider-token note), additional payment options, order-info fields rendered only for the invoice's `need_*` flags, `validateOrderInfo` → shipping-option radios, saved-credential radios + new-token field (saved only when `can_save_credentials`, 2FA note when `need_password`), terms-of-service consent checkbox + recurring-terms link, explicit Pay button; `paymentResult` verification URL opens in the OS browser; receipt dialog from paid invoices; `paymentFormTypeStars`/`StarSubscription` decline honestly. Screenshot demo `ReadyPayments` (invoice + Buy + success row + paid invoice + open checkout dialog).
- **Key decisions (ponytail):**
  - No in-app web view and no card tokenization: Quill has no embedded browser, so provider/additional-payment/verification URLs open in the OS browser, and card providers take a provider-issued credential token (`inputCredentialsNew`) — never collected or stored by Quill (the token is read from the input and used only for the submission).
  - Prices render with ISO 4217 exponents (`USD 19.99`, `JPY 2000`, `BHD 19.990`) — TDLib amounts are always smallest units, so the exponent comes from a hardcoded currency list in `envelope.rs` (0-decimal and 3-decimal sets verbatim from the standard, 2 otherwise).
  - The dialog has no `validated` flag of its own — `session.payment_validated` is the single source of truth.
  - Tips (`max_tip_amount`/`suggested_tip_amounts`) are parsed but the checkout sends `tip_amount: 0` — tip UI is out of this slice.
  - `payment_note` is the single dialog-scoped status slot (errors and the ✅/❌ result); the verification URL is stored separately (`payment_verification_url`) and drained by `poll_live`, not encoded in a status string.
- **Not verifiable without live Telegram:** server acceptance of `validateOrderInfo`/`sendPaymentForm` round-trips, real provider URLs, receipt fetching against a real paid invoice.
- **Out of this slice (left unchecked with evidence):**
  - `parity:bots-payment-recurring` — recurring metadata/terms render, but recurring-payment management (subscription control) has no verified TDLib path in this slice.
  - `parity:bots-payment-clear` — clear saved payment/shipping info (`deleteSavedCredentials`/`deleteSavedOrderInfo` exist in the schema) not wired to any UI.
  - Telegram Stars checkout (`paymentFormTypeStars` → `sendPaymentForm` with Stars credentials) — no verified credential flow; the dialog declines honestly.
  - Payment tips UI (`tip_amount` sent as 0).
  - Embedded provider webview / native card collection.
  - `paid_media` on invoices (falls back to the invoice card; paid-media rendering is a media-slice concern).

## Slice C2i — 1:1 CALL SCREEN-SHARE SEND (2026-09-28)

- **Task:** README `parity:calls-screen-share` — the 1:1 send path. (The item's
  "receive" parenthetical turned out to be group-only: the native P2P frames
  callback drops `PLAYBACK+SCREEN` frames — `engine.rs` "Screen sharing is a
  later slice" — so 1:1 peer-screen rendering is still open; see "Out of
  this slice".)
- **Schema (pinned TDLib 1.8.67, `schema/td_api.tl`, concept-level):** no
  1:1 screen-share constructor exists — `startGroupCallScreenSharing`
  (:14303) / `endGroupCallScreenSharing` (:14309) are group-call only;
  `screen` in td_api.tl appears only in `groupCallParticipantVideoInfo`
  (:7184). 1:1 screen share is purely a tgcalls/ntgcalls stream-source
  switch, no TDLib traffic — the peer learns it through the tgcalls
  screen video track.
- **ntgcalls findings (v3.0.0 source, primary):**
  - `ntgcalls/include/ntgcalls/instances/call_interface.hpp:58` —
    `set_stream_sources(mode, config)` is the generic C++ entry point
    for P2P and group connections alike (connection keyed by the
    instance's user/chat id); the C API `ntg_set_stream_sources` is
    declared in `ntgcalls/include/ntgcalls/ntgcalls.h`.
  - `ntgcalls/include/ntgcalls/media/media_description.hpp:68` —
    `MediaDescription{ microphone, speaker, camera, screen }`; source kind
    `Desktop = 1 << 4` (:19) — matches the hand-written bindings.
  - `ntgcalls/src/media/stream_manager.cpp:69` — `set_stream_sources`
    throws `InvalidParams("Cannot mix camera and screen sources")` when
    both `camera` and `screen` are set in Capture mode: **screen share
    replaces the camera**; stopping = re-issue with `screen` null.
  - Desktop capture is ntgcalls-internal (libwebrtc capturer): Quill
    carries no Linux/PipeWire capture code — the engine passes the
    `NTG_MEDIA_SOURCE_DESKTOP` description with NULL input (default
    display), same as the group presentation path.
- **Telegram X evidence (`~/workspace/telegram-x`):** TGX has NO 1:1
  screen-share send — concept-level search (`screenshare`, `screenShare`,
  `SCREEN_SHARE`, `ScreenSharing`, `DesktopCapture` in `*.java`/`*.kt`
  and call UI) finds only display-metric `Screen` usages; the 1:1
  `CallController`/`TgCallsController` have no share-screen path. So no
  TGX shape to mirror; the design follows ntgcalls semantics and the
  codebase's own `set_call_camera` toggle contract.
- **Built:**
  - Engine (`src/calls/engine.rs`): `CallEngine::set_screen_share_enabled
    (call_id, enabled)` — re-issues `ntg_set_stream_sources` with the
    desktop description (`screen_video_description()`, shared with the
    group presentation: 1920x1080@15, NULL input). Enabling clears
    `camera_enabled`; `set_camera_enabled` symmetrically clears
    `screen_share_on` (ntgcalls no-mix rule); failed issuance restores
    the retained `CallMediaConfig` (mirrors the camera path).
    `MockEngine` records `p2p_screen_share_changes`; new
    `EngineError::NoScreenSource`.
  - Driver (`src/connect.rs`): `set_call_screen_share(call_id, enabled)`
    — same contract as `set_call_camera` (engine first, error without
    flipping the flag; intent stored pre-transport and applied on
    connect); rejected without an enumerated `MediaDeviceKind::Screen`
    source (`NoScreenSource`). The gate is the existing device cache,
    generalized as `call_screen_source_available()` with
    `group_call_screen_source_available()` delegating to it.
  - State (`src/state.rs`): `ActiveCall.screen_sharing` intent flag.
  - UI (`src/ui/mod.rs`): "Share screen"/"Stop sharing" button next to
    the camera toggle (video calls only; "No screen source available"
    when the engine reports none — mirrors the camera button's honest
    gating); "Sharing your screen" status line on the call card; local
    PiP says "Sharing screen" while sharing instead of "Camera off";
    engine errors surface in the status note without flipping the flag.
    New screenshot demo `ready-call-screenshare`
    (`quill --screenshot-demo ready-call-screenshare` →
    `docs/screenshots/ready-call-screenshare.png`).
  - Tests: engine mock state machine (`mock_p2p_screen_share_toggle_state_machine`);
    driver gating (`set_call_screen_share_gates_engine_on_transport`,
    `set_call_screen_share_rejected_without_screen_source`).
- **Not verifiable without live Telegram:** real `ntg_set_stream_sources`
  desktop issuance, peer-side screen-track rendering, desktop-capturer
  availability on a real display. The vendored `libntgcalls.so` is
  present and loads on this VM — what can't be exercised here is
  end-to-end issuance on real hardware (same caveat as the group
  screen-share slice). Tested instead: toggle state machines, driver
  gating/error contracts, and the synthetic screenshot demo.
- **Honest limitations / out of this slice (box stays UNCHECKED):**
  - 1:1 RECEIVE side is not done: the native P2P frames callback drops
    `PLAYBACK+SCREEN` frames (`engine.rs` "Screen sharing is a later
    slice"), so a peer's screen share in a 1:1 call is not rendered.
    The README item's "detected and flagged" parenthetical was
    group-only; updated to name the 1:1 receive gap explicitly.
  - Voice-call screen share: the button is gated on `call.is_video`
    like the camera button — unverified video-track upgrade path for
    voice calls stays out.
  - Display/window picker: NULL input = default display only (same as
    group presentation); a picker is a separate slice.
- **Follow-up fixes (same day, pre-commit):** driver-level mutual
  exclusion was missing — `set_call_screen_share(true)` now clears
  `call.camera_on` and `set_call_camera(true)` symmetrically clears
  `call.screen_sharing`, so the driver flags can't desync from the
  engine's no-mix rule (the UI camera button is an action label:
  "Camera on" while sharing = switch back to camera, which stops the
  share). The pre-transport intent now genuinely applies on connect:
  `pump_call_engine` forwards `set_screen_share_enabled(call_id, true)`
  after a successful `engine.connect` when `call.screen_sharing` is
  set, mirroring the existing pre-transport mute pattern (non-fatal).
  Screenshot captured (`docs/screenshots/ready-call-screenshare.png`,
  table entry added to `docs/screenshots/README.md`); capture needed
  correct timing only — the demo paints fine on a fresh Xvfb display
  with the lavapipe ICD (earlier black frames were captures taken
  after the demo's 5s quit timer).