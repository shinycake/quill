# Screenshots

Captured from a real GPUI window on 2026-09-17 (Linux Xvfb + lavapipe). **Not** generated stills. The X11 cursor was excluded with `ffmpeg -draw_mouse 0`.

| File | What it shows |
|---|---|
| `synthetic-chat.png` | Mixed-height history: short row, wrapping paragraph, Arabic RTL, Hebrew+emoji, async-loaded image placeholder, empty composer. No live Telegram. |
| `synthetic-chat-composer.png` | Same window after focusing the composer and typing. |
| `synthetic-chat-auth.png` | Auth cycle at `authorizationStateWaitPremiumPurchase`: sidebar shows unsupported halt, no payment UI. |
| `connect-need-tdjson.png` | Credentials-loaded **NeedTdjson** / MissingTdjson surface (no `tdjson` on PATH / bundle). Driven by `quill --screenshot-demo need-tdjson`. |
| `connect-wait-phone.png` | **WaitPhoneNumber** surface with phone entry UI. Synthetic/injected auth state — no live Telegram. Driven by `quill --screenshot-demo wait-phone`. |
| `connect-wait-code.png` | **WaitCode** surface with verification-code field. Injected auth, no live Telegram. Driven by `quill --screenshot-demo wait-code`. |
| `connect-wait-password.png` | **WaitPassword** surface with 2FA field. Injected auth, no live Telegram. Driven by `quill --screenshot-demo wait-password`. |
| `ready-chats.png` | **Ready** main chat list + selected conversation. Injected `updateNewChat` / `updateChatPosition` / history — no live Telegram. Driven by `quill --screenshot-demo ready-chats`. |
| `ready-chats-composer.png` | Same Ready list with composer text (`hello from composer`). Driven by `quill --screenshot-demo ready-chats-composer`. |
| `ready-unread.png` | Unread **badge** on Demo chat A (`unread_count: 3`) while B is selected. Injected updates, no live Telegram. Driven by `quill --screenshot-demo ready-unread`. |
| `ready-unread-read.png` | Same list **after** `updateChatReadInbox` (badge gone) + outbox receipts (`You · read` / `You · sent`). Driven by `quill --screenshot-demo ready-unread-read`. |
| `ready-media.png` | Photo thumb (local path complete), honest **not downloaded** photo placeholder, and a **notes.txt** document chip. Injected `messagePhoto` / `messageDocument` / `file`, no live Telegram. Driven by `quill --screenshot-demo ready-media`. |
| `ready-send-media.png` | Composer **Attach** chip (`demo-notes.txt`) plus outgoing photo thumb and document chip in history. Injected send path; no live Telegram. Driven by `quill --screenshot-demo ready-send-media`. |
| `ready-search.png` | Ready sidebar **Search** (`hello`) with **Chats** + **Messages** hits. Injected `chats` / `foundMessages`, no live Telegram. Driven by `quill --screenshot-demo ready-search`. |
| `ready-search-in-chat.png` | Ready chat open, **Find in chat** (`hello`) with hits + jumped message. Injected `foundChatMessages`, no live Telegram. Driven by `quill --screenshot-demo ready-search-in-chat`. |
| `ready-reply.png` | Ready chat open, history **quote strip** on a reply plus composer **Replying to** preview and typed text. Injected `messageReplyToMessage`, no live Telegram. Driven by `quill --screenshot-demo ready-reply`. |
| `ready-edit-delete.png` | Ready chat open, composer **Editing message** plus **Delete this message?** confirm on an outgoing row. Injected Ready session, no live Telegram. Driven by `quill --screenshot-demo ready-edit-delete`. |
| `ready-forward.png` | Ready chat open, two history rows selected, **Forward to…** dest picker (Demo chat B), and **Forwarded 2 messages to Demo chat B** success. Injected `forwardMessages` / `messageForwardInfo`, no live Telegram. Driven by `quill --screenshot-demo ready-forward`. |
| `ready-reactions.png` | Ready chat open, **React** picker plus chips (`❤ 3` own highlight, `👍 2`). Injected `addMessageReaction` / `updateMessageInteractionInfo`, no live Telegram. Driven by `quill --screenshot-demo ready-reactions`. |
| `ready-pin.png` | Ready chat open, **Pinned message** bar + history **Unpin** affordance on the pinned row. Injected `pinChatMessage` / `updateMessageIsPinned`, no live Telegram. Driven by `quill --screenshot-demo ready-pin`. |
| `ready-mute-archive.png` | Ready list with a **Muted** badge on Demo chat A, an **Archived** section (Demo chat B), header **Unmute** / **Archive**, and the **Mute for** presets (1 hour, 8 hours, 2 days, Forever). Injected `updateChatNotificationSettings` / archive positions, no live Telegram. Driven by `quill --screenshot-demo ready-mute-archive`. |
| `ready-typing.png` | Ready chat open with **typing…** in the header and the sidebar row. Injected `updateChatAction` / `chatActionTyping`, no live Telegram. Driven by `quill --screenshot-demo ready-typing`. |
| `ready-stickers.png` | Ready chat open, **Stickers** panel (installed set + thumb) and a sticker in history. Injected `getInstalledStickerSets` / `getStickerSet` / `messageSticker`, no live Telegram. Driven by `quill --screenshot-demo ready-stickers`. |
| `ready-voice.png` | Ready chat open, **Recording voice** bar (duration, waveform, Cancel, Send) plus history voice notes (incoming **New** / **Play**, outgoing **Pause** while playing). Injected `messageVoiceNote`, no live Telegram. Driven by `quill --screenshot-demo ready-voice`. |
| `ready-link-preview.png` | Ready chat open, a message URL plus a **link preview** card (site name, title, description, photo). Injected `messageText` / `textEntityTypeUrl` / `linkPreview`, no live Telegram. Driven by `quill --screenshot-demo ready-link-preview`. |
| `ready-gifs.png` | Ready chat open, **GIFs** panel (saved animations) and a history GIF (**GIF · playing** / **Pause**, plus a not-yet-downloaded clip). Injected `getSavedAnimations` / `messageAnimation`, no live Telegram. Driven by `quill --screenshot-demo ready-gifs`. |
| `ready-video.png` | Ready private chat open, a history video (**Video · playing** / **Pause**, duration, caption) plus a not-yet-downloaded video. Injected `messageVideo`, no live Telegram. Driven by `quill --screenshot-demo ready-video`. |
| `ready-video-note.png` | Ready private chat open, a round video note (**Video note · playing** / **Pause**, duration) plus a not-yet-downloaded round note. Injected `messageVideoNote`, no live Telegram. Driven by `quill --screenshot-demo ready-video-note`. |
| `ready-audio.png` | Ready private chat open, a music file (**Night Drive** / Ada Lovelace / **Pause**, album cover) plus a not-yet-downloaded track (**Play**). Injected `messageAudio`, no live Telegram. Driven by `quill --screenshot-demo ready-audio`. |
| `ready-video-send.png` | Composer **Attach video** chip (`demo-clip.mp4`) plus an own-sent video (**Video · playing** / **Pause**, caption). Injected send path; no live Telegram. Driven by `quill --screenshot-demo ready-video-send`. |
| `ready-video-note-send.png` | Composer **Video note** chip (`demo-video-note.mp4`) plus an own-sent round note (**Video note · playing** / **Pause**, duration). Injected `messageVideoNote`; no live Telegram. Driven by `quill --screenshot-demo ready-video-note-send`. |
| `ready-drafts.png` | Ready private chat open with a restored composer draft (`meet at 6`) and **Replying to** the message the draft quotes. Sidebar row shows **Draft:**. Injected `draftMessage` / `updateChatDraftMessage`, no live Telegram. Driven by `quill --screenshot-demo ready-drafts`. |
| `ready-albums.png` | Ready private chat with a **received** photo album (mosaic + caption) and an **own-sent** photo/video album. Composer shows a multi-attach album chip (photo + video). Injected `media_album_id`, no live Telegram. Driven by `quill --screenshot-demo ready-albums`. |
| `ready-sponsored.png` | Demo channel showing injected `sponsoredMessages`: one **Sponsored** row (text + sponsor button + Report) and one **Recommended** row (photo). Report opens the `reportSponsoredResultOptionRequired` picker. Since Phase 2.2 channels are ungated; fixture/proof surface only. Driven by `quill --screenshot-demo ready-sponsored`. |
| `ready-channels.png` | **Phase 2.2** Demo channel (chat 13, **Demo channel**) in the chat list; broadcast history shows two channel posts with the channel as author and live view counts (12.4K, 987). Footer shows **Join channel** (member status `Left`) and the composer stays hidden. Driven by `quill --screenshot-demo ready-channels`. |
| `ready-channels-admin.png` | **Phase 2.3** Same demo channel, but the viewer is an administrator (`chatMemberStatusAdministrator` with `rights.can_post_messages: true`): the composer is visible with typed text above the broadcast posts, and the footer reads **Posting as Demo channel.** plus **Leave channel**. Driven by `quill --screenshot-demo ready-channels-admin`. |
| `ready-bot-chat.png` | **Phase 3.1** Private chat with a `userTypeBot` user (chat 21, **Demo Bot**) in the chat list; history renders bot messages; the **bot panel** under the header shows the bot description and tappable command chips (`/start`, `/help`, `/ping`), and the composer is visible. Injected `updateUser` / `updateNewChat` / `userFullInfo`, no live Telegram. Driven by `quill --screenshot-demo ready-bot-chat`. |
| `ready-bot-keyboard.png` | **Phase 3.2** Same bot chat, but the bot message carries a `replyMarkupInlineKeyboard`: a **URL** button row, a **callback + switchInline** row, and a **copy-text + unknown (disabled)** row. Buttons stretch to share each row's width; `buttonStylePrimary/Success/Default` map to primary/success/ghost. Injected `updateNewMessage` with real `reply_markup` JSON, no live Telegram. Driven by `quill --screenshot-demo ready-bot-keyboard`. |
| `fixtures/demo-voice.ogg` | Tiny local file used as the completed voice `local.path` in the voice demo (not a screenshot). |
| `fixtures/demo-video-note.mp4` | Square 240×240 MPEG-4 used as the picked round video for video-note send (not a screenshot). |
| `fixtures/demo-thumb.png` | Tiny checkerboard PNG used as the completed photo `local.path` in the media demo (not a screenshot). |
| `fixtures/demo-notes.txt` | Tiny text file used for outgoing document attach / send demos (not a screenshot). |

