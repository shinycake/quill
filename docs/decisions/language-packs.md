# Interface language packs

Parity item `appearance-localization` (parity-plan bundle 7, "LAST,
alone"). Architecture pass 2026-10-14. The design below is the plan for
the whole migration; the sections at the end say what each PR built and
how it was checked. The owner chose to build this while the video player
overhaul runs, so `src/ui/video_player` and the media viewer are
converted last, after that work merges.

Checked against Quill `a21fa816`, tdesktop `lang/*` and
`boxes/language_box.cpp`, TDLib 1.8.68 (`td_api.tl` lines 7962–8000,
10970, 12904, 15566–15598).

## Where Quill stands

- No localization at all: about 7,500 English literals across 395 files
  in `src/ui` (`rg` count), plus `format!` strings (1,383) and English in
  the lib crate (`src/*.rs` helper texts, service messages, errors).
- `settings::LanguagePrefs` (src/settings/chat_prefs.rs:157) stores a
  `system_language_code` sent once in `setTdlibParameters`; the
  appearance page lists `SUPPORTED_LANGUAGES` tags, but nothing changes
  visibly. Its doc comment says the full translation is "a separate
  project": this is that project.
- `language_link.rs` + `link_info_boxes.rs` already look up a pack from
  `t.me/setlanguage/<id>` (`getLanguagePackInfo`) and show it, but never
  switch.
- Plural helpers are ad hoc (`plural` functions in eight modules),
  English-only.

## What tdesktop does

- Strings are keyed `lng_*` in `Resources/langs/lang.strings`
  (8,780 lines; 1,160 plural forms as `key#zero/#one/#two/#few/#many/
  #other`), with `{tag}` placeholders (`{count}`, `{user}`, `{name}` …)
  and `tr::lng_key(tr::now, lt_count, n)` call sites. Markdown-ish
  `**bold**` in some values, resolved at render time.
- `Lang::Instance` resolves a key: current pack → its
  `base_language_pack_id` pack → the hardcoded English. Plural category
  per language from CLDR rules (`lang_tag.cpp` `GeneratePluralRulesMap`),
  chosen by the pack's `plural_code`.
- `CloudManager` keeps the pack in sync (TDLib does the equivalent:
  `synchronizeLanguagePack`, `updateLanguagePackStrings`).
- Language box (`boxes/language_box.cpp`): title "Language"
  (`lng_languages`), a search field, the list of official packs with
  name and native name in two columns (`lng_sr_languages_column_name` /
  `_native`), the current one checked, a count line "{count} languages",
  empty "No languages found."; beta/unofficial packs from links appear
  with a "Share" / "Delete" menu (`lng_proxy_edit_share`,
  `lng_proxy_menu_delete`), deletion offers "Restore"; a "Translate"
  section: "Show Translate Button" (`lng_translate_settings_show`),
  "Translate entire chats" (`lng_translate_settings_chat`, Premium), "Do
  Not Translate" (`lng_translate_settings_choose`), and on macOS/Linux a
  "Use system translator" toggle (`lng_translate_settings_use_platform_
  mac/_linux` with its about text). Switching a language applies
  immediately, no restart; the app relays out.
- RTL packs (`is_rtl`) mirror the whole layout (`style::RightToLeft`).

## Design

### 1. Keys and English source of truth

`lang/en.strings` in the repo (same Apple-strings format as tdesktop so
the same tooling and the translation platform's exports work):

```
"lng_saved_messages" = "Saved Messages";
"lng_languages_count#one" = "{count} language";
"lng_languages_count#other" = "{count} languages";
"qll_hdr_badge_tooltip" = "Shown in standard range on this display";
```

- A key named `lng_*` is tdesktop's key with the same meaning; Quill's
  English must equal tdesktop's English for that key (the migration tool
  enforces it, see §5). Those keys get every language for free from
  Telegram's translation platform through TDLib, because Quill sets the
  TDLib option `localization_target = "tdesktop"` (writable string
  option) before `setTdlibParameters`, together with
  `language_pack_database_path = <account root>/langpack.sqlite`.
- A key named `qll_*` is Quill-only (text tdesktop does not have). It is
  English unless a `lang/<code>.strings` file in the repo provides it
  (community translations, loaded at startup into the same store as a
  local overlay; the Language box shows "{percent}% translated" for
  Quill-only strings as a secondary line when below 100 %).
- Legal note for Idan: keys are identifiers; the English text in
  `en.strings` is Quill's own (already in Quill's sources). Translated
  strings arrive at runtime from Telegram's servers, the same way every
  third-party TDLib client with `localization_target` set gets them.
  Nothing from tdesktop's GPL repository is copied into Quill.

