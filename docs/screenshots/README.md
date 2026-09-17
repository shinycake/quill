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
| `fixtures/demo-thumb.png` | Tiny checkerboard PNG used as the completed photo `local.path` in the media demo (not a screenshot). |

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
```

Environment used by the script: `DISPLAY`, `LIBGL_ALWAYS_SOFTWARE=1`, `WGPU_BACKEND=vulkan`, `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`.

VoiceOver was **not** recorded here (no macOS GUI runner). Keyboard shortcuts are listed in the status bar.
