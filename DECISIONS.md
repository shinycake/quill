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