`build.rs` (lib crate) parses `lang/en.strings` into `OUT_DIR/lang_keys.
rs`: one `pub const` per key in `quill::lang::keys`, named from the key
(`lng_saved_messages` → `SAVED_MESSAGES`, `qll_*` → `Q_*`), typed
`Key` (ordinary) or `PluralKey`, each carrying the key string and the
English value(s). Build fails on duplicate keys, unbalanced braces, or a
plural key missing `#other`. Keys unused in code are reported by a CI
script, not the build.

### 2. Runtime store (`src/lang/`, lib crate; files < 1,000 lines)

- `store.rs`: `Lang` behind a global `ArcSwap<LangSnapshot>` (crate
  `arc-swap`, MIT/Apache; lock-free reads from render code). Snapshot =
  `{ pack_id, base_pack_id, is_rtl, plural: PluralRules, strings:
  HashMap<Arc<str>, Value>, base: HashMap<..>, overlay: HashMap<..> }`
  with `Value::Ordinary(SharedString) | Plural { zero, one, two, few,
  many, other }`.
- `lookup.rs`: resolution order pack → base pack → Quill overlay
  (`lang/<code>.strings`) → English constant. Deleted strings
  (`languagePackStringValueDeleted`) are treated as missing.
- `format.rs`: `{tag}` substitution (`lt_count` formatted with the
  language's digit grouping through the existing number formatter;
  `{tag}` left literally when the caller passed no value, with a
  `debug_assert!` in tests), `**bold**` kept for callers that render
  rich text (`rich::parse` already understands it).
- `plural.rs`: CLDR category selection with `icu_plurals` (ICU4X,
  Unicode-3.0, already allowed by `deny.toml`; `icu_locale_core` is in
  the lock) keyed by `plural_code`; fallback to English rules when the
  code is unknown. Unit tests cover ru (one/few/many), ar (six forms),
  ja (other only), fr (0 and 1 → one), en.
- `api.rs`: `tr(keys::X) -> SharedString`, `trn(keys::X_COUNT, n)`,
  `tr_with(keys::X, &[("user", name)])`, `trn_with(..)`, `is_rtl()`,
  `current_pack()`. No macro: plain functions keep call sites
  greppable and the key constants typed.
- `sync.rs` (state side, settings domain): `SettingsPurpose::Language(
  LanguagePurpose::{TargetInfo, PackInfo, Strings, Synchronize, Switch,
  Delete, AddCustom})`, payloads `LocalizationTargetInfo`,
  `LanguagePackStrings`, `UpdateLanguagePackStrings` (envelope
  `domains/settings/{mod,parse}.rs`; the existing `LanguagePackInfoData`
  stays). Switch flow: `setOption("language_pack_id", id)` →
  `getLanguagePackStrings(id, [])` (all strings) → if the pack has a
  `base_language_pack_id`, the same for the base → build a new snapshot
  → `Lang::swap` → `cx.refresh()` on the app entity through a
  `LangChanged` event that every window observes; then
  `synchronizeLanguagePack(id)`. `updateLanguagePackStrings` patches the
  live snapshot (copy-on-write, one swap). The choice persists in
  `LanguagePrefs` (`language_pack_id` replaces `system_language_code`
  for the UI language; `system_language_code` keeps being sent to TDLib
  from the OS locale, which is what tdesktop does).
- Startup: load prefs; if a pack is chosen, TDLib answers
  `getLanguagePackStrings` from its local database immediately (it is
  cached in `langpack.sqlite`), so the first frame already renders
  translated except for a short English flash; to avoid even that, the
  snapshot built on the previous run is written to `<account root>/
  lang_cache.json` and loaded before the first window.

### 3. Language settings UI (`src/ui/language_ui.rs` +
`language_state.rs`, `register_dialogs!`)

