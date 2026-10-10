# Refactor 3c: the rest of QuillApp's feature state

Part 3c of the structure refactor, after 3a (`codex-refactor-3.md`) and 3b
(`codex-refactor-3b.md`). Pure refactor, no behaviour change. Same script
and method as 3b.

## What moved

209 fields moved. 208 went into fourteen new structs; `group_video_images`
joined the existing `GroupCallUi` as `group_call.video_images`.

| `QuillApp` field | Struct | Module | Fields | What it holds |
| --- | --- | --- | --- | --- |
| `history` | `HistoryUi` | `history_state.rs` | 31 | history rows and window, scroll probes, highlights, pinned bar, autoscroll, bot stream reveal, vanishing rows, topic and thread info |
| `chat_list` | `ChatListUi` | `chat_list_state.rs` | 20 | rows, scroll, filter, swipe, menus, pin reordering, preview, contacts and calls tabs, forum column |
| `settings` | `SettingsUi` | `settings_state.rs` | 25 | settings and appearance boxes, shortcuts, storage, proxy, translation, app updates |
| `privacy` | `PrivacyState` | `privacy_state.rs` | 14 | privacy rules and exceptions, blocked users, sessions, device link, websites |
| `account` | `AccountUi` | `account_state.rs` | 6 | accounts, account lifecycle, local passcode, freeze and age checks |
| `auth_ui` | `AuthUi` | `auth_state.rs` | 18 | sign-in fields, registration, terms, recovery, login QR, demo auth state |
| `admin` | `AdminUi` | `admin_state.rs` | 18 | invite links, admins, members, permissions, community, group settings, join requests |
| `payments` | `PaymentUi` | `payments_state.rs` | 6 | payment form and the gift marketplace |
| `notify` | `NotifyUi` | `notify_state.rs` | 16 | OS notification clicks, sounds, mute and auto-delete menus, notification defaults |
| `dialogs` | `DialogUi` | `dialogs_state.rs` | 12 | profile, contact, chat look, export, saved tag and call rating boxes |
| `links` | `LinkUi` | `links_state.rs` | 6 | deep link boxes and bot link confirmations |
| `frame` | `FrameUi` | `frame_state.rs` | 18 | animation demand, frame clock, window title and size, quit guard, capture blocking, context menu focus |
| `demo_ui` | `DemoUi` | `demo_state.rs` | 10 | screenshot-demo devices, call frames and the log sink |
| `connection` | `ConnectionUi` | `connection_state.rs` | 8 | connection status, status line, presence sync |

`QuillApp` went from 235 fields to 40: 29 feature structs and 11 shared
fields (`chat`, `composer`, `menu_bar`, `focus_sidebar`, `live`,
`credentials`, `demo_session`, `slices`, `appearance`, `chat_prefs`,
`pending_deep_link`). `ui/app.rs` is 242 lines, `ui/app_demo.rs` 863.

Renames follow 3b: the prefix the struct carries goes
(`history_rows` is `history.rows`, `settings_open` is `settings.open`,
`connect_status` is `connection.status`, `demo_sink` is `demo_ui.sink`).
Existing structs are nested whole: `proxy_ui` is `settings.proxy`,
`privacy_ui` is `privacy.extra`, `passcode_ui` is `account.passcode`,
`community_ui` is `admin.community`, `global` is `chat_list.global`.

`auth_ui` and `demo_ui` carry `_ui` because `session.auth` and the
`ui::demo` module already use the plain names. `pending_deep_link` stays on
`QuillApp` because `main.rs` sets it.

## How it was done

The 3b script, with a second field list. It reverted 40 rewrites the
compiler rejected (other types' fields with the same names) and left no
errors. No struct outside `QuillApp` declares a field with any of the new
names that also has a matching old field, so a wrong rewrite cannot
compile. Every rewrite in code macOS does not compile is in
`demo-capture` tests, which the script checks with `--features
demo-capture`.

## Side effects

The same as 3b: some text fields are created a little later in
`new_with_demo`, and fields drop in a different order at quit.

## Verification

Gate (fmt, core and UI clippy, core and UI tests): core 3019, UI
234, the same as `main`.

## Adding state now

New state goes in the feature's struct and its `new`, read as
`self.<feature>.<field>`. A feature with no struct yet adds
`ui/<feature>_state.rs`, one `mod` line in `ui/mod.rs`, one field in
`QuillApp` and one line in `new_with_demo`. The hotspot check in the
guardrails PR allows exactly that much in those shared files.