Optional recapture of code / 2FA fields (same script now also snaps `wait-code` / `wait-password`):

```bash
# Requires: Xvfb, ffmpeg, mesa-vulkan-drivers (lavapipe), fontconfig
bash scripts/capture-connect-screenshots.sh
# or:
#   cargo run --features ui -- --screenshot-demo need-tdjson docs/screenshots
#   cargo run --features ui -- --screenshot-demo wait-phone docs/screenshots
#   cargo run --features ui -- --screenshot-demo wait-code docs/screenshots
#   cargo run --features ui -- --screenshot-demo wait-password docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-chats docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-chats-composer docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-unread docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-unread-read docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-media docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-send-media docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-search docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-search-in-chat docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-reply docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-edit-delete docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-forward docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-reactions docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-pin docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-mute-archive docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-typing docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-stickers docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-voice docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-link-preview docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-gifs docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-video docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-video-note docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-audio docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-video-send docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-video-note-send docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-drafts docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-albums docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-sponsored docs/screenshots
#   cargo run --features ui -- --screenshot-demo ready-text-entities docs/screenshots
```

Environment used by the script: `DISPLAY`, `LIBGL_ALWAYS_SOFTWARE=1`, `WGPU_BACKEND=vulkan`, `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`.

VoiceOver was **not** recorded here (no macOS GUI runner). Keyboard shortcuts are listed in the status bar.
| `ready-bot-command-menu.png` | **Phase 3.3** Same bot chat with the composer `/` menu open above the composer: bot-specific commands (`/start`, `/help`, `/ping` from `botInfo`) plus a **Global** section (`/settings` from an injected `botCommands` response through the real `getCommands` reducer path), first row highlighted. Injected data, no live Telegram. Driven by `quill --screenshot-demo ready-bot-command-menu`. |
| `ready-text-entities.png` | **Phase 4.1** Dedicated **Demo entities** chat: a `messageText` with mixed nested entities (bold, italic, bold-italic, underline, strikethrough, spoiler chip, inline `code`, URL, `rust` pre block) plus a `messagePhoto` whose caption carries bold + link entities. Injected `updateNewChat` / `updateNewMessage` JSON through the real reducer, no live Telegram. Driven by `quill --screenshot-demo ready-text-entities`. |