Mirrors tdesktop's box with kit components: `Dialog` titled "Language";
`Input` search (filters by name and native name, case/diacritic-
insensitive); `List` of packs (name left, native name right in
muted text, `IconName::Check` on the current one; `Badge` "beta" for
`is_beta`; unofficial packs added from links get a `PopupMenu` with
"Share" (copies `t.me/setlanguage/<id>`) and "Delete" with the
"Restore" toast); footer "{count} languages" / "No languages found.";
below, the "Translate" section rows reuse Quill's existing translate
settings (`settings_ui.rs` translate toggles) so there is one place for
them. Opens from Settings → Language (replacing the `SUPPORTED_LANGUAGES`
picker in `new_accent_picker.rs`, which is deleted). The language link
box (`link_info_boxes.rs`) gains the real "Change" button
(`addCustomServerLanguagePack` for unofficial packs, then Switch).

### 4. RTL

`is_rtl()` flips text alignment defaults in the few places that align
explicitly (chat list rows, composer placeholder, settings rows) through
one helper `lang::start()`/`end()` returning `TextAlign`; full layout
mirroring (tdesktop's `rtl()`) is out of scope for this item and is
recorded as a new gap `appearance-rtl-layout` through the parity
dashboard mechanism. Hebrew/Arabic packs therefore render right-aligned
translated text in an LTR layout, which is what most non-Qt desktop apps
without mirroring do; the decision doc states it.

### 5. Migration tooling (`quill-tools/lang/`, Python, run by the build)

1. `extract.py`: walks `src/ui` and the lib crate for string literals in
   user-facing positions (`.label(`, `.child(`, `.title(`, `.placeholder(`,
   `.tooltip(`, `SharedString::from(`, `format!(` with a literal first
   argument, `.into()` on a literal in a known text field; a stop-list
   for ids, element ids, keys, URLs, JSON, `log::`/`debug_assert!`,
   `#[cfg(test)]` and `*_tests.rs`). Output: `lang/extract.json`
   (file, line, literal, context).
2. `match.py`: normalizes `{}` / `{0}` / `{name}` placeholders and
   whitespace, matches each literal against tdesktop's `lang.strings`
   values (read from Idan's local reference checkout, never copied) to
   propose `lng_*` keys; unmatched literals get generated `qll_<module>_
   <slug>` keys. Writes `lang/en.strings` (sorted, grouped by module
   comment) and `lang/migration.csv` (literal → key → confidence).
3. `apply.py`: rewrites call sites per the CSV: `"Saved Messages"` →
   `tr(keys::SAVED_MESSAGES)`, `format!("{n} languages")` →
   `trn(keys::LANGUAGES_COUNT, n)`, `format!("Delete \"{name}\"?")` →
   `tr_with(..)`; it only rewrites exact literal matches and leaves a
   `// LANG: review` comment where a placeholder mapping is ambiguous.
   `cargo check` after each module; the builder reviews diffs module by
   module.
4. `scripts/check-lang.sh` (CI): fails when a file listed as migrated
   gains a new user-facing literal (same detector as `extract.py`),
   when `en.strings` has keys unused in code, or when a `lng_*` key's
   English differs from tdesktop's (the comparison runs only where the
   reference checkout exists, i.e. locally and in the owner's CI job).

Migration order (one PR each, merged in sequence, no other UI threads):
(a) infrastructure + settings box + the chat list, composer and settings
screens; (b) chats and history (service messages are in the lib crate:
`service_text/`); (c) everything else; (d) remove the `SUPPORTED_
LANGUAGES` picker, enforce `check-lang.sh` repo-wide, tick
`parity:appearance-localization` in (d) only.

### 6. Numbers, dates, service messages

- Counts use `trn`; the existing eight `plural` helpers are deleted.
- Dates keep Quill's formatter but take month and weekday names from keys
  (`lng_month1`…, `lng_weekday1`…, `lng_month_day`, `lng_month_day_year`),
  as tdesktop does.
- Service messages (`service_text/actions.rs`) switch to keys with
  `{from}`, `{user}`, `{chat}` tags, which is where most of the
  `format!` strings live.

### 7. Tests

- Unit: key generation (build-script parser) on a fixture; lookup
  fallback chain; plural categories; tag substitution; snapshot patching
  by `updateLanguagePackStrings` (ordinary, pluralized, deleted).
- Driver: Switch flow sends option + strings + synchronize in order;
  startup with a saved pack; delete pack restores English; unofficial
  pack via link.
- UI (`#[gpui::test]`): the Language box lists packs from a demo
  `localizationTargetInfo`, search filters, selecting a pack re-renders
  the settings title in the demo pack ("Configurações"); RTL pack flips
  the alignment helper.
- Demo fixture: a `pt-br` pack with ~20 strings and a `he` RTL pack.
- CI: `check-lang.sh` wired into `gate.sh`.

## Build plan and model

Opus 5.5 medium for PR (a) (infrastructure, tooling, settings box,
first screens). Sonnet 5.5 medium for PRs (b) and (c), which are
mechanical runs of `apply.py` plus review, with Opus reviewing each.
PR (d) Sonnet. Each PR: gate, screenshots of the Language box and one
translated screen (pt-br demo pack) in both themes, independent
reviewer, queue.sh.

## Deviations

- No layout mirroring for RTL packs (new gap recorded).
- Quill-only (`qll_*`) strings are English unless the repo carries a
  community `.strings` file; tdesktop has no such class because all its
  strings are on the platform.
- No "Use system translator" row: Quill's translate feature decides that
  separately.

## PR 1: core, Settings → Language, main menu and Settings list

What changed:

- `src/lang/`: keys generated by `build.rs` from `lang/en/*.strings`
  (`lng_*` names and English exactly as tdesktop has them, `qll_*` keys in
  Quill's own words for strings tdesktop has no key for); one immutable
  snapshot behind an `ArcSwapOption`; `tr`, `tr_with`, `trn`, `trn_with`;
  ICU4X plural rules by the pack's `plural_code`; diacritic-folding
  search; the choice, recent packs and last strings saved in the app data
  folder so the first frame is already translated.
- TDLib: `localization_target` = `tdesktop` and a shared
  `language_pack_database_path` before init; switching sends
  `setOption language_pack_id`, `getLanguagePackStrings` for the pack and
  its base, and `synchronizeLanguagePack`; `updateLanguagePackStrings`
  patches the snapshot in place, or reloads when TDLib says everything
  changed.
- Settings → Language (new last row of the Settings list): translate
  options, search, recent unofficial packs with Share / Delete / Restore,
  then the official list. A pick applies at once, no restart.
- The old Appearance "Language" picker (a tag sent to Telegram, applied
  after restart) is gone. That tag (`system_language_code`) now follows
  the chosen pack when Quill knows it.
- The translate options moved from Appearance into the Language box, as
  in tdesktop.
- `t.me/setlanguage/<id>`: tdesktop's "Change language?" box with Apply
  Language; it switches like the Language box does.
- Converted strings: the main menu, the Settings list, the Settings page
  titles, the privacy page links, the translate options and the link box.

Deviations in this PR:

- Labels adopt tdesktop's casing where Quill had its own ("New Group",
  "Chat Settings", "Privacy and Security", "Mark all chats as read").
- Settings pages keep their English id (`settings.page`) and map it to a
  translated heading, so deep links and demos keep working.
- `quill::lang::isolate_thread` gives a test thread its own strings, so a
  UI test that switches language does not change what parallel tests see.
- `check-lang.sh` (no new English literals in converted files) comes with
  the later area PRs, once there is enough converted code to guard.
- The long paragraphs of tdesktop's link box (`lng_language_switch_about_*`)
  are not reused; Quill words its own (`qll_language_link_about`).

## PR 2: review fixes, a simpler string API, the chat list

- Review fixes from PR 1: deleted recent packs stay deleted after the box
  closes; picking the pack in use during a switch cancels that switch; a
  reload of the old pack no longer cancels a switch in flight; changes
  queued for the old pack are dropped when a new pack lands; a deleted
  string reloads the pack so it falls back to the base pack; a failed
  switch points TDLib back at the pack in use; the choice, recent list and
  cache are written in order (a newer write of a file always wins).
- `tr` returns `&'static str` and `tr_with`/`trn` return `String`, the
  types the literals and `format!` calls they replace had. Pack strings
  are interned once per distinct text for the life of the process
  (`src/lang/intern.rs`), bounded by the packs used in one run.
- `trn` takes any integer count.
- Chat list, archive row, chat rows, folders, folder tabs and folder
  sharing are converted (`lang/en/chatlist.strings`). Counts use
  tdesktop's plural keys (`lng_filters_by_link_join`,
  `lng_filters_link_chats`, `lng_filters_chats_count`, …). Demo fixture
  text stays English.
- TDLib reads `language_pack_database_path` when it starts, so on the very
  first launch the option set after `setTdlibParameters` takes effect from
  the second launch; strings come from the server meanwhile.

How verified: gate OK (core 4429, UI 692); driver tests for the switch
races, queued patches, deletes and the failed-switch restore.

## PR 3: the chat view

The composer, the history view, chat bars (join requests, pinned
messages), message menus and actions, selection mode and in-chat search
are converted (`lang/en/chat.strings`). Counts use tdesktop's plural keys
(`lng_group_requests_pending`, `lng_pinned_unpin_all`). Generic single
words are matched to tdesktop keys only when the key means the same thing
on that screen (spoiler → `lng_menu_formatting_spoiler`, poll →
`lng_polls_menu_item`); otherwise they get a `qll_chat_*` key. Gate OK
(core 4444, UI 697).

## PR 4: settings, privacy, security and notifications

Privacy, security (two-step verification, passcode, sessions, connected
websites), notification settings, appearance, proxy, keyboard shortcuts,
account lifecycle, data and storage and the other Settings pages are
converted (`lang/en/prefs.strings`). Durations and counts use tdesktop's
generic plural keys (`lng_hours`, `lng_days`, `lng_months`, `lng_years`,
`lng_filters_chats_count`). The keyboard shortcut rows moved to
`keybindings/shortcut_rows.rs` to keep `keybindings/mod.rs` under 1,000
lines. `REBINDABLE_ACTIONS` is a `const` table, so its labels stay English
for now (a later PR turns them into keys looked up at render time).

## PR 5: groups, profiles and contacts

Group and channel settings, members, admins and moderation, invite links,
boosts, forum topics, profiles (including business details and personal
photos), contacts and Saved Messages sublists are converted
(`lang/en/people.strings`). Boost and message counts use tdesktop's
`lng_boosts_list_title` and `lng_forum_messages` plurals. Keys borrowed from
unrelated screens (payments, group calls, the media viewer) were replaced
with `lng_box_remove`, `lng_report_button` or Quill keys. The forum pane's
topic count and the sticker set removal line are still formatted in
English; a later PR moves them to plural keys.

## PR 6: gifts, Premium, Stars, earn and payments

Gifts (sending, upgrades, resale, auctions, crafting, collections),
Premium and Star purchases, paid messages and reactions, giveaways,
boosts, channel and bot earnings, affiliate programs, the TON wallet,
suggested posts, checkout and statistics are converted
(`lang/en/money.strings`). Boost counts use `lng_boosts_list_title`.
Keys that had been borrowed from unrelated screens (AI compose, bot limits,
the scheduler promo, the exe launcher, sticker copy) were replaced with
`lng_channel_public_link_copied`, `lng_intro_next` or Quill keys.

The conversion tool had skipped every `format!` with positional `{}`
arguments (it dropped the first argument while matching placeholders). It
is fixed. About 25 strings in this area that mix named and positional
arguments or build English plurals by hand ("day{}") are still English.
A plural sweep converts them, together with the same leftovers in the
areas from PRs 2 to 5.

## PR 7: stickers, emoji, stories, bots, mini apps and polls

Sticker and emoji panels and set boxes, the sticker set creator, custom
emoji, stories (viewer, composer, albums, insights, the strip), bots
(creation, editing, verification, permissions, inline mode, attachment
menu bots), mini apps, polls, quizzes and checklists are converted
(`lang/en/media.strings`). `history/session_history_row.rs` is one large
function sitting near the 1,000-line limit, so it stays English until a
later PR splits it.

## PR 8: calls, business, sign-in, sharing and dialogs

Calls (outside group calls), Telegram Business (chat links, quick replies,
hours, location, away and greeting messages, recipients), sign-in, the
share box, export and data storage dialogs, accounts and background
accounts, secret chats, scheduled messages, the admin event log, shared
media lists, sponsored messages and the remaining confirm and create
dialogs are converted (`lang/en/ops.strings`).

## PR 9: appearance, OS menus and the remaining screens

Chat appearance and name colors, themes and the theme editor, wallpaper
preview, power saving, spellcheck and dictionaries, translation, the GIF
and mask panels, the photo editor, send effects, the Dock, window menu,
tray and Touch Bar, onboarding, deep links, jump-to-date, recording, and
the remaining message media labels are converted (`lang/en/misc.strings`).
The Dock's playback items use tdesktop's `lng_mac_menu_player_*` keys.
`main.rs`, audio playback and the HDR decoder are still English.

## PR 10: text with positional arguments in the first areas

The conversion tool had skipped every `format!` with positional `{}`
arguments, so PRs 2 to 5 left those strings in English. Re-running the
fixed tool converts them in the chat list, chat view, settings and people
areas. Several strings still build English plurals by hand ("{n} files",
"{len} messages"). Moving them to plural keys needs a change at each call
site and is the next PR.

## PR 11: plural keys for counted text

Twenty strings that built English plurals by hand ("{n} files",
"{len} members selected") are now Quill plural keys with `#one` and
`#other` forms, called through `trn` and `trn_with`, so each pack applies
its own plural rules. One line counting admins, banned users and pending
requests together keeps a single template, since it holds three counts.

## PR 12: the interface strings check

`scripts/check-lang.py` runs in CI after the file size check. It fails
when a UI file has more hard-coded English than its entry in
`scripts/lang-baseline.txt` (files not listed must have none), and when a
key in `lang/en/*.strings` is not used anywhere in `src/`. The baseline
only shrinks (`--update` lowers entries). It starts at 293 strings, most
of them in `const` tables the conversion tool skips (admin rights, the
Business page list, keyboard shortcut actions). The media viewer, players
and group calls are skipped until the video player overhaul lands.

## PR 13: const tables

Tables of labels the conversion tool skips now hold `Key`s and are
translated where they are shown: admin rights and chat permissions,
keyboard shortcut actions (`RebindableAction::label`), the Business page
list, the archive settings rows, the privacy radio labels, the
auto-download kinds and the passkeys page. The strings check baseline
drops from 293 to 204. `quill-tools/lang/table.py` does the literal
part; call sites take `tr(key)`.

## PR 14: the long tail

The strings left in 89 files after the area PRs are converted
(`lang/en/rest.strings`): gift link and crafting boxes (the tool now
treats `BoxView::new` titles as text), auctions, resale, translation
toasts, language link boxes and others. The baseline drops from 204 to
148. What remains is mostly internal page ids in `navigation.rs` that look
like text, playback files, `history/session_history_row.rs` (needs a
split) and `security/passkeys.rs` (another thread is changing it).

## PR 16: media viewer, players and group calls

Converted after the video player overhaul (PRs 1 to 5) and call capture
merged: the media viewer, the video player controls, inline and audio
playback, the player bar and playlists, the video editor, and group calls
(`lang/en/av.strings`). The strings check no longer skips these paths.
`group_call_panel.rs` and `group_call_window.rs` stay in the baseline
until the live story viewer (#852) merges, since it changes both.

## PR 17: the last group call files

`group_call_panel.rs` and `group_call_window.rs` are converted now that
the live story viewer merged, so the strings check baseline has no group
call entries left. This PR ticks `parity:appearance-localization`. What
stays English is listed in the baseline: page ids in `navigation.rs` that
look like text, and a few strings in files where the text is data rather
than interface copy.

## How verified (PR 1)

- `cargo test --lib`: lang store, plural rules (ru, ar, ja, fr, pt-br),
  formatting, search, prefs round trip, envelope parsing, request
  building, and seven driver tests for the switch flow.
- UI test `language_box_tests.rs`: the Settings row opens the box,
  picking the demo Portuguese pack turns "Settings" into "Configurações",
  missing keys fall back to English, and English comes back.
- Screenshots (`ready-language`, `ready-language-pt` demos, light theme):
  the box in English and after the switch. Dark theme not captured.
- Not verified: a live switch against Telegram's servers (needs a
  signed-in account; the TDLib calls match `td_api.tl` 1.8.68).
