# Refactor 6: `Session` state by domain

Part 6 of the structure refactor. Pure refactor, no behaviour change. It
does for `Session` what parts 3a to 3c did for `QuillApp`: fields that
belong to one domain move into that domain's struct, and `Session` keeps
one field per domain plus the shared core. Three PRs, one per domain
group, each opened against `main` from the same script.

## Problem

`Session` (`src/state/session.rs`) had 400 mostly `pub` fields, read
directly across `src/ui`, `src/connect` and `src/state`. Any feature that
kept state added a field there and an initial value in `Session::new`
(`session_new.rs`), so those two files were in most PRs and most
conflicts. Refactor 4 split the request purposes, payloads and handlers by
domain and left this as the follow-up (`codex-refactor-4.md`).

## Decision

Each state domain from refactor 4 (`src/state/domains/<domain>/`) gets a
`state.rs` with a `pub struct <Domain>State` holding that domain's fields
and a `new`, re-exported from the domain's `mod.rs`. `Session` holds one
field per domain, declared where the domain's first field used to be, and
`Session::new` builds it with `<Domain>State::new()`.

| PR | `Session` field | Struct | Fields | What it holds |
| --- | --- | --- | ---: | --- |
| 6a | `calls` | `CallsState` | 22 | 1:1 and group calls, recent calls, call privacy, call prefs |
| 6a | `stories` | `StoriesState` | 23 | stories, the tray, albums, archive, posting, viewers, close friends |
| 6a | `payments` | `PaymentsState` | 23 | payment forms and receipts, Stars subscriptions, Premium, gifts |
| 6a | `settings` | `SettingsState` | 62 | notifications, storage, privacy, sessions, websites, account, prefs, proxy |
| 6a | `auth_state` | `AuthState` | 12 | sign-in errors, countries, phone-number change, two-step verification |
| 6b | `groups` | `GroupsState` | 63 | members, admins, rights, invite links, join requests, boosts, communities, event logs |
| 6b | `chats_state` | `ChatsState` | 26 | backgrounds, themes, accents, action bars, deep links, reactions, send-as |
| 6b | `chat_list` | `ChatListState` | 31 | folders, archive, unread counts, limits, suggestions, previews |
| 6b | `threads` | `ThreadsState` | 7 | forum topics, comment threads, Saved Messages |
| 6c | `messages` | `MessagesState` | 46 | message menu, reports, links, AI drafts, limits, link previews, scheduling, forwarding, polls, drafts, sponsored rows, translation, export |
| 6c | `media` | `MediaState` | 17 | files, downloads, the gallery, the media library, map thumbnails |
| 6c | `search` | `SearchDomainState` | 6 | global search, search in chat, date jumps |
| 6c | `stickers` | `StickersState` | 11 | stickers, custom emoji, GIFs, reactions |
| 6c | `bots` | `BotsState` | 16 | bot info, commands, games, inline queries, callback answers, login URLs, mini apps, reply keyboards |
| 6c | `users_state` | `UsersState` | 13 | contacts, profiles, profile photos, secret chats, the info panel |

378 fields move in all. After 6c `Session` keeps 22 shared fields: the
account and authorization state, the connection, the chat map and orders,
histories, users, the open chat and topic, `sync`, `requests`, `revision`,
`view_generation`, `last_seq`, `shutdown`, `my_user_id`, `app_active` and
the diagnostics sink.

Three field names could not follow the pattern: `session.auth` is the TDLib
authorization state, `session.chats` the chat map and `session.users` the
user map, so those domains' state sits at `auth_state`, `chats_state` and
`users_state`. `SearchState` is the global search's own type, so the search
domain's struct is `SearchDomainState`.

Field names: the calls, stories and payments fields lose the prefix their
struct now carries (`call_error` is `calls.error`, `story_post` is
`stories.post`, `payment_form` is `payments.form`). Every other field keeps
its name, so `session.settings.storage_stats` and
`session.groups.admin_lists` read as before with the domain in front.
Types, doc comments, visibility and initial values are unchanged.

## How it was done

A script did the move, and reruns on a newer `main` when a PR conflicts
(`quill-tools/refactor6/`: `move_session.py`, `fix_errors.py`,
`clean_imports.py`, `update_baseline.py`, one spec per PR, `stage.sh` to
run them and gate):

1. It reads the field list per struct, cuts each declaration with its doc
   comment out of `Session` and each initializer out of `Session::new`, and
   writes the domain's `state.rs`. An initializer that reads a local of
   `new` (only the account, the authorization state and the diagnostics
   sink do) stops the script; those fields stay on `Session`.
2. It rewrites every `.old_name` field access under `src/` and `tests/`
   to `.<domain>.<name>`, leaving method calls alone. A rename is skipped
   when some other struct declares both an `old_name` field and a
   `<domain>` field, the one shape a wrong rewrite could compile through.
3. It loops `cargo check --features demo-capture --all-targets` and
   reverts each rewrite the compiler rejects: another type with a field of
   the same name (`entry.proxy` on `ProxyEntry`, `self.stories` on
   `QuillApp`, `tray.stories` on a story view), including the ones rustfmt
   had split over two lines. It applies the accesses the first pass missed
   the same way. Three rounds leave no errors.
4. `cargo fix` and a pass over the unused-import warnings tidy the new
   files, then `cargo fmt`.

The compiler checks the rest: a rewrite that lands on another type's field
does not build, because no type other than `Session` has both the domain
field and the member behind it.

## Side effects

None at runtime: fields drop in a different order when a `Session` drops,
and nothing implements `Drop` on them.

## File sizes

The structure check (`scripts/check-file-size.sh`) counts lines, and a
longer path (`self.session.settings.storage_stats` for
`self.session.storage_stats`) makes rustfmt wrap chains it used to keep on
one line. In 6a, 14 baselined UI files grew that way, by 77 lines in all
(the largest: `proxy.rs` +13, `story_viewer.rs` +11); no code was added to
them. Their baseline entries were raised by exactly that growth in the same
commit. `session.rs` dropped from 1,265 lines to 818 and left the baseline.
`session.rs` and `session_new.rs` change beyond the hotspot allowance, so
each commit carries a `Hotspot-change:` trailer.

## Verification

6a: gate (fmt, core and UI clippy, core and UI tests) core 3040, UI 237,
the same counts as `main`.

## Adding state now

State for a domain goes in `src/state/domains/<domain>/state.rs`: the
field in `<Domain>State` and its initial value in `new`, read as
`session.<domain>.<field>`. A new domain adds a `state.rs`, its `mod` and
`pub use` lines, one field in `Session` and one line in `Session::new`;
the hotspot check allows that much in those two files.
