## Refactor 5b: split long non-UI files (2026-10-10)

- **Scope:** pure refactor, no behaviour change. Every `.rs` file over
  1,000 lines in `src/connect/**`, `src/calls/**`, `crates/**` and the
  top-level `src/*.rs` (except `main.rs`) is now a module directory
  (`foo.rs` → `foo/mod.rs` plus submodules; `ntgcalls-sys` keeps
  `lib.rs` and gains sibling modules). No file in that scope is over
  1,000 lines. One commit per file.
- **Pattern** (same as 5a):
  - `ConnectDriver` files reopen `impl<S: JsonSender> ConnectDriver<S>`
    in each submodule. Submodules start with `use super::*;`, so they see
    the parent's imports and private helpers. Seams were picked so that
    no private method had to be widened: a helper that the parent still
    calls stays in `mod.rs` (`connect_call_transport`,
    `maybe_send_parameters`).
  - Library modules re-export their submodules with `pub use x::*;`, so
    every public path (`quill::settings::load_media_prefs`,
    `quill::rich::parse_page_block`, `ntgcalls_sys::Loader`…) is the
    same.
  - Files whose code fits in 1,000 lines but whose inline test module
    did not now keep the code in `mod.rs` and the tests in `tests.rs`
    (`#[cfg(test)] mod tests;`).
  - Tests sit next to the code they cover: the shared `settings.rs` and
    `rich.rs` test modules were split per submodule.
- **Not split:** `ConnectDriver::ingest` (819 lines, `connect/core/mod.rs`)
  and `service_text::render_action` (723 lines, `service_text/actions.rs`)
  are single functions; splitting them would change code, so they move
  whole.
- **Non-move edits (all needed by the move):**
  - `include_str!`/`include_bytes!` paths in `spellcheck`, `tray` and
    `notify` (Windows-only icon) gain one `../`, since each file is one
    directory deeper. They embed the same assets.
  - `scripts/app-name-allowlist.txt`: the `deep_link_types` entry points
    at `src/deep_link_types/mod.rs`.
  - Two test modules dropped an import they no longer use
    (`std::fs` in `settings/media.rs` tests, `serde_json::json` in
    `rich/input.rs` tests).
  - rustfmt rewrapped a few test lines after the four-space dedent.
- **Check:** each split was compared with the old file line by line
  (indentation ignored) and token by token (formatting ignored). The only
  differences are `mod`/`use`/`pub use` lines, `//!` docs, reopened
  `impl` headers and braces, and the edits listed above. Lines inside
  multi-line string literals keep their indentation. The core lib also
  passes `cargo check` for the Windows and Linux targets.
- **Tests:** counts unchanged, core 3019, UI 234 (after merging
  origin/main with the mini-app work).

### New layout (lines)

`src/connect/groups.rs` (1,854) → `groups/`: `mod.rs` (749) sticker
choices, supergroup profile and full info, channels, statistics, group
info edits, member tags, slow mode; `forum.rs` (424); `community.rs`
(223); `supergroup.rs` (226) signatures, anti-spam, sticker sets;
`boosts.rs` (103); `welcome.rs` (155)

`src/connect/messages.rs` (1,613) → `messages/`: `mod.rs` (482) history
paging, text and rich sends; `export.rs` (131); `ai.rs` (176);
`media_send.rs` (364); `edit.rs` (285); `delete_forward.rs` (202)

`src/connect/group_calls.rs` (1,558) → `group_calls/`: `mod.rs` (226)
transport pump; `join.rs` (497); `controls.rs` (431); `admin.rs` (420)

`src/connect/settings.rs` (1,345) → `settings/`: `mod.rs` (298) privacy,
blocked senders, local prefs saves; `notifications.rs` (403);
`storage.rs` (211); `sessions.rs` (449)

`src/connect/moderation.rs` (1,305) → `moderation/`: `mod.rs` (328)
ownership, permissions, join-by-request, usernames; `invites.rs` (269);
`admins.rs` (359); `members.rs` (365)

`src/connect/search.rs` (1,305) → `search/`: `mod.rs` (498) global
search, recents, top chats; `sponsored.rs` (210); `chat_search.rs` (608)
shared media, in-chat search, calendar, jumps

`src/connect/stories.rs` (1,215) → `stories/`: `mod.rs` (434);
`albums.rs` (296); `posting.rs` (496)

`src/connect/calls.rs` (1,203) → `calls/`: `mod.rs` (647) engine,
devices, frames, engine pump; `lifecycle.rs` (257); `history.rs` (310)

`src/connect/chat_list.rs` (1,036) → `chat_list/`: `mod.rs` (667);
`folders.rs` (375)

`src/connect/core.rs` (1,029) → `core/`: `mod.rs` (878) `ingest`;
`lifecycle.rs` (158)

`src/connect/tests/`:
- `message_ops.rs` (1,895) → `mod.rs` (624), `group_admin.rs` (714),
  `edits.rs` (564)
- `connect_flow.rs` (1,534) → `mod.rs` (474), `auth.rs` (646),
  `sending.rs` (421)
- `chat_list.rs` (1,494) → `mod.rs` (501), `contacts_bots.rs` (710),
  `open_close.rs` (290)
- `chat_state.rs` (1,225) → `mod.rs` (411), `list_ops.rs` (554),
  `export.rs` (267)
- `messaging.rs` (1,128) → `mod.rs` (470), `downloads.rs` (333),
  `sends.rs` (332)
- `group_calls.rs` (1,040) → `mod.rs` (565), `transport.rs` (479)
- `calls.rs` (1,029) → `mod.rs` (693), `media.rs` (340)

`src/settings.rs` (1,887) → `settings/`: `mod.rs` (330) core prefs,
JSON IO, window/call/proxy prefs; `media.rs` (241); `appearance.rs`
(448); `chat_prefs.rs` (304); `accounts.rs` (496); `badge.rs` (112)

`src/rich.rs` (1,616) → `rich/`: `mod.rs` (129) `RichBlock`; `parse.rs`
(563); `input.rs` (445); `markup.rs` (503)

`src/service_text.rs` (1,435) → `service_text/`: `mod.rs` (554);
`actions.rs` (886) `render_action` plus gift/giveaway helpers

`src/service_text_tests.rs` (1,044) → `service_text_tests/`: `mod.rs`
(273) fixtures, `wording!`, behaviour tests; `chats.rs` (309);
`payments.rs` (201); `content.rs` (273)

`src/calls/engine/ntgcalls.rs` (1,370) → `ntgcalls/`: `mod.rs` (570);
`call_engine.rs` (717) the `CallEngine` impl; `native_tests.rs` (86)

`crates/ntgcalls-sys/src/lib.rs` (1,540) → `lib.rs` (86), `ffi.rs`
(652), `loader.rs` (811)

Code plus `tests.rs`:

| file | before | `mod.rs` | `tests.rs` |
|---|---|---|---|
| `passcode` | 1,214 | 914 | 299 |
| `spellcheck` | 1,198 | 918 | 279 |
| `video` | 1,196 | 868 | 326 |
| `calls/engine/mock` | 1,168 | 629 | 538 |
| `tray` | 1,162 | 770 | 391 |
| `premium_hub` | 1,140 | 922 | 217 |
| `deep_link_types` | 1,099 | 616 | 429 (+ `web_app_link_tests.rs` 46) |
| `translate` | 1,073 | 846 | 225 |
| `composer_doc` | 1,046 | 773 | 270 |
| `emoji` | 1,021 | 553 | 441 (+ `keyword_language_tests.rs` 25) |
| `notify` | 1,013 | 533 | 476 |
| `text` | 1,001 | 510 | 490 |
