## Slice parity/platform-app-icon-badge (2026-09-30)

**Scope:** `parity:platform-app-icon-badge` — unread badge on the app/taskbar icon.

- **Built:**
  - New named module `src/icon_badge.rs` (lib): `sync_icon_badge(Option<&Session>)`,
    called from the same 1s UI-thread timer in `src/main.rs` that drives
    `tray::sync_tray`. Linux-only; a no-op stub on other OSes.
  - Linux mechanism: the Unity `com.canonical.Unity.LauncherEntry` `Update`
    broadcast signal — the de-facto standard Telegram Desktop uses on Linux
    (verified concept-level: the `Update(string app_uri, dict<string,variant>
    {count: int64, count-visible: bool})` protocol is what Ubuntu Dock,
    Dash-to-Dock, and KDE's task manager subscribe to; e.g. gershwin-dock's
    LauncherEntry README documents the same interface/path/member, and
    teams-for-linux ships the same emission). Emitted via `dbus-send`
    (`--session --type=signal / com.canonical.Unity.LauncherEntry.Update
    string:application://quill.desktop dict:string:variant:"count",int64:N,
    "count-visible",boolean:B`) — ships with the base D-Bus install, so no
    new dependency and no hand-rolled D-Bus wire protocol.
  - The count is `tray::badge_count` — the SAME `BadgePrefs`-governed total
    as the system tray badge (muted included by default, archived excluded,
    saturating sum). No second unread definition was built.
  - Emits only on change (`thread_local! Cell<Option<u32>>` last-sent; first
    tick always emits so unread=0 clears a stale badge with
    `count-visible: false`). Any `dbus-send`/session-bus failure is a silent
    no-op (tray module convention).
- **Key decisions (ponytail):**
  - `dbus-send` subprocess over a hand-rolled D-Bus client or a new zbus/dbus
    crate dep: ~15 lines, std only, and the exact command line was
    smoke-tested (`dbus-send` accepted the syntax; it failed only on "no
    session bus", the expected headless-VM path).
  - Reused the existing 1s sync timer instead of a new hook: the badge only
    needs to follow the same aggregated count, and change-gating makes the
    tick free.
  - App URI `application://quill.desktop` hardcoded: the dock resolves the
    badge against the installed desktop file; Quill ships no `.desktop` file
    yet, so this names the id a future packaging slice will install.
- **Not verifiable without a desktop session:** real dock badge rendering
  (headless VM has no session bus / dock — emission no-ops by design).
  Covered by unit tests: signal target/interface/member/app-URI args and the
  `count`/`count-visible` encoding incl. the unread=0 hide case
  (`cargo test --no-default-features`).
- **Out of this slice (left unchecked with evidence):**
  - Windows taskbar / macOS Dock badge APIs — explicitly out of scope for
    this slice (the Linux stub no-ops there).
  - A `quill.desktop` file + install step so docks can resolve the badge —
    no packaging exists in the repo yet; separate slice.
  - Per-chat badges, window-title count fallback — not on the checklist.
  - `parity:platform-os-notifications` (OS notification dispatch) — separate
    slice (PR #239).
