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
  "Demo polls" chat with an open voted regular poll and a closed quiz
  poll (injected JSON through the real reducer), driven by
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
