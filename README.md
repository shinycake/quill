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
- [x] Chat folders: tabs + eager `loadChats(chatListFolder)` paging (injected/replay; live needs tdjson)
- [x] Folder management: create / edit / delete / reorder / tags toggle (`createChatFolder` / `editChatFolder` / `deleteChatFolder` / `reorderChatFolders` / `toggleChatFolderTags`); add/remove chats via `getChatListsToAddChat` + `addChatToList` / `getChatFolder` + `editChatFolder`; manage dialog + per-chat Folders picker (injected/replay; live needs tdjson)
- [x] Chat-list avatars (downloaded `chat.photo.small` thumbnails + colored initial fallbacks; `updateChatPhoto` re-arms) and channel/supergroup header (photo, description snippet, @username, subscriber/member count, "Discuss" via `linked_chat_id`) (injected/replay; live needs tdjson)

Decisions, pins, and blockers: [DECISIONS.md](DECISIONS.md).  
What credentials are needed next: [docs/credentials.md](docs/credentials.md).  
Phase 0 UI proof (real window, not a generated still): [docs/screenshots](docs/screenshots).
