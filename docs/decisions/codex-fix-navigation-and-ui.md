# Navigation, secure passwords and scrollable dialogs

The Ready sidebar contains chat navigation, search, stories and chat rows. Saved Messages, creation actions, downloads and Settings live in the main menu. Settings groups account, appearance, privacy, device, storage, notification, contact and call preferences. Archived stickers live under Chat settings; collectible gifts live in the selected chat's action menu. macOS exposes Settings in the app menu and Cmd+,.

Password entry uses masked single-line InputState and native Password content semantics for sign-in, two-step verification management and bot callback verification. Backend submission and password clearing remain in their existing flows.

Internal status updates no longer create toast notifications or repeat chat-loading status on every incoming update. Feedback occupies one status line; incoming-message and call notifications retain their existing delivery rules.

All 44 kit dialog content builders share a window-sized scroll wrapper. Dialog width and height are bounded by the viewport; headers and footers remain outside the scrolling body. Privacy and group-call overlays use relative bounds, contacts/call lists scroll, and message/composer action rows wrap. Screenshot demos bypass live media-cache cleanup so isolated checks can coexist with a signed-in process.

Validation: formatting and core clippy passed; 1,268 core library tests plus integration tests passed (one preexisting ignored test). UI debug and release builds passed. `scripts/macos-navigation-smoke.sh` exercises native menu activation, settings destinations, visible chat rows, short-window dialog scrolling/closing, and protected password editing. Screenshots verify password bullets and reachable final shortcut controls. The existing signed-in application was left running throughout verification.

This fixes existing UI behavior and declares no additional parity items.
