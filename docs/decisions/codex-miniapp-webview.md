# Mini apps: a web view in a helper process

Telegram bot mini apps (Web Apps) could not open in Quill because GPUI has no web view. They now open in their own window, hosted by a second executable, `quill-webview`, that Quill starts from next to its own. README items: `bots-miniapp-inline`, `bots-allow-write`, `profile-bot-open-app`, `bots-apps-tab`.

## What Telegram Desktop does

From `inline_bots/bot_attach_web_view.cpp`, `ui/chat/attach/attach_bot_webview.cpp`, `ui/chat/attach/attach_bot_webview_linux_shell.cpp` and `Resources/bot_webview_shell_html/`:

- A mini app opens in a separate panel window, 384×694 plus a 60 px header with the bot name, a back button the app can show, a menu (Settings when the app asked for one, Open Bot, Reload Page, Terms of Use, Privacy Policy, Remove from menu) and a close button. Main and secondary buttons sit at the bottom.
- The page talks through `window.Telegram.WebView.postEvent` / `receiveEvent`. tdesktop handles about fifty `web_app_*` commands and answers with the bridge's own events.
- The first time a bot's app opens, a box says "By launching this mini app, you agree to the Terms of Service for Mini Apps" and the answer is remembered per bot (`markPeerTrustedOpenWebView`); verified bots skip it. Adding a bot to the attachment menu asks with an "Allow {bot} to send me messages" checkbox when the bot requests write access. `web_app_request_write_access` calls `bots.canSendMessage` and, on failure, shows "Allow messaging / Do you want to allow this bot to send you messages?" in a popup over the app. Clipboard reads ask too. Closing with `web_app_setup_closing_behavior` set asks "Changes that you made may not be saved. Close anyway".
- On Linux, where tdesktop also runs the web view out of process, the panel chrome is an HTML shell around an `<iframe>` holding the bot page; the shell relays the frame's `postMessage` traffic to the native side.

## One helper process on every platform

Two options were on the table.

1. `wry` inside the GPUI process, attached to a GPUI window as a child view. Works on macOS (WKWebView as an NSView) and Windows (WebView2 on the HWND). Does not work on Linux: WebKitGTK needs a GTK main loop, and the GPUI process has none. tray-icon hit the same wall (see the Linux note in `Cargo.toml`). That would have meant two header implementations and a helper process anyway, for Linux.
2. A helper process on every platform, `quill-webview` (`crates/webview-host`), built on `tao` (windowing; it runs the GTK loop on Linux) and `wry` 0.57 (WKWebView, WebView2, WebKitGTK). Quill talks to it over stdin/stdout, one JSON line per message (`crates/webview-protocol`). The window's chrome is an HTML shell served from a custom scheme; the bot page is a sandboxed `<iframe>` inside it, as in tdesktop's Linux shell.

Option 2 landed. Linux forced it, and it also gives one window, one header and one bridge path on all three platforms. A WebKit or WebView2 renderer bug ends up in a process that holds no TDLib database, no session and no keys; the helper has nothing but a pipe to Quill, and Quill decides everything. Quill's own binary does not link WebKitGTK, so a Linux box without `libwebkit2gtk-4.1` still runs Quill and only mini apps say they need it. The camera scanner (`quill-qr-scanner`) already uses the same helper-process shape on macOS.

The price is two executables to package and a round trip over a pipe for every bridge event, which takes well under a millisecond.

Licences: `wry` is Apache-2.0 OR MIT, `tao` Apache-2.0, `webkit2gtk`/`gtk-rs`/`soup3` MIT, `webview2-com` MIT; `cargo deny check licenses` passes, `THIRD_PARTY_LICENSES.md` regenerated.

## How it works

Quill side (`src/web_app/`, pure; `src/ui/web_app_ui.rs`, the window; `src/connect/web_apps.rs`, TDLib):

