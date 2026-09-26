# quill

Independent, keyboard-friendly **Telegram desktop client** written in Rust (**GPUI Kit + official TDLib**). Working name **Quill**. Not a ZapFast fork.

Phase 0–1: synthetic GPUI chat, ordered tdjson bridge, replay reducers, live connect (phone / code / 2FA), and after Ready a main chat list + text send. Live Telegram still needs owner credentials + tdjson.

## Build

See [docs/build.md](docs/build.md). Short version:

```bash
cargo test --no-default-features
cargo run --features ui          # synthetic chat; live connect if credentials + tdjson
cargo run --no-default-features -- --connect-smoke   # headless WaitPhoneNumber gate
```

Toolchain: Rust **1.98.1**. UI pin: **gpui-kit 0.6.1**. TDLib schema: **1.8.67** (`d1085f9cebc5a62379991ae1652673954f229c1f`).

## Status

- [x] GPUI Kit hello-world shell + synthetic mixed-height chat / composer
- [x] Official tdjson ordered receive bridge (no raw-response logging)
- [x] Auth / chat / history / send reducers with replay tests
- [x] Live connect gate + phone / code / 2FA submit (owner `api_id` / `api_hash` + tdjson)
- [x] Ready → `loadChats` main list, select chat, composer `sendMessage` (injected/replay; live needs tdjson)
- [x] Unread counts + `viewMessages` mark-read + outbox read receipts (injected/replay; live needs tdjson)
- [x] Photo / document receive + display (`downloadFile` / `updateFile`; injected/replay; live needs tdjson)
- [x] Composer attach + send local photo / document (`inputMessagePhoto` / `inputMessageDocument`; injected/replay; live needs tdjson)
- [x] Global search (`searchRecentlyFoundChats` / `searchChats` / `searchMessages`; sidebar + Cmd/Ctrl+K; injected/replay; live needs tdjson)
- [x] In-chat search (`searchChatMessages`; Cmd/Ctrl+F; next/prev + jump-to-message; injected/replay; live needs tdjson)
- [x] Reply to message (composer quote + `inputMessageReplyToMessage`; quote-strip jump; injected/replay; live needs tdjson)
- [x] Edit / delete own messages (`editMessageText` / `editMessageCaption` / `deleteMessages`; confirm; injected/replay; live needs tdjson)
- [x] Forward message(s) (`forwardMessages`; dest picker + attribution; injected/replay; live needs tdjson)
- [x] Emoji reactions (`addMessageReaction` / `removeMessageReaction`; chips + own highlight; injected/replay; live needs tdjson)
- [x] Stickers (`getInstalledStickerSets` / `getStickerSet` / `inputMessageSticker`; picker + history thumb; injected/replay; live needs tdjson)
- [x] Voice notes (`inputMessageVoiceNote` / `messageVoiceNote`; record bar + history play; injected/replay; live needs tdjson + ffmpeg)
- [x] Link previews (`textEntityTypeUrl` / `textEntityTypeTextUrl` + `messageText.link_preview`; card + OS open; injected/replay; live needs tdjson)
- [ ] VoiceOver pass on macOS
- [x] Channels (ungated; broadcast posts with view counts; join/leave; admin posting) / bots (Phase 3)
- [x] Forum topics: topic list + per-topic history + paging (Phase 5.1); posting into open topics (`sendMessage` / media / poll / sticker / GIF with `topic_id = messageTopicForum`; composer hidden for closed topics and no-post permission) (injected/replay; live needs tdjson)
- [x] Chat folders: tabs + eager `loadChats(chatListFolder)` paging (injected/replay; live needs tdjson)
- [x] Folder management: create / edit / delete / reorder / tags toggle (`createChatFolder` / `editChatFolder` / `deleteChatFolder` / `reorderChatFolders` / `toggleChatFolderTags`); add/remove chats via `getChatListsToAddChat` + `addChatToList` / `getChatFolder` + `editChatFolder`; manage dialog + per-chat Folders picker (injected/replay; live needs tdjson)
- [x] Chat-list avatars (downloaded `chat.photo.small` thumbnails + colored initial fallbacks; `updateChatPhoto` re-arms) and channel/supergroup header (photo, description snippet, @username, subscriber/member count, "Discuss" via `linked_chat_id`) (injected/replay; live needs tdjson)
- [x] Stories: active-story tray + fullscreen viewer (Phase 9.1); reactions (quick-react + `getStoryAvailableReactions` picker), interaction counters, replies (`inputMessageReplyToStory`), own-story delete (`deleteStory`) (Phase 9.2; injected/replay; live needs tdjson)
- [x] Media viewer: in-viewer video playback (ffmpeg frame extraction at 8 fps, 720 px, 600-frame cap; audio-only ffplay; silent fallback) + photo zoom/pan (1×–8×, wheel/drag/double-click/buttons/keyboard) + Left/Right navigation + album-mosaic opening (Parity slice 5; injected/replay; live needs tdjson + ffmpeg)
- [x] Notification sounds + notification settings UI (Parity slice 6): incoming-message sounds (synthesized app-default tone on Linux via ffplay, system beep on macOS; custom saved sounds downloaded via `downloadFile`), per-chat sound picker (Default / None / saved `notificationSound`s with ▶ preview) + message-preview toggle + mute presets in the header Notifications panel, and a scope-defaults dialog (private / groups / channels: mute presets, preview, default sound) — via `setChatNotificationSettings` / `setScopeNotificationSettings` / `getSavedNotificationSounds` / `getScopeNotificationSettings` (injected/replay; live needs tdjson + ffplay)
- [x] Slow mode (Phase A1): `supergroupFullInfo.slow_mode_delay` / `slow_mode_delay_expires_in` + boost counts parsed; `Session::slow_mode_wait_secs` gate (creator/admin/boost bypass, local countdown decay per schema :2759 note, refresh on blocked send); composer banner + blocked composer/GIF/sticker/voice-note/poll/forward sends; admin `setChatSlowModeDelay` control in the group info panel gated on `can_restrict_members` (injected/replay; live needs tdjson)
- [x] Secret chat lifecycle (Phase B1): `use_secret_chats` enabled; **Start secret chat** from user profiles (`createNewSecretChat`); Pending / Ready / Closed state parsing and tracking (`updateSecretChat` / offline `getSecretChat`; 🔒 badge in chat list); composer hidden for non-Ready chats ("Waiting for X to come online…" / "Secret chat closed"); **Close secret chat** with confirm (`closeSecretChat`); ordinary `sendMessage` path reused for Ready chats (injected/replay; live needs tdjson)
- [x] Secret chat key verification UI (Phase B2): **Encryption key** section in the secret chat partner's info panel (header title now opens it) — 12×12 fingerprint grid from `secretChat.key_hash` (36 little-endian bytes → 144 two-bit pixels in FFFFFF / D5E6F3 / 2D5775 / 2F99C9, per schema :2812–2813) + verification copy; shown for Ready chats, "still loading…" when the hash hasn't resolved; key material memory-only (redacted `Debug`, no logging/disk) (injected/replay; live needs tdjson)
- [x] Self-destructing messages (Phase B3): per-media `self_destruct_type` on `inputMessagePhoto` / `inputMessageVideo` — **private (1:1 cloud) chats only**, verified against pinned TDLib `d1085f9` (400 "Messages can self-destruct only in private chats" unless `DialogType::User`; secret chats use a different server-side TTL mechanism, so they are out of this slice); composer ⏱ picker (Off / 5s / 30s / 1m / View once — the server caps the timer at 60s) gated to private chats with photo/video pending, driver strips the choice elsewhere; receive parses `message.self_destruct_type` / `self_destruct_in` with live ⏱ countdown badges and a 1s render tick; rows disappear via `updateDeleteMessages`; plain text can't carry self-destruction (`inputMessageText` has no such field — not faked) (injected/replay; live needs tdjson)
- [ ] Post a story (photo composer / caption / privacy) — **blocked**: pinned TDLib 1.8.67 schema has no `sendStory` constructor (see DECISIONS.md Phase 9.2)

Decisions, pins, and blockers: [DECISIONS.md](DECISIONS.md).  
What credentials are needed next: [docs/credentials.md](docs/credentials.md).  
Phase 0 UI proof (real window, not a generated still): [docs/screenshots](docs/screenshots).
