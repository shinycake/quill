# Quill quality audit — 2026-10-02

This is an ongoing quality pass, not a declaration that the app is ready to ship.
No messages or calls were sent to other people. The official Telegram app was inspected read-only; interactive checks used isolated Quill fixtures.

## Repairs in this pass

- macOS notifications use Quill's native notification identity and click callback, instead of AppleScript. Clicks activate Quill and target the originating chat; account tags prevent routing an old notification into another account.
- User downloads resolve the source message before calling `addFileToDownloads`. Files without a message origin use `downloadFile`; the previous `(0, 0)` origin was invalid.
- Embedded file snapshots no longer overwrite a completed or active download with stale, idle metadata. Explicit `updateFile` invalidation still works.
- Contact profile thumbnails are requested and rendered. Static stickers use the full WebP asset, with an existing thumbnail available while it downloads; animated stickers retain their native decoder. Stickers render without a bubble fill; picker cells expose accessible buttons.
- Context menus own keyboard focus, capture chat shortcuts, trap Tab navigation, and restore the previous focus on dismissal. Escape dismisses them before changing the chat.
- Secret-chat contact pickers and story trays render cached profile photos; contact details show the thumbnail while the large photo downloads.
- Custom overlays occlude underlying mouse hitboxes. Kit dialogs have a deferred barrier below their content to prevent hover and scroll from reaching the app.
- Chat selection uses a colored background and border across the row. Empty chats have a readable preview; the default folder is called “All.”
- Message sender IDs survive parsing, history, and search. Group rows resolve the actual member's name and avatar; private and channel rows omit redundant sender chrome.
- Standalone animated emoji keep readable emoji content. Common group service messages render as centered notices instead of unsupported-message cards.
- Message operations live behind an accessible actions button and context menu. Formatting lives in a menu. Developer-specific placeholder text and several technical labels were removed.
- Calling and answering require an available audio engine. Pending call requests prevent duplicate dialing. Native signaling states are still retained for incoming calls and diagnostics.
- Account and database-key preparation runs on a worker during startup, reconnect, and account switching. Native clients and call engines stay on their owning thread. Connection generations prevent a stale result or poll loop from taking over after switching.
- Redundant authentication labels exposing TDLib and internal state names were removed.
- Autostart tests use unique temporary directories rather than colliding timestamps.

## Evidence

- `cargo test --no-default-features --locked`: 1,382 passing checks, two opt-in checks excluded.
- `cargo build --features ui --locked`: successful.
- `cargo test --features ui --locked --bin quill`: 22 passing UI checks, including context-menu shortcut isolation.
- Opt-in native call-engine checks: passed, using local transports without contacting Telegram or opening capture devices. This does not certify end-to-end audio.
- Native TGS and WebM sticker decoder checks: passed, including animation and transparency. Both formats also render in the running picker and history fixture.
- Computer Use fixture checks: avatar images render, selection is visible, private sender chrome is reduced, message actions and formatting retain their operations, settings backdrop clicks do not select underlying chats, and scrolling does not pass through the modal. Context-menu typing leaves the draft unchanged; Escape restores composer focus; Tab cycles within the menu and Return activates Reply.

## Required follow-up

- Live Quill now opens its window while account preparation waits for macOS Keychain authorization. This was verified with Computer Use. Access to the existing encrypted account still requires authorization; do not bypass it or replace the database.
- Once startup is available, verify live avatar replacement, sticker delivery, video download/retry/cancel, native notification ownership and click routing, and Saved Messages send/edit/media flows.
- End-to-end call audio remains unverified under the instruction not to call anyone. Keep that limitation explicit.
- Continue the broader audit: keyboard and accessibility focus across overlays, RTL layout, message grouping and scrolling, small-window layouts, failures and recovery, remaining technical labels, and the many secondary settings and feature flows. Fixture success alone is insufficient.