1. A launch point calls a TDLib open method with `webAppOpenParameters` carrying the kit theme as `themeParameters`: `openWebApp` (menu button, inline `web_app` button, attachment menu bot), `getWebAppUrl` (custom keyboard button, a "simple" app), `getMainWebApp` (profile Open App, Apps tab, `t.me/bot?startapp`), `getWebAppLinkUrl` after `searchWebApp` (`t.me/bot/app`).
2. The answer (`webAppInfo` / `webAppUrl` / `mainWebApp`) lands in `Session::web_apps.open_result`; the UI polls it and spawns the helper with `--title`, `--data-dir` and `--data-id`.
3. The helper reports `ready`; Quill sends `load` with the URL, title, theme and menu. The shell creates the frame.
4. Every `web_app_*` event the page posts comes back as `{"kind":"web_app","event":…,"data":…}` with the data unparsed. `web_app::bridge::parse_event` validates it (names, sizes, colours, URLs, popup limits) and `handle_web_app_event` acts: bottom and back buttons and colours go back to the shell as commands; links go through `link_policy` to the system browser; `tg:` and `t.me` links go through the deep-link handler; write access goes through `canBotSendMessages` → popup → `allowBotToSendMessages`; `web_app_data_send` becomes `sendWebAppData` (keyboard-button apps only, once) and closes the window; custom methods go through `sendWebAppCustomRequest`.
5. Closing: the X, Escape, `web_app_close` or the "close anyway" popup; Quill sends `close`, the helper exits, Quill calls `closeWebApp` with the launch id.

The shell answers viewport, safe-area and theme requests itself (it knows its size and holds the theme), relays main, secondary, back and settings button presses straight to the frame, and shows the popups it is told to show. Everything else waits for Quill.

Features Quill does not have answer with the bridge's failure events at once (`location_checked {available:false}`, `biometry_info_received {available:false}`, `fullscreen_failed`, `device_storage_failed`, …), so apps do not hang. Payments (`web_app_open_invoice`) answer `invoice_closed` with `cancelled` and a toast: out of scope. Phone sharing answers `cancelled` with a toast. Haptics do nothing.

## Security model

- The bot page is a cross-origin `<iframe>` with `sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals allow-storage-access-by-user-activation"` (no `allow-top-navigation`) and `allow="camera 'none'; microphone 'none'; geolocation 'none'; payment 'none'; display-capture 'none'; usb 'none'"`. It cannot reach the shell's DOM, cannot navigate the window, and gets no device permission. The shell has a CSP that allows only its own script and style.
- The helper refuses every permission request (`with_permission_handler` → Deny), every download, every `window.open` and every navigation that is not `http(s)`, `about:blank`, `blob:`/`data:` or the shell itself. No `file:`. Refused URLs are reported to Quill, which applies the link policy ("Open this link?" for hidden or look-alike addresses) and opens the system browser.
- Only the bridge API is exposed, and only what `parse_event` accepts: unknown events are dropped; oversized payloads (over 64 KiB, `web_app_data_send` over 4096 bytes) are dropped; colours must be `#rrggbb` or a known key; `web_app_open_tg_link` paths must start with `/` and carry no scheme or `@`; popups follow Telegram's limits (title 64, message 256, three buttons).
- Storage is per bot and per account: WebView2 and WebKitGTK get a data directory under the app data root (`webview/<account>/bot-<id>`); WKWebView gets a data-store identifier hashed from the same pair (macOS 14+). The HTTP cache is the engine's own.
- Every grant is a question to the person: the first-open terms box (remembered in `web_app_trust.json`; verified bots skip it, as in tdesktop), the attachment-menu add box with its write-access checkbox, the "Allow messaging" popup, the "Paste from the clipboard?" popup. Camera, microphone and location are not offered at all in this iteration.
- The helper's stderr is discarded unless `QUILL_WEBVIEW_DEBUG` is set; it never logs page content.

## Launch points

