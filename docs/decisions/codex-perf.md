# Render-path performance

Profiled with `sample` on a release build of a new stress fixture, `QUILL_DEMO_STRESS=3000,2000 quill --screenshot-demo ready-chats <dir>` (3,000 chats with avatar files, 2,000 formatted messages in the open chat, 60 Hz re-render loop). The main thread fell from ~50% to ~31% of a core.

- **Filesystem syscalls every frame.** `media_display_roots()` (≈27 calls per render) re-created three frame-cache directories each time, walking every path component with `symlink_metadata` and `chmod`. `sandboxed_display_path` canonicalized each file and every root per call. Directories are now ensured once per process (and again after `sweep_media_caches`). Root canonicalizations are memoized. Per-file sandbox verdicts are memoized for one second, so a symlink swapped in later is still caught on the next check.
- **Cloning every chat per render.** The chat list built `Box<ChatSummary>` clones for all chats each render, and cloned each visible item again. Items now hold the chat id and its declared height; rows look the chat up when rendered. `community_mode::retain_community_chats` is generic over `Borrow<ChatSummary>` to filter borrowed lists.
- **Cloning all files per render.** The history snapshot copied `files`, `downloading` and `failed_downloads`; rows now read them from the session at render time.
- **`getenv` per stamp.** `TMPDIR` is read once, and `localtime_r` (which consults `TZ`) is called once per hour bucket.

Left for later: row inputs are still rebuilt for every loaded message per render (`history_message_list`); caching them needs a history revision counter in the core.
