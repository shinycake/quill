# OS deep links and single instance

Branch: `codex/deep-links-single-instance`

## Problem

In-app `t.me` / `tg://` clicks worked (`ui/deep_links.rs`), but the OS could
not hand Quill a link, and launching Quill twice started two processes on one
TDLib database.

## tdesktop references

- `core/sandbox.cpp`: local-socket handshake. A second launch sends its
  command line to the running instance, which activates and handles it.
- `core/local_url_handlers.cpp`: which `tg://` and web links are accepted.
- `Telegram/Telegram.plist`: `CFBundleURLTypes` with schemes `tg` and `tonsite`.
- `platform/win/specific_win.cpp` `psRegisterCustomScheme`: per-user registry
  keys under `HKCU\Software\Classes\tg`.
- Linux packaging: `MimeType=x-scheme-handler/tg;` in the `.desktop` file.

`tonsite` is not registered: Quill does not browse TON sites.

## Design

| Module | Role |
| --- | --- |
| `src/deep_link_inbox.rs` | `sanitize_link` (the only gate), process-wide queue, activation flag, `handle_forwarded_launch` |
| `src/single_instance.rs` | Local-socket election and argv forwarding (`interprocess` crate) |
| `src/scheme_registration.rs` | Windows registry registration; pure entry table + takeover rule |
| `src/main.rs` | Wiring: acquire before any DB/cache work; `on_open_urls`; 250 ms inbox pump |

Links flow: OS callback / argv / socket -> `sanitize_link` -> inbox ->
`install_link_inbox` timer moves one link into `QuillApp::pending_deep_link`
and raises the window -> existing `pump_deep_link` (`getDeepLinkInfo`,
username resolve, invite preview, message links). The pump now takes the next
link only when the previous flow has finished, so a link arriving during an
invite preview waits instead of being dropped.

### Single instance

- Unix: socket `quill.sock` in the app data dir (falls back to a hashed name
  in the temp dir when `sun_path` would overflow), mode `0600`. Windows: named
  pipe `quill-<hash of data dir>.ipc`. Isolated roots (demos, tests) get their
  own endpoint.
- Protocol: `quill-ipc 1` header, one argument per line, empty line, owner
  replies `ok`. Messages are capped (64 KiB, 32 args).
- Election: an `flock` on `quill.sock.lock` serializes connect / unlink-stale /
  bind (Windows relies on the exclusive first pipe instance). A connect
  failure means a crashed owner, so the stale file is unlinked and rebound.
- A connection that opens but never acknowledges within 3 s is a hung owner:
  the new launch refuses to start (protecting the database) and exits 1.
  If the socket cannot be created at all, Quill continues without the guard.
- Only validated links are forwarded. A bare relaunch raises the window; a
  `--start-minimized` relaunch (autostart) stays silent.
- Screenshot demos and CLI flags (`--version`, `--connect-smoke`, ...) return
  before the guard, so they still run beside a live app.

### URL scheme registration

> Superseded: Quill no longer registers itself; see `codex-link-handler-opt-in.md`.

- macOS: `CFBundleURLTypes` (`tg`) in the Info.plist written by
  `scripts/macos-package-smoke.sh`; URLs arrive through GPUI's
  `Application::on_open_urls` (`application:openURLs:`), both on cold launch
  and while running.
- Linux: `assets/quill.desktop` already carries `x-scheme-handler/tg` and
  `Exec=... %u`; `scripts/linux-install.sh` now runs
  `xdg-mime default quill.desktop x-scheme-handler/tg`. URLs arrive as argv
  and go through single-instance forwarding.
- Windows: on startup `HKCU\Software\Classes\tg` is written (default value,
  `URL Protocol`, `DefaultIcon`, `shell\open\command` = `"exe" -- "%1"`). Unlike
  tdesktop it does not overwrite a handler that points at another client
  (anything not containing "quill"), so installing Quill never silently steals
  links from Telegram Desktop. The `--` keeps the link from being parsed as a
  flag. Not verifiable on this macOS host; covered by CI compile and pure
  unit tests of the entry table.

## Security notes

Any web page can fire `tg://` or open a registered handler, so every external
string passes `sanitize_link`:

- accepted: `tg:` / `tg://...` (normalized to `tg://`), and `http(s)` links
  whose host is exactly `t.me`, `telegram.me` or `telegram.dog` with a
  non-empty path;
- rejected: whitespace and control characters, userinfo / port / look-alike
  hosts, other schemes (`file:`, `javascript:`), paths, flags, anything over
  2 KiB;
- the socket owner re-validates every forwarded line (the client is not
  trusted), the queue is bounded (8) and de-duplicated, and the socket is
  owner-only so other local users cannot inject links;
- a link never acts without the existing UI: invites show a confirmation
  preview, bot `start=` only prefills the composer.

## Tests

Unit tests cover sanitizing, the inbox, the wire format, two in-process
endpoints on a temp path (forwarding, eight racing launches electing exactly
one owner, stale socket after a crash), and the Windows entry table.

## Manual check (macOS)

Install the packaged bundle, launch it, then
`open "tg://resolve?domain=telegram"`; the running window raises and resolves
the username. Running `Quill.app/Contents/MacOS/quill` a second time exits
immediately and, with a `tg://` argument, forwards it.

## Link routing fix (live-test finding)

`getDeepLinkInfo` is TDLib's lookup for server-side deep links and answers 404 for `tg://resolve?domain=` and `t.me/<user>`. Like tdesktop's `openLocalUrl`, links are now routed by shape first (`parse_deep_link_url`: `tg://` forms plus `t.me|telegram.me|telegram.dog` usernames, posts, stories, `+hash`, `joinchat`, `c/<id>/<post>`) straight to `searchPublicChat` / `checkChatInviteLink` / `getChat`; only links with no local form still go to `getDeepLinkInfo`. Failures use tdesktop wording ("The username ... is not occupied by anyone.", "This invite link is broken or has expired."); other errors keep the code. Covered by recorded-response tests in `src/connect/tests/deep_link_routing.rs`.