- Bot menu button (`botMenuButton.url`) and inline `inlineKeyboardButtonTypeWebApp` (the sender bot's app, `openWebApp` in that chat).
- Custom keyboard `keyboardButtonTypeWebApp` (`getWebAppUrl`; the app may send data back).
- The paperclip menu lists the attachment menu bots (`updateAttachmentMenuBots`) that can open in the current chat, by the bot's `supports_*` flags.
- A bot's profile shows "Open App" when `userTypeBot.has_main_web_app`.
- The global search has an Apps tab (`SearchScope::Apps`): "Apps you use" (attachment menu bots) and "Grossing apps" (`getGrossingWebAppBots`), filtered by the query. A chip reaches it from the empty, focused search. No message search runs for that scope.
- `t.me/bot/app`, `t.me/bot?startapp=` and `t.me/bot?startattach=` links resolve the bot, then search the app or fetch the attachment-menu terms, then open.

## Per platform

- macOS: WKWebView, verified here. The custom scheme is a secure context (the frame reports `isSecureContext` and `crypto.subtle` works). The helper runs as an accessory app, so no second Dock icon.
- Windows: WebView2 through `wry`, custom scheme mapped to `https://quill.localhost`. The Evergreen runtime ships with Windows 10/11; if it is missing the helper fails to create its view and Quill says so. Not run here.
- Linux: WebKitGTK 4.1 through `tao`'s GTK window. `libwebkit2gtk-4.1` and `libsoup-3.0` are optional: the tarball declares no dependency on them (it has no package metadata), `README.txt` lists them under "Optional", and `check-bundle-elf.sh` only allow-lists them as system libraries; a .deb/.rpm built later should use Recommends/Suggests. Without them the helper fails to start and Quill says what to install. Not run here.

## Packaging

`quill-webview` is built with `cargo build --release -p quill-webview` and copied next to `quill`: `Contents/MacOS/quill-webview` (signed with the rest), `quill-linux-x86_64/quill-webview` (RUNPATH `$ORIGIN/lib`), `quill-windows-x86_64/quill-webview.exe` (required by the PE checker). The three package workflows and the self-hosted macOS build got the extra build step; the Linux job installs `libwebkit2gtk-4.1-dev` and `libsoup-3.0-dev`. Quill finds the helper by its own executable's directory, so developer runs work from `target/<profile>`.

## Not done

- Payments and Stars inside mini apps, phone sharing, camera, microphone, location, fullscreen, home-screen shortcuts, device storage, biometry: refused with the bridge's failure events.
- `web_app_switch_inline_query` only for the current chat (empty `chat_types`); a chat picker is not built.
- Side-menu bots (`show_in_side_menu`) have no side menu to live in; they appear in the Apps tab.
- A second mini app replaces the first; tdesktop also keeps one panel.
- Linux and Windows are built by CI, not run here.

## Verification

- Pure tests: bridge parsing and validation (names, limits, colours, links, popups, refusals, replies), theme JSON both ways, trust rules, menu order, launch sources, envelope parsing, request builders, the driver round trips (`openWebApp` → `webAppInfo`, failures, `getWebAppUrl` + `sendWebAppData`, write access via 404 → consent → granted, grossing apps, attachment menu bots, custom requests, named app links), deep-link routing, the protocol's line codec, the helper's argument parsing, navigation policy, IPC limits and shell assets.
- The helper against a local page (served from a temp directory, never committed): the shell loads from `quill://localhost/`, `shell_ready` gates the queued commands, the frame's events reach stdout, the main button, back button, header colour and a popup render, `--capture` writes a PNG of the window on macOS.
- Quill itself in the `ready-mini-app` demo with `QUILL_DEMO_MINIAPP=window` and a local URL: the real spawn, `ready` → `load`, events in, commands out.
- Demo captures: `ready-mini-app` (`terms`, `terms-write`, `add`) for the boxes; the helper's own `--capture` for the window chrome.
- Not touched: real bots, real accounts.
