# Quill feature map for /verify

One file per user-facing feature: how a user reaches it, which demo kind puts
it on screen, and what end state proves it works. The full kind list is
`quill --screenshot-demo no-such-kind /tmp`.

| Feature | File | Main demo kinds |
| --- | --- | --- |
| Video player (media viewer) | [video-player.md](video-player.md) | `ready-video-controls`, `ready-video-speed`, `ready-video-pip` |
| Chat list | [chat-list.md](chat-list.md) | `ready-chats`, `ready-chat-list-search`, `ready-chat-list-3` |
| Chat history and bubbles | [chat-history.md](chat-history.md) | `ready-media`, `ready-video-inline`, `ready-video-note` |

Add a file when you verify a feature that has none. Keep the four sections.
