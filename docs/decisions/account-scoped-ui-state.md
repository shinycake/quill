# Account-scoped UI state: one value for the account on screen

Design for the structural fix behind the five adversarial review rounds on
`claude/project-thread-npefbc` (scratch `sm-audit-review/claude-review-3..5.md`,
`grok-review*.md`). Read-only study; nothing here is implemented yet. Branch
state read: `account_surfaces.rs`, `shell.rs` at the round-5 fixes.

## 1. The problem

`QuillApp` holds the UI state of the account on screen next to the UI state
of the app. A switch (`QuillApp::switch_account`, `src/ui/accounts.rs:104-207`)
has to empty the first set by hand before `retire_live_to_background` swaps
`self.live` (`src/ui/background_accounts/switch.rs:14-47`). The hand list is
`end_account_surfaces` (`src/ui/account_surfaces.rs:24-59`): 20 explicit
field clears plus `close_dialogs_for_switch` (`src/ui/shell.rs:388-412`),
which runs the `close` of every dialog registered as `OnSwitch::Closes`, and
a second ad-hoc list of input resets inside `switch_account` itself
(`accounts.rs:140-178`). Each review round found more fields the lists did
not know about: the mini app (round 3), 78 dialog flags (round 4), the story
composer, four context menus, the photo editor, the chat-list selection, the
player bar, the chat-window save list, story-reply recordings (round 5). The
`DialogUnscoped` typestate (`shell.rs:146-205`) forces every *dialog* to say
what a switch does to it, but a dialog is one of five kinds of surface
(dialog, escape-stack layer, context menu, helper window, async
continuation), and the other four have no such gate. Round-5 finding 2b
shows the dialog gate itself leaks: a `Closes` close runs only while
`is_open` is true, and compound gates (`giveaway.form.is_some() &&
!payment_dialog_visible()`) are false at switch time.

The list can never be complete because the invariant it protects ("every
account-bound value is gone when `live` changes") is not expressed anywhere
in the types. tdesktop does not have this problem: `Window::Controller`
owns one `std::unique_ptr<SessionController>` and replaces it in
`showAccount` (`window/window_controller.cpp:175-181`); the session
controller owns every per-session widget, layer and lifetime, and
`~Controller` deletes the widgets before the controller
(`window_controller.cpp:120-126`). Nothing has to be listed; it is owned.

## 2. Inventory

### 2.1 How state is split today

| Owner | What | Scope today |
| --- | --- | --- |
| `Session` (`src/state/session.rs:34-`) | the reducer, keyed `account: AccountKey`, `account_generation` | per account, by construction; moves with `LiveConnect` |
| `LiveConnect` (`self.live`) | driver + session + bridge | per account; swapped by the switch (5 writers: `connect_ui.rs:252`, `switch.rs:16,97`, `closing.rs:93,164`; 1,245 readers in `src/ui`) |
| `BackgroundAccounts` (`src/ui/background_accounts/mod.rs:80-125`) | `Vec<BackgroundClient { key, live, status, unread }>`, starting/closing/deleting sets, click queues | app-wide registry; each client's `Session` is per account |
| `WindowSlot` (`src/ui/chat_window/slot.rs:40-62`) | one chat window's `history`, `composer`, `composer_ui`, `message_ui`, `pickers`, `viewer`, `share`, chat search field, slices | per chat window; swapped into the `QuillApp` fields while that window draws or handles input |
| `ComposerSends` (`src/ui/composer_sends.rs:109-148`) | the attachment tray and send options, `HashMap<AccountKey, AccountSends>` | per account by key; survives in the background |
| `NotifyUi::reply_targets`, `preview_waits` (`notify_state.rs:24-30`) | `HashMap<(AccountKey, ChatId), _>` | per account by key |
| `QuillApp` (`src/ui/app.rs:16-162`) | 69 fields, 813 nested fields across 58 feature structs | mixed; the subject of this doc |

The core already has the pattern the UI lacks: every `PendingRequest`
carries `account_generation` (`src/state/requests.rs:456`) and
`Session::apply` drops an answer whose generation is stale
(`src/state/session_apply.rs:26`); every request also records the window
view it was made for (`PendingRequest::view_generation`, `WindowViews`,
`src/state/session_window_chats.rs:87-112`). The UI side got the window half
of this in `sweep-window-slot-scope.md` (`ScopedApp`, `spawn_scoped`,
`in_window_scope`, `scripts/check-window-scope.py`) but not the account half.

### 2.2 The 69 `QuillApp` fields, by bucket

Counting the field list in `app.rs` (a scratch script parsed every owned
struct; 813 nested fields). "Screen" means it belongs to the account on
screen and must not survive a switch. "App" means it belongs to the
process. "Split" means one struct holds both kinds.

**Screen (41 fields, ~470 nested):** `chat_list` (26), `shared_media_grid`
(4), `stories` (41), `premium_preview` (2), `history` (54), `composer`,
`composer_ui` (47), `pickers` (13), `sticker_box` (12), `payments` (29),
`earn` (17), `common_groups` (1), `auth_ui` (18; the sign-in inputs are
already reset by hand in `switch_account`), `search_ui` (9), `location_ui`
(10), `media_search` (3), `share` (13), `ai_compose` (8), `message_ui` (33),
`dialogs` (32), `privacy` (16), `twofa` (9), `screen_share` (12), `recording`
(10), `admin` (26), `inline_members` (5), `links` (7), `giveaway` (1),
`paid_messages` (5), `direct_messages` (1), `suggested_posts` (5),
`gift_market` (10), `paid_reaction` (1), `mini_apps` (9), `viewer` (30),
`folders` (14), `peer_qr` (5), `video_avatar` (2), `profile_cover` (3),
`demo_session`.

**App (19 fields, ~200 nested):** `chat` (synthetic), `menu_bar`, `frame`
(34, frame clock and window bookkeeping), `composer_sends` (keyed by
account), `update_pill`, `takeover`, `focus_sidebar`, `disk` (keyed by
`on_screen`), `live`, `credentials`, `demo_ui`, `background`, `account`
(accounts list, lifecycle, passcode), `spell`, `slices` (cached child
views), `appearance`, `chat_prefs`, `language`, `start_files`, `first_run`,
`pending_deep_link`.

**Split (9 fields, ~140 nested), to be cut into two structs each:**

| Struct | Screen part | App part |
| --- | --- | --- |
| `SettingsUi` (32) | `open`, `page`, `sub`, `log_out_confirm`, `search`, `arrival`, scrolls, `storage_*`, `data_storage_editor`, `proxy` box state, `phone_change_note`, `sticker_settings_open` (tdesktop closes Settings on switch: `ui_hideSettingsAndLayer`) | `app_theme`, `accent_picker`, `system_accent*`, `index_sliders`, `keybinding*`, `update_state`, `update_banner_dismissed`, `whats_new`, `advanced`, `emoji_style`, `translate` |
| `NotifyUi` (19) | `mute_menu_open`, `mute_chat`, `mute_custom_open`, `mute_time`, `ttl_picker_open`, sound pickers, `tone_upload_target`, `defaults_*`, `notifications_confirm`, sliders | `notify_clicks`, `notify_inflight`, `reply_targets`, `preview_waits` (keyed), `notification_sounds`, `polled_redraw`, `windows` |
| `CallUi` (22) | players, `window*`, `ended_at`, `confirm`, `dialing`, `history_*`, `mic_test*`, `add_people`, `conference_link` | `notify_clicks`, `notified` (keyed), `tick_active`, `panel_view`, `sounds`, `sound_marks` |
| `PlaybackUi` (30) | `player`, `playing_*`, `intent`, `playlist_*`, `track_length`, seek fields, `tick*`, `positions`, `playing_animation`, `autoplayed_gifs`, `gif*`, `gif_decodes`, `pending_media_seek`, `error` | `audio` (engine), `stickers` (shared player), `emoji_plays`, `inline_videos`, `userpic_videos`, `speed`, `volume`, `unmuted_volume` |
| `GroupCallUi` (31) | dialogs, `composer`, `window*`, `chat_shown`, `pin`, levels, scrolls, `stream`, `rtmp_start`, `live_pip` | `ptt`, `ptt_clock`, `ptt_capture`, `global_ptt*` |
| `AgentUi` (5) | `picked`, `consent`, `page` | `activity_shown`, `ui_send` |
| `ConnectionUi` (9) | `status`, `generation`, `closed_noted_generation`, `note`, `note_window`, `status_traced`, `presence` | `toasts`, `toast_wake` |
| `ChatWindows` (12) | `open`, `scope`, `last_input`, `dialogs` (the owner of each kit dialog), `closed`, `restore`, `save_pending` | `main`, `main_place`, `cascade`, `main_dialogs_closed`, `_key_scope` |
| `AuthUi` (18) | everything else | `demo_inputs` (synthetic mode) |

Dialog registry (`register_dialogs!`): 189 entries; 163 `.closes(..)`, 24
`.follows_account()`, 5 `.app_wide*()`. Async starts in non-test `src/ui`:
295; 108 through `spawn_scoped*` / `spawn_main*`, 120 plain `cx.spawn` /
`spawn_in`, 11 `// window-scope:` markers, the rest `background_spawn` or
`window.spawn`. None of the 295 carries the account.

## 3. Approaches

### A. One `OnScreen` struct, replaced on switch (chosen, see 4)

Move every Screen field into `pub(super) screen: OnScreen` on `QuillApp`.
The switch does `self.screen = OnScreen::new(epoch + 1, window, cx)`. The
struct *is* the list; a field cannot be forgotten because there is no
second place to put it. tdesktop's `SessionController`, in a struct.

### B. A GPUI entity per account (`Entity<AccountView>`)

Same split, but the Screen side becomes its own entity that the app entity
owns and replaces. Pro: detached tasks holding `WeakEntity<AccountView>`
fail to update once the entity is dropped, so continuations are scoped for
free. Con: every method that touches both halves (hundreds: `self.frame`,
`self.appearance`, `self.connection`, `self.background` are read from
account code everywhere) has to lease the other entity, and a lease cannot
be nested into the same entity. The chat windows' `ChatWindowView`, the
mini-app helper, the call window and the notification windows all hold
`WeakEntity<QuillApp>`; each would need re-pointing. GPUI cached views
(`entity.cached`) only redraw on their own notify, so a second entity also
changes dirty tracking (`quill-architecture` memory: deferred for that
reason). B is A plus a second lease boundary; the boundary buys nothing
that A's epoch does not.

### C. Generation tokens on every value (`Scoped<T>`)

Wrap each account-bound value as `Scoped<T> { account: AccountKey, value }`
and read it through `.for_screen()`. Per-field opt-in: a field declared
without the wrapper is the same hole as today. Doubles the reads
(`self.stories.composer.get(&key)`) at ~8,000 sites. Right for async
continuations (where there is no owner to drop) and wrong for owned state
(where dropping the owner is simpler). Kept for continuations only, as the
epoch in 4.3.

### D. Key every store by `AccountKey`

What `ComposerSends`, `reply_targets`, `preview_waits` and the saved chat
windows file already do. Right for state that must survive in the
background or on disk. Wrong as the general rule: a dialog flag keyed by
account still *exists* after a switch, so `is_open` has to check the key
every time, and a value that nobody looks up again leaks until the account
is removed. D stays for the four stores that need it.

### E. Keep the lists, add a test that every Screen flag is clear after a switch

The round-4 suggestion. A test can only check fields someone enumerated;
the enumeration is the list again.

### Comparison

| | A struct | B entity | C `Scoped<T>` | D keyed | E lists + test |
| --- | --- | --- | --- | --- | --- |
| Correctness guarantee | all Screen state dropped as one value; compile-time by placement | same, plus weak-handle failure for tasks | per field, only where wrapped | state stays; every read must filter | none beyond the list |
| New field forced into a bucket | yes: declared in `OnScreen` or in `QuillApp`; app-wide is the exception and CI allowlists it (4.6) | yes, same | no | no | no |
| Async continuations | epoch on `ScopedApp` (4.3), one helper | free via `WeakEntity` | the whole point | no | no |
| Migration | 41 fields move; `Deref` keeps ~8,000 reads compiling; 9 structs split | 41 fields move and ~1,500 cross-reads become leases; window views re-pointed | ~8,000 reads rewritten | few | small |
| Perf | one struct build per switch (12 text-field entities, a few scroll handles); zero per frame | same plus an entity | a key compare per read | a hash per read | none |
| Conflict risk | `app.rs` once (hotspot trailer); `on_screen.rs` becomes the one-field-per-PR hub that `app.rs` is now | same plus every window view file | every feature file | every store | `account_surfaces.rs` forever |
| Background accounts | nothing of the screen survives (as tdesktop); what must survive uses D | same | n/a | native | n/a |
| Chat windows | `WindowSlot` is a sub-scope of `OnScreen`; `ScopedApp` carries both | each window's slot moves with the entity | n/a | n/a | n/a |
| GPUI borrows | one entity, one lease; `DerefMut` to `OnScreen` means `self.chat_list` borrows all of `self`, so a disjoint `&mut self.frame` in the same expression needs `self.screen.chat_list` spelled out (4.2) | two leases, no nesting | none | none | none |

## 4. The design

### 4.1 `OnScreen`

```rust
// src/ui/on_screen.rs
/// The UI of the account on screen. One value: a switch replaces it whole,
/// so nothing it holds can act for the account switched to. Fields are the
/// feature structs (`ui/<feature>_state.rs`); a new feature struct goes
/// here unless it is the app's (`scripts/app-wide-state.txt`).
pub(crate) struct OnScreen {
    /// Changes with every replacement; work started under an older epoch
    /// does not run (`ScopedApp`).
    pub(super) epoch: ScreenEpoch,
    pub(super) chat_list: ChatListUi,
    pub(super) stories: StoryUi,
    pub(super) history: HistoryUi,
    pub(super) composer: Entity<TextareaState>,
    pub(super) composer_ui: Box<ComposerUi>,
    // ... the 41 Screen fields of 2.2, plus the Screen halves of the 9 splits
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ScreenEpoch(u64);

impl QuillApp {
    /// The only writer of `self.live` and `self.screen`. The two change
    /// together or not at all.
    pub(in crate::ui) fn put_on_screen(
        &mut self,
        live: Option<LiveConnect>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let epoch = self.screen.epoch.next();
        self.screen = OnScreen::new(epoch, &self.chat_prefs, window, cx);
        self.live = live;
    }
}

impl std::ops::Deref for QuillApp { type Target = OnScreen; ... }
impl std::ops::DerefMut for QuillApp { ... }
```

`QuillApp` keeps `live`, `demo_session` and the App fields. `self.live`
(1,245 reads) does not move; its five writers go through `put_on_screen`
(`retire_live_to_background` takes it out, `adopt_live` and
`start_prepared_live_connect` put one in, the two closers take it out), so
the invariant "the screen's epoch changes iff `live` changes" lives in one
function. `demo_session` stays an app field but is set and cleared only by
the demo setup, which runs once per process.

The name: the project already says "the account on screen" everywhere
(`account_surfaces.rs`, `background_accounts/mod.rs`, `disk.on_screen`);
`account_state::AccountUi` is taken by the accounts list and passcode.

### 4.2 Keeping ~8,000 reads compiling: `Deref`

Field access auto-derefs, so `self.chat_list`, `this.viewer.state`,
`app.stories.composer.open` keep compiling after the move with
`impl Deref<Target = OnScreen> for QuillApp`. That is what makes the move
one reviewable PR per group instead of a repository-wide rename. Two costs,
stated so the implementer measures them in phase 2:

- A `DerefMut` field access borrows the whole `QuillApp`. An expression that
  also borrows a disjoint App field (`self.frame.x = f(&mut self.history)`)
  no longer splits and has to spell `self.screen.history`. Expect a few
  hundred such sites; the compiler lists them. If phase 2 finds more than
  about 500, do the mechanical rename (`self.<field>` → `self.screen.<field>`
  for the 41 names, a `sed` over `src/ui`) in that PR instead of `Deref`.
  Either way the end state is the same struct; `Deref` is the ramp.
- `Deref` on a view type is unusual. It is justified here because `OnScreen`
  is not a smart pointer but *the* state the app renders; the Coding Guides
  ask for the narrowest owner that can keep state correct, and `OnScreen`
  is that owner. The doc comment on the impl says so. Clippy has no lint
  against it.

`WindowSlot::swap_slot` (`slot.rs:154-165`) keeps working: `&mut
self.history` resolves through `DerefMut` and the other side is the slot.

### 4.3 Async continuations: one helper, the epoch rides on `ScopedApp`

`ScopedApp` (`src/ui/chat_window/scoped.rs:26-46`) already carries the
window a task belongs to and refuses to run once that window is gone. It
gains the account:

```rust
pub(in crate::ui) struct ScopedApp {
    app: WeakEntity<QuillApp>,
    /// The screen the task belongs to; `None` for app-wide work.
    screen: Option<ScreenEpoch>,
    /// The chat window within that screen (`None`: the main window).
    window: Option<ChatId>,
}
```

`ScopedApp::update` runs `f` only when `app.screen.epoch == screen` (or
`screen` is `None`) and then enters the window scope as today. The
constructors:

| Helper | Screen | Window | Replaces |
| --- | --- | --- | --- |
| `spawn_scoped(_in)` | current | current | itself; now also account-bound |
| `spawn_screen(_in)` (new) | current | main | `spawn_main` at sites that are the account's: the story viewer tick, the mini-app poll, the playback tick, `disk_account_switched`'s per-account part, every picker continuation |
| `spawn_app(_in)` (rename of `spawn_main`) | none | main | `spawn_main` at the poll loop, background accounts, passcode, spellcheck, updater |
| `in_screen_scope(epoch, cx, f)` (new) | given | main | for subscriptions and `cx.defer` bodies, as `in_window_scope` is today |

A value that outlives a draw and names its owner keeps the owner inside the
value, as `recording_state::Capture::origin` does (`recording_state.rs:61-66`):
`Capture` gains `screen: ScreenEpoch` next to `origin`, and `SendTarget`
(`recording_send.rs`) and the story reply's `(chat_id, story_id)` capture go
through `in_screen_scope`. `SendSlot::account` (`composer_sends.rs:111-114`)
stays as it is; it is the same idea with the key instead of the epoch
because its store survives in the background.

Why an epoch and not `AccountKey`: A → B → A must drop A's old
continuation too (the dialog that asked is gone with the first switch).
The epoch is monotonic per process; a key would match again.

The lint `scripts/check-window-scope.py` extends to the account: a closure
passed to `cx.spawn` / `spawn_in` / `defer` / `subscribe` / `observe` that
reads `live`, `session()`, `session_mut()`, `.driver.` or any `OnScreen`
field (list generated from `on_screen.rs` at check time, not hand-kept)
must start with `spawn_scoped*` / `spawn_screen*` or run inside
`in_screen_scope` / `in_window_scope`; `spawn_app*` bodies must not touch
them. Marker for exceptions: `// screen-scope: <why>`. The round-5 2c and
2g sites (`story_viewer/reply_media.rs:395-500`, `bot_edit_dialog.rs:199`,
`sticker_set_box/creator.rs:166`, `wallpaper_preview.rs:95`,
`pull_next_ui.rs:338-374`) are exactly what this flags.

### 4.4 Dialogs

A dialog's open flag lives in a feature struct. With the feature struct in
`OnScreen`, the flag is gone after a switch and `first_open` finds nothing;
with it in an App struct, the dialog stays. Placement decides, so the
`OnSwitch` typestate (`shell.rs:146-205`) and `close_dialogs_for_switch`
(`shell.rs:388-412`) are deleted. `DialogSpec::close` stays optional, for
the agent block (`close_blocked_dialogs`) only: `.closes(c)` becomes
`.close(c)`, `.follows_account()` and `.app_wide()` entries drop the call.
The 24 `follows_account` dialogs read `Session` state and need nothing; the
5 `app_wide` ones (passcode, background call ring, emoji set style, theme
editor) have their flags in App structs already, or move there
(`EmojiStyleUi` is in `SettingsUi`'s App half).

The kit layer is the one dialog state outside `QuillApp`: `close_all_dialogs`
on the main window and `windows.main_dialogs_closed` (`shell.rs:449-456`)
stay, two lines. A dialog hosted in a chat window goes with that window.

Round-5 2b (compound `is_open` gates skip the close) cannot recur: there
is no close to skip.

### 4.5 What a switch still has to *say*: `Farewell`

Dropping state is silent. Some surfaces owe TDLib or disk a word first,
through the *old* client: `closeWebApp` for an open mini app, `closeStory`
for the story viewer, the draft flush and `deselect_chat` of the open chat,
`cancel` of a recording, the chat windows' places saved under the old
account, the Business page save. This is a short, bounded set ("what tells
TDLib or disk something on close", not "what holds state"). It is still a
list, so the design makes the type system ask for it:

```rust
/// What one feature's UI says, through the client it was opened on, when
/// the account on screen is about to change. Most features say nothing.
pub(crate) trait Farewell {
    fn farewell(&mut self, _cx: &mut FarewellCx<'_>) {}
}
```

Every field of `OnScreen` implements `Farewell` (one `impl` per feature
struct; a new feature struct does not compile in `OnScreen` until its
author has written the impl, even the empty one), and `OnScreen::farewell`
is generated by a small `on_screen_fields!` macro that also declares the
struct, so the call list and the field list are one list. `FarewellCx`
hands out `Option<&mut ConnectDriver>`, `&mut Window`, the `ChatWindows`
registry and the `AccountKey`, which is what the six current goodbyes need.
The order of fields in the macro is the order of goodbyes (the mini app
before the chat leave, as `end_account_surfaces` does now).

`end_account_surfaces` becomes:

```rust
fn end_account_surfaces(&mut self, window, cx) {
    let mut fcx = FarewellCx { live: self.live.as_mut(), windows: &mut self.windows, window, cx };
    self.screen.farewell(&mut fcx);
    self.close_main_window_dialogs(window, cx);   // the kit layer
}
```

and `switch_account` loses its 40 lines of input resets.

### 4.6 Forcing the bucket for new state

- `structure.md` "App state" gains one sentence: a new `ui/<feature>_state.rs`
  struct is a field of `OnScreen` (`ui/on_screen.rs`), with a `Farewell`
  impl. It is a field of `QuillApp` only when it is the process's, and then
  it is listed in `scripts/app-wide-state.txt` with the reason.
- `scripts/hotspots.txt` gets `src/ui/on_screen.rs | max:3 | one OnScreen
  field for a new ui/<feature>_state.rs struct`, and `src/ui/app.rs` goes to
  `none` (feature state goes in `on_screen.rs`).
- `scripts/check-screen-state.py` (new, in the same CI job): every field of
  `QuillApp` is either in `app-wide-state.txt` or one of the fixed
  infrastructure fields (`screen`, `live`, `demo_session`, `credentials`);
  `OnScreen` is declared through the macro (the macro enforces the
  `Farewell` impls at compile time); and the async lint of 4.3.
- A test, `on_screen_tests.rs`: build an app with `ready-chats`, open one of
  each surface kind (a flagged dialog, the story composer layer, a context
  menu, the mini-app confirm, a `spawn_screen` task that writes a flag),
  switch, assert the epoch changed, the surfaces are gone, and the task's
  update returned `Gone`. Then switch back and assert nothing came back.
  This replaces the four tests in `account_surfaces_tests.rs` and cannot go
  stale, because it checks the value's identity, not fields.

### 4.7 Threading, perf, memory

All of this is main-thread state inside one GPUI entity. The only shared
queues (`notify_clicks`, `calls.notify_clicks`, `background.clicks`) are
App fields and already carry the account per item. Dropping `OnScreen`
releases 12 `Entity<TextareaState>`/`InputState`, a few `ScrollHandle`s,
the stored `Task`s (`location_ui.*`, `stories.stealth_tick`,
`mini_apps.poll`, `history.pull.jump`), which cancel on drop, and the
screen's image and row caches; that is memory the switch should free
anyway (the round-3 finding on the 30 ms mini-app poll running forever is
one of these tasks). Building the new value is the same work
`new_with_demo` does for those fields today, well under a millisecond. No
per-frame cost: `Deref` is a field offset.

What must survive a switch stays keyed (approach D) and app-owned:
`ComposerSends`, the saved chat windows file (`chat_window/saved.rs`), the
notification `reply_targets` / `preview_waits`, `disk.on_screen`. Round-5
2f (the deferred window close saving an empty list for the incoming
account) is fixed by 4.3: `chat_window_closed` runs from a `cx.defer` made
under the old epoch, so it runs in `in_screen_scope(old)` and is dropped.

### 4.8 Switching back

tdesktop rebuilds the controller on every switch and restores only what
the session saved (active chat, filters). `OnScreen` does the same: a
fresh value, then the existing restores run (`restore_chat_windows`,
`restore_open_draft`, folder tab from `Session`). Keeping one `OnScreen`
per background account instead (a `HashMap<AccountKey, OnScreen>`) would
bring back open boxes on switch-back, which is not tdesktop behavior, and
would hold every account's text-field entities and caches for the life of
the process. Rejected; the map is a two-line change later if wanted.

## 5. Migration

Each phase compiles, passes `gate.sh` (fmt, clippy, tests, file size,
hotspots, window scope), and is one PR. Order matters: the epoch first,
because the struct move in phase 2 is only safe once continuations can be
dropped.

**Phase 1: the epoch and the helpers.** Add `ScreenEpoch` to
`ConnectionUi` for now (it already has `generation`, bumped in `adopt_live`
and `start_connection`; the epoch starts as that counter). Extend
`ScopedApp` with `screen`, add `spawn_screen(_in)`, `in_screen_scope`,
rename `spawn_main` → `spawn_app` with a deprecated alias. Reclassify the
108 existing `spawn_main`/`spawn_scoped` sites: `spawn_scoped` sites change
nothing; `spawn_main` sites become `spawn_screen` or `spawn_app` by what
they touch (the lint lists them). Fix the round-5 2c/2g sites with
`in_screen_scope`. Extend `check-window-scope.py` with the account rules
and the `// screen-scope:` marker, run it, convert or mark the 120 plain
`cx.spawn` sites it flags. Tests: a `spawn_screen` task started before a
switch does not run after it; one started after does; `spawn_app` runs
across. No field moves yet.

**Phase 2: `OnScreen` with the surfaces `end_account_surfaces` touches
today.** Create `on_screen.rs` with the macro, `Farewell`, `Deref`,
`put_on_screen`; move `chat_list`, `stories`, `playback`'s screen half (the
`PlaybackUi` split happens here), `message_ui`, `share`, `viewer`,
`mini_apps`, `recording`, `history`, `composer`, `composer_ui`, `pickers`,
`search_ui`, `sticker_box`, `notify`'s screen half, `folders`. Write the
six `farewell` impls (mini app, story viewer, chat leave, recording, chat
windows via `FarewellCx`, business later). Delete the matching lines of
`end_account_surfaces` and of `switch_account`'s reset list. `app.rs`
changes beyond `max:3`: commit trailer `Hotspot-change: QuillApp screen
state moves to ui/on_screen.rs`. This is the phase that measures the
`Deref` borrow-split count and decides ramp vs rename (4.2). Tests:
`on_screen_tests.rs` first version.

**Phase 3: dialogs.** Move `dialogs`, `admin`, `payments`, `earn`,
`gift_market`, `giveaway`, `paid_*`, `suggested_posts`, `direct_messages`,
`privacy`, `twofa`, `links`, `peer_qr`, `profile_cover`, `video_avatar`,
`inline_members`, `common_groups`, `media_search`, `shared_media_grid`,
`premium_preview`, `location_ui`, `ai_compose`, `auth_ui` (minus
`demo_inputs`), `screen_share`, `group_call`'s screen half, `calls`' screen
half, `settings`' screen half, `agent_ui`'s screen half, `connection`'s
screen half. Delete `OnSwitch`, `DialogUnscoped`, `close_dialogs_for_switch`,
`dialog_switch_scopes`; `.closes(c)` → `.close(c)`; drop
`.follows_account()` / `.app_wide()` calls (189 entries, mechanical). The
`ScreenEpoch` moves from `ConnectionUi` to `OnScreen::epoch`.
`end_account_surfaces` is now `farewell` + the kit close. Tests: the
round-4 table (GroupConfirm, FolderDelete, SessionsConfirm, ClearCalls,
BlockBar, CallConfirm) as one parametrised test over `open_dialog_names`
before and after a switch.

**Phase 4: the guards.** `check-screen-state.py`, `app-wide-state.txt`,
hotspot rules, `structure.md` text. Remove the `spawn_main` alias. Delete
`account_surfaces.rs` (its two remaining functions move to
`on_screen.rs`) and `account_surfaces_tests.rs`.

**Phase 5 (follow-up, not blocking):** merge `connection.generation` into
`ScreenEpoch` (the poll loop's guard is the same idea), and declare
`WindowSlot`'s fields through the same macro so a window's `farewell`
(flush draft, discard recording, close PiP) is generated the way the
screen's is.

Each phase's PR body carries the before/after of the two lint counts
(window-scope and screen-scope flags) and the `end_account_surfaces` line
count, so the reviewer sees the list shrink to zero.

## 6. The same pattern outside account switching

Grep over `src/ui` for owners that change while their state does not:
chat (`dismiss_cross_chat_state`, `leave_open_chat`), window (`WindowSlot`,
`forget_chat_window`), viewer (`close_media_viewer`, `close_story_viewer`),
and caches keyed by an id that is not unique without its owner. Concrete
instances, with the scope that would cover them.

### 6.1 Per-chat state surviving a chat change

`leave_open_chat` (`src/ui/chat_window/mod.rs:610-641`) and
`dismiss_cross_chat_state` (`src/ui/drafts.rs:125-165`) are the chat's
hand lists, the same shape as `end_account_surfaces`. Not on them:

| State | Where | Trigger | Covered by |
| --- | --- | --- | --- |
| `composer_ui.rich_editor_open` | `composer_state.rs:123` | open the rich editor in chat A, open chat B: B's composer is in editor mode (round 5, predates round 4) | a `ChatUi` sub-struct of the slot (6.6) |
| `composer_ui.send_as_open` | `composer_state.rs:178` | open "send as" in a channel, click another chat: the picker is open over a chat with no send-as list | same |
| `message_ui.link_popup` | `message_state.rs:32` | hover popup for a link in chat A, switch chat by keyboard: the popup stays for A's link | same |
| `chat_list.preview` / `preview_press` | `chat_list_state.rs:42-45` | peek preview of chat A open, A leaves the list (archived from another device): the preview shows a chat no row has. Plausible | a `ChatListUi` reset on list rebuild |
| `history.pinned_cursor`, `hidden_pinned` | `history_state.rs:127,130`, `HashMap<i64, _>` by chat id | per chat *and* per account, keyed by chat alone: hide a pinned message in chat 42 on A; on B, chat 42 (the same person) treats that pinned id as hidden if the id exists | `OnScreen` (gone on switch); within one account the key is right |
| `earn.photos_asked: HashSet<i64>` | `earn_state.rs:63` | bot photos asked once; after a switch the same bot id is not asked for B | `OnScreen` |
| `ai_compose.emoji_asked`, `mini_apps.emoji_access_shown` | `ai_compose_state.rs`, `web_app_ui/mod.rs:252` | the same "asked once" sets keyed without account | `OnScreen` |

### 6.2 Per-window state surviving a window close

| State | Where | Trigger | Covered by |
| --- | --- | --- | --- |
| `viewer.pip: Option<PipLink>` | `viewer_state.rs:47` | open PiP from a chat window's viewer, close the window: PiP keeps playing with its link in the dropped slot (`sweep-window-slot-scope.md` follow-up) | `Farewell` for `WindowSlot` (phase 5): the slot closes its PiP |
| the temporary archive in `drop_zones.rs:100-118` | window closed mid-plan | the file stays on disk (sweep follow-up, unverified) | `spawn_scoped` already drops the update; a slot `farewell` would delete the file |
| deferred `on_window_closed` → `chat_window_closed` → `schedule_chat_windows_save` | `window_control.rs:565-578`, `chat_window/mod.rs:512-523`, `saved.rs:86-127` | runs after the switch, saves B's empty window list (round-5 2f) | `in_screen_scope` in the defer (phase 1) |
| session-side per-window state kept per session: `messages.unread_jump`, `messages.composer_preview`, `stickers.custom_emoji_preview` | sweep follow-ups | a jump or link preview asked from a window lands in the main view | `WindowViews` growing those fields (core, out of scope here) |

### 6.3 Viewer and overlay state surviving the viewer

| State | Where | Trigger | Covered by |
| --- | --- | --- | --- |
| `stories.reply_open`, `privacy_edit`, `close_friends_edit`, `share_open`, `report_open`, `cover` | `release_story_viewer` (`story_viewer/mod.rs:634-648`) skips what `close_story_viewer` (`:652-677`) clears | the story is deleted while its privacy editor is open, or the passcode locks: the next viewer opens with the old story's editor, and Save sends the old `(chat_id, story_id)` (round-4 "small leftovers") | one `StoryViewerUi { item, reply, privacy_edit, .. }` value owned by `stories.viewer: Option<_>`, so release *is* close |
| `viewer.photo_editor` | not cleared by `close_media_viewer` (`media_viewer/navigation.rs:250-265`); only `close_photo_editor` (`photo_editor.rs:430`) and the attach path (`composer_sends.rs:415`) | `close_media_viewer` from the passcode lock (`disk_state.rs:411`, `reactions.rs:142`) while an editor is open from the viewer: the editor outlives the viewer. Plausible; Esc order is right because `esc_stack.rs:153-160` puts the editor above the viewer | the editor as a field of the viewer's open state, not beside it |
| `stories.composer`, `stories.page` | escape-stack layers (`esc_stack.rs:112-120`), not dialogs; cleared by nothing but their own close | round-5 2a across accounts; within one account the composer is independent of the viewer by design | `OnScreen` |

### 6.4 Async continuations applying to another chat, message or window

| Site | Where | What it captures | Guard today | Covered by |
| --- | --- | --- | --- | --- |
| story reply voice / round | `story_viewer/reply_media.rs:395-500` | `(chat_id, story_id)`, sends through `this.live` after the encoder | window scope only | `in_screen_scope` (phase 1) |
| `choose_bot_photo` | `bot_edit_dialog.rs:199-234` | `bot_id`; `set_bot_photo` through `live` | none | `spawn_screen` |
| `begin_create_sticker` → `edit_image_for_set` | `sticker_set_box/creator.rs:166-188` | `set_id`; opens the editor over whatever is there | none | `spawn_screen`; the editor then lives in `OnScreen` |
| `choose_wallpaper_file` | `wallpaper_preview.rs:95` | opens the preview for the account on screen | none | `spawn_screen` |
| `pull_jump_when_ready` | `pull_next_ui.rs:338-374` | `next: ChatId`; polls up to 1.5 s then `select_listed_chat(next)` | `machine.jumping()` + window | `spawn_screen`; within an account the machine guard is right |
| `open_chat_preview` press | `message_actions/open_chat_preview.rs:17-25` | `(chat_id, started)` token | token compare | already right; the model for 6.1's preview |
| `pull.jump`, `location_ui.*`, `stories.stealth_tick`, `mini_apps.poll` | stored `Task`s | | dropped with their struct | `OnScreen` drop cancels them |

### 6.5 Caches keyed without their owner

| Cache | Where | Key | Collision | Covered by |
| --- | --- | --- | --- | --- |
| `playback.positions` | `playback_state.rs:75` | `MessageId` alone | TDLib message ids are per chat in supergroups and channels: a half-played voice note in channel X and one with the same id in channel Y share a seek position; `gif_decodes` (`:90`) shares one decode slot the same way | key by `(ChatId, MessageId)`; `OnScreen` covers the account half |
| `vanish::PAINTED` | `vanish.rs:53`, thread-local `HashMap<i64, Painted>` | message id alone, across windows | a delete's dust starts at the bounds of a same-id row in another window or chat | key by `(window, ChatId, MessageId)` |
| `send_fly::CELLS` | `send_fly.rs:63`, thread-local `HashMap<i32, Bounds>` | sticker cell index, across windows | a sticker sent from a chat window flies from the main window's panel cell | key by window |
| `premium_sticker_effect::Played.chats` | `premium_sticker_effect.rs:72-74` | chat id, no account | after a switch the same chat id on B has "already played" effects | `OnScreen` (make it a slot or screen field) |
| `shared_media_grid.page: PageKey { chat, .. }` | `shared_media_grid_state.rs:19-25` | chat id | per account; fine within one | `OnScreen` |
| `notify.reply_targets`, `preview_waits` | `notify_state.rs:24-30` | `(AccountKey, ChatId)` | already keyed by owner (the grok round-1 finding is fixed) | stays App, keyed (approach D) |

### 6.6 Siblings of `OnScreen`

The account is the outermost owner; the same shape repeats inward, and the
design is the same at each level: one value per owner, replaced whole, a
`Farewell` for what it owes on the way out, and `ScopedApp` carrying the
owner's identity for work that finishes later.

- **`WindowSlot`** already is this for the chat window. Phase 5 declares its
  fields through the same macro so it gets `farewell` (PiP, drop archive,
  recording) for free.
- **`ChatUi`** (new, a slot field): the state of *the chat shown*:
  `rich_editor_open`, `send_as_open`, `link_popup`, reply, edit,
  `pending_delete`, `pending_stop_poll`, the command and inline menus,
  `slow_mode_tick_chat`. `select_chat_with` replaces it;
  `dismiss_cross_chat_state` and half of `leave_open_chat` disappear. It
  carries `chat_id`, so the `belongs_to(chat_id)` checks become one check at
  the owner. This is the next list to retire after the account one, and the
  same review rounds found its leaks (round-4 2b, round-5 "two other
  leftovers").
- **`StoryViewerUi`** (6.3): the viewer's open state as one `Option`, so
  `release_story_viewer` cannot be a subset of `close_story_viewer`.
- **`ViewerUi.state` with the editor inside it** (6.3).

Each of these is its own PR after phase 4; none blocks the account work.

## 7. What this does not do

- It does not move state out of `Session` or change the driver. The
  "follows the account" surfaces stay where they are.
- It does not stop a switch from *losing* UI the person had open (a
  half-typed box). tdesktop loses it too; the draft is saved, the rest is
  not. A per-account map (4.8) is the opt-in if that is ever wanted.
- It does not change the `DialogSpec` priority order or the kit's single
  modal dialog.
- The non-account instances in 6 are findings, not fixes; the two with a
  user-visible wrong result today (6.5 `positions`, 6.3 story editors) are
  small and can go in any sweep.

## 8. What the build changed

The phases above were written before the code was opened. Where the code
showed the plan wrong, the code won; this section is the record, one
entry per phase, so the sections above can be read as the intent and
this one as what shipped.

### Phase 1 (the epoch and the helpers)

- **No deprecated `spawn_main` alias.** All 44 `spawn_main(_in)` sites
  were reclassified in the same commit (16 became `spawn_screen(_in)`,
  the rest `spawn_app(_in)`), so the alias had no callers to keep.
- **`defer_screen(_in)` added** next to `spawn_screen(_in)`. The 4.3 table
  had `in_screen_scope` for `cx.defer` bodies; most defer sites wanted
  the epoch captured at the call, not threaded by hand, and
  `Context::defer` gives the body only an `App`, so the helper re-enters
  the entity and the screen scope for them.
- **`StartedOn` stays, as an epoch.** The plan dropped it with the
  account key. A recording's `SendTarget` is held across the encoder
  join and read back on the main window, outside any scoped update; it
  keeps `StartedOn(ScreenEpoch)` so the continuation can tell "my window
  closed" from "the account changed" and post the right note.
  `live_for` and `drop_recording_of_other_account` are gone.
- **A dropped continuation still owes its cleanup.** When a screen-scoped
  send comes back `Gone`, the file is removed and the note is posted
  through `ScopedApp::app()` (`recording_dropped`,
  `recording_dropped_with_screen`). Phase 1 tests for the recording
  sites switch by calling `end_account_surfaces` instead of rewriting
  `session.account`, which means nothing once the epoch exists.
- **Where the epoch moves.** At the end of `end_account_surfaces`, after
  the goodbyes and the dialog closes, so a closer's own spawned
  continuation is dropped too. It also moves in demo mode (no client)
  and when the switch then fails at the credentials check: the surfaces
  are gone either way, and the existing switch tests rely on that
  order. `close_screen_client` (logout, disk full) moves it without
  goodbyes. A client filling an *empty* screen does not move it:
  nothing on the screen was any account's while there was no client.
- **Lint scope.** The screen rule covers continuations only
  (`cx.spawn`, `spawn_in`, `cx.defer`, `defer_in`, `window.spawn`,
  `window.defer`, `on_next_frame`); subscriptions and observers are
  exempt, because an event handler acts for whatever is on screen when
  the event fires. Only `in_screen_scope` satisfies the rule, not
  `in_window_scope`, since the window scope says nothing about the
  account. 118 sites were converted or marked; 45 carry
  `// screen-scope:` markers, nearly all app clocks (frame clock, sticker
  closer, notification windows, animation layer) and flags that must
  survive a switch (`global_ptt_polling`, the drafts flush, the calls
  tick).
- **Two judgment calls worth knowing.** `chat_window/mod.rs` and
  `screen_chooser.rs` return early if the epoch changed before their OS
  window opened (the window would show the next account's chat with the
  old id). `similar_strip.rs` has no app handle at render time and keeps
  the account key with a marker.

### Phase 2 (`OnScreen`, the first sixteen fields)

- **The ramp stays.** `Deref<Target = OnScreen>` for `QuillApp` left 47
  borrow-split errors in 21 files (the 4.2 threshold was 500); each was
  fixed by spelling `self.screen.<field>` on the side of the split that
  names a screen field. No mechanical rename.
- **`OnScreen::new` needs a window.** Its text fields are
  `TextareaState::new(window, cx)`, so the screen is replaced where the
  switch has one (`end_account_surfaces`, now `close_switch_surfaces` +
  `farewell_screen` + `close_switch_surfaces` + the kit close +
  `replace_screen`). The three paths that empty the screen without a
  window (the poll loop's logout, a full disk, a connection that finds a
  client still on screen) call `screen_leaves_later`: the epoch moves at
  once, so no old work runs, and the goodbyes and the replacement run on
  the main window at the end of the same update, with the client already
  gone, which is what those paths always did. Without a main window the
  old UI stays under the new epoch.
- **`put_on_screen` and `take_live`, not `take_off_screen`.** The plan
  had one function that said goodbye, took the client and replaced the
  UI. Since the goodbyes need the client and the replacement needs a
  window, they are three steps: `end_account_surfaces` (goodbyes and
  replacement), `take_live` (the only reader-out of `live`; a debug
  assertion checks the screen was replaced since the client was put on
  it) and `put_on_screen` (the only writer). Quit still takes `live`
  directly. A client filling an empty screen keeps the screen.
- **`Farewell::farewell` takes the app.** A field's goodbye goes through
  the app's methods (`close_player`, `leave_chat_for_switch`,
  `close_video_pip`, `close_mini_app_window`,
  `leave_live_story_for_switch`, `cancel_recording`), so the trait method
  is `fn farewell(app: &mut QuillApp, window: &mut Window, cx)` with the
  field's type naming which goodbye; a value method would need the
  app anyway. `end_mini_apps_for_switch` is gone: its field resets were
  the drop.
- **`ScreenEpoch` moved to `on_screen.rs` here**, not in phase 3: the
  epoch is `OnScreen::epoch` and `screen_leaves` is gone; `ConnectionUi`
  keeps only `generation`.
- **Split names.** `PlaybackUi`'s app half is `players: PlayersUi`
  (engine, sticker and emoji players, inline and userpic videos, speed,
  volume); `NotifyUi`'s app half is `delivery: NotifyDelivery` (OS
  clicks, reply targets, preview waits, sounds, Quill's notification
  windows). The track reads (`is_playing`, `held_paused`, `track_shown`,
  `active_position_secs`) take the engine as a parameter on `PlaybackUi`
  and are forwarded by `QuillApp`, so their unit tests still run without
  an app.
- **Dialogs that are not flags.** The delete box is the kit's alert and
  sets `message_ui.pending_delete` only on OK; the `on_screen_tests`
  read it through `window.has_active_dialog`. The search's `open` is the
  session's, not the screen's, and is not in the test's surface list.
- **The lint's field list** now comes from `on_screen.rs` and is joined
  with the planned names that phase 3 still has to move.
- **Tests that faked a switch** by calling `retire_live_to_background`
  alone (the live story's leave and PiP tests) now call
  `end_account_surfaces` first, as `switch_account` does: the story's
  leave is `StoryUi`'s farewell, not the retire's business.

### Phase 3 (the dialogs' state, the splits, the typestates)

- **Calls stay the app's.** The doc's table put the call window, its
  players, `ended_at` and the group call's window, stream, pin, levels,
  composer and settings/title dialogs on the screen. A call keeps running
  in the background account (tdesktop's call panel belongs to the session,
  which outlives the switch), so `CallUi`, `GroupCallUi` and
  `ScreenShareUi` (the share chooser of a call, with the AppWide
  "screen-privacy" layer) stay on `QuillApp`. Only the boxes that open
  from the account on screen moved: `call_boxes: CallBoxes` (confirm,
  dialing, call history scroll, mic test, add people, conference link)
  and `group_call_boxes: GroupCallBoxes` (start dialog, create request,
  RTMP start, invite, share).
- **The other splits and their app halves:** `settings` /
  `preferences: PreferencesUi` (theme, accent and font pickers, sliders,
  keybindings, updates, what's new, advanced, emoji style, translate);
  `auth_ui` / `auth_demo: AuthDemo` (`demo_state`, `demo_inputs`: the
  synthetic mode's, next to `demo_session`; the doc had `demo_state` on
  the screen); `agent_ui` / `agent_bridge: AgentBridge` (`activity_shown`,
  `ui_send`, and the wake task, which a per-screen `new` must not spawn);
  `connection` / `connect: ConnectGeneration` (`generation` and the
  closed-note mark only: the toasts stay with the note they are made
  from, so `set_note`'s 1,300 call sites did not move); `windows` /
  `main_window: MainWindow` (handle, place, cascade, `dialogs_closed`, the
  key-scope subscription).
- **The new screen's connection status is the old one's.** The switch
  sets it again at once (`adopt_live`, `start_connection`); a fresh
  default would draw the synthetic pane for a frame.
- **Chat windows close in the farewell**, `ChatWindows`' own, before
  the replacement, instead of after `end_account_surfaces` in
  `switch_account`; the new screen's `restore` starts `Pending`, which is
  what that code set by hand.
- **Farewells with a body**, found by reading every `.closes` closer and
  every `LayerSwitch::Closes` row before deleting them: `PaymentUi`
  (the hub, premium, stars, gifts and money boxes keep their flags in the
  `Session`, so without a close they would come back on switch-back, and
  the first-grams hint reaches TDLib), `DialogUi` (a business save in
  flight), `GiveawayUi`, `PaidMessagesUi` (the held paid send),
  `PrivacyState` (the OS passkey ceremony), `PeerQrUi` and `ComposerUi`
  (temp files), `SettingsUi` (the stickers manager commits its edits).
  Everything else was a flag or `Session` reset, which the drop covers.
- **`DialogSpec::new` returns the spec**; `.close(c)` is optional and
  only the agent block reads it. 161 `.closes` → `.close`, 2
  `.app_wide_closes` → `.close`, 25 `.follows_account()` / `.app_wide()`
  dropped. `LayerSwitch` and the `layer!` switch argument are gone.
- **Lint fallout:** the moves made 14 more continuations touch the
  screen; 4 became `spawn_screen` (toast wake, mic-test meter, the group
  call schedule countdown, the picked video cover) and 10 carry markers
  (file-save and failure notes posted as the app, the idle image trim).
- **Tests:** the four field-list tests went; the boxes test and the
  round-4 table are one parametrised test over a `HeldBox` table
  (GroupConfirm, FolderDelete, SessionsConfirm, ClearCalls, BlockBar,
  CallConfirm) that also checks each flag's struct is an `OnScreen`
  field (`OnScreen::FIELD_NAMES`); an app-wide dialog's flag
  (`preferences.emoji_style.open`) is checked to survive. Worth a second
  look later: the Translate dialog's flag is in `preferences`, so it
  survives a switch too.
- **Three auth placeholders** got lang keys (`qll_misc_phone_country_code`,
  `qll_misc_recovery_code_from_email`, `qll_misc_verification_code`) when
  they left the check-lang-exempt `app_demo.rs`.

### Phase 4 (the guards)

- `scripts/check-screen-state.py` and `scripts/app-wide-state.txt` as in
  4.6, run in CI next to the window-scope lint. 33 `QuillApp` fields: 4
  infrastructure (`screen`, `live`, `demo_session`, `credentials`), 29
  listed with their reasons. The macro check is textual: `OnScreen` must
  be declared inside `on_screen_fields!`.
- Hotspots: `src/ui/app.rs` and `src/ui/app_demo.rs` are `none`;
  `src/ui/on_screen.rs` allows `max:3` (one field, its doc line and its
  line in `new`).
- `account_surfaces.rs` is gone: `end_account_surfaces`,
  `leave_chat_for_switch` and `close_main_window_dialogs` live in
  `on_screen.rs`; its remaining behavioural tests (delete box, media
  viewer, checkout, draft save, boxes holding ids, reply-elsewhere, hidden
  dialog, the app-wide flag) are `on_screen_switch_tests.rs`.
- Not done here (phase 5, optional): merging `connect.generation` into
  the epoch, and declaring `WindowSlot` through the macro.

### Phase 5 (the chat, the story viewer, the window, the leftovers)

Section 6 built, on `claude/sm-chat-scope`.

- **`ChatUi`** (`src/ui/chat_ui_state.rs`): one value for the chat the
  view shows, a boxed field of `OnScreen` and of every `WindowSlot`.
  `enter_chat_ui` replaces it when the chat changes, carrying a reply or
  edit aimed at the next chat; `leave_chat_ui` drops it; `sync_chat_ui`
  at the top of `frame_upkeep` catches a chat changed by a path that
  skipped both. `dismiss_cross_chat_state` is gone. Fields:
  `rich_editor_open`, `send_as_open`, `link_popup`, the reply and edit
  (with `EditMode`), the command and inline menus, `inline_query_armed`,
  `pending_delete`, `pending_stop_poll`, and also `selection_anchor`,
  `selection_focus` and `scheduled_selected`, which hold message ids of
  the shown chat and were not in the 6.6 list.
- Deviations from 6.6: `slow_mode_tick_chat` stays in `ComposerUi`; it
  names the chat its timer task runs for, not state of the chat shown.
  `notify.ttl_picker_open` stays in `NotifyUi` (not per window) and is
  reset in `enter_chat_ui`. The forward bar and the recording keep their
  own explicit goodbyes, called from `enter_chat_ui`, since both own
  tasks or devices. `Box<ChatUi>` because `QuillApp` would pass its
  48 KB stack budget otherwise.
- Guard: `scripts/check-screen-state.py` now fails when a view struct
  (`ComposerUi`, `MessageUi`, `HistoryUi`, `PickerUi`, `ShareUi`,
  `ViewerUi`) gains a field typed or named by a chat or message id that
  is not in `ChatUi` and not listed with a reason in
  `scripts/view-state-not-chat.txt` (14 entries), or redeclares a
  `ChatUi` field.
- **`StoryViewerUi`** (`src/ui/stories_state.rs`): what the story viewer
  has open (reaction picker, reply bar and panel, viewers, stats, report,
  cover and privacy editors, close friends, share box, notice) is one
  `Option` inside `StoryUi`, present only while the viewer is open.
  `release_story_viewer` drops it, so a release is a close for this
  state. Writes go through `viewer_ui_mut` / `with_viewer_ui` and are
  dropped while the viewer is closed; reads go through `viewer_ui`, which
  falls back to an empty value. `close_story_viewer` ends the story reply
  before the release so the reply's own cleanup still sees its bar.
- **The media viewer's photo editor is not moved into the viewer.** 6.3
  assumed it could be opened from the viewer. It cannot: it opens from the
  attachment tray, a message edit, the sticker creator and the profile
  photo, never from the viewer's item. Nesting it in the viewer's open
  state would leave it unopenable. It stays in `ViewerUi` beside the
  viewer, which `OnScreen` already replaces on a switch; the Esc order was
  already right.
- **`WindowSlot` through the macro.** `on_screen_fields!` now takes the
  struct name, its farewell trait, the name and parameters of the
  generated farewell method, and a list of bookkeeping fields with no
  goodbye before a `;`. `OnScreen` declares `epoch` and `client_filled`
  there; `WindowSlot` declares `slices`, `images`, `frame_started`,
  `active` and `place`. The slot's trait is `SlotFarewell`, which takes
  no `Window` because `forget_chat_window` has none. Only `ViewerUi` says
  anything: it closes the Picture-in-Picture window its link points at.
  `forget_chat_window` runs `farewell_window` inside the window's scope
  before the slot is dropped. The recording and the drop archive did not
  need slot goodbyes: the recording already ends in `forget_chat_window`,
  and the archive is covered by the work-file guard below.
- **Translate.** The dialog, the bar's toast and its sequence, the offer
  cache and the pump's scan moved from `preferences.translate` to a new
  screen field `translate: TranslateScreen`. Only the prefs stay app-wide.
  The toast timer runs through `spawn_screen`, so an old screen's timer
  cannot clear the next screen's toast that has the same id.
- **Work files.** `quill::scratch::OwnedFile` removes its file (or the
  folder it was made in) when dropped untaken; `keep()` hands the path
  on. The sticker video encode, the camera shot, the avatar video encode
  and the dropped-folder archive wrap their outputs in it, so a
  continuation that never runs (screen or window gone) removes the file
  instead of leaving it for the startup sweep.
- **Logout.** `request_logout` sends `logOut` before touching anything.
  A send failure now leaves the generation, the requests and the shutdown
  phase as they were (before, only the shutdown phase was put back). On
  success `Session::logout_sent` settles the other requests, bumps the
  generation, and puts the `logOut` request back under the new
  generation so its answer is not dropped as stale.
- **Checkout.** `Session::close_checkout` replaces the UI's field-by-field
  clear. While `sendPaymentForm` is in flight it keeps the invoice and
  form (`checkout_closed`), so a paid-media payment that answers after the
  close still leaves its "Media Unlocked" note. `settle_closed_checkout`
  drops them on the answer or the error. A new checkout clears the flag.
- **Hang-up.** `discard_call` no longer hangs up the call engine as soon
  as `discardCall` is sent. `HangUp` gained `engine_hang_up_due`, set by
  TDLib's `ok`; `pump_call_engine` hangs the engine up then, or on the
  terminal `updateCall` as before. An error puts the call back with its
  sound running. `hang_up_for_close` still hangs up at once, because the
  client is going and no answer will come back.

### Phase 6 (the parts left inside app-wide structs)

Round 6 of the review, on `claude/sm-review6`. Phase 2 checked only the
names of `QuillApp`'s fields. A struct listed as the process's could
still hold the account on screen's state, and five did. The reason given
for `calls` and `group_call` ("a call keeps running in the background
account") was wrong: `retire_live_to_background` closes a client that is
in a call, so no call outlives its screen. That made the call ids in
those structs the screen's.

- **Account.** The Account dialog (log out, delete, with the password
  typed), the frozen-account notice and age verification moved from
  `account` to a new screen field, `account_boxes: AccountBoxes`. Before,
  a Delete Account box armed on A came back over B, and its last button
  deleted B. The accounts list and the passcode lock stay app-wide: the
  registry and the lock are the process's, and the list's remove confirm
  is keyed by `AccountKey`.
- **Calls.** `window_closed_by_user`, `ended_at` and the sound marks moved
  from `calls` to `call_screen: CallScreen`. Call ids repeat across
  clients: B's call 1 never opened its window if you had closed A's call
  1. The window, its players and the sound output stay app-wide. They
  follow the session on screen and close when the next account tracks no
  call. `CallScreen`'s goodbye closes the share chooser and drops its
  permission box.
- **Video chat.** The title box, Settings, the in-call composer (and its
  Enter subscription, out of `app_demo.rs`), the window you closed, the
  tile pin, the stream join you asked for and the join on its way moved
  to `group_call_screen: GroupCallScreen`. A join pending at the switch
  could fire on B's call of the same id, as A's join-as peer, and the
  Join button could wait forever on a timeout task dropped with the old
  screen.
- **Share chooser.** `screen_share.opening` was a bool that only the
  listing's continuation reset. A switch dropped that continuation, and
  Share Screen did nothing until restart. It is now
  `Option<ScreenEpoch>`: a listing asked by another screen never blocks.
- **Checkout.** A checkout opened while an earlier payment was in flight
  took that payment's answer: the new form showed as paid, and "Media
  Unlocked" named the new message. tdesktop keeps one process per invoice
  (`payments_checkout_process.cpp`, `Processes`). Quill now ties each
  answer to the request that sent it. `form_request` is the
  `getPaymentForm` the open checkout waits for, so an earlier checkout's
  form is dropped. `paying: Vec<PaymentInFlight>` holds each
  `sendPaymentForm` with its invoice and form, so the answer unlocks the
  media of the payment that was made. It touches the open checkout only
  when that checkout shows the same invoice (`shows`). `sending` and
  `checkout_closed` are gone, and so is `settle_closed_checkout`. A mini
  app's invoice watch reads `is_paying` and `shows` for its own invoice.
- **Chat-window save.** `schedule_chat_windows_save` was a plain
  `cx.spawn`, exempted with a wrong reason. A switch inside its 600 ms
  could write B's list as empty before B's restore. It runs through
  `spawn_screen` now, and the switch saves whenever a save was pending.
  No deterministic test: saves need a live client, and the demos never
  write the file.
- **Deferred goodbyes.** `screen_leaves_later` followed by `put_on_screen`
  in one update relied on the connection generation to keep A's goodbyes
  off B's client. Now `screen_leaves_later` sets `goodbyes_due`. The
  connect loop waits a turn while it is set, `put_on_screen` asserts it
  is clear, and `say_due_goodbyes` drops only the UI if a client got
  there first. `replace_screen` carries the status note, so the restart's
  "Connecting" stays.
- **Log out refused.** A `logOut` error left the session in
  `CloseRequested` with nothing listening. `auth.logout_request` matches
  the answer, the error puts the shutdown phase back to `Running`, and the
  notification pump shows the failure.
- **Agent commands.** A multi-step command captures the screen epoch, and
  `check_step` stops the rest with `policy::account_changed()` once it
  moves. The steps were planned on the old account's tree.
- **`ChatUi` by topic.** It carries the topic too. In the same chat,
  another topic gets a fresh value. A pending reply carries over, as a
  reply carried into another chat already does, but the edit is dropped:
  it belonged to a message of the old topic.
- **Owner keys for caches.** `players.emoji_plays` is
  `OwnedBy<EmojiPlays>` (`src/ids.rs`), which resets when the account
  asking differs. `InlineVideos` records its owner and clears its players
  when the account on screen changes. Both were keyed by raw
  `(chat, message)`.
- **Sponsored views.** The ads painted carry the chat painted, and the
  next frame reports only the open chat's.
- **Guard.** `scripts/check-screen-state.py` now walks every field of a
  listed struct, and of the `src/ui` structs it holds, through multi-line
  types and `Box`, `Rc` and `RefCell`. Each field is keyed by its owner
  (`AccountKey`, `OfAccount`, `AccountChat`, `ConversationKey`,
  `OwnedBy`) or listed in `scripts/app-wide-fields.txt` with its reason
  (188 entries; `name *` covers a struct with no account ids inside). The
  chat-level check walks nested structs too, and counts a raw `i64`,
  `MessageKey` and `ChatMessage` as chat ids. Twelve new hits are listed
  in `view-state-not-chat.txt` with reasons; `MessageUi.rendered_sponsored`
  was a real one (above). `MessageUi.menu_ui` is walked now. Its
  `report_open` and `sticker_set_open` are bools for dialogs whose target
  is in the session (the report flow carries its chat) or a set id, so
  they pass. A flag holds no id, so no type check can see it.
- **On the old tree.** The new script on 6496ce4d6, with that tree's
  lists, fails with 469 lines. They include every field this round moved:
  `account.lifecycle`, `freeze_info_open`, `age_verify_open`,
  `age_verify_started`, `calls.window_closed_by_user`, `ended_at`,
  `sound_marks`, `group_call.title_dialog`, `settings_open`, `composer`,
  `window_closed_by_user`, `pin`, `stream.pending_join`,
  `stream.joining`, `screen_share.opening`, `players.emoji_plays` and
  `inline_videos`, plus the chat-level hits above. Findings 4 and 5 are
  outside what a field check can see. One is a reducer matching answers
  by purpose; the other is a continuation carrying an exemption comment.

### Phase 7 (the chat level, and answers nothing waited for)

Round 7 of the review, on `claude/sm-review7`. The account level held.
What was left sat at the chat level, or was a request whose failure or
late answer nothing handled.

- **Schedule picker and scheduled list.** The picker, its popup flag and
  the scheduled-messages dialog's flag moved from `ComposerUi` to
  `ChatUi`. A Reschedule picker named only a message id. Left open while
  another chat opened (a chat-list click, a notification), its Confirm
  rescheduled, or with "Send when online" sent, the new chat's scheduled
  message with that id. A chat change now drops the picker and closes the
  dialog. The session's scheduled list keeps each chat's rows: an answer
  replaces only the rows of the chat it was asked for, the dialog reads
  `Session::scheduled_in(open_chat)`, and a finished scheduling edit, a
  delete or an edit refresh matches chat and id.
- **Attached Stickers.** A failed `getAttachedStickerSets` now fails the
  box it opened (set 0) instead of leaving it spinning.
  `attached_request` ties the answer to the request the box waits for,
  and the answer opens a set only while that box is still open and
  loading. Before, a late answer could take over another set's box.
- **Saved Messages.** A failed first `loadSavedMessagesTopics` page
  clears `topics_requested`, so the pane no longer waits on it until
  restart. A failed page or failed global tags set `retry_when_online`.
  The pump holds off until `updateConnectionState` reaches ready, which
  stops the tags request going out again on every poll pass. A failure
  while already online (FLOOD_WAIT) waits for the next reconnect.
- **Member search.** The dialog shows the list for the query last asked
  (`MemberDialog::searched`), not the text being typed. Typing alone no
  longer shows "Loading members…" with nothing in flight.
  `supergroup_member_queries` records the latest first-page request and
  its query for each `(chat, filter)`. A search for another query is sent
  while the last is in flight, and only the latest request's answer or
  error fills the list.
- **Recent Actions poll.** When the chat window the poll started in
  closes, the loop's updates return `Gone`. The flag is now reset through
  `ScopedApp::main()`, as round 6 did for `screen_share.opening`. Before,
  Recent Actions for that chat never refreshed again on that screen.
- **Delete animation and info cards.** A dissolve is matched by chat and
  message (`vanish_progress`, `ghost_row`), and the dust and the gap
  remeasure only cover the shown chat's. `topic_info_open` and
  `thread_info_open` moved to `ChatUi`.
- **Guard.** `check-screen-state.py` looks through `Option`, `Vec`,
  `VecDeque`, map values and the payloads of src/ui enums at both levels.
  Under a keyed map the chat check stands down; under a value keyed by
  its account, the account check does. A `*` entry in
  `view-state-not-chat.txt` covers a field's whole subtree. The new walk
  found the Reschedule target (`ComposerUi.schedule_picker.target.Reschedule.0`,
  fixed above) and 20 other view fields. Each carries its own chat, or
  holds no chat's ids (times, sticker sets, effects); all 20 are listed
  with reasons. At the account level, ten app-wide entries became `*`
  (the sidebar slices, the live PiP, which carries its account, the
  background clients, which carry theirs, and others), and nine lines were
  added (the first run's two screens, the stage volume popover's fields,
  the share chooser's call ids).
- **Not done.** Pasted-image temp files: `OwnedFile` deletes on drop, but
  the file must outlive the tray until TDLib has uploaded it, so it does
  not fit in a few lines. Moving the files into a swept work folder would
  fit. No deterministic test for the event-log reset: it needs a chat
  window whose poll timer fires after the window closes.

### Phase 8 (per-window state held screen-wide, and errors that left no state)

Round 8 of the review, on `claude/sm-review8`. The account and screen
structure held. Four kinds of gap were left: per-window state kept
screen-wide, per-chat boxes outside `ChatUi`, per-account state behind
app-wide allowlist entries, and error arms that ended in no state.

- **Poll and checklist boxes.** `poll_dialog` and `checklist_dialog`
  moved from `ComposerUi` to `ChatUi`. A new poll or checklist took its
  chat at Create from the chat shown, so a notification click under the
  open box posted it to the other chat, with that chat's reply. The two
  `view-state-not-chat.txt` entries that excused them were false and
  are gone.
- **Picture-in-Picture from a chat window.** The link stays in the slot
  of the window whose viewer opened it. `take_video_pips` walks the
  viewer in place and every chat window slot, and the lock, the
  deletion check, one sound at a time, a logout and opening another PiP
  use it. Before, a chat window's PiP played over the lock screen and
  next to another sound. Back to the viewer runs in the window that
  opened it.
- **Settings pages on a switch.** The Settings farewell now leaves the
  page shown, as a page change does. The storage limit sliders are
  dropped on every arrival on and leave of Data and Storage and rebuilt
  from the account shown. Before, account B showed A's limits, and a
  drag wrote A's other limit into B's TDLib. The language box is made
  fresh on arrival, so it asks the account shown for its packs. An
  armed push-to-talk capture goes with the Calls page; left armed, it
  ate the next key typed anywhere and bound it. The reason on
  `preferences.index_sliders *` is corrected.
- **AI compose in chat windows.** The open box and the apply-in-place
  request carry the window they came from (`window`). The close check
  reads that window's chat, an apply-in-place answer rewrites that
  window's composer, and the button is decided for the composer it is
  drawn under. Before, a box opened in a chat window closed at the next
  main frame.
- **Saved Messages retry.** Opening Saved Messages again clears
  `retry_when_online`. A server error comes while online, so the wait
  for a reconnect could last the session.
- **Global search.** `open_search` keeps a pending debounce when search
  is already open on a query. It cleared it, which left the legs
  Searching with nothing sent.
- **Group Type link check.** A failed `checkChatUsername` records
  `check_failed` for its chat and text. The line says the check failed
  (`LinkProblem::CheckFailed`), and Save asks again. Before, the box said
  "Checking..." with nothing in flight and Save was refused.
- **Venue bot.** A failed lookup is `VenueBot::Failed`: no places in that
  picker, and the next picker asks again. It was latched `Missing` for
  the session.
- **Leftovers.** A fresh in-chat search starts with an empty field.
  `viewer.pending_seek` is keyed by chat and message, and the message
  menu's timecode reply checks the voice note's chat.
- **Not done.** The tests use `sound_starts` for the chat window's PiP.
  A test that locks with a chat window open does not settle in the
  test harness, PiP or not: `run_until_parked` after the lock never
  returns. That was not traced and needs a look on its own. It may be a
  test-only redraw loop or a real one.

### Phase 9 (work that resumed in the wrong window)

Round 9 of the review, on `claude/sm-review9`. Most findings were one
shape: work started in one window and finished with another window's
state in place, or ran for the main window only.

- **The guard reads the slot.** `check-window-scope.py` no longer keeps
  its own list of slot fields. It reads the `on_screen_fields!` block
  and `swap_slot` in `chat_window/slot.rs`, and fails when a declared
  view field is not swapped or a swap names a field that is not
  declared. With the real list it flagged five sites. Spellcheck was a
  bug (next item). The other four redraw every window on purpose and
  carry a `window-scope:` marker: `observe_self` in `app_slice.rs`, the
  call tick, the frame clock and the live story demo.
- **Spelling per composer.** `misspellings`, `checked_text` and the
  check task moved from `SpellUi` to `ComposerUi::spelling`
  (`ComposerSpelling`). The check resumes with `spawn_scoped`. An engine
  install or a recheck runs in every window (`in_every_window`). Before,
  a chat window's check underlined the main window's composer.
- **Inline bots in chat windows.** `progress_inline_mode` runs in every
  window. A window whose query was held back marks
  `ChatUi::inline_query_waiting`, and only that window sends once the
  bot resolves. Two windows share the session's one resolve slot, and
  the flag keeps them from re-sending in turns.
- **Pins.** A new pin refetches the pinned list of any chat on screen
  (`chat_is_shown`), not only the main window's open chat.
- **Instant View.** The page is part of `SessionView`. The answer goes
  to the window that asked (`PendingRequest::window`), and a window
  closed meanwhile gets nothing. The guard lists `messages.instant_view`.
- **Story search.** `StorySearchState::request` names the page request
  the search waits for. Only that answer or error fills it, so an old
  query's late page no longer fills a new search, and a new query sends
  at once instead of waiting for the old one.
- **Story statistics.** Reopening a story's statistics while its request
  is pending resets the state before returning, and the late answer
  fills the reopened panel.
- **Notification popups.** A popup the user is replying in is not
  reused for a new message from that chat. Hiding is derived on each
  event and tick: a live popup fades when nothing holds it (hover or any
  reply) and it is due. When the hovered popup goes away under the
  pointer, hovering clears and the rest start hiding, as in tdesktop
  (#28813). Every popup still shown rearms when a hold ends.
- **Passcode.** `run_passcode_job` runs its answer in the submitting
  window (`in_window_of`); if that window closed, only `busy` clears. A
  wrong passcode empties the field it was typed in. Unlocking queues a
  clear on every window's field (`clear_lock_fields`), applied at that
  window's next frame.
- **Autoscroll.** `AutoscrollLayer` builds the anchor mark and the left
  or right press that ends the mode from the window's own state. The
  main window and chat windows both use it. Before, a chat window
  autoscrolled with no mark and no way to stop it by clicking.
- **Not done.** The inline resolve slot is still one per session, so
  two windows asking two different bots at once resolve one after the
  other. Force reply, the pinned fetch replacing the whole list, the
  autoscroll lock, and popups left after a logout were not changed. The
  inline progress test covers `in_every_window` only, because the
  progress needs a live client and the demo has none.

### Phase 10 (area A: core answers and chat windows)

Round 10 of the review, area A, on `claude/sm-review10-a`. This covers
the core modules only (`src/state`, `src/connect`, `src/telegram`). All
seventeen findings held up when checked in code. Two shapes were fixed
at the root.

**Post-reducer work covers every shown chat, or carries the window that
asked.**

- **Jumps.** The date jump and the jump to the next mention or
  reaction no longer use `open_chat`. Their answers make a `ViewJump`
  holding the asking window (`PendingRequest::window`), the chat and
  the message. `ingest` runs it with `run_view_jump`, which uses the
  new `in_view_of(window, ..)`. That returns `None` when the window has
  closed. It also does nothing if the window has since moved to
  another chat. `date_jump_pending` now records its chat and is part
  of `SessionView`, so each window keeps its own.
- **Reply strips.** `reply_fetch_candidates` and
  `reply_story_candidates` go over `chats_shown()`. A reply in a chat
  window now fetches its original.

**One-slot answers match by request id or by key.** Each one keeps the
id of the request it is waiting for. An answer or error with any other
id is dropped. The previous rule was "the latest request of this
purpose", or "the box is open".

- Proxy pings: `proxy_ping_requests` per slot.
- Join requests: `join_request_latest`. The `has_purpose` dedupe is
  gone, so a new query is sent at once.
- Send gift: `SendGiftState::request`.
- Saved Messages tag search: `SavedTagSearch::request`. A second tag
  is sent at once. The first tag's page is dropped.
- Member "more" pages: `member_more_bases` maps each page to the query
  it extends.
- Boost slots: kept only while `boost_intent` names the chat. A boost
  pressed while slots are loading takes over that request.
- Folder invite links: `folder_invite_request` covers check and add.
  "Done" and errors from an earlier link no longer land in the dialog
  of a later one.
- Statistics forwards, for posts and stories: reopening while the
  statistics request is out asks for the forwards again, unless they
  are loaded or already requested.

**Folder edits read the current spec.** `folder_specs_stale` marks a
spec stale on `updateChatFolders`, on a folder's edit answer, and on
"add to folder". Code that edits a folder gets the spec through
`fresh_spec`. Removals queue per folder and wait while an edit is in
flight. They refetch a stale spec and send one edit per folder.
Before, a second removal wrote back a chat the first had just removed.
A failed spec fetch drops that folder's queued removals and shows
"Could not remove the chat from the folder". It does not ask again.

**Error arms end loading and roll back.**

- A failed public-link revoke clears the revoke and the public-chats
  fetch, and shows "Could not revoke the link".
- A failed similar-bots load removes its `Loading` entry.
- A topic mute and a poll vote carry a `RequestRollback`
  (`ForumTopicMute`, `PollVote`). `restore_optimistic` puts the
  previous value back, but only while the shown value is still the
  optimistic one. A later update is never overwritten.

**Sticker sets back off.** A failed `getStickerSet` marks
`failed_set_id`, and `selected_needs_load` skips it. Before, every
`ingest` sent the request again. Picking the set again clears the mark.

**Chat-list paging resumes.** A failed `loadChats` page that is not a
404 sets `chats_retry` or `archive_retry`. The wait starts at 1 s and
doubles up to 300 s. `chat_list_tick` in `poll_ticks` sends the page
again once the wait is over, and a page that goes through clears the
retry. A failed main-list page also starts the archive, so the archive
no longer waits for a main list that might never end. A send that
fails at once releases its request.

**Not done.** Message-story candidates still look at `open_chat` only.
The cost is a missing story preview in a chat window's reply strip.
Every fix has a deterministic driver or state test
(`connect/tests/chat_window_jumps.rs`,
`connect/tests/chat_list/folder_removals.rs`,
`connect/tests/refused_requests.rs`, and additions to the proxy,
invite, gift, saved, stats, story-insights, boost, group-type and
inline-members tests).

### Phase 10 (area B: the messaging UI's per-window upkeep and routing)

Round 10 of the review, area B, on `claude/sm-review10-b`. The account
level held. What was left was the window level: upkeep that only the
main window's frame ran, state every window shared because `swap_slot`
did not swap it, and chat opens that skipped the window routing or lost
their context on the way.

- **Per-window upkeep.** `per_window_upkeep`
  (`app_render/window_upkeep.rs`) holds every frame step that keeps one
  window's chat, composer and history in step: the thread advance, gone
  reply and edit targets, `sync_chat_ui`, the history holds, the inline
  return, the pending draft restore, the remote draft, the placeholder,
  the `:` suggestions, emoji words, the AI composer answer, the
  slow-mode, self-destruct and live-location ticks, and the pending link
  click. The main window runs it from `frame_upkeep` and a chat window
  from its render, with that window's state in place. Before, a chat
  window ran three of these. Its link clicks did nothing, a topic switch
  kept the old topic's text and reply and saved them as the new topic's
  draft, comments stayed on "Opening the thread", AI answers were dropped
  and the countdowns froze. `poll_every_window` runs the translation pump
  and Saved Messages' loads in every window from the poll loop, and the
  translation scan is kept by chat. The history holds read every slot,
  so a window's frame does not drop the main window's claims.
- **The guard.** `check-window-scope.py` checks `frame_upkeep` and
  `flush_notifications`. A call there to a method that touches
  per-window state (the same transitive test the async rule uses) fails
  unless it carries `// main-only: <why>`. The steps left there say why:
  notification delivery, deep links, stories, mini apps, the main
  window's boxes, the AI compose box (it carries its window), the one
  composer link preview the session keeps, and the reply target fetch.
- **One router.** `route_chat_open` (`chat_window/routing.rs`) decides
  where a `ChatOpen` goes: the chat's own window, the main window
  (deferred, from a chat window), or the window in place when it already
  shows the chat. The request carries what the open lands on: the
  message (`open_at`, which replaces `ChatListUi::open_at`), a reply
  aimed at the chat from another one, a forum topic, and a comment
  thread resolved in a chat window, which moves into the main window's
  view with its discussion group. `select_chat_with`, the search and
  header opens (`select_search_chat`, `select_search_message`), "Go to
  message", "Reply in Another Chat" and the topic chip all go through
  it. Before, a "Forwarded from" or external reply header in a chat
  window re-pointed that window at another chat, and in the main window
  it opened a chat that had its own window a second time.
- **Per-window motion.** `FrameUi::surfaces` (bars, corner buttons and
  the corner lift, toasts) and `FrameUi::motion` (history reveal,
  selection fades) are swapped with the slot. With one corner-lift
  `Glide`, a window scrolled up and a window at the bottom flipped its
  target at every draw and kept each other on a 60 fps tick. Not
  measured in a live build: the tests do not animate
  (`surface_reveal::animate` is off under `cfg(test)`), so the test
  asserts that each window keeps its own glide.
- **Dialog scope.** `QuillShell::sync_kit_dialogs_in` puts its host's
  state in place for the flags and the close, and the kit dialog's
  builder does the same when the kit layer builds it later. Before, a
  pointer move into a chat window left its scope in place, and the main
  shell closed its own dialog (whose flag lives in the main window's
  `MessageUi`) until the next main-window input.
- **Smaller items.**
  - Escape in a chat window runs `dismiss_window_layer`, which closes
    the topmost of the layers a chat window draws (`WINDOW_LAYERS`), not
    the main window's pages and panels.
  - The driver's debounced draft: a note for another slot sends the
    waiting one first, the main window's chat switch and a thread switch
    send it instead of dropping it, and a send cancels only its own
    chat's save (`cancel_pending_draft_of`). The driver still keeps one
    waiting save; this keeps a chat window's from being lost.
  - A forward's answer shows in the window that shows its source chat,
    and clears only that forward's selection, picker and bar.
  - The forum peek and "topic column shown" are per window
    (`HistoryUi`).
  - Silent and effect options ride with a held send (`SendOnce`, kept
    by the paid-message confirm and the photo-edit hold).
  - The text-limit count lands in the memo of the window that asked
    (`spawn_scoped`).
- **Skipped.** The folder spec cache (finding 5) and the reply target
  fetch for chat windows are area A's. Force reply still arms the main
  window's composer. The comment-thread handoff has no UI test: the demo
  session has no live client, so `advance_thread` does nothing there.
  The held send's options are tested through `with_send_once`, not
  through a live paid-message confirm.

### Phase 10 (area C)

Round 10 of the review, area C, on `claude/sm-review10-c`. The
findings were eight shapes, each a rule from earlier phases that a few
sites missed.

- **A box whose open flag lives in the Session closes in a farewell.**
  Nineteen kit dialogs read `session()` in their predicate and had no
  closer in their type's farewell: collectible info, gifts, the wallet
  notice, the language link, the boost box, masks, emoji sets, archive
  settings, RTMP start, the location picker, chat appearance, the
  sharing request, giveaway info, the replies block, the call link
  confirm and the Earn boxes. Each farewell now calls its closer.
  Guard: `scripts/session_dialogs.py` (run by `check-screen-state.py`)
  parses every `DialogSpec` predicate, follows `self.`/`app.` helpers
  three calls deep, and fails a predicate that reads the session unless
  `scripts/session-dialogs.txt` names its closer or says why none is
  needed. The list has 48 entries.
- **A per-item panel resets when its item changes.** Moving to another
  story closes its Report and viewers panels and clears their session
  flows and the last management error (`close_story_item_panels`). The
  report answer is matched by poster and story (`StoryReportFlow::is_for`).
  Any open story panel holds playback. A video chat's rename, Settings,
  invite, share boxes and pin belong to the call they were opened for
  (`GroupCallScreen::boxes_call`). Another call, or none, closes them,
  and so does a share box whose links failed, since it is never drawn.
- **The lock reaches every surface.** Locking closes the media viewer in
  every window, not only the main one, and closes the mini app window,
  as tdesktop's `closeAdditionalWindows` does. A Touch ID answer runs
  in the window that asked. If that window closed meanwhile, `busy`
  still clears and a success unlocks in the main window. Before, a
  stuck `busy` refused every unlock until restart. The wait moved to
  `passcode/system_unlock_wait.rs`, which keeps `passcode/mod.rs` under
  1000 lines.
- **A surface opened from a chat window is drawn there, or carries its
  window.**
  - The story viewer and the profile modal are drawn only by the main
    window. Opened from a chat window, they hand off to it
    (`hand_to_main_window`): the open runs at the end of the update, as
    the main window, which is brought forward. We kept that rather than
    drawing them in chat windows, because both hold screen-wide state
    (playback, the modal stack) that a second copy would split.
  - Image decodes resume with `run_off_ui_scoped_then`, so the decoded
    image lands in its own window.
  - The GIF decode and the parked play intent (`PlayTarget::Gif` and
    `Viewer`) carry their window. A target whose window closed is
    dropped. The composer's text-limit check resumes in its own window.
  - A mini app's switch-inline text goes to the chat the app was opened
    from (`WebAppLaunch::chat_id`). If a chat window shows that chat,
    the text goes there (`hand_to_chat_window`). Otherwise the main
    window opens the chat first. Before, it landed in whatever the main
    window had open.
  - Guard: `check-window-scope.py` treats the bodies of
    `run_off_ui_then` and `spawn_screen` like other continuations, so a
    new one needs a `window-scope:` marker or a scoped helper. Six
    sites that stay screen-wide on purpose carry the marker. "Reached
    from slot input" would need a call graph, so the guard flags every
    such body rather than tracing callers.
- **Chat windows count as windows.** A chat window mounts the input
  probe, so typing there counts as activity for auto-lock and presence.
  It also applies the capture block itself, so a chat window showing a
  secret chat is hidden from screen capture.
- **An Entity drawn in several windows gets one per window.** The player
  bar's progress slider is one per window (`PlayerBarState::sliders`),
  made at that window's first draw of the track. Closed windows' sliders
  are pruned. Before, one slider kept the last window's geometry, and a
  press in the other window sought to the wrong spot.
- **An error arm ends loading.**
  - A refused `setInactiveSessionTtl` rolls back to the value it replaced
    (`SetInactiveSessionTtl { previous }`), not to `None`, which the
    picker drew as loading with nothing to refetch it.
  - A failed `getChatMember` shows "Couldn't load admin rights" in the
    rights editor and on Save, not "Loading rights…". Reopening the
    editor asks again.
  - A failed username check clears its "Checking…".
  - A refetch that fails after a sticker set edit leaves the box as it
    was. Only a failed load shows "Couldn't load".
  - Settings > Calls fetches its own privacy rules and auto-download
    presets when it opens, so it no longer depends on another page
    having loaded them.
- **Answers are matched by request, not by purpose.**
  - The username check's verdict and error: `username_check_pending`
    holds `(RequestId, text)`.
  - New Contact: `new_contact_outcome` is keyed by the import, and only
    the box that sent it (`AddContactDialog::sent_request`) applies it.
  - The gift craft and resale buy (`craft_request`, `ResaleBuy::request`).
  - The prepaid giveaway launch (`GiveawayLaunch::Launching { request }`).
    The boost cache is still dropped on any success.
  - Send-gift (`SendGiftState::sending_request`).
  - A mini app's replaced popup: the helper shows one popup at a time, so
    when a newer one (the close confirm) replaces another (a write-access
    or phone prompt), the replaced one is answered as dismissed
    (`answer_replaced_popups`). Before, its request never answered, and
    every later one got "busy".
- **Background accounts.**
  - A background account's Star reactions commit when their Undo window
    ends (`paid_reaction_tick` runs before the on-screen return). Before,
    they were held until you came back, then sent at once with no Undo.
    tdesktop's other sessions keep their send timers.
  - A client coming on screen follows the interface language saved
    since it went to the background (`adopt_saved_language`).
- **A saved track has no chat neighbours.** A profile's saved-music
  track does not play on into the open chat's songs, and the bar's
  Previous and Next are off for it. Playing through the saved list
  would be a new feature.
- **Not done, or not tested.**
  - Settings > Calls (1f) has no test, because the fetch needs a live
    driver.
  - Chat-window input activity (1g) has no test, because
    `LAST_INPUT_MS` is a process-global static.
  - Closing the mini app on lock was not tested, because the window
    needs the helper process.
  - The New Contact box's request filter is covered by the driver tests
    only. The demo has no import.
  - The Touch ID test covers the closed-window path with a refusal. A
    success would set the process-global unlock key.
  - `seek_hover` on the player bar is still screen-wide. It is only a
    hover preview, and the next move over a bar resets it.
  - A mini app's switch-inline still waits for the main window's next
    frame when that window is hidden.

### Phase 11 (area A)

Round 11 of the review, area A, on `claude/sm-review11-a`. Core modules
only. All six findings held up in code. Three of them (1, 2 and 5) had
one cause: update paths read `open_chat` or the main window's thread or
Saved view as "what is on screen". During `ingest` the view in place is
the main window's, so a chat window got nothing.

**One notion of every view on screen** (`src/state/session_views.rs`).

- `views_shown()` reads every window's view where it is kept: the one
  in place and each one in `WindowViews`. Each `ShownView` says which
  window it belongs to and carries its chat, topic, thread and Saved
  views. `chat_drawn(chat)` asks whether any window's pane draws a
  chat, counting a comment thread's chats. `chat_is_shown` and
  `chats_shown` are now built on it, and `WindowViews::shows_chat` /
  `shows_topic` are gone.
- `in_every_view(f)` runs an edit once with each view in place, for the
  rows a view keeps itself (thread, Saved sublist and tag filter, the
  gallery, the search jump). Requests made inside are stamped with that
  view. Inside a closed window's throwaway view it runs once, because the
  main window's view is held outside the session then.
- Designs considered. (a) A `&mut` projection over every view's layers:
  no swaps, but `thread_upsert` needs the request table as well as the
  thread, so it would need a second path anyway. (b) Keying threads and
  Saved views by window in the session: a larger move of state that
  `swap_view` already owns. (c) Fixing each call site with its own loop,
  as `maybe_download_shown_thumbs` does: that is the shape that kept
  missing sites. We picked one read projection plus `in_every_view`.
  A swap is nine `mem::swap`s, and with no chat window open
  `in_every_view` runs `f` once with no swap.

What goes through it:

- **Redraw grading (1).** `shows_chat`, `shows_user`, `group_need` and
  `interaction_need` read `views_shown`. New messages, typing, read ticks,
  counters, drafts and positions of a chat shown only in a chat window
  grade `Now`, and so does its peer's status. Before, they graded
  `ChatList`, which redraws only the sidebar.
- **Live thread and Saved updates (2).** New replies (`thread_upsert`),
  the own reply's server id or failure (`replace_view_copy`), edits,
  reactions and content changes (`edit_loaded_message` now edits every
  view's thread and Saved rows), reply counters (`sync_thread_reply_info`),
  deletions (thread, Saved views, gallery, search jump) and a deleted
  Saved sublist all reach every window. `prune_reply_targets` keeps the
  targets of every window's thread.
- **Tap effects (5).** `accept_emoji_effect` checks `chat_is_shown`.
- **Same shape, found in the sweep.** Reaction notifications now skip a
  chat shown in the active chat window (`shown_in_active_window`, as
  message notifications already did). The custom emoji the screen
  shows, and the stories that story messages point at, come from
  `shown_messages()` (every shown chat's history and every view's
  layered rows). Full-media auto-download runs for each chat window
  (`maybe_download_shown_chat_media`). The history pruner's "shown"
  checks read `views_shown`.
- **Guard.** `scripts/view_reads.py`, run by `check-window-scope.py`,
  reads the update, answer and redraw files of `src/state` (`redraw.rs`,
  `session_apply*.rs`, `session_updates.rs`, `updates_sync.rs`,
  `session_notifications.rs`, `session_forward.rs`, and each domain's
  `apply.rs`, `error.rs` and `interaction.rs`). It fails on a read of
  `self.open_chat` / `session.open_chat`, `.threads.thread` or the Saved
  views, unless the line or one of the three above carries
  `// view-in-place: <why>`. Three reads carry it: the sponsored answer
  (applied in the asking view), and the two notification inputs (the
  main window's chat while the app is active). On 7240cf719 it fails
  with 14 lines, among them `interaction.rs:23` (finding 5), the four
  redraw reads behind finding 1, and the thread and Saved reads behind
  finding 2 in `messages/apply.rs` and `edit_loaded_message`. A read split
  over two lines (`session`, then `.open_chat`) counts. It cannot see a
  read inside a helper defined in another file, such as `thread_accepts`
  in `session_thread.rs`.

**Every copy of a message (3).** `loaded_message(chat, message)` finds a
row in the chat's history, a loaded forum topic, or any window's thread
or Saved views. The poll vote and retract look the row up there, and the
optimistic flip, its immediate undo and the `PollVote` rollback go
through `set_poll_marks`, which edits every copy via
`edit_loaded_message`. `apply_update_poll` updates every copy the same
way. Thread pages, live thread replies, Saved pages and a live topic row
outside the main window now index their polls, so `updatePoll` finds
them. `apply_update_poll` returns the number of rows updated in any copy.

**Answers matched to the query that asked (4, 6).**

- Recent actions: `GroupsState::event_log_waits` holds the page and poll
  each chat's log waits for (`EventLogWait`). A refresh forgets both and
  is sent at once, even while an older page is out. A page, poll or
  error with another id changes nothing. `has_event_log_in_flight` is
  gone. A failed poll now clears its wait, so the next tick polls again.
- Recent calls: `recent_calls_loading` is now
  `recent_calls_request: Option<RequestId>`, and `recent_calls_loading()`
  is derived from it. Reopening the tab clears it and asks for the first
  page again. "Clear all"'s `ok` forgets the page in flight. Only the
  awaited page or error lands.

**Tests.** Every fix has a deterministic test of what the user sees:
`state/tests/redraw.rs` (a chat window's typing, message, read tick and
status grade `Now`, also with the window's view in place, and a chat
window's thread counts), `connect/tests/chat_window_views.rs` (a live
reply, the own reply's server id, edit and delete in a chat window's
thread, a vote and a refused vote there, a vote in a forum topic),
`connect/tests/latest_query_wins.rs` (search typed during a load,
filters saved during an older page, a poll from before a refresh, a
Calls reopen, Clear all during a page),
`state/tests/emoji_interaction.rs`, `notify/tests.rs`, `emoji/tests.rs`,
`state/tests/message_story.rs` and `connect/tests/update_batch.rs`. With
`in_every_view` and `loaded_message` reduced to the view in place, the
five chat-window thread tests fail.

**Not done.** File-progress grading still treats chat windows' automatic
downloads as off-screen (`Later`), because only the main window reports
the rows it shows. A Saved Messages sublist takes no live new messages,
in any window, as before. `drop_thread_pages_past` matches pages by
thread, so two windows showing the same thread mark each other's pages
stale.

### Phase 11 (area B: keys, actions, carried opens and per-window layers)

Round 11 of the review, area B, on `claude/sm-review11-b`. Findings 1
and 14 belong to area A. The rest come down to three shapes: input
bound or checked for the main window only, chat opens that skipped the
router or left what they carried behind, and layer state that every
window shared.

- **Keys per window.** The key interceptor moved to `window_keys.rs`.
  `key_window` tells the main window, a chat window and the app's other
  windows (Settings, notification popups, call windows) apart, and keys
  in an "other" window are left to that window. `context_menu_open` and
  `dismiss_context_menus` count the main window's menus (the chat-list
  row, archive and story menus, a playlist's, a folder tab's) only while
  the main window's state is in place. A chat window's Escape and arrow
  keys no longer go to a menu it does not draw. The lock dismisses each
  window's menus inside `in_every_window`.
- **Actions on every root.** `window_actions.rs` binds the composer,
  history, selection, in-chat search and viewer actions, plus Copy and
  Paste in the capture phase. The main root and every chat window's root
  bind the same list. `chat_window_app_actions` gives a chat window the
  app's own commands. Lock (Cmd/Ctrl+L), the theme toggle and Help run as
  the main window. Quit, Settings and Shortcuts go through
  `hand_to_main_window`. Minimize, Zoom and Full Screen act on the chat
  window itself. Before this, all of them did nothing while a chat window
  had the focus. The chat list's keys (pinned chats, folders, the
  list's arrows) and the story viewer's keys stay main-only, because
  only the main window draws those surfaces.
- **Opens carry their state.** `ChatOpen` gained `ChatOpenCarry`:
  messages to forward, dropped files, composer text and the inline
  results button's round trip. `apply_open_carry` applies them in the
  window that shows the chat, from `land_chat_open` and from
  `select_chat_request`. Composer text that arrives without a window at
  hand waits in `ChatUi::compose_after_open` for that window's
  `per_window_upkeep`. These now route with what they carry:
  - the Forward box's "continue in chat": the source window keeps no
    forward, selection or comment;
  - a forward drag or a file drop on a chat row, and the drop-topic
    chooser;
  - the thread back button, which opens the origin chat at the post;
  - the pull-to-next jump. If the jump went to another window,
    `select_chat_request` returns false, and the pull machine resets
    instead of staying in its jump;
  - the inline query typed back into its chat;
  - a deep link for a chat that has a window of its own. The jump, the
    `/start` prefill, a link's draft and the inline start are applied in
    that window;
  - a story area's message link.
- **The routing guard.** `check-window-scope.py` fails a `QuillApp`
  method that opens a chat through the driver (`select_chat`,
  `select_chat_open`, `select_search_chat_open`, `select_search_message`
  or `switch_to_thread_chat`) without calling `route_chat_open`. An open
  that has to stay in place says why with `// routed: <why>`. The only
  one is a basic group replaced in place by its supergroup upgrade.
- **Per-window layers.** The subsection tabs' strip and column scroll
  and their revealed key moved from `FoldersUi` to `HistoryUi`
  (`SubsectionTabsScroll`), so they are swapped per window. Every
  window's history scroller has its own observer
  (`observe_history_scroll`), which puts that window's state in place
  before it records the scroll position, preloads or moves the date
  pill. Before, a chat window's scrolls were noted only by its renders.
  The GIF panel is read from the window's own media panel. The session's
  one GIF gate (`gifs.open`) follows whether any window shows the GIFs
  tab (`sync_gif_downloads`). The sticker panel, which is session-wide,
  counts only in the main window, so Escape in a chat window no longer
  closes it.
- **Smaller fixes.**
  - Jumps are deduped by `(jump_serial, message)`, so going to the same
    message again after scrolling away lands again.
  - Each Saved Messages sublist and tag view has its own history key: a
    negative third part hashed from the view.
  - A reply-keyboard button always sends a new message, even while
    editing, with a forward bar or with a tray.

Tests: `chat_window/routing_carry_tests.rs` (forward, continue-in-chat,
composer text, `/start` link, files), `chat_window/window_input_tests.rs`
(Lock from a chat window, main menus vs a chat window's keys, the GIF
panel per window, the tab strip per window, Saved view keys),
`landing_tests.rs` (the same jump twice) and `composer/bars_tests.rs`
(a keyboard button while editing).

Left open:
- The thread back button and the pull-to-next reset have no test,
  because the demo has no live comment thread and no pull session.
- The per-window scroll observer has no test either. Its effect is a
  saved position and a date pill, which the harness does not scroll.
- A deep link's thread or media timestamp, for a chat with its own
  window, still runs in the main window (`finish_typed_link`).
- A link's draft replaces the routed window's composer text. In the
  main window it is appended to the text already there.
- "All Messages" from the drop-topic chooser, for a forum with its own
  window, leaves that window's open topic as it is.

### Phase 11 (area C, part 1)

Round 11 of the review, area C, findings 1 and 3 to 20, on
`claude/sm-review11-c1`. Finding 2 (Lock Quill bound only on the main
root) is the action bindings' fixer's. The findings were four shapes.

- **The lock has one owner.** `passcode/lock_gate.rs` holds both
  halves. `close_for_lock` stops or closes what the lock screen does
  not cover: a recording (cancelled, so a round video at its limit is
  not sent), every window's media viewer, the story viewer, the mini
  app window, context menus, notification windows, every video PiP and
  the device-linking camera scanner. `refused_while_locked` stops new
  ones: recordings, a mini app whose URL lands under the lock (TDLib's
  launch is closed) and links. A launch or OS link is held until the
  unlock, as tdesktop holds start links (`checkStartUrls`): the poll
  does not resolve it, and a link resolved just before the lock opens
  its chat or box at the first frame after the unlock
  (`run_resolved_deep_link`). Call windows stay up, as tdesktop leaves
  its call panel, so a call can still be answered.
  Guard: `scripts/lock_surfaces.py` (run by `check-screen-state.py`).
  Every production function in `src/ui` that opens an OS window
  (`.open_window(`), starts a helper process (`Command::new(`) or
  starts a recording (`.begin_capture_start(`) must call
  `refused_while_locked()` or be listed in `scripts/lock-surfaces.txt`
  with the closer `close_for_lock` calls, or `-` and the reason the
  lock need not reach it. `refuses` pins the link paths, which no
  pattern finds. The list has 14 entries.
- **What is open over a story belongs to that story.**
  `leave_story_item` runs on every step, every open over an open
  viewer and every release (a deleted story, the lock): it ends the
  reply row and its recording, closes the item's panels and replaces
  `StoryViewerUi` with a fresh one (close friends' edit and save
  survive, being the account's). Before, the cover and privacy
  editors survived Prev and Next and saved the story they were opened
  on, and an open over the viewer kept the old reply and share panel.
  A story reply whose story went is cancelled, never sent to the chat
  under the viewer (`story_reply_send_target`). The ⋯ menu pauses the
  story while open (`menu_open`) and each entry carries the story it
  was drawn for, so a pick after the story moved on does nothing.
  Edit and Repost pause the viewer under the composer. A finished
  story page op ("Saved", "Failed") is settled when its story or page
  goes (`settle_story_page_op`); one in flight keeps its line. A live
  story's pending Stars commit from the poll, for a background account
  too, and `next_deadline` wakes for them, so Stars tapped before the
  story went to Picture-in-Picture are sent, not dropped.
- **An answer acts on the owner it was asked for.**
  - The header's Mute box keeps the chat it was opened on
    (`mute_chat`, no fallback to the open chat). The auto-delete picker
    is now `ttl_picker: Option<ChatId>`, the chat it was opened for, so
    one opened from a profile's menu no longer sets the open chat's
    timer.
  - The call rating draft carries its call (`RatingDetail::call_id`).
    Another call's summary neither shows it nor submits it, and Skip
    drops it.
  - The screen chooser and the screen access box belong to their call.
    The video chat's id is recorded at open (`ScreenShareUi::group_call`).
    When that call ends or another takes its place they close
    (`sync_screen_share_call`), and a pick that lands after shares into
    nothing. Closing the video chat window closes its Settings and
    rename boxes.
  - The free-transcription quota last seen is per account
    (`OwnedBy`), so a switch toasts nothing.
  - A GIF file plays in the window that pressed it (`PlayingGif`
    carries the window), and that window's close stops it.
- **One copy per screen or per window, not one shared.**
  - The Privacy page's password check has its own field
    (`TwoStepUi::check_password`). It shared the 2FA box's field, which
    painted in two places, showed the same text twice and redrew every
    frame (`lock-field-per-window.md`).
  - "Send messages with" reaches every window's composer, not only the
    main one's.
  - The push-to-talk release slider moved from the app-wide
    `GroupCallUi` to the screen-scoped `SettingsUi`, so each account's
    Settings > Calls seeds its own.
  - A switch during video full screen gives the window back
    (`ViewerUi`'s farewell).
- **Nothing waits on the main window's frame.**
  - The call windows' sync ran only in the main window's upkeep, which
    a hidden or minimized main window never draws. It now runs from the
    notification pass (every poll) and from a chat window's draw
    (`sync_call_windows`), and the call screen's linger timer syncs when
    it fires. An incoming call is shown as an OS notification unless
    the main window or the call window is active (`call_ring_seen`),
    so a call is not silent while you work in a chat window.
  - View Profile, Statistics and Monetization from a chat window hand
    off to the main window, which draws the info column. "View all
    photos" is left out of a chat window's viewer menu: no chat window
    draws the shared media gallery.
- **Findings 21 to 40.** None shares a root cause with these fixes. The
  call window id leftover in 40 was not touched.
- **Not done, or not tested.**
  - Joining a video chat from a chat window: the start box and the
    RTMP toasts still wait for the main window (part of 15).
  - No behaviour test for "Send messages with" in a chat window: the
    kit's input has no getter for the mode, and a test would have to
    press Enter in the window's composer.
  - No in-app test for the per-account transcription toast, the PTT
    slider or the full-screen farewell: each needs a second account or
    an OS full-screen window. `OwnedBy` and the screen-state guard
    cover the first two.
  - The OS notification for a ringing call and the call sync from a
    chat window were not run with a real hidden main window.
  - The mini app refusal under the lock was not tested; the window
    needs the helper process.
  - The 2FA field split is covered by a test that checks the password
    without touching the box's field. The redraw loop itself was not
    measured.

### Phase 11 (area C, part 2)

Round 11 of the review, area C, findings 21 to 40, on
`claude/sm-review11-c2`. Part 1 (`claude/sm-review11-c1`) took the
lock, the chat-window entry points, the GIF, video chat boxes and the
PTT slider; nothing here overlaps it.

- **An answer lands where its request came from.**
  - Off-UI work (`run_off_ui_then`) now resumes with `spawn_screen`, so
    a result never finishes on another account's screen. The profile
    photo crop resumes with `run_off_ui_scoped_then`, in its window.
  - The resale gift buy goes to the recipient the Send Gift box picked
    (`start_gift_resale_buy(name, recipient, for_craft)`), not to the
    chat open behind it. Opening the market copies the recipient only
    while the send box is open.
  - A 3-D Secure step closes the checkout only when it came from the
    checkout on screen (`PaymentVerification::closes_checkout`). A
    receipt fetch no longer overwrites the open checkout's invoice.
  - Mini app launches keep a list of pending contexts keyed by request
    (`WebAppPending::request`), so two overlapping launches each open
    with their own bot and source. A launch whose send fails drops its
    entry, and a new mini app window closes the old one first.
  - New Contact ignores a second Create while its import is in flight
    and clears the request after any answer.
  - The bot-create poll waits until it gets an answer or the box closes,
    instead of giving up after 200 tries.
- **A member change refetches the lists that show it.** A
  `setChatMemberStatus` or tag success marks the chat's member list
  stale and drops its administrator list and the user's cached rights;
  `updateChatMember` marks it stale too (`mark_member_list_stale`). The
  poll refetches the administrators of a stale chat with none cached.
- **Per-window state moved to the slot; screen state moved off it.**
  - The topic strip's scroll and reveal moved from `FolderUi` to
    `HistoryUi`, so each window keeps its own.
  - The sticker and masks manager moved from `PickerUi` (per window) to
    `SettingsUi`, because the session has one manager and its close runs
    as the main window.
  - A set's video pick carries its window. Its editor opens there
    (`hand_to_chat_window`), and the pick is dropped if the window
    closed. Closing the viewed sticker set closes its box in every
    window.
  - The location picker remembers the window that opened it and closes
    with that window.
- **A box or line belongs to what it was opened for.**
  - New Topic from a chat window reopens the manage box for that
    window's forum instead of editing the main window's.
  - The revoked-links disclosure is keyed by chat. The Promote picker
    shows Recent until a search is actually sent.
  - Closing the profile modal gives the info column back to what it
    showed before. Opening another chat drops the modal and its saved
    column.
  - The Direct Messages price box takes the price when it arrives,
    instead of keeping the 0 it was opened with.
  - The 1:1 call window's close reads the call it shows now. Before, a
    window reused for a second call kept the first call's id, so
    closing it opened it again at the next sync.
- **Playback and the viewer.** A saved track's title opens its owner's
  playlist, and its row counts as playing from any chat. Closing the
  viewer drops a pending seek. A main-window viewer stop drops only the
  main window's play intent. Closing the forward picker drops its
  timestamp.
- **Lines that outlive their box.** Reopening 2FA clears its last error
  (unless a request is in flight), the Account box clears its error,
  and the emoji sets box clears "Could not update emoji packs". A
  supergroup's Appearance box shows "Loading your emoji packs" only
  until the list answers (`EmojiPanel::installed_loaded`), then "You
  have no emoji packs".
- **Animated userpics follow the account.** `UserpicVideos::own` drops
  the failed list and the slots when the account on screen changes, so
  one account's failures do not hide another's clips.
- **Not done, or not tested.**
  - The bot-create poll and the Promote picker's Recent fallback have no
    tests; both need a live driver.
  - A 3-D Secure page from a checkout that was replaced still opens
    (only the checkout stays); tdesktop opens it too.
  - Mini apps still have one `open_result` slot; a second answer before
    the first is drawn replaces it.

### Phase 12 (area C, part 1)

Round 12 of the review, area C, findings 1 to 30, the lock findings, and
the two part-2 findings that need a dialog host per window (47's other
half, 53), on `claude/sm-review12-c1`.

- **The lock is a render state** (docs/decisions/lock-is-a-render-state.md).
  - A kit root plugin, `passcode::lock_layer::LockLayer`, runs last in
    every window. While locked it drops the built surface, the kit's
    dialog, sheet, popover and tooltip layers included, and draws the lock
    screen alone, or nothing for a window type it does not name. Call
    windows and notification windows stay as they are.
  - While locked, the main window and each chat window build only their
    lock screen (`locked_root`, `render_locked`), so no deferred overlay
    exists to paint over it.
  - The shell does not sync dialogs while locked, so a box neither opens,
    closes nor moves under the lock. It is back after the unlock, in its
    own window.
  - `set_locked` is the one writer of the flag. `refused_while_locked` is
    the one gate for what is not drawn: app key interceptors
    (`intercept_keys_unlocked`), account switches, OS opens, the Dock and
    the window menu.
  - `close_for_lock` now also turns off the profile photo camera and
    disarms both key captures.
- **The agent socket pauses while locked.** Every tool and the unread
  resource return `LOCKED_MESSAGE` (`agent::access::paused_by_lock`, a
  process flag that only a real lock sets).
- **Nothing switches account behind the lock.** `switch_account` refuses.
  A notification click from another account only shows the window, and
  a stale Dock account item only raises it. A ringing call's Answer
  still switches (`switch_account_even_locked`).
- **`lock_surfaces.py` covers all of `src/`.** It checks windows, helper
  processes, captures, app-wide key hooks and action handlers. A gate
  must be tested in a condition. Entries are keyed by `path::fn`, and a
  `via:` entry names a function that gates. The guard also checks that
  `set_locked` is the only writer and that `LockLayer` is registered.
- **Each window shows its own kit dialog** (`chat_window/dialog_hosts.rs`).
  - A kind is owned by the window that showed it. One with no owner opens
    in the window of the last input.
  - A chat window's boxes close with it, each with its spec's close,
    instead of moving to the main window. Every spec now has a close,
    except the call swap and agent request prompts, which must be
    answered.
  - New Topic or Edit Topic from a chat window claims the manage box for
    that window (`claim_dialog`), so the main window's box does not turn
    into another forum's editor.
- **What Settings opens belongs to its page.** Closing Settings leaves the
  page on screen, Account when none was chosen (`shown_settings_page`).
  Account drops Log out?, and Privacy drops its rule editor, exceptions
  list, pickers and Unblock confirmation (`close_privacy_layers`).
  Settings is the main window's: a chat window's ⋮ actions leave it open,
  and Agent access from a chat window hands off to the main window.
- **The 2FA status card has its own password field**
  (`TwoStepUi::status_check_password`), so one field is not drawn twice in
  a frame.
- **A box keeps the chat it was opened for.** Add to folder carries
  `FolderUi::menu_chat`. Profile › Auto-Delete opens the picker for the
  profile's chat. The Business start page takes a sticker only from the
  window that shows its box.
- **Polls wait for an answer or their screen.** The Chatbots poll and the
  quick reply poll no longer stop after a count. The error lines that
  outlived their box are cleared where the box opens.
- **App prefs live outside the accounts.** The appearance, chat,
  spellcheck, translation and settings-search prefs, the recent hashtags
  and the custom themes moved from `accounts/primary/`, which removing
  the first account deleted, to `<app root>/app`
  (`settings::app_prefs_paths`). The move runs once, on first use. Each
  item moves on its own, the app folder wins a conflict, and a marker is
  written last.
- **Calls and stories.**
  - A call window's target is decided by what happens now: an active
    call, then a ringing conference invite, then an ended call's summary.
  - Boxes drawn in a call window take Esc there (`CALL_WINDOW_LAYERS`).
  - The story viewer plays only while the main window is active.
  - Story edit, cover, privacy and page ops match answers by request id.
  - Shares that are not the composer's message use
    `send_options_to(chat)`.
- **The viewer acts on its item's chat.**
  - Show in Chat and View all follow the item's chat.
  - A viewer emptied by deletes closes through `close_media_viewer`.
  - An account switch clears the OS media session.
  - A chat window's viewer hold and parked intents are that window's.
- **Not done, or not tested.**
  - Finding 10 (recent hashtags per account) needs `composer_suggest.rs`,
    which belongs to another area.
  - Finding 6 was not confirmed: the link inbox retries while a link is
    pending. The unlock pokes `wake::LINKS` anyway.
  - The camera's close on lock, the notification-click and Dock
    refusals, the Settings polls, the error lines and the media session
    clear have no tests: they need ffmpeg, a second signed-in account or
    a live driver.

### Phase 12 (area A)

Round 12 of the review, area A, on `claude/sm-review12-a`. Both material
findings held up in code.

**Your own level ran only on updates (1).** `pump_call_audio` is the
only reader of your microphone level and the only caller of the
speaking tracker. It ran from `run_post_ingest_pumps`, so it moved only
when a TDLib envelope arrived. In a call where nobody else speaks, the
halo froze at the peak of the last gap. Your row kept its speaking mark
after you stopped. TDLib heard "speaking" once per envelope, not every
500 ms.

- `pump_call_audio(now)` now runs in `request_tick`, as
  `pump_call_engine_reports` does. It no longer runs on ingest. The poll
  ticks run after every ingest pass, so a mute in an update still closes
  the tap in that pass.
- The capture writes the level cell without waking the poll loop. While
  the tap is open and the microphone runs (or you are still marked
  speaking), `next_deadline` wakes the loop one level window (92 ms)
  after the last read: `ConnectDriver::call_audio_deadline`. The panel
  redraws at 30 fps during a call (unless power saving stills it), so the
  level is not a redraw.
  `request_tick` asks for one only when your speaking state changes.
- A poll with no new window calls `SpeakingTracker::idle`. Speaking runs
  out after tdesktop's 1.5 s, "stopped" is sent once, and the level drops
  to 0 once no window has come for 1.5 s. Before, a stopped capture or a
  rejoin left `speaking` as it was.
- The tracker's clock is the poll's `now`, not `Instant::elapsed`, so
  tests drive it with fixed times.
- The pump checks the source's error on every read, not only after a
  level arrived.

**The other post-ingest pumps, checked.**

| Pump | Inputs that change with no update | Moved |
| --- | --- | --- |
| `pump_call_engine` | engine reports (already polled through `pump_call_engine_reports`) | no |
| `pump_group_call_transport` | the picked camera, a device list landing, an engine mute or camera switch to retry | added per poll for connected calls |
| `pump_call_media` | already in `request_tick` | no |
| `maybe_send_parameters` | auth state only | no |
| `maybe_probe_channel_membership` | the open chat (its opener calls it), `my_user_id`, answers | no |

`pump_connected_group_calls` runs `pump_one_group_call` per poll for
each call whose transport is up. A call still connecting is left to
ingest: its join answer is an update, and a failing `connect_group_call`
would otherwise be retried on every 92 ms wake. Teardown stays on
ingest, because it needs the ids from before the update.

**A ringing invitation is the current call (2).** As tdesktop's invite
`Call` is `_currentCall` (`Instance::inCall`, `calls_instance.cpp:842`):

- An incoming 1:1 call while it rings goes to `busy_decline_queue`
  (`Session::decline_call_while_invite_rings`, from `accept_call_update`)
  and never becomes `active_call`. The busy banner names the caller, as
  for a second call during a call (tdesktop `:705-713`).
- A second invitation is declined (tdesktop `:1194`).
- `start_call` (once `createCall` goes out), `join_video_chat`, and a
  conference join that is not this invitation call
  `decline_ringing_invite`. It clears the invitation and sends
  `declineGroupCallInvitation`, as tdesktop's `hangup` of the invite call
  does. tdesktop asks first ("leave your active voice chat?"). Quill's
  call window offers no join while the invitation rings, so these come
  from a chat's Join button or a link.

**Smaller items on the same paths.**

- A failed `declineGroupCallInvitation` shows nothing. Before, it set
  "Voice chat request failed" on whatever voice chat you were in.
  tdesktop sends it with no fail handler, and the invitation has already
  left the screen.
- An invitation to the conference you are in, or a repeat of the one
  ringing (same message, or same `unique_id`), is left alone, as in
  tdesktop `:1179-1186`. `ActiveGroupCall::unique_id` and
  `ConferenceInvite::unique_id` are new for this.
- `ringing_invite_key` mixes the chat id into the window key (FNV-1a,
  folded to a negative 31-bit value). Two invitations from different
  chats with the same server message id get different windows.

**Tests.**

- `connect/tests/call_audio.rs`,
  `your_level_and_speaking_move_on_polls_without_updates`, runs with no
  envelope at all. It checks the 92 ms wake, the level per poll,
  speaking and its send, the 500 ms re-send, the kept-for time, the run
  out with one "stopped", and the wake ending on mute. With the per-poll
  call taken out of `request_tick` (the old, ingest-only pump), it fails
  at its first step: the level stays at 0 instead of 0.1.
- `connect/tests/group_calls/ringing_invite.rs`: busy decline of a 1:1
  call, a second invitation, repeats left alone, placing a call, joining
  a voice chat, joining another conference, accepting the invitation
  itself, an invitation to your own conference, and a failed busy
  decline that leaves `group_call_error` empty.
- `state/conference_invite_tests.rs` covers the reducer side, and the
  `call_conference_ui` tests cover the window key.

**Not done.**

- tdesktop's "leave the current call?" confirm before joining over a
  ringing invitation. Quill declines it without asking.
- The raw level tap (non-UI builds) still keeps a stale error after a
  call ends, as round 12 noted. UI builds do not take that path.
- Not run on real hardware.

### Phase 12 (area B)

Round 12 of the review, area B, on `claude/sm-review12-b`. The findings
share one shape: state that every window shares, written by whichever
window drew or acted last.

- **Copy protection checks its own window's selection.** The selected
  message and the selected emoji moved from two thread-locals into one
  map keyed by `WindowId` (`selectable_text.rs`). Paint writes the
  painting window's entry and `selected_message_text` reads the asking
  window's. Before, the main window repainting after a chat window's
  drag replaced the selection, so Copy in the chat window checked the
  main window's chat and could copy a protected chat's text.
- **Each window's message menu has its own focus.** The menu's focus
  handle, item registry, open flag, previous focus and layer focus
  moved into `MenuFocus` (`context_menu_keys.rs`). `FrameState` and
  `WindowSlot` each hold one, and `swap_slot` exchanges them. The
  hand-off (focus into the menu on open, back on close) is the first
  step of `per_window_upkeep`, so a chat window's menu takes the keys
  and gives the focus back to its own composer. `focus_return_layer_open`
  counts only the window's own layers in a chat window.
- **A link from a chat window brings the main window forward.** A
  queued link remembers the window it was clicked in (`flow_window`,
  carried through `pump_deep_link` the way the agent origin is). When
  it opens a chat that has no window of its own, the main window is
  raised after the open. A chat with its own window opens there.
- **The media panel reads only its window.** `story_reply_panel_active`
  and `group_title_emoji_active` are false in a chat window, so the
  main window's story reply or group title emoji no longer hides a chat
  window's panel or takes its picks. A Business Start Page sticker step
  takes only a pick made in the main window
  (`route_business_start_page_sticker`).
- **A failed members page can be asked for again.** The members-more
  error marks `(chat, filter)` in `GroupsState::member_more_failed`.
  The inline members list takes the mark and, at the next user scroll,
  clears `failed_offset` so the page is asked for again. A frame alone
  does not retry, so a failing server is not asked in a loop. An empty
  page still stops the list.
- **Recent hashtags are the account's.** The list moved from the
  composer's `SuggestUi` (one per window, loaded from the app root) to
  `OnScreen::recent_hashtags`, an `OwnedBy` list loaded lazily from the
  account's own `AccountPaths`. Every window offers the tags sent from
  any of them, and another account keeps its own.
- **Same shape, found in the sweep.**
  - The sticker send flight names its window. Only that window draws
    it, so the main window no longer draws a copy in the chat window's
    coordinates (`send_fly.rs`).
  - The spoiler specks' "painted this frame" flag is a set of
    `WindowId`s. Each window's frame takes its own and asks for its
    tick, so a chat window's spoilers keep moving while the main window
    is not drawn.
- **The guard.** `scripts/ui_statics.py`, run by
  `check-window-scope.py`, lists every production `thread_local!`
  static and static atomic or lock in src/ui. Each one has a type that
  names `WindowId` or an entry in `scripts/ui-statics.txt` with the
  reason it may be shared (scoped and restored around one draw, keyed
  by something that is in one window at a time, a cache, or the app's
  own). An entry whose static is gone fails too.

Tests: `selectable_text_window_tests.rs` (two windows, two selections),
`chat_window/window_menu_tests.rs` (a chat window's menu takes Down and
gives the focus back), `chat_window/window_pick_tests.rs` (title emoji,
Business sticker step, send flight, hashtags across windows and
accounts), `spoiler_fx_window_tests.rs`, `inline_members_retry_tests.rs`,
`state/tests/inline_members.rs` (the failed mark) and the deep link
pump test (a queued link keeps its window).

Not done, or not tested:
- The main window raise has only a pump test; the open itself needs a
  live client.
- The members retry test drives the state and `maybe_load_more_inline`,
  not a real scroll.
- A typed link box (`DeepLinkState::Ui`) opened from a chat window does
  not raise the main window.
- The review's items marked not material are untouched.

### Phase 12 (area C, part 2)

Round 12 of the review, area C, findings 31 to 61, on
`claude/sm-review12-c2`. Part 1 (`claude/sm-review12-c1`) took findings
1 to 30, the lock, and the per-window dialog host; the findings here that
need that structure are listed at the end.

- **An answer acts for the request it answers.**
  - An Earn withdrawal keeps the id of the `getChatRevenueWithdrawalUrl`
    it waits for (`Withdraw::request`). An answer or error after Cancel
    opens nothing and leaves the next attempt alone.
  - A checkout keeps the id of its `validateOrderInfo`
    (`PaymentsState::validate_request`). Closing the checkout or opening
    another form drops it, so a late answer lands nowhere.
  - New Contact keeps the id of its import (`new_contact_request`).
    Closing the box forgets it (`forget_new_contact`), so a closed box
    opens no chat, and the next box's Create is sent.
  - A failed Premium bot link drops `hub.auto_start`, so a later link
    does not send `/start` by itself.
  - A mini app launch dropped by an account switch opens nothing. The
    farewell clears the session's pending launches
    (`forget_web_app_launches`). An answer that arrives later, or one
    that landed but was not shown yet, gets `closeWebApp`
    (`WebApps::dropped_launches`, closed after each ingest).
  - A setting saved with a basic group's type change (History: Visible)
    joins the upgrade already in flight (`upgrade_with` queues its step
    on that request), and every step queued on one request runs.
- **What is drawn per window acts in its window.**
  - The player bar's title jumps in the track's own window
    (`select_chat_request` with the message), and a saved track's
    Playlist opens in the main window (`hand_to_main_window`).
  - A main-window chat change stops only that window's GIF, and the
    autoplay marks are kept per chat (`HashSet<ChatMessage>`).
  - The gift market, craft and transfer follow-ups run in the frame of
    the window that hosts the box (`gift_boxes_upkeep`), not only in the
    main window's.
  - A post's Statistics from a chat window hands the whole open to the
    main window, so the post page is set after the panel opens.
  - The sticker set box is open in one window at a time: viewing a set
    closes the box in the other windows, and Add, Remove and Archive
    close it everywhere (`close_sticker_set_dialog`).
- **A box belongs to what it was opened for.**
  - "Don't ask me again" for a paid send is tied to the chat and price
    it was ticked for (`confirm_trust`); Cancel drops it.
  - Picking another gift drops the first gift's Upgrade or resale
    confirmation.
  - "Buy a collectible by name" from Send Gift buys for that box's
    recipient (`marketplace_chat`), not the chat open behind it.
  - The profile modal gives the info column back as soon as it closes,
    including when a button inside it navigates. A target opened over
    it replaces it, and a switch closes it.
  - Deleting or leaving a group from its Edit box closes the box. An
    upgrade closes the old group's Edit box and moves its chat window to
    the new supergroup (`move_upgraded_chat_window`).
  - The rights editor's Save with untouched checkboxes saves the rights
    its lookup brought in after it opened.
  - Mini app consent is kept per account (`web_app_trust.json` keyed by
    account). Logging out or removing an account drops it. The old
    app-wide list is not read, so each account asks once more.
  - The Earn layer closes on a switch and gives its column back.
- **Error arms end loading, and a poll gives up only when its box
  closes.**
  - A deleted voice note or song stops the player bar.
  - The Contacts tab fetches its list again after a contact change
    invalidates it (`refetch_invalidated_contacts`).
  - An ownership transfer, added members and an approved join request
    mark the member list stale.
  - The bot-create poll is screen-scoped and leaves the dialog to the
    shell. The Edit Bot fill poll has no cap, and Save waits for the
    fill, so it cannot clear the about text.
  - A sticker set's video pick is dropped when its box is closed in the
    pick's window, so no editor opens with no box.
- **Skipped for part 1.**
  - Finding 53 (New Topic from a chat window retargets the main window's
    manage box) needs the dialog host per window.
  - Finding 47's other half: a Create Bot box in a chat window that
    closes moves to the main window. That is the box-moves-on-close
    structure. Only the poll was fixed here.
- **Not tested.** The bot-create and Edit Bot polls, the Edit box
  closing on delete or upgrade, and the upgraded chat window's move all
  need a live driver. They have no tests.

### Phase 13 (area A)

Round 13 of the review, area A, on `claude/sm-review13-a`. All eleven
material findings were fixed. A sweep of core for the same shapes found
five more, and four of those were fixed too.

**Per-account stores are forgotten in one place (1).** Account keys get
reused: a logout keeps `account-<n>`, and `add_account` hands out the
lowest free one. Agent grants, the agent action log, mini app trust,
notification volumes and chat-window layouts were keyed by that string.
So were the web view data folder and three files in the account folder.
A new sign-in in the slot took all of them over. tdesktop's
`Account::loggedOut` wipes every per-account key through
`Storage::Account::reset`. Quill now does the same through
`account_data`:

- `account_data::forget(app_root, key)` drops every store above, plus
  the folder's `recent_hashtags.json`, `paid_trust.json` and
  `launch-lines.bin`, and `webview/<key>/`. A prompt the account is
  still waiting on is answered Denied. Logout and account removal call
  it (`forget_account_sends`), and so does `delete_account_folder`, so
  the next start's sweep covers a removal that was cut short.
- `account_data::claim` records the signed-in user in
  `accounts/<key>/owner.json`. The first ready session of a different
  user in that slot forgets the slot before anything reads it. That
  covers a crash or quit in the middle of a logout.
- `every_store_is_classified` walks `src/` for file-name literals
  (`.json`, `.jsonl`, `.bin`, `.key`, `.wrapped`). Each one must be
  listed as dropped or as kept with a reason, so a new store fails the
  test until someone decides.
- Writing the test turned up lost updates in `grants::update` and in
  web app trust, where read, change and write were separate steps. Both
  now change under one lock.
- Kept on purpose: `exports/` (call logs and exports the user saved)
  and device preferences (tdesktop keeps app settings too). (This line
  also listed the macOS WKWebView store as safe because it is keyed by
  account and bot. It was not: the account key is what gets reused.
  Phase 14 (area A) salts it.)

**The lock holds for a whole agent call (2).** `set_paused_by_lock`
counts locks and wakes every `wait_for`. A wait checks the lock each
time it wakes, so one that sees the lock returns nothing.
`call_on` reads the count at the start. If Quill locked at any point
during the call, a read tool returns the lock message, and a write tool
returns `LOCKED_WHILE_RUNNING_MESSAGE` ("it may have been done"). Its
images are dropped too.

**A recording keeps the place it was sent from (3).** `SendTarget` now
carries a `SendPlace` (topic, thread or direct-messages topic) taken at
Send. `send_voice_note_to` and `send_recorded_video_note_to` route by
it, and a closed topic is still refused, whatever chat is open when the
file finishes.

**Edits drop translations (4).** `updateMessageContent` with different
text removes that message's translations and their jobs, in every
language. An answer still in flight for the old text is dropped with
them.

**Answers settle only their own request (5, 6, 8, 10, 11).**

- A history page dropped by a window reset ends the jump or the date
  jump that waited for it (`abandon_history_around`).
- A failed share search answers the search with no results.
- The gift-code check and apply, the giveaway info and the language
  link box each store their `RequestId`. Answers for other requests are
  dropped.
- The AI style editor waits for `style_save`. Opening an editor
  forgets the last one's save. A late answer still adds the style to
  the list but leaves the new editor alone.
- The bot edit dialog keeps its loads and saves as `RequestId`s, so a
  reopened dialog for the same bot ignores the old one's answers.
  `saving` is now `saving()`, the count of saves in flight.

**Recent hashtags stay with the first account (7).** The app prefs move
no longer takes `recent_hashtags.json`. A file moved by an earlier
build goes back to the first account's folder, or is removed when that
account is gone.

**A failed set loads again (9).** Reopening the sticker or emoji panel,
or reconnecting, clears `media_library.failed`.

**Sweep.**

- The profile gallery's Stories tab never failed. Its purpose belongs
  to the stories error arm, and the gallery arm sat in the media error
  arm, which that purpose never reaches. The stories arm now fails the
  tab. A failed further page no longer zeroes a loaded tab's count.
- The Premium sticker examples could not be fetched again after a
  failure. The error arm now clears `asked`.
- An earlier "From:" picker query's error wiped the newer query's id,
  so its answer was dropped. The error now has to match the stored id.
- The affiliate setup page closed on any `setChatAffiliateProgram`
  answer. It now waits for its own `RequestId` (`SetupPage::waits_for`),
  so a save from a page you left no longer closes the next page. The
  toast still shows.
- Not fixed: the affiliate program lists (the join page and the
  programs box) take any page answer of their purpose. A late "more"
  page from an earlier join page or sort can mix into the new one. They
  need a request id per list.

**Tests.** `account_data_tests.rs` (forget, reuse, prompts, and the
store classification), the lock tests in `agent/access.rs`,
`connect/tests/send_place.rs`, and new cases in `translate.rs`,
`history_window.rs`, `share.rs`, `link_info.rs`, `giveaways.rs`,
`media_library.rs`, `ai_compose.rs`, `owned_bots.rs`, `earn.rs`,
`shared_media_tabs.rs`, `find_in_history.rs`, `premium_purchase.rs`
and `settings/app_prefs.rs`. None of them waits on time. The
round-trip wait test ends on the lock, not a timeout.

### Phase 13 (area B: a box belongs to the window that opened it)

Round 13 of the review, area B, on `claude/sm-review13-b`. Findings 1 to
3 came from one flaw in round 12's per-window dialog host. A box's window
was chosen when a window's sync first saw its flag set, from the window
of the last input, and the host was keyed by kind alone. So:

1. Mute asked again from a chat window, while the main window showed
   Mute for another chat, stayed in the main window and switched to the
   chat window's chat.
2. A per-window kind (Scheduled, a sticker set, a poll) shown in one
   window held the same kind back in every other window. A closing chat
   window ran the close of a kind against the main window's state.
3. "Last input" counted pointer moves and scrolls. An answer opened its
   box in whichever window the pointer had crossed while the request ran.

**Designs considered.**

- *A. Detect the open at the scope choke point.* Compare every flag on
  each `set_window_scope` and credit a flag that turned on to the window
  in place. An ask repeated while its flag is already set leaves no edge
  (finding 1 again). An answer from the poll loop turns its flag on with
  the main window in place. There is no cheap generic way to compare 150
  flags, and it costs every frame. Rejected.
- *B. A typed `Hosted<T>` wrapper on every session-wide flag.* The flag
  carries its window, and the type makes an open without one impossible.
  This is the strongest guarantee, but it touches about 150 fields, most
  in src/state, which area A was changing in the same round. Rejected for
  now. It stays the way to go if the static guard proves too loose.
- *C. Keep round 12's host, and let sites opt in to claiming the box.*
  This is what the review sketched. An open that forgets to claim falls
  back to the same wrong window, so the fix depends on every site
  remembering. Rejected.
- *D (chosen). Every open names its window, and a guard checks that.*

**What was built (`chat_window/dialog_hosts.rs`).**

- A spec is session-wide unless it is marked `DialogSpec::per_window()`.
  Per-window kinds keep their flag in the window's swapped state
  (`chat_ui`, `message_ui`, `composer_ui`, `pickers`, `viewer`, `share`),
  so the key is effectively (kind, window). Each window shows its own,
  and the hosts table never holds them. 33 specs are per-window.
- A session-wide open calls `QuillApp::open_box(kind)` as it sets the
  flag. That names the window in place: input, `spawn_scoped`
  continuations and a window's own frame all run with their window in
  place. An answer whose flow knows its window uses
  `ChatWindows::open_box_in(kind, window)`, for example the deep link
  pump with the link's `flow_window`. A resolved link runs in the main
  window's frame, so `run_resolved_deep_link` wraps its opens in
  `as_asked_by(flow_window)`. A per-window box such a flow opens
  (a sticker set, Share) goes through `in_asking_window`. A box that
  waits for an answer is asked for at the request site (Boost, gift
  info, payment form, login URL, export), so the answer shows where the
  request was made.
- Asked again from another window while shown, a session-wide box moves
  there. An ask made before its flag is set waits for the flag.
- An open no path named shows in the window of the last press or key
  (`windows.last_press`). Pointer moves and scrolls only put a window's
  state in place (`note_window_pointer`). They no longer change where a
  box or a toast goes.
- A chat window that closes from inside (its close button, Cmd/Ctrl+W,
  the platform's close, or its chat going away) runs
  `close_window_boxes` with its own state in place and its own `Window`.
  Every per-window flag set there closes with its own close, and so does
  every session-wide box it showed or asked for. The main window's box
  of the same kind, and its flag, stay. A session-wide box left open
  by a window that went without a close is closed by the main window's
  next sync, with the main window's state in place. Before, any window's
  sync could close it against its own state.
- The lock is unchanged: `sync_host_dialogs` still returns at once while
  locked. An ask made under the lock is recorded and shows after the
  unlock, in the window that asked (`lock-is-a-render-state.md`).

**The guard.** `scripts/dialog_hosts.py`, run by
`check-window-scope.py`:

- A spec's `.per_window()` must match its predicate. A predicate that
  reads swapped window state (following `self` and `app` methods and
  free helpers three calls deep) must be per-window, and one that does
  not read it must not be.
- The script takes the flag fields from each predicate, or from
  `scripts/dialog-hosts.txt` when it cannot read them. Every write that
  sets a session-wide flag in src/ui must have an `open_box` or
  `open_box_in` that names the kind in the same function, or a
  `// box-host: <why>` marker. Markers are for a refresh of a box that
  is already open, a demo, or an answer whose request site asked.
- A flag set in src/state or src/connect needs a UI `open_box` naming
  the kind, or an `answer` entry in `dialog-hosts.txt` with its reason.
- `open_box` on a per-window kind fails, and so does a stale entry.

**The other findings.**

- *4. The reply bar's missing message.* `fetch_missing_reply_target`
  moved from the main-only upkeep into `per_window_upkeep`, so a chat
  window's reply bar asks for its own message. The driver still asks
  once per message.
- *5. Screen reader focus.* `history_a11y::install` now runs for each
  window slot's history focus handle. Before, it ran only for the main
  window's. `a11y_active` moved from `Slices` into `WindowSlices`, and
  `per_window_upkeep` writes it for the window that draws. So a screen
  reader on one window does not make another window's rows report focus.
- *6. The chat window title.* The main window's title function is now
  `window_title_in_place`. `chat_window_title` runs it with the chat
  window's view in place, so Saved Messages and an open topic, with the
  topic's unread count, read the same in both.

**Sweep.** The main-only steps of `frame_upkeep` were checked for reads
of swapped window state. The only other match is
`maybe_prefetch_link_preview`, which reads the composer in place, so a
chat window's link preview chip never prefetches. It was left alone on
purpose. The session keeps one `composer_preview`, and an answer for a
URL other than the current one is dropped (`connect/core`). With two
windows asking for different URLs, each answer would undo the other's,
so the prefetch would loop. The fix is to key the preview by window or
URL in the session and in the driver's answer path. That is a
src/state and src/connect change, so it is left for a round that owns
those files.

Tests: `chat_window/dialog_host_window_tests.rs`. It covers Mute moving
to the window that asked, an open naming the window in place rather than
the last press, two windows showing their own Scheduled box, a closing
window closing only its own boxes, a session box closing with its
window, an answer showing in the window that asked, pointer moves not
counting as a press, and a typed link's box opening in the window it was
clicked in. `chat_window/upkeep_tests.rs` covers the reply fetch, the
screen reader focus and flag per window, and the Saved Messages and
topic titles. The existing `dialog_host_tests.rs` still pass.

Not done, or not tested:
- An ask that is never answered stays in the hosts table until the
  same kind is asked for again. It shows nothing, because its flag is
  clear.
- A typed link's other effects (opening a chat, a mini app) still run in
  the main window's `Window`. Only the boxes it opens, and the scope for
  Share and sticker sets, follow `flow_window`.
- `app_theme/editor.rs` (`show_theme_editor`, through a plain
  `spawn_in`) and the mini app's remove-from-menu poll name the window in
  place when they run.
- The guard reads writes by field path. A write through a local alias
  (`let state = &mut self.x; state.open = true`) is not seen.
- `kit_dialog_open` and `open_dialog_name` still answer for the whole
  app, not per window. The review did not mark this material.
- The window close path is tested through `ChatWindowView::close`. The
  platform's close button and a chat that goes away use the same
  `closing(.., Some(window), ..)` path but have no test of their own.

### Phase 13 (area C)

Round 13 of the review, area C, on `claude/sm-review13-c`. The finding
numbers are the review's ranked list. Area A took 7 (`wait_for_message`
across the lock) and 13 (recent hashtags), and area B took 21 (the topic
edit's window).

- **The lock screen's Log out lifts the lock only when the wipe started
  (1).** `begin_local_wipe` returns why it did not start, and
  `confirm_lock_logout_in` keeps Quill locked with the reason on the lock
  screen. This covers an unwritable record and a missing data folder.
  Before, a read-only `accounts.json` unlocked Quill with no passcode typed.
- **What runs while locked** is now a rule
  (docs/decisions/lock-is-a-render-state.md, "What runs while locked"),
  for 2–6, 8–12 and 16–18:
  - **Pumps wait.** The main frame's upkeep returns on `lock_waits()` after
    notification delivery, so `per_window_upkeep`, the sign-in sync, story
    opens and the Grams and gift follow-ups wait with their state kept
    (8, 11). The poll's drains moved to `notifications/poll_drains.rs`,
    which runs only while unlocked: payment verification, callback and
    login-URL answers, profile galleries, story stealth and the Earn drain
    (2, 11). Parked plays wait too.
  - **Opens are queued, not dropped (2).** Every `os_open.rs` open under the
    lock is queued with `wait_for_unlock` and runs after the unlock. The
    lock screen's Log out drops the queue.
  - **Calls (4, 5, 18).** The lock closes the call windows' boxes and hides
    the stream key (`calls::lock_layers`). Their openers, the RTMP copy and
    revoke actions and the stream PiP refuse while locked. A background
    account's ringing call shows on the lock screen with Answer and
    Decline, and its OS notification keeps its buttons. Closing the stream
    boxes or the stream window hides the key again.
  - **Not looked at (6).** `app_active` is false while locked. The lock
    clears `active_window_chats`, and a locked chat window does not set it
    again, so the open chat notifies and sounds. Story playback and the
    cover's video avatar stop.
  - **Media controls (8).** Now Playing is cleared and media keys are
    ignored while locked. The unlock publishes again.
  - **Connections (10).** `start_connection` and a `LockedStore` refusal
    under the lock defer to the unlock. `ensure_background_accounts`
    starts nothing while locked and starts the accounts at the unlock.
  - **Quit (9).** Under the lock, Quit stops the exports and quits instead
    of opening a box nobody can see.
  - **Kit close (12).** `close_kit_dialog_if_done` does nothing while
    locked, and the video editor's encode continuation goes through it.
  - **Passcode job (16).** A lock asked during a passcode job lands with
    its answer (`lock_after_job`).
  - **Full screen (17).** A chat window's locked frame runs
    `viewer_follow_window_fullscreen`.
- **The guard.** `lock_surfaces.py` check 4 lists every step that the main
  frame upkeep and `poll_live` run outside the gate. Each such step needs a
  `runs-locked` entry with a reason. A new one fails the check, and so does
  a listed one that is gone. The locked frames must run no upkeep.
  `scripts/test_lock_surfaces.py` tests the check.
- **A request or update acts for its owner (14, 19).**
  - A failed quick reply message load is marked failed (`messages_failed`)
    instead of un-requested, so the poll stops rather than re-sending every
    tick. Opening the box clears the error line and retries once, and an
    `ok` clears the line too. The dialog owns its poll task
    (`QuickRepliesDialog::watch`), so a submit cannot stack a second loop,
    and closing the box ends the poll.
  - A call declined as busy is remembered (`busy_declined_calls`), and its
    later `updateCall`s are dropped, the terminal one included. The call
    window no longer shows that caller's "Call ended".
- **The sticker set box is claimed in one place (15).**
  `claim_sticker_set_box` handles viewing a set and "Attached Stickers".
  It closes the box in the other windows, and every window's photo or video
  editor for another set.
- **A window's state is cleaned when it goes (20, 22, 23, 24).**
  - A track whose chat was deleted stops, and every stop clears
    `playback.player.chat`.
  - The checkout prefill runs in `per_window_upkeep`, in the window that
    hosts the box.
  - A closed chat window drops its GIF autoplay marks.
  - Leaving Privacy closes the "Last seen" warning.
- **Tests.**
  - `passcode/lock_waits_tests.rs`: a link and a Grams withdrawal wait and
    then open; Log out drops the queue; a locked window is not looked at; a
    lock during a job; the call boxes; the deferred connection.
  - `deleting_tests` `a_lock_screen_log_out_that_cannot_start_the_wipe_stays_locked`.
  - `background_accounts::tests`
    `a_call_on_another_account_can_be_answered_from_the_lock_screen`.
  - `export_quit` `a_quit_from_the_lock_screen_stops_the_export_and_quits`.
  - `notify_call` `a_locked_app_hides_the_caller_and_keeps_the_call_answerable`.
  - The tests for 14 to 24 are in `connect/tests/quick_replies.rs`,
    `ui/quick_replies/tests.rs`, `state/conference_invite_tests.rs`,
    `chat_window/player_box_tests.rs` and `privacy/premium_rows_tests.rs`.
- **Not done, or not tested.**
  - With no live client, nothing tests the poll's drains, the
    `app_active` change in `poll_live` or the background start deferral.
  - The full-screen hand-back and the stream PiP refusal are untested.
  - The 1:1 device menu stays usable under the lock.
  - Finding 10's sibling, an export box closed without its `on_close`,
    was not confirmed and is not fixed.
  - A deleted chat's history is trimmed to its closed tail, not
    forgotten (`chat_is_gone` needs the chat entry).
  - The video editor branch of 15 has no test.

### Phase 14 (area A)

Round 14 of the review, area A (core: state, connect, telegram, agent,
`account_data`). All seven findings were fixed.

**The macOS mini app store follows the slot's user (1).** WKWebView keeps
cookies, local storage and IndexedDB per data-store identifier, under
`~/Library/WebKit`, not in Quill's folder. The identifier was a hash of
the account key and the bot, and the key is reused, so a new user in the
slot opened the last user's mini app sessions. Deleting `webview/<key>/`
did nothing on macOS. `account_data::webview_store_id` now mixes in a
random salt saved as `webview/<key>/store-salt.key`. `forget` deletes it
with the folder, so a log-out, a removal, or a new user's `claim` gives
the next sign-in new stores. When the salt can't be saved, every launch
gets a fresh one and keeps nothing. WebView2 and WebKitGTK were already
fine, since they take the deleted folder. The old stores are left on disk.
Removing them needs `WKWebsiteDataStore.remove(forIdentifier:)` in the
helper process, which has no hook for it yet.

**The lock (2-5).** The `quill://unread` watcher no longer spins a core
while locked. Writes asleep in the paced queue, `download_file`'s copy and
`resources/read quill://grants` now ask the lock. Details are in
`lock-is-a-render-state.md` ("The agent socket"), and check 5 of
`scripts/lock_surfaces.py` holds them in place.

**A bg link preview shows only its own search (6).**
`chats_state.background_search` holds the `RequestId` of the search the
preview waits for. `apply` stores a `background` answer only for that id,
and the error arm shows only that search's error. Closing the preview
clears it, so a late answer for link A never shows, or gets applied, under
link B.

**A failed custom emoji lookup asks again (7).** Every
`getCustomEmojiStickers` now goes out through
`ConnectDriver::lookup_custom_emoji`. It marks the ids attempted and
records them per request (`emoji::custom_lookup`). A lookup that was not
sent, failed, or was swept unanswered unmarks its ids, so the next resolve
pass asks again. An id stops being asked after three failures, so a
lasting error can't turn every pass into a request. A rate limit waits and
re-sends instead (`getCustomEmojiStickers` joined `RETRYABLE_TYPES`). The
status note shows only while the status picker is open. The status
picker's earlier path also marked ids attempted when the lookup was
deduped and never sent. The seam dedupes before it marks.

**Guards.** Check 5 of `lock_surfaces.py`, and the test
`custom_emoji_lookups_go_out_through_one_seam`.

**Tests.** `a_reused_slot_opens_other_mini_app_stores`
(`account_data_tests.rs`), `a_lock_while_a_write_waits_stops_it` and
`a_locked_watch_look_fails_without_waiting` (`agent/tools`),
`a_locked_quill_reads_out_only_the_guide` (`agent/server_tests.rs`),
`a_late_background_search_answer_never_lands_under_another_link`
(`state/tests/chat_list_more.rs`), `a_failed_custom_emoji_lookup_asks_again`
(`emoji/tests.rs`), and the `emoji/custom_lookup.rs` unit tests. None of
them waits on time.

### Phase 14 (area B: what opens, opens for the window that asked)

Round 14 of the review, area B, on `claude/sm-review14-b`. The finding
numbers are the review's ranked list. Findings 1, 3, 4, 5, 9 and 10 share
one root. Round 13's `open_box` named the window whose state was in place.
In input that is the window of the press. In the poll loop, the main
window's own frame steps, a toast drain and tasks started for the screen,
the state in place is the main window's, whichever window the user acted
in. So answers opened their boxes in the main window, links queued there
had no origin, and a link resolved for a chat window waited for the main
window's frame, which never comes while that window is minimized.

**Designs considered.**

- *A. Put the asking window into every request in src/state.* Each
  `PendingRequest` would carry a window and every answer would read it.
  That is the strongest form, but it touches the core types for a UI
  concern, and most answers already know their chat. Rejected.
- *B. Mark each answer site `box-host:` and keep `open_box` as it was.*
  Every new site would have to remember. Rejected.
- *C (chosen). Say who asks, once, where the code starts running.*
  `ChatWindows::asker` is an `Asker`: `InPlace` (input, a window's own
  frame, `spawn_scoped` work), `Window(w)` (a flow that carried its window,
  `as_asked_by`) or `Nobody` (`unasked`). `open_box` opens in the asking
  window, or in the window of the last press when nobody asked.
  `queue_deep_link` records the asking window as the link's origin. The
  poll pass (`poll_pass`), the main frame's upkeep, the toast drain and
  every `ScopedApp` update of a `spawn_screen`, `spawn_app` or
  `defer_screen` task run as `Nobody`. A `spawn_scoped` task keeps the
  asker it started under. Sites that know their window say so with
  `as_asked_by(window, ..)`: `window_showing(chat)` for an answer about a
  chat, `window_of_box(kind)` for one that follows a box.

**What was built, by finding.**

1. *A link clicked in a chat window waits for the main window.* The
   resolved half runs in the frame it belongs to. `resolved_link_frame`
   gives the chat window for a link whose chat has a window, or for a box
   (`QrCode`, `ChangePhone`, `PeerColor` and the like). A settings page
   (`opens_in_main_window`) or a chat with no window gives the main window,
   which is raised. `run_resolved_deep_link` moved from `frame_upkeep` to
   `per_window_upkeep`, and runs only when its frame is the window in place.
2. *A send being prepared is dropped with its window.* `SendPrepState`
   moved from the per-window `SendBoxUi` to the app-wide `ComposerSends`.
   A prep whose window closed finishes through `ScopedApp::app()` with no
   window: the send goes through the account's client (`live_of`, the
   screen's or a background account's), and only the stored draft is
   cleared. A closing window no longer clears another window's job.
3. *Join in a chat window's invite box opens the chat in the main window.*
   Join keeps the asking window as the link's `flow_window`, so the chat
   opens, and the result shows, where Join was pressed.
4. *Boxes from the poll, the main frame and toast drains.* Proxy links run
   under the window that queued them. The folder limit follows the box that
   hit it (`window_of_box`). `PromoRequest` carries the ad's chat and
   `pump_promo_request` runs per window, in the window showing that chat.
   Grams Needed and Stars run in the window of the suggested post's chat.
   Area C found two more: the mini-app poll (`startattach` terms, Remove
   from menu) keeps `MiniApps::from_window`, and QR Share puts the chooser
   into the window that pressed Share. The swipe poll, the wallpaper file
   pick and the touch bar name their window too.
5. *`queue_deep_link` took the window in place.* It takes the asker. A link
   from the OS, a mini app or a notification is asked by the main window
   (`as_asked_by(None, ..)`), so the pointer's last window no longer
   decides.
6. *An invite box waiting held every later link.* A new link replaces a
   waiting invite preview: the session's `deep_link` and the box's
   `deep_link_invite` are cleared and the new link goes on.
7. *The round-video circle in every window.* The overlay shows only in the
   window recording (`recording.origin()` or `opening_origin()`), and
   `RoundPreview::end()` drops the last frame, so the next recording does
   not start on it.
8. *The quote picker outlived its reply.* `reply_quote_for` keys the
   picker to the reply's (chat, message). `reply_quote_open` compares it
   with the current reply, and a cleared, sent or replaced reply closes it.
9. *Share pick, custom emoji card and the transcription toast.* The share
   pick holds its link for the window that picked
   (`hold_resolved_link`). The card shows in the window of the tap
   (`custom_emoji_tapped`), and chat windows draw it. The speech trial
   toast moved to `per_window_upkeep` for the window of the last press.
10. *A second window's ask moved the old box with its old content.*
    `press_login_url` no longer asks for `LoginUrlConfirm`. The consent box
    opens only at its answer, under the window of the button's chat, with
    that answer's content. An `Open` answer opens no box.
11. *Low.* The auto-delete picker closes only when its own chat is left
    (`close_ttl_picker_of_left_chat`). "Group media" is the tray's
    (`PendingSend.group_media`), so it is per chat. The effects cap is per
    chat (`chat_is_full`). A window's GIF tab follows its own field
    (`follow_gif_field`).

**Guard.** `scripts/dialog_hosts.py` rule 5 walks the call graph from
where nobody asks: `poll_live`, the main frame's steps (not
`per_window_upkeep`), `drain_suggested_post_notes`, and the bodies of
`spawn_screen*`, `spawn_app*` and `defer_screen*` calls. It fails on an
`open_box(` or `queue_deep_link(` reached outside `as_asked_by` or
`asked_in_place`, unless a `// box-host: <why>` marker says why. Closures
that run later (listeners, toasts' links, spawned tasks) are skipped. It
also fails when a root is called outside `unasked(..)`. It runs from
`check-window-scope.py`, with offline tests in
`scripts/test_dialog_hosts.py` (CI step "window scope check tests").

**Tests.** `chat_window/asker_tests.rs` covers the asker in place, under
`as_asked_by` and `unasked`; screen and scoped tasks; queued links' origin;
an unasked box going to the last press; a link run in a chat window's
frame with the main window never drawn; which frame a resolved link
belongs to; the share pick; the folder limit; the No ads preview; the
camera circle; the custom emoji card; the auto-delete picker; the prep
guard across windows; group media per chat; the GIF tab; and QR Share.
Also: `composer/bars_tests.rs` (quote picker), `emoji_effect` (per-chat
cap), `web_app_ui/mini_app_flow_tests.rs` (mini-app poll window),
`state/tests/sessions.rs` (the callback answer keeps its chat) and the
updated `premium_ui/preview_tests.rs`.

**Not done, or not tested.**

- With no live client, nothing tests the proxy link, Join (3), the callback
  and login URL answers (10), Grams Needed, or a prep finishing after its
  window closed (2). The guard covers their window choice.
- The invite preview hand-over (6) has no test of its own.
- The speech trial toast's move has no test.

### Phase 14 (area C)

Round 14 of the review, area C (the rest of `src/ui`), on
`claude/sm-review14-c`. The numbers are the review's finding numbers.
Area B took 14 and 19 (a `spawn_screen` task's `open_box`, the shared
seam). The other twenty were fixed.

- **A job's mark lives with the job, not its box (1).** `PasscodeUi::busy`
  and `lock_after_job` became one value, `PasscodeJob` (`Idle`, `Running`,
  `RunningThenLock`). `close_passcode` no longer touches it, so Esc or
  "Lock now" during a Turn off leaves the lock waiting for the answer.
  The two baseline lines for `PasscodeUi::busy` are gone.
- **The lock's owner-less deferrals now name their owner (2, 6).**
  - `adopt_live` clears `deferred_connect`, and `resume_after_unlock`
    connects only when the screen has no client. So the unlock no longer
    closes the account adopted from the lock card's Answer, and the call
    answered there keeps going.
  - An unlock intent keeps the account and the window that asked
    (`asking_window`). It runs as that window, and it is dropped when
    another account is on screen at the unlock.
- **Calls under the lock (3, 4, 5, 21, 22).**
  - The stream's Leave/End box is not refused.
  - A stream PiP the lock closes clears `window_closed_by_user`, so the
    stream window comes back.
  - The background ring sounds over an older call's summary.
  - The lock stops every window's inline clips.
  - Answering from the lock card dismisses the OS notification.
  - Details are in lock-is-a-render-state.md.
- **Drafts are compared with what they were built from (7, 8).**
  - Each Business draft (greeting, away, hours, location, start page)
    keeps `base`, the saved value it was made from. Leaving saves only
    when the draft differs from `base`, not from the server's value now.
    An untouched screen writes nothing, even when the full info arrived
    or changed after it was built. Chatbots already worked this way.
  - The story reply and live comment fields empty when the viewer moves
    to another story or closes (`clear_story_fields`). A restart of the
    same story keeps them.
- **Session pages opened by a box close with it (9, 16).**
  - `drop_invite_link_pages` removes another admin's links, a link's
    join requests and its members (and the UI's details and revoked
    marks). It runs for the chat when the Group Settings box closes, and
    for every chat in `AdminUi`'s farewell.
  - `GiftMarketUi`'s farewell stops following the auction on the old
    account's driver.
- **A box shows where it was asked (10, 11).**
  - "Abort recovery email setup" is an inline `TwofaConfirm` inside the
    2FA box. `GroupConfirmAction::AbortRecoveryEmailSetup` is gone.
  - A same-forum topic edit calls `open_box(ForumManage)`, so the box
    moves to the chat window that asked.
- **Late answers check that they still apply (12, 13, 18).**
  - A pending profile gallery (`PendingGallery`) records its window, the
    chat that window showed, and its viewer open count. It opens only in
    that window, unlocked, with no viewer opened since, on the same chat.
  - A parked inline round video is dropped on a chat change
    (`enter_chat_ui` and `sync_chat_ui`) and at resume when its window shows another chat.
  - A set image that lands after the sticker set box moved to another set
    opens no editor, and its written PNG is removed (`sticker_box_shows`).
- **Holds and lists (15, 20).**
  - A parked voice note or song holds its chat's history while the next
    track downloads.
  - The shared-media list tabs reset scroll and "asked" per page
    (`list_scroll(tab, PageKey)`), as the grid tabs did.
- **Log-out and the mini app (17).** `close_mini_app_for_forget` closes
  the account's mini app with `reap_after_notice`. Then
  `account_data::forget_after_helper` removes the web view folder only
  once the helper is gone, on a thread that waits for the notice.
- **Tests.**
  - `closing_the_passcode_box_keeps_a_lock_waiting_for_the_job`
  - `under_the_lock_only_the_leave_box_opens`
  - `locking_quill_closes_the_live_pip` (extended)
  - `an_older_call_summary_does_not_silence_another_accounts_ring`
  - `an_unlock_intent_runs_as_its_window_and_only_for_its_account`
  - `locking_stops_a_chat_windows_round_video`
  - `an_untouched_screen_never_overwrites_a_newer_saved_value`
  - `a_reply_draft_does_not_follow_into_the_next_story`
  - `another_admins_links_close_with_the_group_settings_box`
  - `aborting_the_recovery_email_asks_inside_the_2fa_box`
  - `new_topic_for_the_main_windows_forum_opens_in_the_chat_window`
  - `the_group_avatar_opens_its_photos_newest_first` (extended with the
    late gallery)
  - `another_chat_drops_a_round_video_waiting_to_play`
  - `the_next_song_downloading_keeps_its_chats_history`
  - `a_set_image_that_lands_after_the_box_moved_on_opens_nothing`
  - `a_list_tab_starts_over_on_another_chats_page`
  - `the_web_view_data_waits_for_the_mini_app_helper`
  - None of them waits on time.
- **Not tested.**
  - The `deferred_connect` skip (it needs a live client to adopt).
  - The gift auction farewell (it needs a live driver).
  - The OS notification dismissal on answer.
  - The web view folder's removal after the helper exits (only the wait
    before it is tested).
- **Not done.** The Business screens still show the empty state until the
  full info arrives. Nothing is written from it now, but the screen is not
  rebuilt when the info lands.

### Phase 15 (area A)

Round 15 of the review, area A (core: state, connect, telegram, agent,
`account_data`, `emoji`). All four findings were fixed, and the four
round-14 changes that got past `check-implicit-state.py` became one value
each.

**A rate limit no longer gives custom emoji up (1).** Round 14 sent
`getCustomEmojiStickers` through the flood retry. A wait over 60 s, or a
fourth 429, still reached the stickers error arm, which counted a failure
for every id. Nothing backed off, so three passes a few seconds apart gave
the ids up for the session. A rate limit is now no failure: the error arm
frees the ids without counting (`custom_emoji_lookup_rate_limited`), and
`absorb_flood` sets `ConnectDriver::custom_emoji_hold` to the end of the
wait. `custom_emoji_lookup_idle` is false while it holds, `request_tick`
lets it go when it is due, and `next_deadline` wakes the poll loop for it.
The hold is for the method, not the ids: TDLib limits the call, so any
batch would hit it.

**A user's photo gallery follows their photo (2).** `updateUser` with a
different `profile_photo.id` (a new photo, or none) and `updateUserFullInfo`
with a different `photo_id` drop `user_profile_photos[user]`
(`Session::forget_user_photos`, next to `forget_chat_photos`). The viewer
fetches the gallery again the next time it opens. That covers a contact's
change and your own change from another device; the own Set/Delete OK
uses the same helper. A fetch that was out when the photo changed could
still land the old list, so each stage of `ProfilePhotosFetch` and
`ChatPhotosFetch` now names its request: `Loading(RequestId)` for the
first page, and `next_page: PhotoPage` (`Idle` or `Asking(RequestId)`)
in place of the `loading_more` bool. An answer or error lands only when
it is the request the list waits for, so a dropped fetch's first page or
later page never shows, and a late later page can no longer be taken as
the first page of a new list (the round-15 "own profile photo pages"
note). Tests: `a_user_photo_change_while_a_page_is_out_drops_its_answer`
(`state/tests/profile_panels.rs`) and
`a_chat_photo_change_while_a_page_is_out_drops_its_answer`
(`state/tests/photo_gallery.rs`).

**A grant change stops waiting writes (3).** The per-call thread locals
(the switch's generation, the connection, the lock count) are one
`CallStart`, which also records `grants::generation()`. `still_allowed`
refuses with `GRANTS_CHANGED_MESSAGE` ("nothing was sent or changed") once
the grants moved since the call began. It refuses rather than re-resolving,
since it has no target: any grant change while a write waits is rare, and
the agent retries against the new grants. `common::confirm` now asks
`still_allowed` after the user's Allow too, like `td_call`'s prompt.

**`mark_read` is one queue turn (4).** `viewMessages` and
`toggleChatIsMarkedAsUnread` go out in one turn (`common::change_steps`),
so the turn's check is the only point where the tool can answer "nothing
was sent or changed". A lock between the two requests now gives
`LOCKED_WHILE_RUNNING_MESSAGE` ("may have been done").

**One value each.**

- `bots.last_callback_answer` is an `Option<CallbackAnswer>` holding the
  answer and the chat of the press. `last_callback_chat` is gone.
- `chats_state.background_search` is a `BackgroundSearch` enum (`Idle`,
  `Waiting(RequestId)`, `Found(Background)`). `searched_background` is
  gone. A failed search goes back to `Idle` and shows `background_error`.
- `EmojiPanel.status_resolution_attempted` is gone. Each id's stage is an
  `IdLookup` in `CustomEmojiLookups` (`Asking`, `Failed`, `GaveUp`,
  `Answered`), and callers ask `custom_lookups.attempted(id)`.
- `LOCKS_AT_START` is part of `CallStart`, held in an `Option` that a
  `CallScope` guard clears when the call ends. Off a call, `still_allowed`
  and `locked_since_call_began` refuse.

**Guards.** Check 5 of `scripts/lock_surfaces.py` now also needs
`still_allowed()` after every `prompts::wait(` in a write tool
(`src/agent/tools/write/`, `td_call.rs`), with unit tests in
`scripts/test_lock_surfaces.py`.

**Tests.** `a_long_custom_emoji_rate_limit_holds_lookups_instead_of_giving_up`
and `custom_emoji_rate_limit_retries_that_run_out_hold_lookups`
(`connect/tests/flood_retry.rs`), `a_changed_user_photo_drops_their_gallery`
(`state/tests/profile_panels.rs`), `a_grant_change_while_a_write_waits_stops_it`
(`agent/tools`), and `mark_read_is_one_turn_of_up_to_two_requests`
(`agent/tools/write/manage.rs`). The existing callback, background and
custom emoji tests moved to the new types. None of them waits on time.

**src/ui lines touched.** Only what the type changes forced:
`ui/notifications/poll_drains.rs` (the callback answer drain) and
`ui/wallpaper_preview.rs` (`previewed_background` and
`close_wallpaper_preview`).

### Phase 15 (area B: the Asker decides toasts too, and boxes go with their chat)

Round 15 of the review, area B, on `claude/sm-review15-b`. The numbers are
the review's ranked findings; the three "dodges" are round-14 state the
implicit-state check did not catch.

**Toasts and the Asker (finding 7, first dodge).** A toast's window was a
copy: `ConnectionUi::note_window`, written beside `ChatWindows::last_press`
in `note_window_press` and cleared by hand on window close. Boxes of the
same flow went to the Asker (`asking_window`), toasts to the last press,
so a bot's answer to a button pressed in a chat window toasted in the main
window once the user had clicked there. The copy is gone. `set_note` is a
`QuillApp` method that takes the window from the Asker, as `open_box`
does (`asking_window_or_last_press`): the window of the press, of the
frame, of the deferred work, or the one a flow carried under
`as_asked_by`; with nobody asking, the last press. The 1,300 note sites
call it. Where the client is borrowed, they call
`connection.set_note_for(&windows, note)`, which reads the same Asker
(`ChatWindows::asking_window_or_last_press`). `set_note_in` (a window
named by hand) stays for the seam and the forward result, which names the
window it puts in place. `report_failed_open` keeps the asking window
across the OS's answer and sets the note under `as_asked_by`.

*Designs considered.* (a) Keep `note_window` and write it whenever the
Asker changes (`with_asker`, the scope swaps): still two fields kept in
step by hand. Rejected. (b) Give each toast a "resolve later" host and
place it at the end of the update: the Asker can change several times in
one update, so the place would be wrong. Rejected. (c, chosen) The note's
window is read from the Asker when the note is set, in one function.

*Guard.* `scripts/note_hosts.py`, run by `check-window-scope.py`, fails
on `set_note_in(` or `toasts.push_in(` outside `connection_state.rs`, and
on a read of `windows.last_press` outside `src/ui/chat_window/`, unless a
`// note-host: <why>` marker says why (two sites: the forward result and
the sticker fly's cell). Offline tests: `scripts/test_note_hosts.py` (CI
step "window scope check tests"). `lock-surfaces.txt` lists `poll_live ->
set_note` as runs-locked, as the old `connection.set_note` was.

**Box hosts (second dodge).** `dialog_hosts::Host { window, shown: bool }`
is `enum Host { Asked(window), Shown(window) }`.

**What was built, by finding.**

1. *The rich draft's boxes acted on the chat shown now.* "Discard" and
   "Remove Formatting" are one `RichDraftBox` (`Closed`, `Discard`,
   `PremiumChoice(plain)`) in `ChatUi` (third dodge: the
   `premium_choice: Option<String>` flag and `discard_confirm` are gone
   from the per-window `RichDraftUi`). A chat or topic change replaces
   `ChatUi`, so the box closes. A click that lands between the chat
   change and that frame does nothing: both buttons ask
   `chat_ui_is_current()` first.
2. *The AI compose box and the apply-in-place shortcut advanced only on
   main-window frames.* `ai_compose_window_upkeep` is a step of
   `per_window_upkeep`. In the box's own window's frame it closes the box
   when that window shows another chat, takes the box's answer, the style
   editor's answer, the style refresh and the saved-style pick. In the
   asking window's frame it takes the shortcut's answer and rewrites that
   composer in place, with no defer. A box or a request whose window
   closed is dropped in any frame. The button's blink, the style link's
   preview and notices, and the emoji files stay in the main upkeep.
3. *AI error toasts showed in the main window.* They follow from 2: each
   push uses the `Window` of the frame that took the answer.
4. *A prep that ended through the fallback left the sent caption on
   screen.* When the window Send was tapped in is gone, the fallback asks
   `window_where` for a window that shows the slot now (the chat reopened
   in a new window, or the account back on screen in the main window).
   It finishes there with that `Window`, so the composer, reply and
   options are cleared as they would have been. With no such window it
   clears the stored draft, as before.
5. *The app-wide pending link outlived its account.* The link and its
   origin are one `QueuedLink { link, account, window, by_agent }`
   (`LinkUi::agent_pending_link` and `window_pending_link` are gone). A
   link from outside Quill (`QueuedLink::outside`: the launch argument,
   the OS through `queue_outside_link`) has no account, and whichever
   account is on screen takes it. A link clicked in an account is followed
   only by that account (`for_screen`) and is dropped once another account
   is on screen. So a t.me/c post link or a bot start link of account A
   never resolves on B, and an agent's link is still the agent's.
6. *The quote picker came back after a normal send.* The picker is part of
   the reply bar: `ChatUi`'s reply is a `ReplyBar { reply, quote_picker }`.
   Every way a reply goes (a send's `set_reply(None)`, Escape, a reply to
   another message) takes the picker with it; a new quote for the same
   message keeps it open. `ShareUi::reply_quote_for` is gone.
7. *Toasts ignored the Asker.* Above.
8. *Re-entering the GIF tab dropped the emoji section.* Each window keeps
   what its tab asks for: `PickerUi::gif_wanted: GifWanted` (`Saved` or
   `Search(query)`; trending is the empty query). A footer emoji empties
   the field but stays the window's section. Entering the tab asks for the
   window's own section again when the session's one list holds another.
   With two windows' tabs open, the window of the last press asks for its
   own (`gif_tab_upkeep`, per window), and the other window's tab shows
   none of those GIFs until a press there. A request that cannot be sent
   leaves the window on what the list holds, so nothing asks every frame.
9. *A chat window's link-preview chip never loaded.* The session keeps a
   preview per URL (`ComposerLinkPreviews`, the last eight asked), and
   `maybe_prefetch_link_preview` runs in every window's upkeep for that
   window's composer. A composer with no URL clears nothing: each chip
   reads its own URL's preview.

**Tests.** `rich_draft_tests.rs` (`a_rich_draft_box_goes_with_its_chat`),
`ai_compose/dialog_tests.rs` (a chat window's box takes its answer and
toasts with no main-window frame), `composer/bars_tests.rs` (a text send
closes the picker), `chat_window/tests.rs` (a note toasts in the window
that asked, in the window's own frame, and at the last press when nobody
asked), `chat_window/asker_tests.rs` (queued links' origin, each window's
GIF section, `window_where`), `deep_links.rs` (`QueuedLink::for_screen`)
and `connect/tests/message_ops` (previews by URL).

**Not done, or not tested.**

- The prep fallback (4) needs a live client to send; only `window_where`
  is tested. The account round trip was not run live.
- The style notices (`ai_style_notice_upkeep`) still run in the main
  frame.

**Follow-up (the two leftovers above, fixed).**

- *A second window's GIF tab showed nothing until a press there.* The
  session still pages one search, but now keeps the searches answered
  lately by query (`GifPanel::searches_kept`, the last eight). A window
  shows what it wants (`QuillApp::gif_shown`): the live list when it
  holds the window's section, the saved GIFs, or its search kept from
  before. A window that has none of these asks once, on its first frame
  with the tab open (`PickerUi::gif_asked`, an enum), after any answer on
  its way, so it never cancels another window's. The window in use
  takes the list back for paging only when nothing is loading. Only the
  live list pages or shows the spinner.
- *The style preview ran in the main frame.* It runs in
  `ai_compose_window_upkeep` of the window that shows its box
  (`window_of_box(AiStylePreview)`), or the main window's once that
  window closed. Its toasts show in that window.

Tests: `asker_tests.rs` (`a_gif_tab_shows_its_section_without_a_press`,
and `each_window_keeps_its_own_gif_section` now checks that the other
window shows its kept section without asking), `ai_compose/dialog_tests.rs`
(`a_chat_windows_style_preview_takes_its_answer_in_its_own_frame`).

### Phase 16 (area A)

Round 16 of the review, area A (core outside `src/ui`). All five findings
were fixed.

**Allowing a confirmed write lets it go again (1).** Round 15 made
`common::confirm` and `td_call::ask_user` ask `still_allowed()` after the
user's Allow. The Allow itself runs `grants::update` with `apply_ask`,
which changes nothing for `Ask::Write` and `Ask::Raw`, but `update` moved
`grants::generation()` on every call. With "Confirm each send" on, every
write the user allowed was refused with `GRANTS_CHANGED_MESSAGE`. `update`
now compares the account's grants before and after the change and saves
nothing, keeping the generation, when they are equal. This is the shared
seam: every grant edit goes through it, so a no-op row click doesn't stop
waiting writes either. Tests: `a_write_request_shows_the_text_and_answers_allow_or_deny`
and `a_raw_call_shows_the_risk_and_the_request` (`ui/agent_dialogs_tests.rs`)
press the real Allow button (`answer_agent_request`) and check that the
generation did not move. `allow_applies_the_grant_then_answers` checks
that a real grant still moves it. `an_update_that_changes_nothing_lets_a_waiting_write_go`
(`agent/tools`) checks `still_allowed` on both sides.

**A community's mute holds without opening it (2).** The community's
notification settings live only in `communityFullInfo`, which was fetched
for collapsed communities, for the info panel and for a ban.
`ConnectDriver::sync_community_full_infos` (formerly
`sync_collapsed_communities`) now asks for the full info of every community
with `have_access`, once each, and it runs in `poll_ticks` for every
account, including background ones. The UI drain no longer calls it.
Test: `every_community_gets_its_full_info_once_in_the_poll`
(`connect/tests/communities.rs`).

**A flood wait belongs to its account (3).** `pacing::State::flood_until`
is a map keyed by `AccountKey`. `take_turn` and `flood` take the account,
`common::write_error(ctx)` passes `ctx.account`, and
`account_data::forget_after_helper` calls `pacing::forget_account` next to
`grants::forget_account`, so a log-out clears it. The per-day cap on new
people stays process-wide (it protects the api_id). Test:
`a_flood_wait_refuses_that_accounts_sends_until_it_passes`.

**Add All doesn't bring handled requests back (4).** On the `ok` of
`processCommunityAddChatRequests`, `reset_add_chat_requests` takes every
in-flight `GetAddChatRequests` request for the community out of the
registry (`RequestRegistry::take_purpose_for_community`) and sets the list
to an answered empty one. A page that answers afterwards has no pending
request and is ignored. The old `drop_add_chat_requests(Option)` is now
two functions, one for a single request and one for the reset. Test:
`a_page_asked_before_add_all_does_not_bring_handled_requests_back`.

**The manage screen sees a Remove or Add at once (5).** Checked in the
TDLib 1.8.70 source: `ToggleCommunityPeerLinkQuery` calls
`CommunityManager::on_change_community_dialogs`, which reloads only the
community's full info before the `ok`.
`ChatManager::on_update_channel_linked_community_id` changes the
`Channel`, not the `ChannelFull`, so `supergroupFullInfo.community_id`
changes only on the next full fetch. The `ok` of `addCommunityChat` and
`removeCommunityChat` now updates the chat's cached supergroup or user full
info (`Session::note_chat_community`). A remove clears only a link that
still names that community. An `updateCommunity` with `have_access: false`
drops that community's cached full info, and `chat_community_id` ignores a
link to a community without access, so a deleted community's chats no
longer map to it or follow its mute. Tests:
`remove_and_add_ok_update_the_chats_cached_community` and
`a_community_without_access_has_no_chats` (`state/tests/communities.rs`).

**Build.** `origin/main` (af2a519b2) did not build the UI binary
(`src/ui/communities` called the removed `ConnectionUi::set_note`). This
branch merges `fix-community-notes` (PR #1415) for that and does not fix
it separately.

**Guards.** None added. Finding 1 was fixed in `grants::update`, the one
place every caller goes through. The account-keying in finding 3 can't be
checked from the source text.

### Phase 16 (area B: every main-only step says why, and per-chat state is per conversation)

Round 16 of the review, area B, on `claude/sm-review16-b`. The numbers are
the review's ranked findings. The four "dodges" are round-14 and round-15
state that the implicit-state check did not catch. Two shapes kept coming
back, so each now has a guard at its seam.

**Main-only frame steps (findings 6, 7 and 11).** `frame_upkeep` runs only
in the main window's frame. Each round found another step there that a
chat window's box or press depends on, so with the main window minimized
that work never moved on. Every step left in `frame_upkeep` now has its
own `// main-only: <why>` comment directly above it. These are: restoring
chat windows, the sticker box, the earn pages (only main draws the info
panel), the AI button's blink, a pending story open, a pending mini-app
action, the story viewer's upkeep (moved out into `story_frame_upkeep`)
and sign-in. The rest moved to `per_window_upkeep`:

- The video chat and stream boxes (`call_boxes_upkeep`) run in the window
  that hosts the box (`dialog_may_show_in`), so "Start video chat" pressed
  in a chat window's header takes its "join as" answer there (6).
- The AI style notices run in the preview's window when a preview is open,
  else in the box's window, else in the window of the last press. The
  emoji and icon fetches run where the box or preview is drawn (7).
- The Premium-limit toast runs in the window of the last press (11).
- The folder editor's prefill and a failed collectible lookup run in the
  window that hosts their box. The seek slider runs in every window.

*Guard.* `check-window-scope.py` now fails CI on any `self.step(` call in
`frame_upkeep` without a `// main-only:` comment block directly above it,
and on any inline `self.field` work there. That work belongs in a step
that says why it is main-only, or in `per_window_upkeep`. Profiling,
`lock_waits` and `asked_in_place` are not steps. I checked the guard by
adding an unmarked step and an inline write: both fail.

**Per-conversation state (findings 1 and 2).** The forward selection and
the AI box were keyed by chat only. A forum's topics, a direct-messages
topic, a thread and a Saved Messages sublist share one chat id, so a
selection made in one topic stayed live in the next topic of the same
forum. There, Delete or Forward acted on the old topic's messages, and
the AI answer wrote into the next topic's draft.

- `TopicId` has a `Thread { message_thread_id }` variant and a `None`
  default. `Session::open_conversation(chat)` gives the full
  `ConversationKey`. The order is: a thread if one is open, else the
  direct-messages topic, the forum topic or the Saved sublist.
- `ForwardDraft` carries `from_topic`. `toggle` restarts the selection
  when it comes from another conversation. `forward_draft()`, the seam
  every reader goes through, hides a selection from another topic of the
  chat on screen, even in the frame before the upkeep drops it. The
  forward bar's own destination is not affected.
- `ChatUi.topic` is the full `TopicId`, so a Saved sublist or thread
  change also replaces `ChatUi` whole. On that change,
  `dismiss_forward_for_view` drops the selection and its focus.
- `AiComposeOpen` and `ApplyInPlace` hold a `ConversationKey` in place of
  the chat id. The box closes, and the shortcut's answer is dropped, when
  the window shows another conversation.

*Designs considered.* (a) Clear the selection in each topic switcher
(forum topics, threads, direct-messages topics, Saved sublists): the next
switcher would forget. Rejected. (b) One selection per conversation, kept
in a map: it would bring back a selection the user left. Rejected. (c, chosen) The selection names
its conversation, and the read seam hides one that is not on screen.

**What else was built, by finding.**

3. *A deep-link flow outlived an account switch.* `ChatsState::
   end_deep_link_flow` clears the link, its agent mark and the original.
   `LinkUi`'s farewell calls it, so switching back does not replay the
   flow.
4. *A toast for a closed window blocked the queue.* `asking_window()`
   filters `Asker::Window(w)` by `has(w)` in the seam, as `open_box_in`
   already did. A flow whose window closed (a mini app, the wallpaper file
   picker) now asks as the main window, so its toasts and boxes show.
   `forget_chat_window` also tells the mini app its window closed
   (`mini_app_window_closed`).
5. *A prep whose Send window now shows another chat.* The prep finishes
   in that window only while it still shows the slot. Otherwise it falls
   through to `window_where`, as a closed window does.
8. *Quick search and hashtag search from a chat window opened the main
   window's search out of sight.* `open_search_ui` and the global branch
   of a hashtag search hand the work to the main window
   (`hand_to_main_window`), which comes to the front. A scoped hashtag
   search closes that window's chat search first.
9. *A link-preview failure was kept for the session.* An answer is one
   `LinkPreviewFetch` (`Asked`, `Loaded`, `NotFound`, `Failed`). Error
   codes 400 and 404 mean there is no preview. Any other error is
   `Failed`: the chip says the preview could not load, and the composer
   asks again once the URL changes (`preview_tried`).
10. *Sticker and emoji keyword search used per-window "already searched"
    copies.* `KeywordEmojis` keeps its query, and the readers use
    `for_query`. `MediaPanel::searched` and `keyword_searched` are gone.
    Only the window of the last press asks, and only when the session's
    query differs. Found stickers show only for their own query.

**Dodges.** `StyleEditor::pending` is `StyleSave { Editing, Sent }`. The
preview's `token`, `style: Option<_>` (used as the "lookup running"
flag) and `removing` are one `PreviewStage { LookingUp { token },
Shown(style), Removing(style) }`. The baseline line for
`StyleEditor::pending` is dropped. The `MediaPanel` copies and the mini
app's `from_window` are covered by 10 and 4.

*Main build fix.* `origin/main` (af2a519b2) did not build: the
communities module called `ConnectionUi::set_note`, which phase 15
removed. This branch is built on `fix-community-notes` (PR #1415), which
fixes it.

Tests: `chat_ui_tests.rs` (`a_selection_made_in_one_topic_goes_with_it`),
`ai_compose/dialog_tests.rs`
(`the_box_closes_when_the_view_moves_to_another_topic`,
`the_shortcuts_answer_skips_another_topic`), `composer/actions.rs` (a
toggle from another topic restarts the draft), `on_screen_tests.rs`
(`a_switch_ends_the_link_flow_the_account_was_running`),
`chat_window/dialog_host_tests.rs`
(`a_closed_windows_flow_asks_as_the_main_window`,
`a_chat_windows_video_chat_start_goes_on_in_its_own_frames`,
`a_premium_limit_toast_shows_in_the_window_that_hit_it`), the connect
link-preview test (a 429 is `Failed`), `reactions_live`
(`for_query`), and `mini_app_flow_tests` (opens a real chat window first,
so the Asker filter keeps it).

**Not done, or not tested.**

- The prep fallback (5) needs a live client to send. It has no test.
- Search hand-off (8) and the link-preview retry (9) were not run live.


### Phase 17 (area B: one stage for the forward, one mode for the panel, and flows that ran in the wrong place)

Round 17 of the review, area B, on `claude/sm-review17-b`. The numbers are
the review's ranked findings. Three round-15 and round-16 "dodges" (state
kept in step by hand next to other state) are converted too; findings 6
and 7 came from one of them.

**The forward is one stage (dodge, findings 6 and 7).** `ShareUi` held
the forward as three fields kept in step by about ten writers: the draft
(`pending_forward`), the chat the forward bar waits in (`forward_bar_dest`)
and whether the share box is open (`forward_picker_open`). It is now one
`ForwardStage` (`src/ui/forward_stage.rs`):

- `None`; `Selecting(draft)`, selection mode in the conversation the
  messages were picked in; `Picking { draft, bar }`, the share box over a
  selection (`bar: None`) or over a forward bar ("Change recipient",
  `bar: Some(chat)`), which leaving the box returns to; and
  `Bar { draft, dest }`, the forward bar above `dest`'s composer.
- The transitions are methods (`open_picker`, `close_picker`, `drop_bar`,
  `selection_mut`, `take_draft`), so a writer cannot leave a bar with no
  draft or a box over nothing.
- A forward bar is no selection mode (6). `selecting_in` reads only
  `Selecting` and `Picking { bar: None }`, and requires the selection's
  own conversation, so a bar into the selection's own forum no longer
  shows "3 selected" and a Delete over hidden ids in every topic. Picking
  again (a toggle, a drag, Shift+click) turns a bar back into a
  selection (`selection_mut`).
- `ForwardFlight` and `ForwardResult` carry `from_topic` (7). A forward's
  answer ends the window's draft only when the draft `is_from` the
  result's chat and topic, so a newer selection made in another topic of
  the same chat stays.

**The emoji panel is one mode (dodge).** `MediaPanel { open, reaction,
status }` is `panel_mode: PanelMode { Closed, Compose, Reaction(target),
Status(target) }` (`media_panel/mode.rs`). Opening the reaction selector
over the status selector left the status target set, and the readers
disagreed on which won (`sync_media_panel_rows` picked the reaction,
`panel_query` and the custom emoji cell the status). One writer,
`set_media_panel_mode`, drops a selector's rows and stops resolving the
status choices when the status mode ends; it is listed in
`scripts/single-writers.txt`, so `check-screen-state.py` fails any other
whole write outside tests.

**The link preview's "already asked" mark is the session's (dodge).**
`ComposerUi::preview_tried` was a per-window copy next to
`Session::composer_previews`. Any entry there (asked, loaded, none,
failed) now means "asked". A keystroke that brings a URL into the field
forgets a `Failed` entry for it (`retry_failed_link_preview`, from the
composer's Change event, not the frame), and a request that could not be
sent is stored as `Failed` (`send_failed`), so it is not asked again on
every frame.

**What else was built, by finding.**

1. *A topic change while editing left the edited text as a plain draft.*
   `sync_chat_ui`'s topic branch asks for the slot's draft restore
   (`request_draft_restore`) when the value it replaces was editing or had
   the rich editor open. The restore runs later in the same upkeep, after
   the edit is gone, so the stashed draft and its reply come back. A Saved
   sublist or a thread inside a forum topic keeps the draft slot, so no
   switch path restored it before.
2. *Window-menu and Dock commands ran in a chat window's scope.* `on_app`,
   the one path every menu-bar and Dock handler takes, runs the command
   through `as_main_window_command`: the main window's state in place
   (`in_window_scope(None, ..)`) and asked by it (`as_asked_by(None, ..)`).
   The command raises the main window, so its box shows there.
3. *A reply typed into a toast of an account not on screen was lost and
   lingered.* `send_typed_notification_reply` is the one place the typed
   text is taken. `drain_background_clicks` calls it for the account on
   screen, and `run_notification_action` calls it for a Reply click that
   switched accounts first (`open_pending_account_chat`). The chat opens
   only when nothing was sent.
4. *A stale Join opened the community-limit box.* `limit_box` is computed
   only for an answer that is not stale.
5. *A sticker category listed another window's search.* The session's
   found list says what it is for: `StickerSearch { Typed(query),
   Category(source) }` replaced `search_query`. A window lists the found
   stickers only while they are for its own query or category
   (`wanted_sticker_search`), and the window of the last press asks for a
   category the same way it asks for a typed query, so
   `search_chosen_sticker_category` and the `category_changed` key compare
   are gone.
8. *A stale chat-list check revived select mode.* The main window's frame
   clears the checks once none of them is listed
   (`end_selection_left_list`, a `// main-only` step: only the main
   window has a chat list). "Select unread" checks the listed chats only.
9. *A channel's reaction row stayed after Delete.* The purpose carries the
   full `MessageSender`, so `drop_reactor_from_audience` matches a chat
   sender too.
10. *The D-Bus notifier gave up for the session.* A failed connect is not
    cached. `RetryGate` lets the next notification try again after
    `REPROBE` (30 s), the capability probe's interval.

*Guards.* `scripts/single-writers.txt` lists `panel_mode`. The screen
state check's chat-key list (`scripts/view-state-not-chat.txt`) names the
two forward-bar chats in the stage. The OS-menu fix sits at the one seam
all handlers use (`on_app`), so no extra script.

Tests: `chat_ui_tests.rs` (the forum-topic edit test now checks the field
lost the edited text; `a_forward_bar_into_the_selections_forum_is_no_selection_mode`,
`a_forward_answer_keeps_a_newer_selection_of_another_topic`),
`forward_stage.rs` (the stage's transitions),
`chat_window/asker_tests.rs` (`a_menu_bar_command_runs_as_the_main_window`),
`notifications/reply_after_switch_tests.rs`,
`state/tests/premium_limit.rs` (`a_stale_invite_link_join_opens_no_limit_box`),
`media_panel/panel_tests.rs` (`a_category_does_not_list_another_windows_search`),
`media_panel/status_tests.rs` (`the_reaction_selector_replaces_the_status_selector`),
`derived_state_tests.rs` (two chat-list tests), `ownership_types.rs`
(`a_deleted_channel_reaction_leaves_the_who_reacted_list`), the connect
link-preview test (`forget_failed`, `send_failed`) and `linux_dbus.rs`
(`a_failed_connect_is_retried_after_the_reprobe_interval`, Linux only).

**Not done, or not tested.**

- The notification reply (3) is tested through a test-only text seam; a
  real toast reply after an account switch was not run.
- The D-Bus retry (10) builds and tests on Linux CI only; it was not run
  against a daemon that starts late.
- The OS menu fix (2) is tested at the seam; the real menu bar was not
  clicked.

### Phase 18 (area B: one runtime per widget with one width owner, drags that end with their rows, and resolves that name their window)

Round 18 of the review, area B, on `claude/sm-review18-b`, plus the widget
items of area C's review (2, 3, 5, 8, 10) and area A's widget query
tokens. The numbers are area B's ranked findings unless marked.

**A widget drawn in two windows has one width owner (1, 3; dodges).**
`WidgetEntry` kept the window, view and width of whichever window
reconciled last, so the same chat in main and in a narrower chat window
flipped the runtime's width every pass, each relayout reported a height
that redrew the app, and the loop ran at display rate. Where a widget is
drawn is now one value, `Presence` (`widgets_state.rs`):

- `Hidden { since }`, or `Shown { owner, at, others }`. The first window
  to show a widget owns it: only the owner's pass moves the runtime's
  width and theme, and the widget ticks in the owner's frames. Other
  windows draw the owner's frame scaled down into their own card
  (`widget_bubble::frame_scale`, pointer coordinates scaled back), and a
  new frame notifies every window's view. The owner leaving hands the
  widget to another window at that window's width; the last one leaving
  hides it. This also replaces the hand-kept `visible: bool` +
  `hidden_at: u64` pair, and `wants_frames` is the runtime's own report
  again (ticks are gated on the owner, not forced off on hide).
- `input_window` records the window of the widget's last press
  (`widget_pointer_down`, with the pressing window's state in place). Its
  links (`as_asked_by`), its link confirm and its copy toast go to that
  window while it still draws the widget, else to the owner (3).
- `Lifecycle<R>` (src/message_widget) holds the runtime in its live
  stages, `Loading(R)` and `Running(R)`, so `runtime: Option<Runtime>`
  next to the lifecycle is gone; `started()` moves a loading runtime to
  running.
- `WidgetsUi.landing: bool` is `lander: Option<Task<()>>`: the screen
  owns its landing loop, which ends with it.

**Hidden widgets cost nothing (C2, C3).** The runtime thread keeps the
visibility it is told and does not paint while hidden (a whole frame on
coming back), and `land_widgets` neither notifies the views of a hidden
widget (its `Presence` has none) nor redraws the app for its height or
stop. The `wants_frames` compare is against the stored value, so a hidden
animating widget no longer marks itself changed on every land.

**Widget query tokens are unique for the run (area A; C5).** The counter
lived in `WidgetsUi` (per `OnScreen`) and restarted at 0 on every switch,
so a late answer from the account's last visit matched a new query after
switching A, B, A. It is a process-wide counter
(`widgets_state::next_query_token`, listed in scripts/ui-statics.txt). A
stale answer still in `bots.widget_answers` drains into no matching query
and is dropped; nothing in the session needs clearing.

**A chat window's widgets go off screen under the lock (C8).** The locked
render of a chat window draws no history, so it now calls
`widgets_left_window(Some(chat))`, as the main window does: the widgets
stop running and polling.

**A logged-out account's widgets stop (C10).** `forget_widget_storage`
drops every entry (and with them the runtimes) and sets
`WidgetsUi.storage` to `WidgetStorage::Forgotten`: no widget starts and
`widget_dir()` is `None` until the screen is replaced, so nothing can
write the removed folder back.

**A pinned drag ends with its rows (2, 4; older dodge).** The drag ends
only through the chat rows' listeners. With Contacts, Calls or search in
their place the release never arrived, the row stayed lifted, the settle
check asked for 60 fps forever and the next release sent the abandoned
order. `sidebar_ready` now drops the drag whenever something else takes
the rows' place, and `set_chat_list_view` drops it on a view change; a
drag move in a view that does not show its pinned list
(`pinned_list_shown`, shared with the rows' draggable check) drops it
instead of starting one. `pin_reorder` + `pin_reorder_archived` are one
`Option<PinDrag { archived, reorder }>`.

**A forward's answer never ends a forward (5).** Every send ends the
stage (the share box, the bar's Send, and now the demo bar path too), so
an answer that finds a stage finds a newer forward, even from the same
chat and topic. `present_forward_result` shows the result and leaves the
stage alone.

**The reply chooser is a forward stage (dodge).** `ShareUi::
reply_elsewhere_open` sat next to `ForwardStage::Picking`; the two boxes
share the box's selection and search field, and opening the forward box
never closed the chooser. It is `ForwardStage::ReplyChooser(under)`, over
the forward that was there: `open_reply_chooser` closes the forward box
first, `open_picker` closes the chooser first, and `close_reply_chooser`
returns to what was under it.

**`panel_mode` is private (dodge).** The field is private to
`media_panel`, so no other module can write it at all; inside the module
`scripts/single-writers.txt` still holds it to `set_media_panel_mode`.

**The D-Bus connect never runs on the UI thread (6).** Closing a chat's
toast (`close_linux_notification`, UI thread) reads
`connected_notifier()`, which never connects: with no connection no toast
went out over D-Bus. Only the toast worker connects
(`notifier_on_worker`), so the UI no longer waits on the retry gate or on
a hanging daemon's D-Bus timeout.

**The link-preview prefetch is armed once per URL (7).** Every frame
bumped `preview_token` and spawned a new 500 ms timer, so while anything
animated (a spoiler at 30 fps) the request never went out.
`ComposerUi.preview_timer: Option<(String, Task)>` holds the URL waited
on and its timer; a frame with the same URL leaves it running, a new URL
replaces it.

**A resolve names its window (8).** Five sites (the bot-limit toast's
"@BotFather", Create Bot, Edit Bot, an inline result's Start, Premium's
bot purchase) called `driver.resolve_deep_link` directly, so the flow had
no window and the chat opened in a main window that may be minimized,
behind the chat window that asked. `resolve_deep_link_here` sets the
flow's window to the asking window as it sends; `hold_resolved_link` then
opens the chat there and raises main when needed.
`scripts/main_frame_queues.py` (run by `check-window-scope.py`) rejects
`.resolve_deep_link(` anywhere in src/ui but `deep_links.rs`.

Tests: `widgets_state_tests.rs` (the owner's width never moves for a
second window, the owner leaving hands on, scaling, unique tokens),
`Lifecycle` holding its runtime (`message_widget/tests.rs`),
`forward_stage.rs` (the chooser and the box never both open),
`chat_ui_tests.rs` (a late answer keeps a newer selection of the same
topic), `chat_list/pin_drag_tests.rs` (Contacts over the rows and another
folder each end a drag unsaved), the prefetch timer helper, and the
script's own unit tests. Not tested here: the D-Bus close (Linux, needs a
daemon) and the resolve seam's window (needs a live driver; the script
guards the shape).

### Phase 15 (area C)

Round 15 of the review, area C (the rest of `src/ui`), on
`claude/sm-review15-c`. The numbers are the review's finding numbers. All
twelve were fixed, and so were the three sets of fields kept in step by
hand. Four of the findings (1, 4, 5, 6) were round-14 fixes that did not
go far enough. This round fixes them at their source.

- **A Business screen waits for the saved value (1).** `saved_business`
  is `None` until the own user's full info has arrived. A draft is built
  only from a saved value (`new_*_draft(saved, ..)`). A screen opened
  before the info arrives shows "Loading…" (`business_waits_for_info`),
  and the frame upkeep of the window hosting the box builds the draft at
  the first frame after the info is stored (`business_info_upkeep` in
  `per_window_upkeep`). The screen is derived from the state, with no
  waiting task and no poll. Nothing is ever built from empty values, so the `base` comparison from round 14
  now compares against the real saved value. Hours fall back to the
  saved zone when the zone list has not loaded.
- **A second call under the lock (2).** The swap prompt is a kit box of
  the locked main window, so it is not drawn. The lock screen now draws a
  card for it (`swap_call_lock_card`, Decline and End & Answer) next to
  the background ring's card. Both use one card builder,
  `lock_call_card`, and `locked_root` stacks them. The wording follows
  tdesktop's call panel: "Incoming call" or "Incoming video call", then
  "Name is calling you…" (`swap_call_text`, also used by the swap box). The second call also
  rings: `sync_call_sounds` loops Incoming while the swap prompt shows.
- **An invitation alone keeps the call tick (3).** `call_tick_wanted`
  (call on screen, background ring, or conference invitation) is the one
  test for both starting and keeping the tick. Before this, an
  invitation with no call started the tick and then stopped it at the
  first beat.
- **"A lock in between" is a counter (4).** `PasscodeUi::locks` counts
  locks, and it moves only with the flag (`set_lock_flag`, called by
  `set_locked` and `force_locked`). A pending profile gallery records the
  count and the info panel it was asked from. `locked_since(count)` drops
  it after any lock, even when Quill is unlocked again by the time the
  photos land. A panel that closed or changed drops it too
  (`open_info_target`).
- **A parked clip belongs to a chat visit (5).** `ChatUi` carries a
  `ChatVisit` token, and `replace_chat_ui` is now its only writer
  (`enter_chat_ui`, `leave_chat_ui`, `sync_chat_ui`). Replacing the value
  drops the play intents of the visit that ended
  (`drop_play_intent_of_visit`). The resume also checks that the window
  still shows that visit. Esc followed by reopening the same chat is a
  new visit, so the clip does not start by itself. The hand drops in the
  chat change paths are gone.
- **Invite pages are one value (6).** `Session::invite_pages` is an
  `InvitePages` value that belongs to one chat. It holds that chat's admin
  links, a link's join requests and its members. A page for another chat
  replaces it whole. The Group Settings box holds the details link and
  the revoked toggle (`GroupSettingsDialog::details_link`,
  `revoked_open`). `set_group_settings_dialog` is the box's only writer,
  and it clears the pages when the box closes or changes chat. The two
  `AdminUi` fields and the three maps are gone.
- **Play intents at the lock follow tdesktop (7).** tdesktop keeps a
  voice note or song playing under the lock and disables only the OS
  controls. Its lock closes the media viewer and the call layers. So the
  lock drops parked clips, both the viewer's and the inline ones
  (`drop_play_intents_for_lock` in `close_for_lock`). A parked voice note
  or song stays, and `poll_live` resumes it under the lock (a
  `runs-locked` line in lock-surfaces.txt). Clip targets that are not
  dropped are also refused while locked.
- **Every Settings link goes through one seam (8).** `open_settings(page,
  cx)` hands off to the main window first (`hand_to_main_window`), so a
  link opened from a chat window's box or menu shows Settings in the main
  window. The Privacy entry no longer opens and then switches the page
  in two steps.
- **Redial places the call (9).** The call window's Redial and the end
  card's Call again call `redial`, which dials at once, as tdesktop's
  panel does (`Call::redial`). The confirm box opened in the main window,
  hidden behind the call window that had just closed.
- **A clip error belongs to its window's viewer (10).** `PlaybackUi::error`
  moved to `ViewerUi::clip_error`, which is swapped per window with the
  rest of the viewer.
- **A list keeps its place while its items change (11).** The
  shared-media page key is the chat, the anchor day and the query's hash.
  It no longer includes the newest item, so deleting the newest file no
  longer scrolls the list to the top. Each tab's scroll, paging mark and
  page are one value (`GridTab`, `ListTab`) instead of three arrays kept
  in step.
- **The lock closes the passkey ceremony (12).** As tdesktop's lock does
  with its boxes, `close_passkey_ceremony_for_lock` cancels the key and
  removes the PIN field. A PIN sheet on its way can no longer take focus
  from the passcode field.
- **The held connect names its account.** `deferred_connect: bool` became
  `connect_at_unlock: Option<AccountKey>`. The unlock connects only the
  account that waited, and only while it is still on screen, so
  `adopt_live` no longer has to clear it by hand.
- **Guards.**
  - `scripts/single_writers.py` (run by `check-screen-state.py`, with the
    list in `scripts/single-writers.txt`) fails a whole write of
    `chat_ui` or `group_settings_dialog` made anywhere but its one writer.
  - `lock_surfaces.py` now accepts `set_lock_flag` as the flag's setter,
    and only `set_locked` may call it.
  - CI runs the unit tests of both checks.
- **Tests.**
  - `a_screen_waits_for_the_full_info_before_it_can_be_changed`
  - `a_second_call_under_the_lock_rings_and_can_be_declined`
  - `an_invitation_alone_keeps_the_call_tick`
  - `the_group_avatar_opens_its_photos_newest_first` (extended with a
    lock and an unlock in between, and a closed panel)
  - `another_chat_drops_a_round_video_waiting_to_play` (extended with Esc
    and reopen)
  - `a_page_for_another_chat_replaces_every_page` (state)
  - `the_box_for_another_chat_replaces_the_invite_pages`
  - `the_lock_drops_a_parked_clip_and_keeps_a_parked_voice_note`
  - `a_settings_link_in_a_chat_windows_box_opens_in_the_main_window`
  - `a_clip_error_stays_in_its_windows_viewer`
  - `the_same_list_keeps_its_place_when_its_newest_item_goes`
  - `the_lock_closes_the_ceremony_and_its_pin_field`
  - `the_unlock_connects_only_the_account_that_waited`
  - None of them waits on time.
- **Not tested.** Redial (it needs a live driver to dial). The rewiring of
  the two buttons is covered only by the build.

### Phase 16 (area C)

Round 16 of the review, area C (the rest of `src/ui`), on
`claude/sm-review16-c`. The numbers are the review's finding numbers. All
thirteen were fixed. Three of them (4, 8, 9) were round-15 fixes that
stopped short, and this round finishes them at the source. The parked
voice note or song that resumes under the lock (round 15, #7) stays as it
is, since tdesktop does the same.

- **Edit Contact waits for the note (1).** The box knows whether its note
  is known (`ContactNote::{Waiting, Seeded}`). Opened before the contact's
  full info, it asks for it, shows "Loading…" as the note's placeholder,
  and the frame upkeep of the window hosting it seeds the note when the
  info lands (`edit_contact_note_upkeep`), keeping anything typed
  meanwhile. Save while waiting with an empty note asks again and keeps
  the box open, so an unknown note is never saved over the real one.
- **The account's farewell flushes the community undo (2).** `AdminUi`'s
  farewell calls `flush_and_close_community_box` first, so an answer to a
  Pending Requests entry still in its undo time is sent for the account
  that made it, as closing the box does.
- **Earn paging is one state (3).** `PageLoad { Unasked, Loading, Ready,
  Failed }` replaces the `loaded`/`loading` bools of the TON and Stars
  histories and the affiliate program lists. A failed page is not asked
  again by the scroll upkeep (`may_page`); the program list shows a Retry
  row (`programs_retry_row`), and a failed first page retries from the
  start.
- **The passkey tick refuses under the lock (4).** The tick is a task the
  passkeys state holds (`PasskeysUi::ticker`), not a bool. No tick starts
  while locked, `tick_passkey_ceremony` refuses and closes the ceremony,
  and the lock drops the task and the session's ceremony state with the
  box (`close_passkey_ceremony_for_lock`).
- **Reset All keeps the scopes (5).** TDLib answers
  `resetAllNotificationSettings` after sending the updates with the reset
  scope settings, so clearing the scope cache at the `ok` dropped the
  fresh values. The `ok` now changes nothing.
- **A drag-pan ends with its button (6).** A move with no left button
  held ends the viewer's drag (`viewer_drag_moved`), for a release off the
  picture whose mouse-up never fired, and closing the viewer drops it.
- **The public links box lists afresh (7).** Opening the limit box resets
  the created public chats unless a load is running, the "too many public
  links" error opens the box the same way, and a failed load shows Retry
  (`retry_created_public_chats`) instead of an empty list.
- **The Business wait is a Fetch (8).** `BusinessSettingsState::own_info`
  is a `Fetch<()>`: begun when a Business screen opens without the own
  full info, settled when the info lands (`settle_own_full_info`), failed
  by the error answer or a send error (`fail_own_full_info`). The waiting
  screen shows the error with a Retry button (`retry_business_info`), so
  it no longer says "Loading…" forever.
- **The call window answers a second call (9).** The swap box is
  session-wide and shows in the window of the last press, which can be a
  main window hidden to the tray or behind the call. The call window now
  draws the swap card too (`swap_call_window_card`, from the same builder
  as the lock card), locked or not.
- **Notification exceptions are a Fetch per scope (10).**
  `notification_exceptions: HashMap<Scope, Fetch<Vec<i64>>>` replaces the
  list plus the `loading` set. A failed load shows its reason and Retry
  (`retry_notification_exceptions`), and the Notifications page asks for
  every scope it shows.
- **The bot verify box starts clean per chat (11).** Moving to the Add
  step resets the description to the bot's default
  (`set_bot_verify_step`), so text typed for one chat does not carry to
  the next.
- **Community outcomes stay with their screen (12).** `put_on_screen`
  drops the communities' finished-request outcomes
  (`forget_outcomes`), so a toast for the account that left does not show
  on the next one.
- **A let-go soundtrack is tapped again (13).** A clip whose audio output
  let go of its tap (no device at attach, or the device was lost) gets a
  new tap once an output is up (`Player::reattach_audio_tap`, called from
  `attach_sound`), instead of running silent on its wall clock.
- **Fields kept in step by hand.**
  - `PasscodeUi::locked` + `locks` -> `LockState { Unlocked { locks },
    Locked { locks } }`; `set_lock_flag` is still the only writer, and
    `lock_surfaces.py` checks writes to `lock_state`.
  - `CommunitiesUi::pending_ban` + `awaiting_new_chat` -> one
    `waiting: Option<CommunityWait>`; the chooser closes through
    `flush_and_close_community_box`.
  - `CommunitiesUi::requests_scroll` + `requests_asked` ->
    `RequestsView`, keyed by community (`of`, `reset`).
  - `AdminUi::event_log_filter` moved into the Group Settings box
    (`GroupSettingsDialog::event_log_filter`), dropped with any other page.
  - `InvitePages` names no chat once its last page closes
    (`drop_chat_if_empty`).
  - `GroupSettingsDialog::revoked_open`: its doc now says it is the
    disclosure only (the fetch is `InviteLinkFetch`).
  - Not converted this round: the media viewer clip and controls fields,
    the Premium hub's parallel fetch fields, `AudioEngine`, `PlaybackUi`,
    `ThemeGifts`, `ton_unavailable`, the Open QR bools,
    `BotCreateDialog::submitted`, `BotEditDialog::filled`, the mute box
    and `tone_upload_target`, and the groups-in-common maps. They stay in
    `scripts/implicit-state-baseline.txt`.
- **Guards.** `scripts/lock-surfaces.txt` pins `tick_passkey_ceremony` as
  `refuses`. `scripts/lock_surfaces.py` treats a write to `lock_state` as
  a lock-flag write. The baseline lost the earn `loaded`/`loading` lines.
- **Tests.**
  - `edit_contact_waits_for_the_note_before_saving`
  - `a_note_typed_while_waiting_is_kept`
  - `an_answer_in_its_undo_time_is_sent_when_the_account_leaves`
  - `a_failed_page_is_not_asked_again_by_itself`
  - `a_failed_first_page_retries_from_the_start`
  - `no_ceremony_ticks_under_the_lock`
  - `reset_all_notification_settings_keeps_the_scopes_its_updates_set`
  - `a_drag_released_off_the_picture_stops_panning`
  - `the_public_links_box_lists_the_chats_afresh_each_time`
  - `the_business_wait_ends_with_the_own_full_info_error_or_answer`
  - `the_call_window_answers_a_second_call`
  - the notification exception tests, moved to `Fetch`
  - `a_description_typed_for_one_chat_does_not_carry_to_the_next`
  - `outcomes_of_a_left_screen_are_forgotten`
  - `a_let_go_soundtrack_is_tapped_again`
  - `closing_the_last_page_names_no_chat`
  - None of them waits on time.
- **The flaky `settings::media` test** was already fixed on the merged
  base (`Start the media prefs test from an empty folder`).

### Phase 17 (area A)

Round 17 of the review, area A (core outside `src/ui`). All three findings
were fixed, and the round-16 `own_info` dodge was converted.

**An agent send gets only its own outcome (1).** TDLib numbers messages
still being sent per chat (`MessagesManager::get_next_message_id`), and
scheduled sends share `ScheduledServerMessageId(1)` per date. `agent::events`
matched `updateMessageSendSucceeded/Failed` by client and temporary id
only, and kept entries after a waiter read them. So a send could get
another chat's delivery or error, or an older send's outcome. The log now
stores each outcome with its chat and a sequence number. `send_inner` takes
`events::send_mark()` before the request goes out, and `wait_sent(client,
chat, since, ids, ..)` accepts only outcomes for that chat recorded after
the mark. The activity page's Undo also offers only the user's own
messages: `agent_undo_for` drops any logged id whose loaded message is not
outgoing. An id that isn't loaded is still refused by
`ConnectDriver::delete_selected`, which needs the message in the history.
This also covers logs written before the fix. Tests:
`a_send_gets_only_its_own_chats_outcome_after_it_went_out`, the old
`send_outcomes_reach_their_waiter` (it no longer sleeps), and
`undo_never_offers_someone_elses_message`.

**Approve all doesn't bring handled join requests back (2).** This is the
round-16 #4 shape for the groups lists. On the `ok` of
`processChatJoinRequests`, `Session::drop_chat_join_request_pages` takes
every in-flight `GetChatJoinRequests` and `GetMoreChatJoinRequests` for the
chat out of the registry (the new `RequestRegistry::take_purpose_for_chat`)
and clears `join_request_latest`. A late page then has no pending request.
Only clearing `latest` would not be enough, because the answer arm accepts a
missing `latest`. The link-list arm also takes its `GetLinkJoinRequests`
pages and clears `request`. `LinkRequestsState::loading` was that same
stage kept by hand, so it is now `loading()`, derived from `request`, and
its two baseline lines are gone. Tests:
`approve_all_drops_the_chat_wide_page_in_flight` and
`approve_all_for_a_link_drops_its_page_in_flight`.

**A refused approve keeps the list (3).** The `processChatJoinRequest`
error no longer writes `JoinRequestFetch::Failed`.
`Session::fail_join_request` keeps both loaded lists, puts the line in
`invite_link_error` (shown as a toast), and removes the row when the server
says the request is gone (`HIDE_REQUESTER_MISSING`,
`USER_ALREADY_PARTICIPANT`). The success path and the "gone" path share
`forget_join_request`, which removes the row from the chat-wide list and the
link list. Test: `a_refused_approve_keeps_the_list`. Demo:
`ready-join-request-refused` (`QUILL_DEMO_JOIN_ERROR=other` for a refusal
that keeps the row).

**Dodge: the Business wait is derived (round 16).**
`BusinessSettingsState::own_info: Fetch<()>` was replaced by
`own_info_error: Option<String>`, which holds only the last failure.
`Session::business_own_info()` derives the stage: Ready when the own full
info is cached, Loading while a `getUserFullInfo` for it is in flight,
otherwise Failed with the stored reason, or Idle. `settle_own_full_info` and
its two callers are gone. Test:
`the_business_wait_ends_with_the_own_full_info_error_or_answer`, which now
also checks that dropping the info stops it reading as ready.

**Checked, not a bug: an Allow that widens grants.** Reviewer C asked
whether an Allow that widens grants refuses the call that asked for it
(`still_allowed`, `agent/tools/mod.rs`). It doesn't. A widening ask (`Ask::Chat`, `AllChats`,
`Contacts`) comes only from `request_access` (`tools/ask.rs`), and that
call returns on Allowed without calling `still_allowed`. Writes then run
as new calls with a new `CallScope`. The asks that do run `still_allowed`
after Allow are `Ask::Write` and `Ask::Raw`. `apply_ask` changes nothing
for those, and `grants::update` keeps the generation on a no-op (Phase 16
#1). What remains is a write from another, concurrent call that is waiting
when the user widens a grant. It is refused with `GRANTS_CHANGED_MESSAGE`
and works on retry. That is a spurious refusal, never a write that goes
out without a grant.

**Guards.** None added. Matching an outcome to its send and dropping pages
after a bulk reset are about what each request means. A script reading
the source text can't check them.

### Phase 17 (area C)

Round 17 of the review, area C (the rest of `src/ui`), on
`claude/sm-review17-c`. The numbers are the review's finding numbers. All
twenty-four were fixed, along with the three dodges the review named and
the gift-box answers it flagged outside the area. Two of the findings
(1, 22) were regressions from round 16's let-go soundtrack fix. Several
shared one shape: a chat window's ask queued for a frame only the main
window draws. Those are now fixed at the seam, and a guard checks the
seam.

- **Clip audio asks only when it can help (1, 22).** The FFmpeg backend
  reattaches a tap only when the player's sound is detached
  (`Player::sound_detached`: a soundtrack exists and its tap was let go).
  A silent clip never asks. The output answers through
  `SharedOutput::ready_or_wake`, which says yes only once a device is up.
  Before this, every frame opened the device for a silent clip, and with
  no device a clip with sound reattached (and so seeked) every 5 s.
- **Admin rights are asked afresh (2).** Opening the rights editor drops
  a `Loaded` answer and asks again (`refresh_admin_rights`), and the
  editor seeds from the cache only in demo mode. Save no longer writes
  back rights changed elsewhere since the first load.
- **A new native window gets the capture block (3).** Attaching a main
  window (`attach_main_window`) forgets what the old native window was
  given (`FrameUi::forget_native_window`). The block is applied again
  after a Windows GPU-loss reopen.
- **Main-window queues raise the main window (4, 13).** Mini app actions
  (t.me links, the link confirm, inline switch, bot chat, invoices) and
  video chat "Share to" queue work for the main frame. They now go
  through `raise_main_window_later`, which restores a minimized or hidden
  main window, instead of `cx.activate`. The video chat's private
  `raise_main_window` is gone.
- **The location picker is a box with a host (5).** It opens through
  `open_box(DialogKind::LocationPicker)`, and its own `opened_in` field
  is gone. A second window that asks moves the picker there, and sends go
  to that window's chat.
- **A call that fails to start reports where Call was pressed (6).**
  `CallBoxes::card_window` holds the pressing window
  (`asking_window_or_last_press`), and the call card draws only in that
  window (chat windows draw `call_overlay` too).
- **A second call reopens a closed call window (7).** The call window
  remembers which ringing call it was raised for (`raised_for:
  Option<i32>`, replacing `window_raised: bool`). A new ringing id raises
  it once, even after the user closed it.
- **Gift market boxes keep a purchase in flight (8) and start clean (16).**
  Popping the Buy or Resale box keeps a buy that is sending. An empty
  stack resets "Hide My Name".
- **Group Edit reports non-field failures (9).** Upgrades and the
  history, auto-translate and sign-messages toggles record their failure
  as `GroupInfoField::Settings`, shown under the form, so the box no
  longer closes as saved. Saving the type clears earlier errors first.
- **The paid-messages save names its target (10).** `PaidSave { Idle,
  Saving(target), Failed { target, line } }` replaces `saving` and
  `error`. The error shows only in the chat or channel it was for.
- **Business sticker pick closes the sticker panel (11),** on a pick, on
  leaving the page, and when Business closes.
- **A refused chatbot save is forgotten (12).** The connected bot is
  cleared on the error, so the next visit asks the server again.
- **Secret-chat clips stay out of PiP (14).** `viewer_may_pip` refuses a
  secret chat's item, and the PiP button is hidden for it.
- **Mini-app answers carry their bot (15).** The `found` and
  `attachment_menu_bot` answers hold the bot id from the purpose, and the
  poll takes only the answer for the bot it asked about.
- **A refused invite link create keeps the box (17).** Only `Ok(Some)`
  counts as sent. Otherwise the box comes back with an error note.
- **Opening Hours Back retries the zones (18).** Leaving with no zone
  list asks for it again. The page shows both the local line and the
  server's error.
- **A mini app's data send is reported when it answers (19).**
  `Session::web_app_answer_due` keeps the mini-app poll alive while a data
  send or a menu toggle is pending.
- **The call device menu closes outside an active call (20).**
- **A sticker-set video pick opens in its own window's frame (21).**
  `sticker_box_window_upkeep` runs per window and drops picks whose
  window closed. It replaces the main-only `sticker_box_frame_upkeep`.
- **New passkey options drop unstarted ones (23).** `begin_register`
  clears the ceremony before asking, so options that landed after the lock
  never start a second sheet.
- **Edit contact saves without the note (24).** A note the box never
  learned is sent as null, which TDLib reads as keeping the current note.
  Save works after a failed full-info fetch, and a contact whose names
  and note are unchanged sends nothing.
- **Dodges from round 16.**
  - `GroupSettingsDialog::event_log_filter` is gone. The filter box is the
    payload of `GroupSettingsView::RecentActionsFilter(Box<…>)`, so it
    cannot outlive its page (the view is `Clone`, no longer `Copy`).
  - `EmojiStatusFlow::asking` + `granting` -> `AccessStage { Idle, Asking,
    Granting }`.
  - `CommunityWait` is gone. A waiting ban (`ban_wait`) and a new chat for
    a community (`new_chat_for`) are separate slots, so starting a new chat
    no longer drops a ban waiting for the member's chats.
- **Gift box answers belong to their box (outside item).**
  `SendGiftState::{gifts_request, premium_request}` hold the ids the box
  asked with. An earlier box's gift list, Premium options or their
  failure are dropped (`asked_gifts`, `asked_premium`).
- **Guards.**
  - `scripts/main_frame_queues.py`, run by `check-window-scope.py`: a
    write to `mini_apps.render_action` or `links.pending_deep_link_open`
    needs `raise_main_window_later` (or `hold_resolved_link`) within 12
    lines, or a `// main-frame:` marker that says why not.
  - Capture-block tests read what was handed to the native window through
    a test-only probe (`SHARING_APPLIED`).
  - The baseline lost `PaidMessages::saving` and
    `PaidMessages::trust_loaded+error`.
- **Tests.**
  - `a_silent_clip_has_no_detached_sound`
  - `ready_says_no_while_the_device_opens_or_after_it_failed`
  - `refreshing_admin_rights_drops_a_loaded_answer_and_asks_again`
  - `a_reset_hands_the_capture_block_to_the_new_window`
  - `an_invoice_request_brings_the_main_window_forward`
  - `a_location_picker_opened_again_moves_to_the_window_that_asked`
  - `a_call_that_fails_to_start_reports_in_the_window_that_pressed_call`
  - `a_second_call_reopens_a_call_window_you_closed`
  - `closing_the_buy_box_mid_purchase_keeps_the_purchase`
  - `hide_my_name_does_not_carry_to_the_next_auction`
  - `a_refused_upgrade_is_recorded_for_the_box`
  - the paid-messages tests, moved to `PaidSave`
  - `leaving_the_sticker_pick_closes_the_sticker_panel`
  - `a_refused_bot_save_is_forgotten_and_asked_for_again`
  - `a_secret_chats_clip_never_goes_to_picture_in_picture`
  - `a_search_answer_names_its_bot_even_when_refused`
  - `a_data_send_is_due_until_it_answers`
  - `a_new_options_request_drops_unstarted_options`
  - the Edit contact note tests (a null note keeps the current one)
  - `an_earlier_boxs_gift_answers_are_dropped`
  - None of them waits on time.
  - No test for 13, 17, 18 and 20. They need a live connection or a
    native window that the offline harness lacks.

### Phase 18 (area A)

Round 18 of the review, area A (core outside `src/ui`). Findings 1 and 2
were fixed and the `SendGiftState` dodge was converted. Finding 3 (widget
query tokens restarting on an account switch) moved to the area-B fixer,
who rewrites `ui/widgets_ui.rs` this round.

**Approve all on one link leaves another link's list alone (1).** The
`ok` of `processChatJoinRequests` for one invite link emptied whatever
link list was open for the chat and took every in-flight
`GetLinkJoinRequests` page. Opening link L2 before L1's Approve all
answered left L2 empty, total 0 and not loading, with its page dropped.
`GroupsPurpose` is `Copy`, so the link rides the pending request instead:
`PendingRequest::invite_link`, stamped by
`Session::request_for_invite_link`. The ok arm empties the open list only
when its `invite_link` matches, and takes only that list's page in flight
(`state.request`). The chat-wide list and badge refetch as before. Test:
`approve_all_for_one_link_keeps_another_links_list`; the old
`approve_all_for_a_link_drops_its_page_in_flight` now stamps its link.

**A get answered after a set no longer flips "Hide read time" back
(2).** `settings.read_date_loading` was one bool for "a get or a set is in
flight", cleared by whichever answer came first. TDLib sends no update for
this setting, so a `getReadDatePrivacySettings` answer that landed after
the click wrote the old value over the optimistic one and stayed. It is
now `settings.read_date_stage: ReadDateStage` (`Idle`, `Getting(id)`,
`Setting(id)`, `Failed`), which also replaces `read_date_error`. Only the
answer or error of the request named in the stage lands. A set supersedes
a get in flight, and `fetch_read_date_privacy` sends nothing while a set
is in flight, because the set already holds the value the server will
have. Test: `read_date_get_answered_after_a_set_is_ignored`; the old
roundtrip test reads the stage.

**The send-gift lists are one stage each (dodge).** `SendGiftState` kept
`gifts_loading`, `gifts_loaded` and `gifts_request`, and `premium_loaded`
with `premium_request`, in step by hand. Each list is now one
`GiftListFetch` (`NotAsked`, `Asking(id)`, `Answered`, `Failed`).
`asked_gifts` / `asked_premium` match the id in `Asking`. A Premium
options request that fails to send now ends as `Failed` instead of
staying unanswered. "No gifts are available" shows only after an answer,
not under the load error line. `sending` was the same pair with
`request`; `SendGiftState::sending()` now reads `request`. The baseline
lost the four `SendGiftState` lines and `SettingsState::read_date_loading`.

Neither fix has a visible result in the offline demos: both are races
between two TDLib answers, which the demos do not replay. None of the
tests waits on time.

### Phase 18 (area C)

Round 18 review of the rest of `src/ui`. Fixed here: findings 1, 4, 6, 7, 9
and 11 and the listed dodges outside the widget code (findings 2, 3, 5, 8
and 10 are widget items another fixer owns).

- **Stream boxes belong to their call (1).** `StreamWindow::close_boxes`
  closes the box over the stream window and hides the revealed key. It runs
  from `sync_group_call_boxes` when the call tracked changes, beside the
  other boxes the call owns, and from `StreamWindow::clear`. End, Revoke and
  the key warning therefore never act on the stream that replaced the one
  they were opened for.
- **Stream photo (6).** A chat without a photo now clears the blurred photo
  instead of returning early and leaving the previous chat's behind it.
  (The cache is keyed by the photo's file path, which is already unique per
  photo, so no chat id was added to the key.)
- **Auto-translate refusal (4).** `ToggleSupergroupAutoTranslate` is a
  Messages purpose, so `apply_messages_error` now also calls
  `record_group_info_edit_error`; the Edit box keeps its Settings error
  instead of closing as saved.
- **Proxy list box (7).** The box is the sign-in pane's: its dialog is
  drawn only while `pane_mode() == Connecting` (`proxy_list_box_shown`),
  and `sync_proxy_list_box` clears the bool each frame once the pane is
  gone, so it neither covers the chat list nor returns on a later
  reconnect.
- **Agent filter (9).** `AgentUi::pick_account` is the one way to change
  the picked account; a different account resets the activity filter.
- **Premium preview files (11).** The two polling tasks are gone.
  `sync_premium_preview_files` runs each main frame while a preview page
  shows and asks for the clip and the example stickers that are not on
  disk (`download_file` skips requested and finished files), so a slow
  answer is picked up whenever it lands.
- **Dodges converted.**
  - `stream_joining` + `stream_join_timeout` are one field,
    `Option<StreamJoining { call, timeout task }>`; dropping it ends the
    wait.
  - `hide_armed` + `hide_task` are `StreamWindow::hide_timer`, the task
    itself.
  - `StoryViewerUi::cover_sent` / `privacy_sent` are `cover_request` /
    `privacy_request` (the request id). `StoryManageState` keeps the last
    settled call, and `outcome(request)` says InFlight, Saved, Failed or
    Lost, so another call's answer never closes the panel.
  - `language.packs` + `packs_loading` are `Fetch<Vec<LanguagePackInfoData>>`;
    a refused list now ends the wait and keeps its reason.
  - `SendGiftState` was converted by area A.
- **Baseline.** Lost `LanguageState::packs_loading` (and `+error`),
  `StoryViewerUi::cover_sent`, `StoryViewerUi::privacy_sent` and
  `StreamWindow::hide_armed`.
- **Tests.** `a_switch_to_another_stream_closes_the_boxes`,
  `the_proxy_list_box_ends_with_the_sign_in_screen`,
  `picking_another_account_resets_the_activity_filter`,
  `a_refused_pack_list_stops_loading_and_says_why`, and the Auto-translate
  refusal assertion in `channel_auto_translate_is_parsed_gated_and_toggled`.
  None waits on time. No test for 6 and 11 (they need a photo file and a
  download answer the offline harness cannot produce).

### Sweep fixes (s12-ui)

- **Window targets.** The `/` menu and the quick-reply row read
  `viewed_chat()` (the window's chat), not the main window's `open_chat`
  (12-049). The music row's play button carries its message's chat
  (11-p3-001).
- **Confirms act on what was confirmed.** Manage Topics Delete opens the
  `DeleteForumTopic` box (10-068). The bulk "Delete chats" box carries its
  chats in `GroupConfirmAction::RemoveSelectedChats { ids }`, captured when it
  opens (09-050, 10-087).
- **Failed pages wait.** `TopicHistory::older` (`OlderEnd`) keeps a failed
  older thread or topic page from being asked again on every render (06-001,
  06-002). It is also the topic's first-page stage: `topic_load_failed` is
  gone, "Couldn't load messages" is `older == Failed` with nothing loaded, so
  Retry (`retry_topic_history`, `retry_topic_older`) reopens the one stage and
  a failed jump to the latest no longer ends in "No messages in this topic".
  The thread's first page is `ThreadStatus::Failed`, already one stage. A
  failed next inline page (`InlineQuerySlot::more_failed`), story viewers
  page, similar-channels list and folder-invite chat are asked once (10-121,
  13-144 with a Retry button, 13-044, 09-110: a chat counts as asked only
  after the ask was sent). The installed-wallpapers list is a `Fetch` with a
  Retry, on the Appearance page and in the chat look box (07-035). Thread paging notifies only when a request went out (13-104).
  A failed older page of a non-empty thread or topic draws a Retry strip
  over its rows (`conversation/older_retry.rs`); jumping to the latest or
  reopening clears it too.
- **Ends in an error.** Statistics: a chat that cannot show them, or a send
  that failed, lands in the Failed state the panel draws (13-021). A chat
  whose `supergroupFullInfo` is not known yet is `WaitingForInfo { is_dark }`:
  the ask for the full info goes out, and the update that stores it
  (`resume_waiting_statistics`, run where the paid-prices sync runs) sends the
  statistics request or fails the panel; a failed full-info request fails it
  too (13-022). No polling. A
  refused live-story join shows the Join button (13-089); the viewer's join
  is one enum, `LiveJoin` (`Idle`, `Watching { target, seen_call }`,
  `Refused(target)`), not `watching`, `seen_call` and `refused_join`.
- **Smaller.** A language switch whose send fails clears `switching`
  (02-133); `updateChatPendingJoinRequests` with null parses as zero
  (06-050); the folder editor is `FolderLoad` (`Loading`, `Failed`, `Ready`): Save
  waits for `Ready`, a failed `getChatFolder` (`folder_spec_errors`) shows its
  error with Retry instead of loading forever (09-104, 09-X052); 08-049 is
  skipped: tdesktop's `ContactStatus::setupReportHandler` runs one callback for
  both Unarchive-or-Report and Report spam, and both call `deleteConversation`
  (leave), so the bar reports and leaves as before; a story
  text reply keeps the chat's draft (07-005); a second edit keeps the real
  draft (08-127); Promote forgets a selection the new search page hides
  (10-078); each export worker call carries a call number and its answer comes back
  under it, so a call never takes the late answer of one that timed out, nor
  the answer to a request nobody waited on; the `abandoned` flag is gone
  (03-040; the link cannot know the driver's `RequestId` when the worker
  sends, so the number is the worker's own); the spell download's worker runs
  under `run_guarded`: a panic comes back as the row's `Err` and the progress
  still ends, so awaiting the worker never panics on the UI thread (05-002);
  the agent's download re-checks access after the copy (01-013).
- **Off the UI thread.** A round video's thumbnail is made by
  `VideoNoteCapture::finish` on its background task and rides in
  `VideoNoteDraft::thumbnail`, so a story reply or chat send never waits for
  ffmpeg (03-014). Dropping a folder on the "files in folder" zone walks it
  in `run_off_ui_scoped_then` and applies the plan only if the composer's
  send slot is unchanged (09-054). Add to Dictionary and Ignore drop the
  word's underline by its text; the background check settles the rest, so
  the spell backend is never asked on the UI thread (13-057;
  `build_composer_menu` still asks it, behind the deferred menu).
- **Paid chats.** `paid_messages_guard::set_paid` makes a full
  `messageSendOptions` when a builder sent `"options": null` (media, album,
  voice and video note, checklist, plain forward), so the agreed price
  reaches TDLib (07-001).
- **Pinned drag.** A window that deactivates ends a pinned drag unsaved, so
  a lost mouse-up no longer keeps the 60 Hz redraw going (08-071). Esc does
  not cancel it: tdesktop does not either.
- **Instant View.** The page was already per window (2bae8b8); its
  browser fallback now carries the asking window (`InstantViewFallback`) and
  only that window's frame opens it; closing the window drops it (09-X047).
- **Inline results Start.** `InlineReturn` is `Opening { parameter }` then
  `Started { baseline }`, carries its account, and ends when the bot chat's
  lookup fails (`cancel_inline_opening`, from the deep-link outcome and a
  failed send), when another link ends in any chat but the bot's
  (`cancel_inline_opening_unless_chat`, an invite preview, a text or typed
  link), or when another account is on screen (10-119). No timer: a lookup
  that never answers is the deep-link flow's own concern.

### Sweep fixes (s3-groups--1)

Groups, channels and business, first half. Optimistic edits roll back
through `RequestRollback` (restored only while the screen still shows what
the edit drew); a fetch that cannot be sent ends in `Failed`, not in nothing
(which a page draws as a spinner); a stage is one enum.

- **Business bot bar** (01-052, 02-088). Pause, resume and remove record
  `RequestRollback::BusinessBotBar { previous, shown }`; the refusal puts
  the bar back unless an update changed it since.
- **Business links and bot** (02-031, 05-129, 08-005, 08-008).
  `chat_links` and `connected_bot` are `Fetch<..>` (the two `*_requested`
  bools are gone from the baseline). A failed load is drawn with Retry and
  is never asked again from a render; `ensure_*` asks once when the screen
  opens, `retry_*` asks again. A refused delete re-inserts its row
  (`RequestRollback::ChatLinkDelete`). `ensure_user_full_info` (the ask a
  start-page intro makes from render) skips users whose last ask failed;
  `fetch_user_full_info` stays the explicit retry. The failed mark is part
  of the per-user `Fetch` in `UserFullInfos` (the `user_full_info_failed`
  set is gone). A links or chatbot load that cannot be sent lands in
  `Failed` (Retry), not `Idle` (a spinner, asked again from render).
- **Link box save** (08-003). `ChatLinkSave { Idle, Saving(request), Saved }`
  replaces the `chat_links_saving` counter; the box stays on its draft until
  its own answer, goes back to the list with "Chat link saved." on success
  (frame upkeep), and a refusal returns the stage to `Idle` under the error
  line. Opening the box or the Business dialog resets a stale stage.
- **Closing Business** (08-001, 08-002, 08-029). Closing from a sub-screen
  (a day, the picker, the sticker panel) saves the screen it hangs from;
  the line a leave would show on the next screen (a refused send, "stays
  off") goes to the status line when the box closes; a save TDLib refuses
  after the box closed is shown once in the status line
  (`business_closed_error_upkeep`).
- **Protected content** (02-073). The rollback also restores the "you
  disabled sharing" record it replaced.
- **Type box** (02-120, 12-016, 05-088, 02-121, 05-086). The link check is
  one `UsernameCheck` enum set only after the send went out. A revoke
  records its `PublicLinkRevoke` after the first request, so a failed second
  send still settles it (note and a fresh list). Edits queued behind a
  basic-group upgrade report an error when there is no upgraded chat to
  send them to. The new-channel type step watches its Save's requests
  (`NewChatState::type_saves`): any refusal, a lost request included, keeps
  the step with TDLib's reason instead of moving on.
- **New chat** (04-006). A create's dialog is `CreationAsker::Dialog` or
  `Closed` (`abandon_new_chat`, from `close_new_chat`): a closed one does
  not block the next Create and does not open its group over what the
  person is doing. Closing the dialog with a create out stays allowed
  (tdesktop keeps Cancel live). A `creations` entry is dropped when its
  answer is applied: at once for a closed dialog's refusal, after the photo
  and link for its made chat, and by `abandon_new_chat` when the dialog
  takes the answer into its next step, closes, or is replaced.
- **Membership probe** (02-118). The probe after `getMe` runs for every chat
  window's chat under its own view, as it did for the main window's.
- **Permissions** (02-167). A save while `setChatPermissions` is out is
  sent; its rollback restores the values from before the first. An accepted
  save rewrites the rollbacks of saves still out to its own values, and a
  refusal restores only when no other save is out, so save A (accepted) then
  B (refused) shows A, which the server has.
- **Admins page** (05-083). A refused status or title change goes to
  `member_action_error` only (drawn above the list with Close); the loaded
  list is kept.
- **Conference invite** (05-044). The invite carries `rings_until` (its date
  plus the ring timeout); `request_tick` ends a ring that outlived it and
  declines the invitation, and `next_deadline` wakes the poll loop for it.
- **Ordinary link** (03-066). "Require Monthly Fee" is offered on a new link
  and on an existing subscription link (locked), not on an ordinary link.
  tdesktop's source is not in this session; the register entry says it offers
  the switch for new links only, which is what is implemented.
- **Group settings pages** (10-042). The invite links, join requests and
  administrators fetches store `Failed` when their send fails, drawn with
  the page's Retry.
- **Recent actions** (09-094, 09-093). The search box shows the stored query
  of the chat whose log is open (`EventLogSearch { input, shows }`, refilled
  by frame upkeep); the "load older" mark is per chat and cleared when the
  list starts over or the page opens.
- **Communities** (08-121, 08-122, 08-123). A waiting ban ends with a note
  when its ask left no answer (`BanStep::Cancel`). Its warning opens when
  no box is open or the one Ban was pressed from is back; while another box
  is open the confirmed ban keeps waiting, it is never dropped. A second
  Create (Enter) while one is out sends nothing and tells nothing. A send
  answering
  `Ok(None)` (Ban, Remove, Add, Create) is told in a note
  (`community_step_went`). The chooser and the picker draw a failed list
  with Retry instead of an empty one.
- **Drop on a forum** (08-067). The chooser keeps the dropped payload until
  the chat opened; a refused or failed open leaves it with a note.
- **Already per window, proven by tests** (02-119, 02-184). `open_topic` and
  the shared-media gallery are part of `SessionView`, swapped with the
  window in place (`with_window_chat`), so a chat window's topic and gallery
  are its own; tests for both.

### Sweep fixes (s3-payments-1)

- **A stage that has not left is not entered.** The checkout's form,
  validate and pay requests are stored only after the send went out
  (02-153); a gift-limits batch, a peer's balances, and the earn page's
  loads are `BatchAsk` / per-peer values set from the sends that went out
  (`state/fetch.rs`), and a send that fails ends them in `Failed`, never in a
  spinner (03-025, 03-063, 02-046, 03-024, 09-064). A failed batch is not
  asked again by itself (no request per frame); the next open forgets it
  (`forget_failed_balances` where the profile upkeep runs, `may_ask` at the
  gifts open). The recipients of an affiliate program are `Recipients`
  (`Unasked`, `Loading { waiting, found }`, `Ready`, `Failed`): both
  requests are recorded as they are sent, a refused send makes it `Failed`
  and late answers of that attempt change nothing (03-023).
- **Answers are matched to what asked.** `PageLoad::Loading` carries its
  request; a program list takes only that page's answer or error, so a sort
  change or a restarted list never takes an older page (02-047, 03-026). The
  collectible lookup is `LookupStage` (`Unsent`, `Loading(request)`, `Ready`,
  `Failed`), so a replaced lookup's answer or error is not the new one's
  (02-001, 02-157). The Gift Stars packs are `GiftStage::Packs { target,
  load: PackLoad }` with the asking request inside `Loading`; Back and close
  drop it (02-201). The receipt dialog is `ReceiptStage` (`Closed`,
  `Loading(request)`, `Open`): Esc forgets the request, so a late answer does
  not reopen it (05-109). A Stars subscriptions page, and a received-gifts
  page, settle only for the request in flight; an appended page does not
  clear "stale", and a refetch waits for a page in flight (02-155, 02-174).
  `BoostsListState` is one `BoostsStage` (`Loading(request)`, `Ready`,
  `Failed`) and the Show more page is `Loading` too (02-126). A star
  reaction keeps each tap's request (`PaidTap`), so a refused tap rolls back
  only its own Stars, never a newer entry's (04-058).
- **A failed next page keeps the rows.** Boosts, invite-link members and
  link join requests keep what is loaded and show their error when only the
  next page did not leave; a first page shows its error. A join-request
  search that cannot be sent restores the list, query and request it was
  going to replace (02-128).
- **Results with nobody to show them are told.** `PaymentsState::notices` is
  a queue the poll drains into the status bar. A payment answered after its
  checkout closed (05-110), a gift sent or refused after its box closed
  (03-065), a commit of Star reactions that TDLib refused, and an add, undo
  or commit that failed (11-p7-004) all land there. A reaction is `PaidStage`
  (`Waiting`, `Committing(request)`, `Undoing(request)`; one design, merged
  with the integration branch's): it stays until TDLib answers its commit or
  undo. A refused commit or undo, and a commit that cannot be sent, release
  the Stars and tell the user once; nothing is sent again on its own, so a
  low balance cannot loop (02-159). A paid send the guard holds
  leaves the guard only after the send went out (02-162).
- **Locks are not reset by an open.** Reopening the gifts box keeps
  `gift_mutating` of a toggle or sale still out; its own answer clears it
  (02-173, 10-014). `open_gift_resale`, `start_gift_resale_buy` and
  `open_gift_craft` refuse while a purchase or craft is in flight (02-065): a
  new `CraftSession` never orphans a craft whose answer is out.
  A failed first page of the Stars history, the gifts list or Grams ends in
  the error line, not an empty box (04-040).
- **An auction follow is `AuctionFollow`** (`None`, `Following { gift_id,
  request }`, `Failed(gift_id)`): a refused open is not asked again every
  frame while the same bid box shows; closing it follows afresh. The
  `Farewell` of `GiftMarketUi` already closes the follow on an account switch
  (03-062, 02-066).
- **The gate follows the viewer's Premium.** `PaidMessages::gates` is
  `GateFetch` per user (`Asking { before }`, `Known { viewer_premium }`,
  `Failed { before }`): the open chat asks again when `is_premium` changes,
  and an error answer ends the ask, so the next open asks at once instead of
  after 30 s. `before` is the answer ahead of the ask (`Prior`, with the
  Premium it was given under). A failed refresh keeps it, so a known price
  does not vanish and the next send is still confirmed (04-056). Under
  another Premium only "restricts new chats" and "ok" are old; a price stands,
  so buying Premium keeps the guard's prices (04-053). The send path does not
  refuse while nothing is known: tdesktop shows no such state, so the
  smallest safe choice was to leave the composer as it was.
- **Smaller.** The Premium gift message is kept whole when
  `gift_text_length_max` is not known yet (04-060). The Collect and Withdraw
  buttons show a spinner while the first withdrawal request is out, and
  closing the earn layer cancels it (09-066). The gift transfer upkeep runs
  in the window of the transfer box, else of the gifts box it started from,
  and `reset_for_owner` drops one-shot results nobody took (03-068).

- Re-review of s12-ui. 10-119: both cancels also run over the round trips
  that chat windows keep in their own slots
  (`ChatWindows::cancel_slot_inline_openings`), so a start-bot pressed in a
  chat window ends when a link overtakes the lookup or the lookup fails.
  13-022: when the `getSupergroupFullInfo` send that statistics wait on
  fails, the panel goes to `Failed` instead of staying `WaitingForInfo`.
  09-110 stays as is: a partly failed send re-asks only chats that are
  neither in flight nor known, so nothing is asked twice.
### Sweep fixes (s12-money)

Payments, Stars, gifts, accounts, sign-in and passcode. Each entry was
re-read at its spot first.

- **Fixed.**
  - Re-review pass:
    - Passcode: when a delete fails and the put-back is partial, every key
      whose put worked is deleted again (it has a wrapped copy), so no key
      is plain in the OS store while the passcode is on.
    - 10-001: a verification answer moves the giveaway launch to
      `GiveawayLaunch::Verifying { chat_id }` whatever the checkout shows;
      `is_done_for` covers it, so Start stays refused until the form
      closes. `close_giveaway_form` refreshes boost status for Paid too.
    - Star reactions: a refused commit or undo releases its Stars and stops
      (tdesktop does not re-send); a low balance no longer loops.
    - 09-X061: failed previews are forgotten when the first box opens
      (`open_market_box`), not only on close.
    - 09-122: `auction_recipient(gift_id)` defaults to the owner of the
      user's existing bid. tdesktop's source was not reachable; this is
      the smallest safe choice.
    - 11-p7-001: `OrderInfoStage::validated_for(&order)` is used by render
      (Pay, shipping options) and submit.
    - 02-062: reopening the send box for its own in-flight send keeps the
      typed message; another recipient gets its own note.
    - 13-140: `CloseFriendsList` carries the `getCloseFriends` request id,
      so its failure no longer reads the viewer's single page op.
  - 04-052: `passcode::enable` keeps each OS-store key until every delete
    is done; a failed delete puts the already-deleted keys back before
    the wrapped files go, and if a put-back fails the wrapped files stay
    and the passcode stays on. No database key is lost.
  - 01-044: `read_zip` refuses an archive that reuses one local header or
    inflates past `SCHEME_LIMIT + BACKGROUND_LIMIT` in total.
  - 13-140: `CloseFriendsStage { Closed, Loading, Editing, Saving }` in
    `story_extras` replaces `close_friends_edit` + `close_friends_saving`.
    Loading shows no checkboxes and no Save, so a save always starts from
    the loaded list.
  - 02-075 / 09-X013: a failed send in the login review drops the sends
    that never went out from `review_pending`.
  - 02-082 / 13-045: `PhoneLookups { Idle, Asked }` on the sign-in screen;
    the lookups are asked on arrival, never again from `sync_signin` after
    an empty or failed answer.
  - 13-064: see the third pass; a render asks only
    from `Idle`, Appearance and the look box reopen a failed one.
  - 09-X061: `gift_market.failed_previews`; a failed preview is not asked
    again by the auction info box's render.
  - 02-152 / 10-152: `submit_payment_form` refuses while the invoice has a
    `sendPaymentForm` in flight (every caller, not the render).
  - 05-017: `StarsGiftState::asked: Option<RequestId>` replaces
    `loading`; only that request's answer or error lands.
  - 03-058: the transfer in flight carries the recipient as confirmed
    (`GiftManageState::transferring`); the toast reads it, not the picker.
  - 03-059: `start_gift_resale_buy` refuses while a buy is sending.
  - 03-060 / 09-123: closing the market boxes during the forge keeps the
    craft; its outcome opens the result box.
  - 04-080: a registry file that exists but cannot be read is never saved
    over by the default registry.
  - 02-158: Undo forgets the waiting Stars only once the remove was sent.
  - 02-083: one login-URL context at a time; a second button waits.
  - 07-012: the hidden translate bars are keyed by account.
  - 09-122: raising a channel's bid keeps the channel as the recipient.
  - 10-001: closing a paid giveaway checkout closes the giveaway form.
  - 11-p7-001: Pay refuses once the order fields differ from the last
    validated order.
  - 11-p6-003: the ownership answer closes and leaves only for the dialog
    of the chat that was transferred.
  - 12-086: the delete boxes act only on the account that opened them.
  - 01-051: opening the boost box clears the old chat's intent and slots.
  - 09-X012: closing a chat window no longer dismisses the queued account
    notices (not seen on screen).
- **Fixed, second pass.**
  - 04-057: `PendingPaid::stage` is `PaidStage { Waiting, Committing(req),
    Undoing(req) }`. A due or undone reaction stays in the list, and
    reserved against the balance and the cap, until TDLib's `ok` for that
    request id; an error puts it back to `Waiting` with a fresh window, so
    the toast and Undo return and the next tap counts the old Stars.
  - 12-001: the render's fallback lands through `ScopedApp::app()` (no
    screen epoch) instead of `main()`, which kept the stale epoch. The tray
    sits under its account, so the PNG lands there or goes, and
    `photo_render` is always cleared.
  - 01-085: `KeyFilter::resync` gives the `Up` a lost event hid. The macOS
    tap calls it when re-enabled, the Windows hook on the next event after
    a skipped one, the X11 thread when its stream ends. The app ends the
    hold (`ptt_end_hold`) when a new hook starts, one fails to start, or
    the hook's channel closes. No polling. tdesktop's behaviour here could
    not be checked, so a key-down of another key does not end a hold (a
    chord with the push-to-talk key would cut the microphone).
  - passcode `enable`: when a put-back fails, the accounts after the stuck
    one still leave the OS store (the passcode is on).
- **Fixed, third pass** (a reviewer re-read the branch; these supersede the
  earlier lines for the same ids).
  - 11-p7-001: `OrderInfoStage { Unvalidated, Validating{request, order},
    Validated{order, info} }` in the session replaces `validated`,
    `validate_request` and the dialog's `validated_order`. Continue moves
    it to `Validating` and drops the shipping choice, so Pay has no order
    id until the answer to that same request arrives; the answer promotes
    the order that was sent. An error, a new form and a closed checkout
    leave it `Unvalidated`. Pay compares the typed fields with the
    validated order.
  - 01-044: a stored (method 0) entry must hold exactly its declared size,
    the number the running total counts.
  - 10-001: a paid giveaway invoice moves the form to
    `GiveawayLaunch::Paid { chat_id }` when its payment succeeds, whether
    or not its checkout is still open; the 3-D Secure step names the chat
    (`PaymentVerification::giveaway_chat_id`) and closes that form with the
    checkout. Start is refused, and shows its spinner, while that giveaway's
    `sendPaymentForm` is out or after it was paid.
  - 09-X012: `dialogs_window_closed` frees an `AccountNotice` like a box
    with no close of its own (it shows in the last-press window) instead of
    handing it to the main window's close.
  - 03-059: `open_gift_resale` keeps a buy whose `sendResoldGift` is out.
  - 02-062 (fixed now, not skipped): `open_send_gift_to` keeps the stage of
    a `sendGift` in flight. The same recipient reopens on that send (Send
    stays off, its answer shows); another recipient waits for the answer.
  - 11-p6-003: `OwnershipTransfer::leave_after` rides with the transfer
    that was sent; the answer reads it, not whichever dialog is open.
  - 13-140: `CloseFriendsStage::Failed`; Loading and Failed both show Cancel,
    Failed adds Retry.
  - 09-X061: `failed_previews` is cleared when the market box closes, so
    the next open asks again.
  - 13-064: one `installed_backgrounds: Fetch<Vec<Background>>` replaces the
    list and its `Fetch<()>`.
  - 09-122: `bid_gift_auction` takes the recipient; a raise
    (`increaseGiftAuctionBid`) keeps the bid's owner and needs no user, so
    a channel-owned bid can be raised. A new bid still goes to a user only.
- **Skipped.** 02-160 (the Star reaction box is already on `OnScreen`, so
  per account).
- **Tests.** `enable_failing_midway_keeps_every_database_key`,
  `enable_with_an_unrestorable_key_still_clears_the_accounts_after_it`,
  `a_refused_commit_returns_the_stars_to_waiting_and_adds_up_with_a_new_tap`,
  `a_refused_commit_keeps_the_stars_and_the_next_tap_counts_them`,
  `a_render_finishing_after_the_screen_was_replaced_still_lands_and_ends`,
  `resync_ends_a_hold_whose_release_was_missed`,
  `a_zip_reusing_one_local_header_is_refused`,
  `close_friends_stage_waits_for_the_list`,
  `a_failed_send_does_not_leave_the_review_pending`,
  `a_failed_upgrade_preview_is_not_asked_again`, the second-Pay assertion,
  `a_late_answer_for_another_contact_is_dropped`, the transfer picker move
  in `a_transfer_goes_only_to_the_contact_the_confirmation_names`,
  `closing_the_boxes_during_the_forge_keeps_the_outcome`,
  `an_unreadable_registry_is_not_overwritten`, `hidden_chats_toggle`. No
  test for 02-083, 12-086, 01-051 (they need a fixture the offline
  harness lacks). Third pass:
  `an_earlier_order_id_never_outlives_a_new_continue`,
  `a_rejected_continue_leaves_the_order_unvalidated`,
  `a_stored_entry_larger_than_its_declared_size_is_refused`,
  `a_paid_giveaway_finishes_its_form_even_with_the_checkout_closed`,
  `a_giveaway_verification_step_names_its_chat`,
  `an_account_notice_is_freed_by_its_windows_close_not_dismissed`,
  `opening_the_resale_list_keeps_a_buy_that_is_out`,
  `reopening_the_send_dialog_keeps_the_send_that_is_out`,
  `leave_after_travels_with_the_transfer`,
  `a_failed_list_request_ends_the_wait_and_only_the_wait`,
  `a_channel_owned_bid_can_be_raised_but_a_channel_cannot_get_a_new_one`.
  No test for 09-X061 and 13-064 (the render-side clear and a type
  change; the existing background and preview tests cover the reducers).


### Sweep fixes (s3-platform)

Agent access, windows and platform code. Entries by area:

- **Agent grants and writes.** `grants::update` and `change_all` write the
  file first and return `Result<(), SaveError>`; a failed save leaves the
  cache and the generation as they were, so Settings never shows a revoke a
  restart would undo, and `QuillApp::update_grants` toasts the failure. An
  Allow on a prompt whose grant could not be saved answers Denied (01-012).
  A finished write result is sent with a note when access went off meanwhile
  (`Reply::writes`), never replaced by "Agent access is off" (01-011).
  `Presence::drop` withdraws the connection's prompts (`Pending::conn`), so
  a click after the agent left sends nothing, and `still_allowed` refuses a
  call whose connection is gone (01-039). A send returns a `Batch`: every
  message delivered or still sending is returned, refusals are listed, the
  log names only delivered ids and says "N failed" / "N still sending";
  only a batch with nothing delivered fails (01-030, 01-031). Folder edits
  read the folder inside the queue turn (`change_built`, 01-035). The
  class cache keeps only classes that cannot change (private and secret
  chats) and is capped; a channel's ownership is read each call (01-014).
  The bridge keeps the agent's subscriptions and replays them on a new link
  (01-009). The UI driver's `Pending` and `Ticket` share one `Stage`
  (`Queued`, `Running`, `Injected`, `GaveUp`, `GaveUpAfterInjecting`) under
  one lock; a timeout after input went in is `BridgeError::RanNoAnswer`,
  which tells the agent not to repeat the command (01-040).
- **Skipped: 01-024.** Every agent reads the same launch token from one
  file, so Quill cannot tell two processes of one client apart, and the
  self-declared name is the only identity it has. A per-client identity needs
  a pairing step (a token per client, written by `connect`), which is a
  design of its own; reset-token would end every connection, which is the
  too-broad half of the entry.
- **Environment switch.** `testmode` removes the account again when the
  marker cannot be written (01-023).
- **Pinned drag.** The release merges the dragged order into the pinned list
  as it is now (`merge_order`), so another device's pin or unpin is kept
  (04-020). A move with no button down ends the drag unsaved (04-021).
  The Touch Bar drag is `PinTouch::{Pressed, Dragging}` in `model.rs`; only
  an Ended touch in `Dragging` saves, a cancelled one snaps back
  (`TouchEnd`, 13-151).
- **Permission prompt.** `PermissionAsk::{Idle, Asking}` is held by
  `RecordingUi`: a second press while the system prompt is up is ignored and
  the answer starts its press only while the same call is active (08-054).
  A press that finds a device already granted or denied no longer sets the
  state to `Idle`: only the answer to the ask itself ends it, so a later
  Allow is not dropped.
- **Country list.** `AuthState::countries` is `Fetch<Vec<Country>>`; the
  error answer lands in `Failed`, the boxes draw "Couldn't load the
  countries" with Retry, and only a box opening or Retry asks again, never a
  frame (09-017). The sign-in picker is one of those boxes: it draws the
  failed row and asks again when it opens.
- **Grants.** Changes are `Widen` (save first; a failed save changes
  nothing) or `Narrow` (the cache takes it even when the file cannot be
  written, and the failure is logged and shown). `forget_account` and the
  Settings revokes narrow, so a logged-out account never keeps all-chats
  access in memory for the next sign-in under its key (01-012).
- **Agent `td_call` sends** return the refused messages as `failed` beside
  the delivered ones (01-031).
- **Saved tag box** moved to `MessageUi` and is `per_window` (12-072).
- **Debug mode.** `debug_mode::switch` returns `DebugLog::{Off, On,
  TdlibRefused}`; the flag and the saved file follow the state after TDLib
  answered, and a refusal is shown (01-025).
- **Share box.** Only the destinations that failed stay ticked; with no
  client nothing is cleared (13-073, 13-072). `ShareSelection` holds a
  `ChatSend::{Due, CommentSent}` per ticked chat, so a retry sends a chat
  whose comment landed only the forward, never the comment twice.
- **Connect.** `deselect_chat` tears down locally before the TDLib closes
  (02-024); the Replies block box closes only after the send (02-179).
- **Keychain.** Delete maps `errSecItemNotFound` to Ok and every other
  status to an error (04-025).
- **Story composer.** Its save waits on its own `RequestId`
  (`SaveStage::Saving`); a call pushed out of the slot says so instead of
  closing as saved (07-036; the cover and privacy editors were keyed).
- **Header menu** entries carry the chat and the screen epoch they were
  built for (08-062).
- **Dialog hosts.** An `Asked` host ends when no request is in flight and
  its flag is still clear (08-108).
- **Saved windows.** A save during a restore keeps the windows not yet
  reopened (`windows_to_save`, 08-143).
- **Notifications.** A notification waiting for its avatar is shown in the
  locked form when the lock engaged meanwhile (11-p5-021). A refused reply
  in a notification window opens the chat (`InlineReplyOutcome`, 11-p6-004).
- **Toasts.** The lock ends a hover that can no longer end
  (`ToastQueue::pointer_gone`, 13-115). A host window hidden or minimized
  with the pointer over its toast is not covered: no event says so.
- **Touch Bar.** The composer observation follows the composer on screen
  (13-152); commands are refused while locked (13-157).


### Sweep fixes (s3-calls)

- **Call rating.** `CallSummary::rating` is `CallRating` (`Offered`,
  `Sending(request)`, `Sent`, `Failed(line)`), replacing `rating_sent`. The
  send moves it to `Sending`; only the matching `ok` reaches `Sent` (the card
  hides, as tdesktop shows no toast after a rating) and the matching error reaches `Failed`, which draws the card
  again with the error and its retry (02-015, 05-026, 08-081). A busy or swap
  decline's `discardCall` error no longer lands on the active call's banner:
  only the request that owns `HangUp` sets `calls.error` (05-035). Redial and
  Call again check the engine and the network before they dismiss the end
  summary (08-078). A dial carries its target in the continuation that waits
  for microphone access (`with_capture_access` takes a closure), so a second
  press cannot be answered by the first prompt (08-076).
- **Group join.** `connect_group` leaves `GroupJoin::Created` for
  `AnswerApplied` once the answer is applied and for `Connected` only after
  the sound, mute and videos steps worked; a failed step puts it back
  to `AnswerApplied`, so the retry redoes the steps (01-080). The camera is
  the last step and not part of the join: `connect_group_call` answers
  `Ok(Some(error))` for a camera that would not start, the call connects with
  audio (the microphone starts), and the camera step after the connect
  retries it and shows its error, as tdesktop keeps the call running when the
  camera fails (01-080 regression, 02-101). A failed
  `connect_group_call` spends the join answer (`join_payload` is cleared), so
  group-call updates no longer retry it (02-101). A presentation that cannot
  connect is ended on Telegram (02-099). A stream reply of "call left" shows a
  failed join with its retry (02-111). Muting before your participant row
  exists is sent for your account or your join-as (02-104).
- **Live story extras.** The streamer, top donors and comment senders are three
  `Fetch` fields on `LiveStoryState`, each asked while idle; a failure stays
  failed until the next Join of the story (02-114).
- **Call links.** One `CallLinkStage` (`Idle`, `Confirm`, `Leaving`,
  `Offered`) replaces the three fields kept in step. `Leaving` holds the whole
  confirm (so a live stream stays one) and the call being left. It ends with a
  hang-up that cannot be sent, a `discardCall` TDLib answers with an error (the
  hang-up error arm drops it for that call), another call taking the place,
  and the call boxes' farewell; `Offered` reaches the UI so a stream opens in
  its window (01-066, 01-067).
- **Create video chat.** A "join as" answer is dropped when no box waits for
  its request, and a box whose request is no longer the session's closes
  (01-074).
- **Windows.** A call window that fails to open is remembered per call in
  `window_open_failed`, apart from `window_closed_by_user`: a second call
  ringing reopens a window you closed but not one that failed, so the sync
  does not retry and note it every time; the call bar or a notification's Open
  tries again (08-034, 10-047). Settings > Calls redraws only when its sessions fetch went
  out (08-033).
- **Capture.** A run of 50 frames the engine refuses ends the capture with a
  `Failed(Refused)` event (01-071); the microphone is then not reopened until
  the call, the microphone or the mute changes (or the transport goes down
  and comes back), so the error line does not flicker every 2.5 s. The screen
  share's wait reads the pump's `now`, and the texts of the ended call, the
  stream that would not load and the share that could not start are lang keys. A lost WASAPI output device is reported through
  `SystemAudioMix::fail` and the driver turns the audio share off (01-063). A
  chosen screen share waits at most `ENGINE_WAIT` for the engine's frame
  sender, then ends with an error (02-013). A macOS start that times out stops
  its stream (01-064). A lost microphone withdraws the speaking mark (02-009).
  A failed output reopen is retried three times, 5 s apart (01-094). An X11
  push-to-talk thread that ends is a failed hook (`HookLife`) and releases a
  held key (01-086).
- **Skipped.** 02-103: the native presentation is already stopped when the
  end request fails, so there is no sharing stage to return to, and the error
  already shows as "Could not stop screen sharing". The tdesktop source was
  not available to this run to confirm that it does not revert either; the
  smallest safe choice was to leave it.


### Sweep fixes (s3-accounts)

- **Passkeys are one stage per ask.** `PasskeysState` has `add: PasskeyAdd`
  (`Idle`, `FetchingOptions(id)`, `Starting(options)`, `Cached(options)`,
  `Sending(id)`, `Added`) instead of `registration`, `ceremony`, `adding` and
  `added`. A request stage holds its request id and only that answer (or
  error, or lost request) moves it on, so a box closed before
  `getPasskeyParameters` answered (`cancel_register`, also run by the lock)
  starts no ceremony (04-065), an `addLoginPasskey` answer without an id
  (`PasskeysPayload::AddedUnreadable`) ends `Sending` (04-061), and an
  `Added` left behind by the lock is dropped by the next Create instead of
  closing its box (12-078). `PasskeysPurpose::List(generation)` carries the
  generation a removal bumps, so a list asked before the removal cannot bring
  the passkey back (04-062). `request_passkeys` records `requested_at` only
  after a successful send and the answer, not the send, clears `stale`
  (02-180). A refused `finish_register`/`finish_login` is a note (04-066).
- **Owner of an account folder.** `claim` reads the owner as `Previous::{Unknown,
  Known}`. An unreadable `owner.json` with leftovers counts as someone else
  only when this run signed in fresh (`SignInOrigin::Fresh`, set by
  `Session::set_auth` when TDLib asks for a phone, code, QR, email,
  registration or password step). A session TDLib resumed
  (`SignInOrigin::Resumed`) is the folder's own account: a user updating from
  a release that kept no `owner.json` keeps their grants, trust, volumes,
  windows, hashtags and widgets, and the owner is just written (01-005).
  An empty `recent_hashtags.json` (what a wipe writes) is not a leftover, so a
  failed owner write (`Claimed::owner_error`, logged by the driver) does not
  wipe again at the next start. A different known owner always forgets.
- **Exports settle and stay honest.** A worker that unwinds publishes
  `Failed` through `SettleOnPanic` (a drop guard in the thread body), so the
  Done button appears and a new export is not refused (03-042). `result.json`
  is written as `result.json.part` and renamed only when the export finished,
  for both the account and the chat export (03-043). The account export's
  progress box is `AccountExport::view` (`Shown`/`Hidden`), kept with the
  export in the session, not in the screen's `DialogUi`, so switching
  accounts and back leaves a hidden export hidden (07-015).
- **An unreadable `accounts.json`** keeps the `pending_deletes` it still holds
  (recovered from the quarantined text) and saves them into the new registry
  before any client opens (04-082).
- **Reviews and notices.** The new-login review is `ReviewStage` (`Idle`,
  `Reviewing { confirmed, waiting, done, error }`, `Failed { error, done }`);
  each `confirmSession`/`terminateSession` carries its session id, an entry
  leaves the alert when its own request succeeds, and a retry sends only the
  rest (05-020). `acceptTermsOfService` is `terms_accept: Fetch<()>`, and a
  send that cannot go out fails it with a line the locked box draws (09-024).
  The terms prompt is `TermsPrompt { terms_id, stage }` (`Terms { age_ok,
  age_error }`, `DeclineSorry`, `DeleteWarning`): another terms id, or none,
  replaces it whole (09-023). A force-logout notice is not popped by any
  close, and its Log out button dismisses it only once `request_logout` was
  sent; otherwise it stays with a note (09-026, 09-025).
- **Switching accounts** checks the root, the credentials and the write of the
  active account before anything is torn down (`prepare_account_switch`), so a
  refused switch keeps the chat, the boxes and the client. Without a client
  (the screenshot demos) the screen is still replaced, as before (07-017).
- **Sign-in forms keep what was typed until the step is done.** The phone is
  `PhoneStep` (`Entering`, `CodeSentTo`, `Editing`, `Sending { phone, back }`):
  the code step names the new number only once TDLib took it, a rejection
  returns to `back` with the old number (13-046). Registration names, the
  accepted terms and the email stay after a send and are emptied by
  `sync_auth_forms` once auth leaves the step (08-042). The two-step and
  recovery forms keep their fields when the driver refuses the request and
  say so, and are emptied by `sync_twofa_forms` when the step finished
  (12-076, 12-094).
- **Lock and focus.** A lock-screen log out whose wipe reports an error stays
  as `Deferred` (nothing opens the old database, "Restart Quill to finish
  logging out") (08-037). `looked_at` is the one rule for `app_active`
  (focus and not locked) for the account on screen and for the background
  accounts (08-036). The QR box's clock and contact-link ask wait under the
  lock (11-p8-007). A passcode job whose window closed lands in the main
  window (11-p7-005, `passcode/job.rs`). A refused gifts request settles the
  panel (`ThemeGifts::refuse`) instead of asking on every frame (11-p8-012).
  A save that fails after its QR box closed is told through the status note
  (04-012, 11-p8-008).
- **Tests.** `passkeys::tests`, `connect::tests::passkeys` (stale refetch send,
  generation, cancel, unreadable add, stale `Added`), `account_data_tests`
  (unreadable owner), `export::tests` (`SettleOnPanic`), `account::tests`
  (`result.json.part` after a cancel), `settings::accounts` (pending wipes),
  `connect::tests::account_hygiene` (partial review retry, terms send),
  `peer_qr::theme_tests` and `connect::tests::share_theme` (refused gifts),
  `ui::peer_qr::tests` (late save failure), `ui::dialogs::account_notices_tests`,
  `phone_step_tests`, and `a_passcode_job_answer_lands_when_its_window_closed`.
  None waits on time. Not tested: 07-017 needs a live client with a failing
  registry write, which the offline harness cannot make; 08-037, 08-036,
  11-p8-007, 12-076 and 12-094 call the driver or the lock from the frame and
  are only checked by the lock-surface and window-scope scripts and by eye.
- **Not done.** Nothing left open.


### Sweep fixes (s3-media)

Media and players. Entry ids in brackets; a skipped entry says why.

- **Video editor.** The cover still has its own cutter, `StillCut::{Idle,
  Cutting { token, next }}`, matched by its own token (`next_editor_serial`),
  so an encode's serial bump no longer orphans it and `still_busy` /
  `still_wanted` are gone (14-002). A failed tray or edit-box encode
  reopens the player through `start_video_editor_player` (14-001); the
  editor's single stage enum was not built, the player stays an `Option`
  that the Err arm refills, as the avatar path does.
- **Edit media.** A cover and the video editor's extras live in
  `EditMediaReplacement`, so replacing or clearing the replacement starts
  the next video without them (02-091); a replacement of the same file (the
  "as a document" box) keeps them. The original video's own cover stays in
  `MediaEdit::cover`. `EditorPurpose::MessageEdit { chat, message }` and the
  cover picker carry the edit they were started for and drop the answer when
  another edit is pending (09-070, 09-071).
- **Voice and recording.** The voice encoder thread writes why it ended
  into `Control::ended`, and `VoiceCapture::failure` also reports a finished
  worker, so the bar ends the recording with the reason (14-069). The chat
  action, the play-once offer (and its send-time re-check) and the
  secret-chat layer check use the recording's own chat, not the main
  window's open chat (12-050, 12-051, 12-052); `sync_voice_recording` and
  `sync_video_note_recording` take that chat.
- **Disk pauses.** A healthy reading removes an id from `disk_paused` only
  after its resume went out, and a failed send leaves the rest for the next
  reading (02-140; the low-disk and saved-pause loops follow the same
  order). The saved-pause search is `DiskSearch::{NotAsked, Asked { request,
  keys }, Done}`: set after the send succeeded, its answer matched by
  request id, and a failed search goes back to `NotAsked`; so does a lost
  one, because the search is swept like the other dedupe-guarded lookups
  and the sweep settles it through the error arm (05-082).
- **Quick reaction.** `setDefaultReactionType` carries
  `RequestRollback::DefaultReaction { previous }` (02-142; one line in
  `session_apply_error.rs`, with a `Hotspot-change` trailer: every new
  rollback needs an arm there).
- **Media keys.** An OS media key with no player publishing is dropped
  (`media_session::enqueue`), so it is not replayed on the next track
  (03-089).
- **Shared media.** A Gifts page for another chat writes no `next_offset`
  (06-003: `accept_profile` says whether it took the page). `PageKey` carries
  the tab's stamp, so a closed and reopened gallery of the same chat starts
  at the top (13-075). A ticked row asks for its own properties and every
  answer lands on the ticked entry by message id (13-050); a row menu
  captures the account and chat it was built for and drops the click when
  the gallery shows another (13-049).
- **Profile photos.** The chat gallery click reports a failed send and gives
  up when the current photo's request ended without an answer
  (`chat_photo_request_ended`, 12-040). 02-183 is skipped: `fetch_chat_photos`
  runs from a click only (never from render) and skips what is in flight, so
  a re-send after a failure is the retry.
- **Profile boxes.** Edit contact, birthday, personal channel and photo,
  and share contact send their request and stay open as
  `ProfileDialog::Saving { dialog, request, saved }`; the request is watched
  by id (`SaveWatch`: one slot, since one box is open at a time, so a box
  replaced without closing leaves nothing behind). The toast is earned by
  the answer itself (`Session::apply` marks it), not by the request leaving
  flight: a request dropped unanswered puts the box back with no success
  toast. A refusal puts the box back and says why, closing it forgets the
  wait (12-042). The personal-channel entry in the profile
  menu has no box, so it still toasts on send.
- **Pickers.** `PickerUi`'s farewell closes the GIF gate and the
  status-choices resolver on the client being left (11-p1-001). The Masks
  strip is wanted while some window's editor is in Stickers mode and is
  released by `sync_editor_masks` every frame, whatever closed the editor
  (11-p8-016). 11-p8-016 asked for a drop guard; the strip's gate is the
  session's, which a value dropped in the editor cannot reach.
- **Players.** A click demand is `DemandStage::{Asked { pass }, Started}`:
  it leaves `Asked` only when the sound really started (10-137) and expires
  when its window ends a pass or leaves the list with no row that rendered
  it (10-136). The HDR viewer's poll and draw read one headroom
  (`display_headroom`), so a screen that lost EDR does not repaint every
  second (10-097); a chat window runs the EDR sync too, under its scope guard like the rest
  of its upkeep (10-098). The HDR
  JPEG XL decode converts a chunk at a time into the final buffer and caps
  at 32 Mpx (10-103). A sticker worker that ended fails its clips and gets
  no new ones (`WorkerLife`, 06-011). The sticker pump redraws only for a
  clip's first frame (13-015). A bot draft with no text keeps one rows key
  until its content changes (08-019).
- **Sticker box.** `Encoding::{Idle, Running { cancel }}` is ended only by
  the job holding that flag (13-003); a chosen video whose download failed
  ends the wait with a note, and a download that could not be sent says so.
  The wait asks with an automatic download, whose end `status` reads as
  `Idle` (only user downloads fail), so it checks
  `DownloadIntents::ended_without_file`, which holds for any origin
  (13-005; no progress bar: tdesktop shows none for this wait).
- **Spoiler tile.** The media spoiler tile is built on the background
  executor (`TileState::{Idle, Building, Ready}`) and released only by the
  idle image trim, not every three seconds. The frame callback claims the
  build (`claim_build`), not the ask, so a window that closes before its
  next frame leaves the tile `Idle` for the next window to ask again
  (13-041).
- **Statistics export.** A workbook is refused while an async graph of the
  page was never asked or is loading, and the toast says the statistics are
  still loading. A graph whose request failed is exported without its
  sheet, as one the server could not build is: the page only asks a graph
  once, so waiting on it would block the export for good (13-027).
- **Playlist.** Remove and reorder report a send that failed; TDLib's later
  refusal already lands in the session's note (12-002).
- **Edit media.** A replacement video is probed in the background when it
  is staged or re-cut (`EditMediaReplacement::probe`, `ReplacementProbe::{
  NotNeeded, Probing, Ready}`), matched by path. Save says "Preparing the
  video…" and waits while it is `Probing`; a failed probe drops the
  replacement with the reason; the driver sends only a `Ready` probe, so
  no ffprobe subprocess runs on the UI thread (02-144, fixed after being
  skipped).

### Sweep fixes (s3-payments-2)

Payments, gifts and Stars, second half. A failure or an answer lands only on
the request that asked; a stage that waits has a way out.

- **Errors matched to what asked.** The Stars history and the received gifts
  list apply an error only when it is the newest request's (`tx_request`,
  `gifts_request`; 04-036, with 05-114). Resale search, auction state and
  craft candidates compare the failed request with `request_id`,
  `auction_request` and `candidates_request` before touching the stage, so an
  older failure no longer stops the newer search or clears the auction box
  that was waiting (03-061, with 05-120). `PaymentsPurpose::GetGiftCollections`
  carries its owner and the list applies only to the open owner (03-064).
- **One enum per lookup.** `BankCardLookup` is `Loading { request }` /
  `Ready` / `Failed`; the answer applies to the lookup that waits for it
  (05-107). The wallet's `state_requested`, `rates_requested` and
  `explorer_url_requested` are `Fetch<()>`: `begin()` before the send,
  `cancel()` when it never left, `Failed` on an error (the next open asks
  again), `Ready` from the answer (05-126, 07-011). The wallet transaction
  lookup is `TxLookup` (`Idle` / `Loading { request }` / `Failed`): a late
  answer for a card that was replaced is dropped (07-007). `BoostsListState`
  holds one `BoostsPage` (Show more now shows progress and a double click
  sends once, 05-233). Earn balances are a `BalanceFetch` per peer; a failure
  stays until the profile is left, so the per-frame upkeep never re-asks
  (09-056). The countries list is `Fetch<Vec<Country>>`: the per-frame
  `fetch_countries` asks once, `retry_countries` is for a click, the giveaway
  page draws the failure with Retry (10-009). The market's acquired gifts and
  upgrade previews are `Fetch` per gift: "N items bought" shows the failure
  with Retry (09-X062) and the preview is asked as the info box opens, never
  from its render (09-128).
- **Stages that kept a choice.** The gift transfer confirmation becomes
  `TransferStage::Transferring` once `transferGift` is out: Cancel and Esc
  wait, the answer still brings the toast and the chat, a failure returns to
  the confirmation with the reason drawn in it, not under the dialog via
  `hub.gifts_error` (10-002). `GiftMarketUi::auction_open` replaces
  `pending_auction` and is dropped when the send box it came from closed
  (10-005). A giveaway launch in flight survives reopening the form
  of the chat it was started for only (`GiveawayLaunch::for_form`): another
  chat's form starts `Idle`, so A's answer shows nothing in B's form and only
  drops A's cached boost status (10-004; no toast for A, which tdesktop shows
  in the form it came from). The Direct Messages box has a
  `DirectStage`: it stays open while Save is out, closes from the answer and
  shows a refusal in itself; another target's save holding the shared
  `PaidSave` slot never closes it as saved (11-p6-002). The Star reaction box stays open
  with the reason when Send is refused or cannot go out, and only closes
  when the Stars were added (11-p7-003). Files chosen for forwarding stay in
  the chooser until their chat is open (05-034, 13-036).
- **Owner going away.** A join box parked for the recipients is dropped by the
  earn Farewell, when the layer is left and when the bot profile it came
  from closes (`set_open_info_panel`, 09-X032). A "learn more" gift
  value lookup is dropped when the gift box it was asked from is gone
  or shows another gift (10-003). The Premium bot's queued `/start` (`PremiumAutoStart`) is tied to the
  generation of the link's resolve: that resolve's error, stale or not,
  drops it, another flow's does not; it is also dropped by the payments
  Farewell and when another link opens a chat (04-048). The Stars history
  fields (04-036 fold into one enum) were not converted.
- **Failures that said nothing.** A failed collectible lookup other than a
  username that is not a collectible one posts a status note (08-118; we
  chose a note for the phone case too: the register asks for it and a dead
  tap is worse). A listing purchase that cannot start posts a note, and a
  second listing cannot replace a purchase on its way (09-127). The craft
  "Select Gifts" list shows a failed search and Retry, not "none on sale"
  (09-X065). The withdrawal amount is brought down to the available balance
  and the option maximum when Withdraw is pressed (09-065). The receipt's
  Total includes the shipping option and the tip (06-061). `ISK` is a
  zero-decimal currency (06-062; ISO 4217 says 0, Telegram's currency table
  could not be read here). The GPU-loss recovery waits for a new device for at
  most 20 s on the executor clock, then gives up and quits (13-039).
- **Already fixed on this branch.** 09-X023: `checkout_prefill_upkeep` fills
  the checkout at the first frame after the form lands, in the window that
  hosts the box (test `the_checkout_prefills_in_the_window_that_hosts_it`).
  08-015: `send_boost_chat` already leaves `Working` when the send fails; the
  list is now put back too (test). 12-014, 12-015: the fixed-time pollers are
  gone; the downloads are a function of the hub state (`preview_downloads`,
  tested), run at every main frame.
- **Merged with the s1/s2 money fixes.** The transfer's request id lives in
  `TransferStage::Transferring { request }` (main's separate `transferring`
  slot is gone); its ok and error settle the picker only for that request
  (test `a_transfer_settles_only_on_its_own_request`). `for_form` never
  forgets a `Launching`: the form and the driver compare `chat_id`, and
  `Launched` and `Failed` carry the chat, so going A, B, back to A keeps A's
  Start disabled (test `going_back_to_a_chat_keeps_its_launch_in_flight`)
  and A's answer never shows in B's form. The upgrade previews are `Fetch`
  values, so main's `failed_previews` set is gone. The paid-message save
  holds its request (`PaidSave::Saving { target, request }`) and only that
  request's ok or error settles it (test
  `only_the_save_that_holds_the_slot_is_settled_by_its_answer`).


### Sweep fixes (s3-links-tdlib)

Links, mini apps, location, and TDLib state and connection.

- **AI compose box.** A failed request is `CardState::Failed(AiBoxError)`, so
  "Increase Limit" follows the card (a good result after buying Premium can be
  applied) and clicking the active tab asks again after a failure (01-053,
  01-054). The apply-in-place binding is per account
  (`AiComposePrefs::bound_style_names`; the old app-wide value moves to the
  primary account on load), and the request carries its account and the
  composer's text, so an answer lands only while the composer still holds that
  text (01-056, 07-028). The style preview's "Another example" fails at once
  when no client is there (01-046).
- **Links.** A new launch link replaces a finished flow nobody consumed
  (02-041). A linked media timestamp waits for the unlock and is dropped when
  its chat is no longer shown (09-012, 09-X008). A thread link of a chat with
  its own window runs in that window's scope (09-013). A flow whose account
  left the screen is ended by `LinkUi`'s farewell, so its late answer finds no
  flow (09-010: already handled, test added).
- **Mini apps.** `WebApps::replace_open_result` queues the launch of an
  unshown `Opened` answer for `closeWebApp` (03-002); the emoji-status switch
  goes back at once when the send never left (03-001). A phone share's answer
  is `PhoneShare::Sent|Refused(request)` and only the flow that sent it takes
  it; a timeout clears it (14-044). The marketplace gift says why Buy did
  nothing and asks the text limit again (02-136; its notes and the mini-app "no address"
  line are lang keys).
- **Location picker.** Venue queries are numbered from a counter that is never
  reset (`LocationSharing::venue_generation`); the ask for a place carries its
  stage (`Unsent|InFlight|Answered|Failed`), so closing the search after a
  failed lookup asks again; Resolving and Missing bots reset at `open()`
  (03-073, 03-085, 03-074). A locate's 30 s limit pauses while the system's
  permission prompt is open (`Locating::permission_prompt`, `WaitBudget`;
  10-166). Dropping a `Locating` stops the macOS location manager unless a
  newer lookup replaced it. The macOS side could not be built here.
- **Connect.** Answers captured from an envelope are stashed before any step
  that can fail (02-037). A Grams name whose request never left is asked
  again (04-041); the secret-chat queue keeps the ids behind a failed send
  (02-212). The system proxy step's outcome is its own (`SystemStepState`,
  matched by request id) and the box's error line is neither read nor cleared
  by it (02-187, 06-033). The offline network re-probe runs at most once a
  second and off the tracker's mutex (04-001). The media cache scope follows
  the client put on screen, not the first connect (03-070). Background
  accounts follow the OS proxy through the mirror of the list on screen
  (03-013: by design, test added).
- **State.** A failed map tile is retried when its chat is opened afresh
  (`MapThumbs` is one map of `Tile` stages; 05-187). A created invite link is
  merged only into a loaded list (05-197). `request_kind` classifies requests
  by an exhaustive `kind()` on every purpose enum (`state/request_kind/`, one
  file each, so a new request must be classified; lookups carrying a payload,
  like `GetGiftOption(PinnedMax)`, stay lookups). Logout settles every request
  but a login submit, and the failure notes that settle would write are put
  back (`logout_notes`; 05-200). A calendar's
  generation never repeats (05-231). Call privacy is one stage per rule
  (`CallPrivacyAsks`; 05-251). Saved Messages views page by request id
  (`SavedPage`), a failed page stops paging with a Retry strip
  (`OlderEnd::Failed`; 05-261, 05-263, 05-262). A sponsored report answer is
  taken only for the request the flow waits on (05-249). A deletion in another
  chat leaves the search jump alone (06-005). `updateDefaultBackground` with a
  null background removes the wallpaper, and a refused delete keeps it
  (06-032; since the s3-settings merge the delete applies on `ok`, 02-087). A replacement hub takes over a stopped one's routes and stopped
  flag and its `seq` counter, which the sessions' out-of-order check needs
  (06-065). A profile change that fails after Save closed its dialog
  shows as a note (05-155).
- **Not changed.** 05-043 (`join_conference` already refuses a second join, and
  a test says so), 05-184 (tdesktop's `loadMoreRows` has no failure handler
  either, as the code comment says), 05-196 (the payload's own setters
  overwrite those fields right after the rebuild, so nothing is lost).

### Sweep fixes (s3-groups--2)

Second half of the groups, channels and business sweep. One shape ran
through most of it: a box sent its request and closed (or cleared what was
typed) when the driver queued it, so a refusal from TDLib had nowhere to
land and the typed form was gone.

- **`Submit`** (`src/state/submit.rs`) is one value for a box's Save:
  `Editing`, `Sending(ids)`, `Failed(reason)`. The request registry keeps
  the last 64 refused requests with their reason (`RequestRegistry::failure`,
  recorded in `apply_error`), so a box reads its outcome from the ids it
  sent: `Sending` leaves on the answer (the box closes, with its note) or on
  the error (the box stays with the typed form and the reason, Save is
  enabled again). Cancel leaves it too. One pump, `settle_box_submits`
  (`src/ui/box_submits.rs`, from `poll_live`), settles every box; the
  welcome box needs its window to clear a field and settles in the
  per-window upkeep. `scripts/check-submit-settle.py` fails when a file sets
  `Submit::sent` and none of its `settle_*` functions is called from those
  pumps. Boxes on it: promote, edit rights and demote (10-062, 10-063),
  restrict and ban (10-063), permissions (10-064), custom title and
  community name (10-089), New community (10-092), topic editor (10-069),
  welcome add and edit (10-074, 10-075), invite link box (10-149), Edit
  profile (12-032).
- **Edit box** (10-053, 10-054, 10-056, 10-088). Its `saving` is the enum
  `SaveStage` (`Editing`, `Saving(ids)`, `Refused(reason)`): a send the
  driver refused (a second Save while the first failed, a partial send) sets
  `Refused` instead of leaving the old ids to be read as "saved". A request
  no field names (protected content, join switches, username reorder,
  Delete and Leave) now fails the box through the registry; a link that
  needs Premium or an unknown username problem records a Username error
  too. A refused photo has no box to show it (it uploads after the box
  closes, as in tdesktop): it is the action error toast, worded with a lang
  key and the normal error reason. Delete and Leave keep the Edit box open
  until TDLib answers (`track_chat_exit`, `SaveStage::Exiting`): an earlier
  refused Save no longer decides their outcome (10-088), and a refusal is
  copied into `SaveStage::Refused` by `settle_group_edit_box` so the
  registry forgetting it cannot close the box as saved. Leaving a group says
  "Leaving group…", a channel "Leaving channel…". The demote box shows its
  button busy while the request is out; the welcome box's two refusal
  lines have distinct ids.
- **Promote search** (10-079): `searched` follows the query the session
  recorded as asked, and the page is drawn only when its recorded query is
  the one in `searched`.
- **Permissions Save** (10-064): `save_permissions_box` returns
  `PermissionsSent` (`Unchanged`, `Refused`, `Sent(ids)`); only `Unchanged`
  closes silently.
- **Stops slider** (10-080): both callers only stage the stop, so the
  failure it describes could not happen there; the widget itself now puts
  the thumb back on the caller's value after a release the caller did not
  take.
- **Info panel** (10-093, 10-124): the members block restarts every time a
  panel opens and when it closes (`InlineMembersUi::reset`); the event-log
  search box shows the chat's own stored query (`sync_event_log_search`).
- **Members paging** (10-123): `Paging` (`Idle`, `Asked`, `Failed`)
  replaces `failed_offset` and `retry_at_scroll`; a page whose send failed
  is `Failed` and the next scroll asks again.
- **Type box** (10-085): the usernames refusal is cleared on open and close.
- **Invite links** (10-142, 10-145, 10-146): the details of a link open once
  the members request is out; the Delete-all alerts hold the screen they
  were opened on (`screen_app`: nothing happens after an account switch) and
  the admin they were opened for (`delete_all_revoked_admin_links` takes it).
- **New Group/Channel** (11-p5-002): Cancel, Escape and the close button
  always close the box (tdesktop `GroupInfoBox` keeps Cancel live; TDLib
  holds the create while offline, so a box that waited would trap the
  person). A create still out is detached (`detach_new_chat`): its answer
  opens the new chat, a channel too. A second create while one is out is
  refused with a note ("The chat is still being created.").
- **Sharing request** (12-045): the box is cleared only when the request was
  sent; a refused send keeps it and does not open the chat.
- **Statistics** (13-023, 13-026, 13-028, 13-029): the loaded statistics are
  an `Arc`, so a redraw shares the graphs' JSON and the parsed-chart cache
  hits; a graph that failed to load is a section with its reason and Retry;
  a story page opens once its request was accepted and has Retry; a refused
  retry says so.
- **Topic icons** (10-070): downloaded when the editor opens and when the
  icon list lands (`ingest`), not on every build of the dialog.
- Not tdesktop-visible: most of tdesktop's boxes close on send and show an
  error toast later; Quill keeps them open until the answer, which is the
  smallest way to meet the register's "a refusal must be shown in the box"
  without a new look. The exception is the admin-rights box, which tdesktop
  also keeps open until the answer, so Quill matches it there.

### Sweep fixes (s3-messages-2)

- **One answer board for every request a surface waits on.**
  `Session::acks` (`RequestAcks`, `src/state/acks.rs`) keeps the answer of
  each request a surface registered with `expect`, settled by
  `Session::apply` on the request's own `@extra` (an `ok`, a `message`, an
  error: any answer), and kept only until the surface `take`s it or
  `forget`s the request. The chat look dialog's `chat_look_oks` counter and
  its use of the shared `background_error` are gone (08-094, 08-095,
  08-096): Apply is a `LookStage` (`Editing`, `Blocked(why)`, `Sent(sends)`),
  every change is `InFlight(id)`, `Done` or `Failed(why)` on its own answer,
  a failure leaves only the failed change to send again (the done ones are
  not planned twice), a boost-level refusal is `Blocked` in the dialog, and
  closing the dialog forgets its requests. The three chat look purposes no
  longer write `background_error`, so the Appearance pages' error is theirs
  alone. `check-implicit-state.py` now also rejects acknowledgement
  counters (`*_oks`, `*_acks`, `*_acked`).
- **An edit leaves edit mode on its answer** (08-131). `EditMode` has a
  stage (`Editing`, `Sending(request)`); the frame upkeep of each window
  (`settle_edit_send`) takes the answer: `ok` restores the draft, an error
  returns to `Editing` with the edited text in the field. A second Enter
  while `Sending` sends nothing; leaving the edit or the chat forgets the
  request. Each edit has an `EditId`, so a picker's answer (replace
  attachment, attach) is dropped when its edit was cancelled or another one
  started (08-129). Edit refuses, with a note, while an attachment tray
  waits (08-128): tdesktop has no tray to lose (its files wait in a box of
  their own), so refusing is the smallest safe choice.
- **A rich send keeps its text until TDLib answers** (08-135, 10-116).
  `ChatUi::sent_text` (`SentText::Sent { request, markup }`) restores the
  text and the rich editor on an error when the field is still empty; an
  inline pick that cannot be sent puts the typed `@bot query` back.
- **Pending draft clears are per slot, not per window** (08-149, 08-150).
  `ComposerUi::clear_draft_on_success` is gone; `ComposerSends` keeps one
  `DraftClear` per account and draft slot (`set_draft_clear`,
  `take_draft_clear`), consumed by the ack whichever window sent, replaced
  by a newer send of the slot, dropped with the account. An ack names the
  message's topic, so a clear of another topic of the same chat is the
  fallback.
- **The `/` menu runs the gates first** (08-132). `send_menu_command`
  answers `MenuSend::{Sent, Refused, Unavailable}`; the typed token is
  removed only for `Sent`. 08-133 is skipped: a command into a paid chat is
  held by the sender's paid guard, and the confirmation box draws for a held
  send (`paid_confirm_view`), as
  `a_menu_command_into_a_paid_chat_waits_for_the_confirmation` shows.
- **The AI answer replaces the draft it was asked about** (08-134):
  `ChatUi::ai_ask` carries the markup asked about; the answer applies only
  while the field still holds it and no edit started, else a note says the
  draft changed.
- **Chat change goes through one function** (08-104, 08-105): choosing the
  open chat again keeps a recording; `enter_chat_ui` and the frame's
  `sync_chat_ui` both call `end_leftovers_of_left_chat` (TTL picker,
  forward selection, recording, profile modal). `sync_chat_ui` takes the
  context for it.
- **Inline-bot state belongs to its chat and window** (09-X001, 10-115,
  10-140). The secret-chat alert stash moved to `ChatUi`. Each window copies
  the session's bot resolution for its own username into
  `ChatUi::inline_bot` and reads that (`inline_bot_for`); a window whose
  answer was overtaken by another window's trigger asks again once the
  other one settled, so its panel does not wait for good. The Start round
  trip ends when the user leaves the bot's chat after the start (no timer).
- **Keyboards and polls** (09-015, 09-016): a one-time keyboard is dismissed
  only after its text went out (`send_command_text` answers whether it did)
  and a Poll button's keyboard is dismissed when the poll is sent (the box
  carries the keyboard to dismiss).
- **Emoji sets** (09-077, 09-087, 09-X043): the box opens with an empty
  search field; the set choice is one `SetStage` (`Idle`, `Switching`,
  `Downloading`, `Failed`): a new choice replaces it, which drops the old
  task and tells a download to stop, so a finished download never switches
  away from the set chosen last; a pack that was just installed and does not
  load is `Failed` on its row instead of downloading again; the saved
  choice is written by the switch still in place. "Suggest Emoji" is
  `AskedWord::{Idle, Waiting}` and ends with a note when the keyword lookup
  gave up (`KeywordLookups::gave_up`).
- **Join requests, join bar, add-bot** (08-059, 08-009, 08-061, 08-012): an
  ask that cannot go out (`ConnectDriver::ask_join_requests`) leaves the list
  `Failed`, which the box draws with Retry; `join_channel` says whether the
  join went and the bar words its note only then; the Add all / Dismiss all
  confirm acts only on the screen it was opened on (`ScreenEpoch`); the
  add-bot box derives "loading" from the request in flight and shows a
  failure with Retry. The Add all count in the box text is the one
  `processChatJoinRequests` acts on (all pending), so a stale count changes
  only the wording; a changed list from another device is not re-read there.
- **Forced replies are per chat** (09-112): `pending_force_replies` holds
  one target per chat and each window arms the one for its own chat on its
  frame (`arm_force_reply_here`), so a chat window shows its bar and a target
  for a chat nobody shows waits. 08-152 and 09-111 are skipped: the preview
  timer is armed once per URL (`preview_timer_needed`) and a forward's
  answer no longer touches a newer forward
  (`a_forward_answer_keeps_a_newer_selection`).
- **Review fixes (s3-messages-2, second round).** A waiting force reply asks
  for a redraw only when it can arm (`Session::force_reply_ready`: its chat
  is shown and its message loaded), so a target for a chat nobody shows no
  longer forces a full redraw on every poll; a later incoming message with
  any markup drops the waiting target, as tdesktop takes the force reply
  from the chat's current keyboard when the chat opens (09-112). A window
  copies the bot resolution only once it is settled, so a `Resolving` copy
  cannot outlive the slot another window's lookup replaced (10-115). A
  refused inline pick gives the `@bot query` back on TDLib's error answer
  through `SentText::InlinePick` (10-116); a refused rich send drops its
  draft clear, so a later send of the chat cannot delete the draft given
  back (08-149, 08-150). `Session::apply` settles the answer board as
  failed when the answer is for a gone account, so nothing awaited waits
  for good. The AI composer answer carries the asking window and only that
  window takes it, even when another shows the same chat (08-134). The
  group settings join-request page asks through `ask_join_requests` (open,
  refresh and Retry), so a send that cannot go out shows the error with
  Retry instead of a spinner (08-059). The page itself is not covered by a
  UI test (no fake live connection exists); the ask is covered by
  `a_join_request_ask_that_cannot_go_out_fails_instead_of_loading`.

### Sweep fixes (s3-messages-3)

- **Menus follow their target.** `close_menus_of_gone_targets` (every
  window's `per_window_upkeep`) closes the message menu when its message left
  the session, the chat-row and top-person menus when their chat did, and a
  pinned list opened for another chat (11-p2-007, 11-p2-008). The overlay
  drew nothing and the open value stayed, so the arrows were swallowed and the
  sidebar peeked until Escape. `HistoryUi::pinned_list_open` is
  `Option<AccountChat>`, read through `pinned_list_open_for` (a lone pin has
  no list, as in tdesktop), so another chat never shows it open (11-p2-014).
- **Per chat, not per composer.** `pending_inline_bot_alert` moved from
  `ComposerUi` to `ChatUi`, so a secret chat's warning stash goes with the
  chat that asked (11-p2-012). The Save-as-HTML folder picker is
  `HtmlSave::Picking(Task)` in `ChatUi`: a second click does not open a second
  picker, a chat change drops it, and a pick for a message the selection no
  longer holds starts its export without clearing another selection (12-088).
- **Confirm boxes run in the window of the bar.** The pinned bar's Hide and
  Unpin all confirm closures carry `QuillApp::scoped_app` (screen epoch and
  window), not a bare weak handle (11-p2-013).
- **Answers matched to what asked.** `MessageReportStage::Checking` and
  `Sending` carry the `RequestId` they wait on; `accept_message_report` and
  `fail_message_report` apply only that answer, so a cancelled report's late
  answer never reaches a newer report of the same chat (11-p3-007). The
  report box's flag is cleared in every window when a report opens or closes:
  the flow is the session's (11-p4-008). A sticker set box's Add / Remove /
  Archive is `SetChangeStage` in the viewed set (`Working`, `Done`, `Failed`):
  `Ok(None)` (skipped as busy) is reported as not done, the button waits, and
  the box closes on the answer (13-001, 13-002). The send-as list is a
  `Fetch` per chat: a refused request shows Retry, an answered empty list is
  not a spinner (12-099). A refused translation request shows failed, not
  the placeholder (`TranslateAsk::Refused`, 13-162), and the offer cache holds
  one entry per chat (13-163).
- **Rapid poll taps** build on the marks of the newest `setPollAnswer` still
  in flight (`RequestRegistry::poll_vote_in_flight`), not on marks an older
  answer's `updatePoll` put back (11-p4-009).
- **Owners.** The location lookup is `Locate::Searching(Task)` inside
  `RequestShare`: closing the box drops it (11-p4-003). What was read about
  tray files (`TrayReads`) lives in `ComposerSends`, the tray's owner, not in
  each window's `SendBoxUi` (12-102). A thread's start is taken by the window
  that asked (`ChatWindows::thread_asker`), not by whichever draws first
  (13-105). A send's notes about preparation failing or a missing client are
  shown only when the send's account is on screen (12-100). Locking takes
  every window's message widgets off screen (14-048, the lock case; minimized
  and tray-hidden windows have no signal to hang it on, so they are not
  covered). A failed widget storage write stays pending: the entry is clean
  only after the write took (14-055). `prune_hidden` counts only hidden
  widgets that hold a runtime or a frame; one whose write fails sheds both
  and keeps its `stored` copy for the save timer, so it is neither retried nor
  logged on every pass, and it never keeps frames. `WidgetEntry::wants_frames`
  is the runtime's own ask; `ticks()` is that ask while a window draws the
  widget, and a widget that comes back on screen asks for its animation tick
  again, as nothing else would (14-049). `ChatWindows::thread_asker` is the
  `ThreadAsker` enum (`Nobody`, `Main`, `ChatWindow`).
- **One-shot outcomes.** `Session::forget_screen_outcomes` runs in
  `put_on_screen`: a bank page, a copied link, a forward answer or an old
  error that landed while the account was in the background is not drained
  as news when it comes back (11-p6-005). It covers every outcome
  `poll_drains` takes (bot callback answers, login-URL answers with their
  request, receipt and unlock notes, send-permission and flood notes); a test
  scans `poll_drains.rs`, so a drained outcome that is not forgotten fails.
- **Lost input.** A reply typed into a notification that could not be sent
  waits in `unsent_replies` and goes into the composer of the chat the click
  opened (11-p5-024); a chat with a window of its own gets it in that
  window's composer (`compose_unsent_reply` in the window's scope, at its next
  frame), and behind the lock it is kept until a click after the unlock. A held history press ends when a move reports the
  button up, or the window loses focus, or Quill locks, instead of polling
  for ever (11-p4-002). A saved manager change that failed (a refused or unsendable
  step) leaves `StickersManager::failure`, which the poll turns into one toast
  (13-010); only the failure of that change's own request counts
  (`ManagerCommit::sent`), not a set box's refused change. An answered empty
  send-as list opens no menu. "Done" in the New Sticker box leaves the box only when the upload
  went out (13-007); a still sticker to adapt that is not downloaded is
  fetched and decoded on arrival (13-006).
- **Pull to the next channel.** The machine leaves `jumping` when the jump
  ends without a new chat (`pull_jump_ended`), and the wait stops under the
  lock instead of opening the chat behind it (04-074, 04-075).
- **Inline results Start.** `InlineStage::Started` carries a `Baseline`: when
  the bot chat's history was not loaded at the start, only messages after the
  user's own start count, not old ones that load later (10-139).

### Sweep fixes (s3-chat-list)

- **A box waits for its own answer.** The folder editor, the Share Folder
  form, the "Add folder" box, the recommended-folder Add and the Add
  Contact box keep the id of the request they sent in one stage value
  (`FolderLoad::Saving`, `LinkOp::Saving`, `InviteStage::Adding`,
  `ContactSave::Saving`) and take only that request's end from
  `ChatListState::dialog_answers` (`DialogAnswers`, filled in one place,
  `Session::apply_payload`, for the purposes `answered_to_a_box` lists).
  Nothing closes on the send any more (03-048, 09-106, 09-X002); a refusal
  shows in the box (09-034, 09-108); a late answer of a form that was
  cancelled lands nowhere. The one-shots `folder_link_saved`,
  `folder_share_error`, `folder_invite_{info,error,done,request,link}` are
  gone (05-054). `scripts/check-dialog-answers.py` keeps a purpose from
  joining the list without a box that takes it.
- **A limit hit names its asker.** `folder_limit_hit` carries the request id;
  the limit box opens in that box's window and only that box's own answer
  ends its wait (09-100).
- **Ends in an error.** The Share Folder box's links and chats are
  `Fetch`es per folder (`FolderShare`), its spec the existing
  `folder_spec_errors`; the "Add to folder" box is `Fetch<Vec<ChatList>>`
  per chat; both draw the error with a Retry (09-X053, 05-062, 09-X057).
  A failed `getChatFolder` ends the editor in `FolderLoad::Failed`
  (09-033, 09-105: already `FolderLoad` from the s12-ui sweep; a test now
  walks the answer). A check of an `addlist` link that cannot be sent or
  answers an error is `InviteStage::Failed` (09-X054, 09-109).
- **Save does not overwrite.** When the folder changed since the editor
  was filled (`fresh_spec` is none), Save fetches it (`FolderLoad::Merging`)
  and sends the person's edits onto the fresh folder
  (`FolderEditor::merged_onto`: pinned chats are the fresh ones, added and
  removed chats are the person's diff against `base`). tdesktop saves the
  box's own copy whole; the merge is the smallest choice that cannot undo a
  pin made elsewhere (03-049).
- **Optimistic edits roll back.** `reorderChatFolders`, `toggleChatFolderTags`,
  `removeRecentlyFoundChat`, `removeTopChat` and `setOption(disable_top_chats)`
  carry `RequestRollback` values (`FolderOrder`, `FolderTags`,
  `RecentSearchRemoved`, `TopChatRemoved`, `TopChatsDisabled`); each puts
  back only if what it showed is still there (02-020, 05-131, 02-202).
- **Recommended Add.** `FolderUi::recommended_adds` is the request per
  recommendation name; a second tap waits, the row resolves by name
  (09-107).
- **Sidebar tab.** `SidebarTab { Chats, Contacts, Calls }` replaces
  `contacts_tab_open` and `calls_tab_open`; a folder switch, Escape and a
  community's "View chats" set `Chats` (08-068, 07-043, 08-092).
- **Pages that fail wait.** In-chat search paging is
  `ChatSearchPaging { End, More, Loading, Failed }` with the cursor inside;
  a failed older page keeps the results and draws a Retry strip (05-130).
  Global search paging is `SearchPaging`; a refused ask (client not ready)
  is `Failed`, not asked again every frame (12-074).
- **Smaller.** `ArchiveMute::reload` is `ArchiveReload { Idle, Reloading,
  Failed }`, left by the error answer and by a send that did not go
  (01-045). A failed `openChat` send closes the chat locally, so the next
  select sends it again (02-023; same for `open_chat_window`). The
  leave-then-delete queue keeps the chats it did not send (02-019). The
  shared-folder "N new chats" bar goes on the `ok` of
  `processChatFolderNewChats`, stays and says so on a refusal, and a fetch
  counts as asked when it answers, not when it is sent (02-058). The date
  jump box, its resolved jump and its note close with the chat that opened
  them and with the account leaving the screen (05-133); a picked day whose
  box belongs to another chat than the view in place jumps nowhere
  (02-194: the search and the pending jump were already per-window views).
  Chat previews are kept by chat, so a late answer for one chat never
  replaces another's (05-061). A found chat or message the driver refuses
  does not enter the chat UI or clear the field (12-075). A bulk Pin stops
  at the first refusal with the limit box up (08-073; the pre-check already
  counted the pins of the batch, since the driver flips a chat when it sends
  its `toggleChatIsPinned`, so only the repeated limit box was wrong).
- **Review round: the twin, the merge, the limit line.**
  - *Edit Contact* (09-X002 twin): its `addContact` Save is
    `ContactSave::Saving(request)` in `EditContactDialog::save`, settled from
    `dialog_answers` by `settle_edit_contact`. The box closes with the
    "Contact saved" toast only on `ok`; a refusal is an inline line and Save
    sends again. The note-only `setUserNote` path is unchanged (nothing asks
    on its answer).
  - *Save onto the current folder* (03-049): Save sends
    `FolderEditor::spec_onto_current(base, fresh)`: the merge whenever a
    fresh copy exists and differs from `base` (another consumer refetched
    after `updateChatFolders`), the form as it is when they are equal, and a
    fetch first (`Merging`) when there is no fresh copy.
  - *Merging has an exit*: the fetch is asked once on entry
    (`start_folder_editor_merge`); a send that fails goes back to `Ready`
    with the error. The per-frame settle asks nothing while the request is
    in flight; a copy that went stale again with nothing in flight is asked
    for once, and a failing send ends the stage in `Ready` with the error,
    so it cannot loop (an `Err` from the inactive path writes no
    `folder_spec_errors`).
  - *Limit errors say it once*: a folder request whose error carries a limit
    hint (`RequestPurpose::folder_op`) answers `DialogAnswer::LimitHit`, so
    the box draws no line (nor toast, for a recommended Add) on top of the
    limit box, as in tdesktop.
  - *Answers nobody waits for are not queued*: the remove-from-folder chain
    sends `RemoveChatFromFolder` (not `EditChatFolder`), which
    `answered_to_a_box` does not list, so its edits cannot push a waiting
    box's answer out of the 16 `DialogAnswers` keeps. (A box that closes
    before its answer still leaves one until it ages out.)
  - *Smaller*: no "Jumping to date" toast when `jump_to_date` sent nothing
    (`Ok(None)`: the guard refused, or the target was already in view); a
    refused `reorderChatFolders` / `toggleChatFolderTags` says why in the
    toast as its rollback snaps the UI back (02-020); the `purpose.rs` docs
    name `ChatListState` fields, not removed `Session` ones.
- **Skipped.** 08-072: `set_pinned_chat_order` already refuses an order
  whose id set differs from the current pinned set
  (`cl2_set_pinned_chat_order_id_mismatch_is_noop`).
### Sweep fixes (s3-messages-1)

- **The sticker panel has a stage per list and one for the set.**
  `StickerPanel::failed`, `loading_sets`, `loading_set`, `set_request`,
  `failed_set_id` and `loaded_set_id` are gone. `StickerPanel::lists` holds one
  `Fetch<()>` per tab (installed, recent, favorites, trending, archived,
  search): begun when the request goes out, ended by its answer in
  `apply_stickers_payload` or its error in `apply_stickers_error`, so one
  tab's failure no longer shows on another and a started request no longer
  clears an earlier failure (02-225). A failed change (favorite add or remove,
  clear recent, reorder, set install or removal) is a `creator_note` toast, not
  a list failure (05-137). `StickerPanel::set` is `StickerSetStage` (`Idle`,
  `Loading { set_id, request }`, `Ready`, `Failed`): an answer or error applies
  only to the request the panel waits for, choosing another set asks for it at
  once instead of waiting for the first answer, and a failed set is asked again
  when the panel opens again (05-266). `refresh_stale_panels` clears each stale
  flag only after its refetch went out, so a locked window or a refused send
  keeps the rest (02-223). The category strip is `Fetch<Vec<EmojiCategory>>`
  (`Failed` on an error answer, cleared when the panel opens again, never asked
  again from a render) (02-220). A refused manager step is reported by the
  manager's own failure note (`StickersManager::failure`), not a second
  `creator_note` (02-221; the merge kept that design).
- **Status choices stop with their panel.** `PickerUi`'s farewell (kept
  from the earlier sweep at merge time) closes the GIF gate and the status
  choices (03-033; 02-052 is the same bug: the chat look box's
  `chat_status_open` is already closed by `DialogUi`'s farewell).
- **Lookups that may pass are asked again.** `MessageEffects::lookups` holds
  each effect as `Loading`, `Gone` or `Waiting { until }`: only 400 and 404 are
  terminal, every other error and a refused send wait 5 s, doubling to 5 min,
  and an ask whose request is gone is asked again (03-091, 03-092).
  `ReplyTarget::Retry { until }` does the same for `getRepliedMessage`: the
  strip keeps "Loading…" instead of "Deleted message" (05-258).
- **AI answers are matched to their ask.** At merge time the earlier
  sweep's design stayed: the asking window takes the answer and
  `ChatUi::ai_ask` holds the asked markup, so the answer applies only while
  the composer still shows it, else a note says the draft changed (02-146,
  05-106). `AiComposer` and `note_ai_ask` were dropped.
- **Notes queue.** `Notes` (state/domains/messages/notes.rs) replaces the seven
  `Option<String>` one-shot slots; the drains show every note, oldest first
  (05-103).
- **Answers match what asked.** The Report flow keeps the request in
  `Checking(request)` and `Sending(request)`; an answer or error of a
  closed flow's request changes nothing (05-236). `MenuActions` keeps
  `getMessageProperties` answers by (chat, message), so a gallery row's ask no
  longer overwrites the menu's; a failed ask hides the gated rows, and the
  reducer refreshes a ticked gallery row from each answer (05-104). A
  failed `getSharingProtection` opens the "Send Request?" box instead of
  turning protection off (05-105; tdesktop's source was not at hand, so this is
  the safe choice, and `noforwards-requests.md` says so). A START parameter
  leaves `bot_start_params` when TDLib answers the start message, so a refusal
  keeps it (08-026).
- **Forced replies per chat.** Kept at merge time: `pending_force_replies`
  holds one target per chat and `arm_force_reply_here` arms the window's own
  chat; a target no window shows waits until one does, then arms and
  dismisses its standalone keyboard (05-215). `ForceReplies` was dropped.
- **Failed sends are not forgotten.** A draft save that cannot be sent is kept
  as the pending draft for the next flush, typing or chat leave (02-026). A
  caption chosen for a pending upload keeps the caption it replaced, puts it
  back with a note when the edit cannot be sent, and goes with its message
  when that leaves the history (02-145). The bot editor drops its half-started
  state when the second load cannot be sent (02-164). A pasted image goes into
  a folder of the private work root (`quill-send-photos`, swept, removed at
  exit), made new with mode 0600 (02-004). The same quick-reply change or send
  is not sent twice while the first is on its way (02-190). `stop_poll` uses
  `loaded_message` like the other poll calls (02-195). `resendMessages` goes
  through the paid guard (07-002). `createForumTopic` also lets the chat list
  row ask for its topics again (05-169). A failed game button press is noted
  (05-036).
- **Boxes close on the answer.** The Add-bot and Share-game boxes stay in
  `BoxSend::Sending { request }` and close on the answer (`Session::acks`,
  settled in `apply`); an administrator's start payload goes after the promotion's
  answer; an error leaves the box open with the reason (08-011). A send belongs
  to the box that made it: closing the box (`close_profile_dialog`) forgets the
  send (`RequestAcks::forget`), and an answer, or the confirm button's disabled
  state, applies only while the open box has the same kind and chat, so the next
  box for another chat is neither held nor closed by it. The Create Bot dialog
  can be closed while "Creating…" (the state is not reset then); `poll_bot_create`
  carries the manager and share it needs, so the answer still toasts, opens the
  chat and shares when the box is gone, and a failure toasts its reason
  (08-021). The sticker panel and the manager read loading from
  `StickerPanel::lists`, not `requests.has_purpose`; the lists' data stay in
  their own vectors (moving six differently shaped vectors inside the `Fetch`
  was not cheap). The two English strings left in the answers ("could not
  change the caption", "Could not load sticker categories") are lang keys. The
  Send To switch is `SendToMenu` (`Unread`, `Reading`, `Known`,
  `Applying { want, queued }`): the shell is read once off the UI thread and a
  click during a write waits, so the last click decides (04-087).
- **Tests.** `connect/tests/sweep_s3_messages_1.rs` (a sender that refuses
  sends), `ai_tools`, `message_menu`, `quick_replies`, `private_sharing`,
  `deferred_caption`, the effect and reply-target units, `paid_messages_guard`,
  `notes` and `send_to_menu_section`. None waits on time: waits
  are compared against explicit instants.
### Sweep fixes (s3-settings)

- **Answers under their request.** `state::answers::Answers<T>` files the
  answer of a request a box waits for under that request's id; the box takes
  its own and everything else is dropped. The wallpaper preview is one
  `PreviewStage` (`Editing`, `Sharing { request }`, `Copied`, `Failed`).
  Apply closes the box at once, like tdesktop's `applyForEveryone`; its
  request lives in `DialogUi::wallpaper_applies` (per account, dropped with
  the screen), so the `setDefaultBackground` answer flips the Telegram
  wallpaper whether or not the box is still open, and a refusal is a
  status note. Opening or closing a preview no longer clears
  `background_answers` (answers are matched by id). A Share link lands only for the preview that asked, so a
  closed or reopened preview never copies a stale link (14-020, 14-021,
  02-089). Wallpaper deletions (`deleteDefaultBackground`,
  `removeInstalledBackground`, `deleteChatBackground`) change the lists when
  `ok` arrives, so a refusal leaves them (02-087; the first two purposes now
  carry what they remove). At merge time this replaced the earlier sweep's
  optimistic remove (`removed_default_backgrounds`, restored on refusal);
  its test now checks that a refusal keeps the wallpaper. The chat look
  box kept the earlier sweep's acks-matched `LookStage`; this branch's
  counter-based `LookStage::Awaiting(LookWait)` was dropped.
- **Chat Appearance.** `AppearanceStage` (`Editing`, `NeedsBoosts`,
  `Applying { waiting, failed }`, `Applied`, `Failed`) replaces `sent`,
  `needs_boosts` and the session-wide `appearance_error`. The driver checks the
  whole plan (values, and that the emoji pack may be set) before sending any
  part, returns every request it sent, and the four purposes (name, profile,
  status, pack) answer under their own ids; editing the draft ends the stage
  (08-050, 08-052). The boost-level message of a boosted chat's wallpaper is
  `LookStage::Blocked` of the chat look box, not the wallpaper pages' shared
  error (08-053).
- **Privacy rules.** `PrivacyKeyState` is `Loading(request)`, `Ready`,
  `Saving { detail, confirmed, request, earlier }`, `SaveFailed { detail, error }` or
  `Failed`. A refused save puts back what the server kept (and the call rules
  mirrored from it), keeps the rule editable and says why in the editor;
  a load answer or error applies only to the key's live request; an update
  while a save is out is its echo (04-046, 04-045, 12-013). `confirmed` is the rule the server is
  known to hold: it stays through a second save, moves forward when an
  older save's `ok` arrives (`earlier` keeps what those saves sent) and is
  what a refusal of the live save restores, never an unconfirmed edit. The remembered
  password check is `Checking { request }` and dismissing drops it (05-255).
  New-chat privacy keeps its value while it refreshes and drops a refresh
  that is older than the last save (02-177). A blocked-senders page carries
  the list generation it extends; pages of an older list are dropped and
  appended ids are deduped (05-132).
- **Notification saves.** `OptimisticSaves` keeps scope and reaction
  notification settings sent whole: the edit shows at once, so a second edit
  starts from it, updates are the echo while a save is out, and the last
  save's refusal puts back the settings the server holds: the value of the
  last accepted save of the burst, else the one from before it (11-p5-016). The tray's
  Disable notifications is the app's setting: every connected account takes it
  (11-p5-009). Leaving the Notifications page ends the corner preview
  (11-p5-008). A mute preset closes the mute box only for its own chat
  (08-102; the four mute flags are one `Option<MuteBox>` with a
  `MuteSection` — collapsed, custom row or sound picker, one at a time); the custom-duration fields are
  made in the window that asked (11-p5-015).
- **Language page.** The pack list is asked whenever this session was never
  asked, not once per visit, and a failed list is drawn with Retry (a failed
  send is `Failed`, so nothing asks again from render) (10-156, 10-157). A
  deleted recent pack leaves `LanguageUi::recent` at once; the box keeps it
  with its place for Restore, so every save writes one list (10-155).
- **Settings window.** The log-out box closes only when the request went out
  (13-065). Undo of an agent's reaction or send marks the entry only when the
  change went out, and the delete does not touch the user's forward draft:
  `delete_messages` is the delete, `delete_selection` adds the selection-mode
  exit (13-066, 13-067, 13-068). The contact chooser is a field of `ChatUi`,
  so it goes with its chat and sends to it (13-071). The composer's context
  menu is built in the window of the click (13-056).
- **Off the UI thread.** A wallpaper render that panics finishes its job as a
  wallpaper that cannot be drawn, in the worker and in the inline fallback
  (14-023). Switching a downloading dictionary off ends the row at once and
  drops the progress loop; the blocking read finishes alone and its result is
  not used (05-003). The Windows spell backend's `judge` answers `None` when
  its thread did not answer in time, and `SpellChecker` neither flags nor
  caches that word (05-007).
- **Review fixes.** A failed installed wallpaper list shows only its Retry
  row in the chat look box (the page-level error line is hidden while the
  list failed); a failed language list request carries a translated generic
  message.
- **Skipped.** 13-069: `AgentUi::pick_account` already resets the activity
  filter wherever `picked` changes, with a test
  (`picking_another_account_resets_the_activity_filter`).

### Sweep fixes (s4-platform-2)

- **Agent driver.** A send the composer hands over is stamped with the
  screen epoch and only the command on that screen takes it; a command
  starts by clearing a send an earlier command's deferred half left
  (07-026). The history and the composer carry the open chat's `chat=<id>`
  tag, so redaction follows the chat the frame drew, not the open chat of
  the moment the agent reads (07-027). The takeover pill records the window
  that painted it; another window's pointer is never "on the pill" (07-034).
- **Chat windows.** Activating a window whose state is in place focuses the
  app's compose field, not the slot's copy (08-111). A chat window that
  fails to open gives the chat back to the main window that left it
  (08-110). "Reopen closed window" pops its entry only once the window
  opens; an entry whose chat is gone is dropped (08-109). An open that
  cannot reach its window posts a note instead of vanishing with its
  carry (08-145; the carried forward draft is not restored, tdesktop has no
  equivalent). Places are saved from the geometry each slot recorded, so a
  window the switch runs inside is saved too, and the OS window is removed
  in a deferred step (08-146). `save_pending` is `SaveTimer`, whose `Due`
  holds the timer task and its screen epoch; a timer of an older epoch no
  longer counts (08-142). A lost "Restore chat windows?" box is asked again
  while the stage is still `Offered` (08-144).
- **Demo.** Every `apply_demo_*` numbers its envelope through
  `apply_demo_json`, which catches the counter up to `session.last_seq`
  (09-021).
- **Notification windows.** `Shown` has one `Stage` (`Live`, `Replying`,
  `Unlinked`): an unlinked window is not replying (11-p6-008). A
  notification whose window cannot open goes back to the head of the queue
  and waits for the next change (11-p6-007).
- **Touch Bar (macOS; not built on the Linux runner, reviewed by reading).**
  Only decoded pictures are cached (13-159). A hidden window skips the sync
  and an unchanged bar is not applied again; Quill cannot ask AppKit
  whether the Mac has a Touch Bar, so the model is still built on those
  Macs (13-160). The bar is bound per main window (`Binding`), including
  the window the GPU-loss recovery opens (13-153). A dropped pinned order
  stays on screen as `Order::Reordering` until the model echoes it, the pins
  change, or the request is refused (13-156). Touches carry the screen epoch
  of the bar they came from and are dropped when stale (13-158).
- **Platform.** The timer's update check leaves "Updated to Quill X" alone
  (14-007). A download the disk held restarts at once when the disk is
  healthy by then (14-008). `AccountBoxes::notice` is one `OpenNotice`
  (`None`, `Frozen`, `AgeVerify`), closed when its condition goes away; the
  age check is `AgeCheck` (14-010). A failed request on the cached X11
  connection drops it (14-060). `window_frame`'s applied map loses an entry
  with its window (14-059). The update helper waits on the launch's lock on
  the claim, with no deadline while it is held, and falls back on the pid
  only where the file system has no locks (14-068).
- **Review fixes (Touch Bar).** The bar's screen stamp (`BAR_SCREEN`) is
  set on every sync that reaches the host, not only when the model changed,
  so a screen change onto an identical bar keeps its touches. A pinned
  reorder dropped as stale (screen epoch moved, lock engaged) or taken by
  nobody ends `Order::Reordering` through `reorder_refused`.
  `forget_connection` uses `try_borrow_mut`.

### Sweep fixes (s4-platform-1)

S4 entries, each proved against the code before it was fixed.

- **Agent link (01-008).** The MCP bridge's pending requests and its end are
  one `Waiting::{Open(map), Dead}` under one lock, so a request filed after
  the reader drained is answered at once instead of hanging (the old
  `alive` flag and the map were two things kept in step). A failed write
  ends the link and answers everything still waiting.
- **Agent TDLib calls (01-015).** `PENDING` is keyed by client; the hub's
  `Routes::unregister` (and a dead reader) calls `td::client_closed`, which
  wakes that client's waiters as `TdError::Send` ("could not pass the
  request") instead of at their 30 s or 300 s timeout. A call whose route is
  already closed fails at once.
- **Agent name (01-020).** `CallScope::drop` clears the thread's agent name.
- **download_file copy (01-016).** The copy goes to a sibling temp file and
  is renamed over the file `create_new` reserved; any error removes both.
- **wait_for_message (01-017).** `target::remember_grant` caches only an
  answer, in `wait_for_message` and in `granted_message_arrived`; a failed
  lookup is asked again for the next message.
- **td_call sends (01-018).** `grants.writable` runs before the user is
  asked, so an Allow is never clicked for a send that is refused anyway.
- **Offset tokens (01-022).** A token is bound to the account, agent and
  method that got it (`td_offset::binding`) and refused for another.
- **UI send (01-032, 01-033).** The composer clear after an agent send is
  never turned away by a full bridge queue (it is the one command exempt
  from `MAX_WAITING`), and a failed post is said in the tool's answer.
  `UiSend` and `ComposerSent` carry the account the window showed;
  `finish_ui_send` refuses when the active account changed, and the window
  clears only the same account's chat.
- **Screenshot slot (01-034, 01-058).** `shot::reserve` returns a `Slot`;
  `Asked::give_back` and `take_screenshot` release it when the command is
  busy, times out, is refused, or comes back without a picture.
- **getMessage errors (01-038).** Only TDLib's "message not found" is "there
  is no message"; other errors keep their own text, and a failed re-read after
  a successful edit still says the edit went through.
- **mute_chat (01-036).** The notification settings are read at the write's
  queue turn (`change_built`) and only `mute_for` changes.
- **Folders (01-037).** Membership is read from the lists (`Membership::
  {Pinned, Included, Excluded, None}`): remove takes a listed chat off the
  lists and excludes only a chat that is in by the folder's rules (as
  Telegram Desktop does); add leaves a pinned or included chat alone.
  `CHATLIST_INCLUDE_INVALID` is no longer reported as "folder is full".
- **Secret codes (01-026).** On the sign-in screen a focused text field
  (2FA password, email, name) is not fed to the code buffer and clears it.
  The phone step still feeds it: its field takes digits only. tdesktop reads
  the codes on the settings page of the sign-in screen, not in its fields.
- **Launcher badge (03-069).** Linux: `Launcher::{Idle, Pending, Delivered,
  Failed}`; the dedupe records a count only after the worker emitted it, and
  a failed emit is asked again on the next tick. `invalidate_icon_badge`
  (dock restart, account switch) also resets `Delivered` to `Idle`
  (`Launcher::invalidate`), so the same count is handed over again. A failed
  emit is still asked again every tick, not backed off: the ask is a slot
  write and the worker's connect is the only cost.
- **Launch lines (03-081).** macOS: the process-wide once-cells are one
  `Slot::{Empty, Open, Forgotten}`; each account's open replaces the file to
  write, and a log-out forgets only the account whose file is open. The
  macOS-only code could not be built in the Linux session; the slot logic is
  platform-neutral and tested.
- **Tray under the lock (03-090, 07-013).** `TrayToggles::{Hidden, Shown}` is
  the menu's toggles: left out of the SNI menu while locked, disabled in the
  macOS/Windows menu (an item cannot be hidden there). The two tray actions
  go through `toggle_notifications_from_tray` / `toggle_sounds_from_tray`,
  which test `refused_while_locked`. (`main.rs` calls them; hotspot change.)
- **Key store (04-026).** A failed `FileSecretStore::put` removes the temp
  key file.
- **Open-link child (04-027).** `answer_on_exit` keeps the child in a slot a
  failed thread start can take back and hand to `wait_in_background`. When
  that cannot start a thread either, the child is parked (`PARKED`) and
  polled again by the next such call, not dropped as a zombie.
- **Screen reader (04-035).** The Linux watcher is restarted by `running()`
  after `RETRY_AFTER` once it ended (`Watch::{Idle, Watching, Ended}`; the
  pause doubles with each watch in a row that never reached the bus, up to
  64 times `RETRY_AFTER`, and a read resets it) and
  clears its value when it ends.
- **Tray registration (07-014).** The registration worker shuts a handle
  down itself when nobody waits for it (the tray was turned off while it
  registered); a registration lands showing what it was created with.
- **UI driver input (07-025).** `Driver::{Idle, Running{id, waiting}}` on the
  app: input commands run one at a time, the next starts when the running
  one answers (every switch checked afresh).
- **Skipped.** 04-009: a second process on the same logs folder cannot pass
  the single-instance check (`begin_app_run` runs after it, and says so);
  the only way past is an I/O error from the check, where TDLib's database
  lock stops a second client as well.

### Sweep fixes (s4-payments-2)

- **Earn boxes.** A join or revoke box waits on a `BoxSend` (`Idle` or
  `Sending(RequestId)`), set when the send succeeded, and only the answer
  or error of that request closes the box, opens the link box or toasts;
  the lists still learn a joined or left program (09-X034, replaces
  `JoinBox::joining` and `RevokeBox::sending`). A program click carries the
  list it came from (`ProgramSurface`), and its answer opens a box only
  while that programs box or that peer's join page is open (09-X035). The
  bot photos of a program list are asked every upkeep and the file layer
  dedups, so a refused send is tried again (09-X042, 09-067; no
  `photos_asked` set). The setup sliders are one `SetupSliders` keyed by bot
  and the program the page opened with, rebuilt when either differs
  (09-X036). A reopened Stars Balance page refills the amount (09-068).
  Every earn note toasts (09-069).
- **Gift market.** The bid in flight is `(gift_id, RequestId)` and only its
  answer clears it (09-X067; `bid_gift_auction` already refused a second
  bid, so this guards a late or foreign answer). Stack changes go through
  `GiftMarketUi::pop/clear/close_where`, which drop the slider, custom field
  and Hide My Name when the last box owning them leaves, however it left
  (10-006). A sticker file counts as requested only when its send went out
  (10-012; the auction follow was already `AuctionFollow::Failed`). Every
  market note toasts (10-013). The send-gift choices are one
  `SendGiftInputs` replaced whole by `reset_gift_inputs` when a send or buy
  step begins, so the message, Hide My Name, upgrade and pay-with-Stars
  choices of one recipient or listing never reach another (10-007, 10-008).
  Delete Gift acts for the account and window that asked and only for a gift
  still in the list (10-015).
- **Giveaway form.** `GiveawayOptions` holds `Fetch` lists and the chat the
  Premium prices were asked for; asking for another chat drops the old
  prices at once and the earlier chat's late answer is ignored (10-010).
- **Paid messages.** `PaidSave` is one entry per target, so a Remove Fee
  never disables a channel's Save, and an answer settles only its own
  target (11-p6-006).
- **Premium and groups.** The preview clip slot is `ClipSlot` (`Idle`,
  `Playing`, `Failed(file_id)`), so a clip that cannot be opened is tried
  once per preview (12-018). Closing the Stars box folds "Buy more Stars"
  (12-019). A revoke from the limit box ends as `PublicLinkStage::Freed`;
  the poll drain takes it once and the Type box checks its link again, so
  the 10 s timer is gone and a slow TDLib no longer leaves "link taken"
  (12-017).
- **Suggested posts.** Publish on a busy suggestion keeps its box open with
  the button waiting, and a suggestion that is no longer pending sends
  nothing (13-112). The decline reason's Enter runs in the window of the
  field (13-111). Every failure toasts and every refused payment asks for
  its top-up (13-108).
- **Smaller.** The Fragment line of the collectible box is found in the
  translated sentence and dropped when absent, and its prefix is a lang
  key (08-119). Show My Last Seen / Read Time keeps its box in a `Saving`
  stage and toasts on the answer, or the refusal (12-048; tdesktop's
  source was not reachable, this is the smallest safe choice). A refused
  quit request clears the noted quit, so a lost main window is reopened
  (13-040).
- **Guard.** `scripts/check-drained-notes.py` fails on
  `notes.into_iter().last()` in `src/ui`.
- **Review fixes.** A Publish in flight (the `busy` entry of the post)
  refuses a second Accept Terms box, and the suggested-posts drain closes
  the open box once its post is no longer Pending, so the answer of the
  first Publish cannot leave a live Publish button that then toasts
  "Couldn't publish the post" for a published post. The resale and craft
  Buy steps reset only the gift choices (`reset_gift_choices`), so the
  typed send-gift message survives Back. The Show Last Seen / Read Time
  box waits while any save of its key is out instead of reporting "shown"
  for an unrelated one. The join box's `BoxSend` gave way to the
  integration branch's `JoinStage`/`Resolve` (same fix); `BoxSend` stays
  for the revoke box.
- **Skipped.** 10-011: both openers already call
  `open_box(DialogKind::SendGift)`. 13-037: handled; files handed to
  Quill wait in the app's own `start_files`, and the test
  `files_waiting_for_a_chat_survive_a_switch` pins it.

### Sweep fixes (s4-settings)

- **Wallpapers.** A default-wallpaper request carries the theme it was sent for
  (`SetDefaultBackground { for_dark_theme }`, the Local one too) and its answer
  files under that theme, so two sets in flight cannot swap themes; the
  session field `background_set_for_dark` is gone (02-086). An installed
  wallpaper whose pattern file cannot be read is not remembered as unreadable;
  the fill-only picture is cached until a retry time, not under the final key
  for good (`Rendered::complete`, `Entry::retry_at`). A photo or pattern mask
  that gave nothing at all has the same retry time (`retry_time` takes the
  wallpaper: a file-backed one is tried again, colours alone are not), so a
  photo TDLib was rewriting does not leave the chat bare (14-026). The preview
  slot is hashed with its window id like the history's, so two preview areas
  of different windows stop replacing each other's render job (14-024). A
  gradient bubble uses the history area only when its own window reported it
  (08-014).
- **Settings state.** A saved-sound list answer older than the stale mark is
  dropped with the request (`mark_saved_sounds_stale` takes the one in flight)
  (02-209). A custom sound whose download cannot be sent leaves no pending
  marker (02-210). Turning on "Use less data for calls" and saving the storage
  limits send every network and option even if one send fails, and report the
  first error (02-207). Privacy-data errors are one line per control
  (`PrivacyControl`), shown where that control is; network usage is a
  `Fetch<NetworkUsage>` (05-254). Updates to the language pack being switched
  to wait on the switch (`LateChange`: a patch, or a reload when a string was
  deleted or the base pack changed) and apply when it is ready (05-139). The
  phone prompt's "No" lives with the suggestion (`PhonePrompt`) and starts at
  the question whenever TDLib adds or removes the action (12-107). The phone
  pickers' empty state is Loading, Couldn't load with Retry, or No more
  contacts (12-020).
- **Notifications.** Forum bursts coalesce per chat and topic (04-011). The
  badge pref is saved before the session takes it (`set_badge_prefs`)
  (11-p5-010). The story sound picker is a `MuteSection`, so the Mute panel
  reopens collapsed (11-p5-012); a preset closes the box only when the change
  went out (11-p5-013); the custom fields reset on every opening
  (11-p5-014); the sound pickers close only when the change went out
  (11-p5-017). A sound picked after the lock waits for the unlock
  (11-p5-018). The upload's picker target is part of `ToneUpload` in the
  session, so an account switch cannot part them (`ToneTarget`, shared with
  the UI's `SoundPickerTarget`) (11-p5-019). A failed delete of a saved sound
  says so, at send time and when TDLib refuses it (the error arm adds the
  note and marks the saved sounds stale so the list is read again)
  (11-p5-020).
- **Appearance and spelling.** A refused delete leaves the custom theme
  applied (07-040); a renamed one repoints both slots (07-041). One task waits
  for the Linux accent read (07-049). A zoom change inside the resize callback
  runs it again (`Refire`) (10-141). The dictionary list is read from disk
  when it opens and after a download or removal (13-062); a second download
  says to wait (13-061); a user cancel is `DownloadError::Cancelled` (05-005);
  a refused remove leaves the language chosen (05-006). A dictionary that
  failed to load is tried again after 30 s, and the checker does not remember
  the word it could not judge (05-013); a word of a script no dictionary
  covers is still taken as correct. The resize callback's run ends (back to
  idle, callback restored) even if it panics (10-141). A replaced engine's warm-up thread
  stops (05-014). A verdict asked for before a cache clear is not stored after
  it (05-008). Check spelling on checks every window's draft (13-058).
- **Settings window.** The activity page size lives in `AgentPageUi`, starts
  over with the filter, the account and each visit (13-070). The Add-a-chat
  picker is keyed by a fingerprint of the chats it lists (13-076). Leaving a
  page drops its search arrival, and a search result for the page already
  shown runs leave then arrive (13-078, 13-077).
- **Skipped.** 02-203: the refusal already restores the confirmed rule and the
  call mirror (2ca5a183). 02-206, 02-211: a request that is never answered
  settles through `settle_lost_request` (account loss, failed flood re-send),
  whose error arms end the stages; TDLib itself does not drop requests (test
  `an_unanswered_device_login_and_tone_upload_settle_with_their_account`).
  03-080: a deleted string already asks for a reload of both packs.
  05-004: cancel drops the app task, so the completion arm that enables the
  language never runs (29f24e11).

### Sweep fixes (s4-media-2)

- **Viewer.** A Shared Media viewer merges older pages and asks for more only
  while the session's shared-media panel is for the chat of the viewer's
  current item (11-p1-008). A Picture-in-Picture clip's deletion is looked up
  in the clip's own account, the one on screen or a background client
  (11-p2-005). Speed and volume reach every window's viewer player through
  `in_every_window` (11-p2-006). "Share at" keeps its position only when the
  forward picker is open over that very message (11-p2-004). The edit-video
  frame strip is matched by its own cancel token, so an encode's serial bump
  (and a failed encode) no longer orphans a strip still being cut (14-003).
  The set video editor keeps the cut time with its cover still and hands the
  box none rather than an older frame (14-004).
- **Transcribe** shares the request in flight for the message
  (`recognize_speech` returns `Ok(None)`), so a second press sends nothing
  (11-p3-005).
- **Sticker editor.** "Keep original aspect ratio" uses the ratio of the
  picture as it lies after Rotate (12-003).
- **Profile.** A failed profile chat list keeps its Failed entry and Retry
  when nothing went out (12-039). The personal-photo picker names no box host
  until it is answered, and applies the answer only over the same dialog stage
  it started from (12-038, 12-053 for the camera: the answer also names the
  photo target it was asked for). Take photo stops the stream when pressed, so
  the frozen preview is the frame saved; a failed save clears `busy` and
  offers Retake (12-054). The rating box ends at build when the person's
  rating is gone (12-035).
- **Gallery.** A story tile whose story is not loaded asks for it through the
  story opener (its note says when it could not load) (13-086). The Playlist
  page closes back to the gallery under it, Esc included (13-085); its rows
  count as playing only for the chat the track plays for (13-088); it asks for
  its playlist as it opens and a failed first page draws Retry (13-087); offline
  (nothing can go out) `open_playlist` marks the playlist failed, and
  `retry_playlist` puts the removed Failed entry back, so Retry stays.
- **Keys.** The sender status head is keyed by `ChatMessage` (13-016). A
  switch of account ends the spoiler reveal fades, which last 200 ms (13-042).
- **Players.** A speed drag cut off by the clip ending is cleared with the
  clip (14-018). A player still opening asks for per-vsync frames for 10 s,
  not for ever (14-028).
- **Skipped.** 11-p1-009: the menu actions are already a map keyed by
  (chat, message) in `messages.menu_actions`. 12-044: tdesktop counts the box
  as shown when it opens (`kDisableSharingBoxMaxShowsCount`); not changed.
  12-056: `SliderState::set_value` emits no event, so syncing the controls
  cannot feed back. 14-061: a failed clip is `Failed` for good by design
  (`transport.rs`, "Failed stays failed"), so the decode error left after a
  seek is never read. 14-004's stale-cover drop has no cheap test: it needs a
  decoded video in the editor.

### Sweep fixes (s4-messages-1)

- **Lists and covers.** `EmojiPanel::failed` is gone: `EmojiPanel::lists` holds
  one `Fetch<()>` per tab (installed, trending, search), begun when the request
  goes out, ended by its answer or error whichever tab is shown, so a failure
  stays on its tab and an unrelated request no longer clears it (02-053,
  05-136). The group sticker box reads the installed list's error the same way.
  Covers of emoji sets and of the sticker manager count as asked only once
  their download went out, and the trending sets lose their unread dots only
  once `viewTrendingStickerSets` was sent (02-054, 02-222). Suggestions that
  wait for the installed sets stop waiting when that request fails (02-224).
  A mask list asked before a change is dropped by the change, so it neither
  lands as the new list nor blocks the refetch (05-138).
- **Requests that never left, and answers.** `closeChat` of the main window and
  of a chat window goes through `send_json_request`, which takes the request
  back when the send is refused (02-025). A swept `searchEmojis` counts as a
  failed attempt (03-034). Callback-button answers are a queue
  (`BotsState::callback_answers`), so two answers between polls both show;
  `attachment_menu_toggled` is a list whose entries name their bot, and the
  flow waiting takes only its own (05-047, 06-007). Sticker set edits are
  tracked by request id and their toasts are queued (05-135). `getGiftAuctionState`
  is one `AuctionFetch` (`Idle`, `Loading { request }`, `Ready { gift_id }`);
  the error already applied only to its request (07-004). A bot Stop is a
  `BotStopStage` (`Running`, `Failed`, `Stopped`); sending Stop again leaves
  `Failed` (08-018).
- **Typing and windows.** `OutgoingTyping` holds the topic the action went to;
  `sendChatAction` is built with it (`send_chat_action_in`), a change of topic
  inside the chat cancels the old one and the cancel goes to the old topic
  (07-003). Turning spell check on re-checks every window's composer
  (07-042). The effect badge lookup covers every shown chat (05-134). A
  macOS text replacement applies only while the composer that asked is the one
  shown (06-049, macOS only: not built on Linux).
- **Selections and quotes.** Delete for everyone in a selection needs every
  message to be yours and still open to it (`allows_revoke`), in the offer and
  in the request (02-148). The quote of a selection points at the occurrence
  selected, not the first (`quote_for_nth`; 04-076). Two quotes on adjacent
  lines are one quote in the document (02-098). A poll not sent yet keeps its
  options as choices (04-029). The effect watch drops its oldest key, not
  everything, at the cap (03-095). The reply bar's fetched copies are kept in
  fetch order and show an edit (05-055). Chat-row animations are kept in
  `ChatListUi` (replaced with the account). A row that no list build drew
  between two sightings snaps when it returns; this is counted in builds
  (`RowFxMap::begin_pass`), not seconds, so an idle list still animates a
  change that arrives later (04-030). The quote occurrence counts
  non-overlapping copies of the trimmed selection, as `quote_for_nth` walks
  them.
- **Bot boxes.** The create watcher ends when a later create starts
  (`CreateBotState::starts`; 08-022). The edit box keeps trying to fill when no
  window could take the texts (08-024). Its Save rejection stands only while
  the fields hold the texts it was found for, and "Saved" only while they hold
  what the server has (`SaveCheck`; 08-025).
- **Skipped.** 02-005: already fixed by f86e9e7c (the deferred caption goes when
  its message leaves the history). 02-028: a window's events run with that
  window's view in place, and the menu is drawn only for the shown chat.
  02-228: the Attached Stickers action takes over the one set box by design,
  and every exit of it is wired. 05-046: one inline query per (chat, bot) is in
  flight and the slot records its query. 05-217: already fixed by e38b4dae
  (a `Fetch` per chat). 05-267: already fixed by f86e9e7c (`StickerLists`).
  05-270: `search_gifs` and `show_saved_gifs` drop every in-flight page before
  the query changes. 08-028: tdesktop closes its verify box at once and reports
  the result as a toast, as Quill does.

### Sweep fixes (s4-accounts-1)

- **Answers matched to their request.** `PendingRequest` carries
  `subject_id` (the terms `acceptTermsOfService` accepts, the gift `sellGift`
  sells) and `user_ids` (what `setCloseFriends` saved), so an id-less `ok`
  settles that subject only: newer terms pushed while accepting stay
  (05-022), selecting another gift while one sells keeps its panel and
  convert box (05-223), and the close friends list takes the ids of the
  save that was acknowledged (05-221; s4-stories made the same fix for
  03-010, and the merge keeps its one `user_ids` field and
  `request_for_user_ids`). A
  failure of older terms' accept does not fail the current box. Profile chat
  lists are `ProfileChatsFetch::{Loading(RequestId), Loaded { ids, more },
  Failed}` with `MorePage::{Idle, Loading(RequestId), Exhausted}` inside the
  loaded list (05-264, 05-265): an old page-2 answer or an old failure after
  the list was asked for again changes nothing; `CommonGroupsPaging` is
  gone. In the merge with s4-links-tdlib-2 this design replaces its
  `CommonGroupsPaging::next_page: PhotoPage` (05-185): the follow-up page
  lives inside the loaded list, so a new first page drops it whole and its
  late answer lands nowhere. The proxy list waits for one `getProxies` as `ProxyFetch::
  Loading(RequestId)` (04-069): an explicit refresh does not send a second
  fetch, a mutation's success drops the awaited id so an answer sent before
  the change cannot clear `stale`, and `ProxyState::loading` left the
  baseline.
- **Dropped answers settle.** An envelope ignored as out of order takes its
  pending request and settles it as lost, so a download is no longer counted
  as running and a mutation's spinner clears (05-199).
- **Guards before the send.** The 2FA sign-in requests (password, recovery
  request, recovery code) go through `login_request`, one per purpose
  (02-081). `request_twofa_recovery_code` and `set_login_email` clear their
  "code sent to" step only after the request was accepted (02-080). The
  sign-in recovery request keeps s4-accounts-2's `PasswordRecovery` stage:
  it is sent through `login_request` and becomes `Requested(id)` after the
  send, so the code step still waits for its answer.
- **Writers.** `update_registry` is the one way to change `accounts.json`
  and holds a lock across load and save (04-083). The deferred settings
  writer starts its thread under the queue lock (04-092) and a
  `WriterExit` drop guard resets `writer_running`, marks the cut-short write
  failed and wakes `flush` when a sink panics (04-091). A failed
  `window_state.json` write puts the state back and raises the status note
  (04-094). A stray `recent_hashtags.json` is kept when its owner folder
  cannot be created (04-084). The web view delete that waits for a helper
  checks the salt it read when the account left, so the next sign-in's
  folder is not wiped (01-006). A failed first-run write is logged (04-096).
- **Smaller seams.** A call created while another took the slot is
  discarded (`created_call_discards`, no "declined" banner; 05-225).
  Switching chats releases the left chat's open topic (05-243). `Initial`
  is not offline, like the connection strip (05-246). A deleted direct
  messages conversation forgets its count and revenue (05-248). A draft save
  that failed and was left behind clears the slot's unsaved mark
  (05-244). The peer-QR theme save answer applies only for the save in
  flight (04-014). Leaving, being removed from, or deleting a channel drops
  its joined notice and strip (05-276).
- **Skipped.** 05-019: already fixed by f51f1abc (`ReviewStage`; a failed
  send settles the unsent ids). 05-275: both paths that call
  `set_supergroup_member_counts` insert the full-info entry first (tested by
  `connect::tests::permissions_box`). 05-257: `send_proxy_mutation` refuses
  a second mutation while one is out (`a_mutation_refetches_the_authoritative_list`
  asserts it), so the first answer cannot free a second. 04-093: retrying
  on a timer needs a waiting writer thread; every later submit and the quit
  flush already retry, the failure is shown once, and tdesktop does not
  retry either. 04-096: when the "started" write fails nothing durable can
  tell an install from an upgrade; only the log line was added.
- **Review fixes.** The settings writer's exit guard acts only when the
  thread unwinds: a clean exit already cleared `writer_running` under the
  lock, and clearing it again after another thread spawned a new writer
  allowed two writers at once (a stale batch landing after a newer one, an
  early `flush`). A created call discarded because the slot was taken is
  recorded in `busy_declined_calls` through the same
  `remember_busy_declined` as `queue_busy_decline`, so its end shows no
  "Missed call" and a late `Pending` cannot take the call window. The
  out-of-order answer and the writer-stopped texts go through lang keys.

### Sweep fixes (s3-stories)

- **One story-page op per request.** `StoriesState::page_ops` (`StoryPageOps`)
  tracks every story-page call by its `RequestId`, with the chat page it is
  for, in place of the one `page_op` slot. A read (paging the archive, the
  posted list) and a mutation (delete, rename, pin) no longer replace each
  other, so a failed delete shows while the archive loads and the loading
  state of one chat's page is not another's (02-198, 02-199, 13-135). A send
  that fails fails its own op (`fail_story_page_send`); closing the page
  forgets its calls (`leave_story_page`) and an account going away clears
  them, so a lost request cannot say "Sending" for good (05-281).
- **The page waits for the answer.** `StoryPage::wait` (`PageWait`:
  `Creating`, `SavingPicker`, `Deleting`) holds the one mutation the page
  waits on; `story_page_upkeep` applies its follow-up when that request
  succeeds. The picker and the typed album name stay until the album is made
  or the add is saved, the album view stays until the delete is answered, and a
  failure leaves all three with the error on the status line (13-129, 13-130,
  13-131). A second mutation of the page is not sent while one is in flight
  (a second Create, a second pin overwriting the first, 13-129, 13-132).
  The delete's second click is part of the view (`AlbumView::Album { delete }`),
  so Back, the picker and an answer disarm it (13-133).
- **Composer stage per request.** `StoryComposer::stage` (`ComposerStage`)
  replaces `check_sent`, `post_sent` and `save_sent`. `Checking`, `Posting`
  and `Saving` carry the request they sent and leave with the answer to that
  request; `Posted` keeps Post off so the media is not posted twice
  (05-160, 13-138, 13-139, 06-010). `StoryPostState` keeps `canPostStory`
  answers and `postStory` outcomes with their request ids (`StoryCheck`,
  `StoryPostOutcome`), and `check_can_post_story` always sends so the composer
  has its own id. `StoryManageState` tracks each editStory, cover and privacy
  call by request (`in_flight`, `settled`), so the cover picker, the privacy
  editor and the composer each read the answer to their own call whatever call
  took the status slot; a failed cover or privacy save shows under its story
  (13-123, 06-010).
- **Answers matched to what asked.** The viewers list (`ViewersPage`), the
  report flow (`StoryReportStage::Checking`/`Sending { request }`), the
  insights' public forwards (`ForwardsPage`) apply an answer only for the
  request they wait for, and the dedupe checks look at the panel's own stage,
  not at a request left in the registry by a panel since closed (05-278,
  05-280, 03-007). A first forwards page replaces the rows. `viewMessages`
  answers settle their own ids only (`HistoryState::viewing` is
  `RequestId -> ids`, 05-178).
- **Refused stories are asked again.** A 400 from `getStory` marks the story
  missing as before (the error carries no message to tell "chat not found"
  from "story not found"), but the mark goes when the story arrives, when its
  poster chat becomes known, and on a new Ready (05-163).
- **Viewer.** A story opened over the open viewer goes through the same
  `replace_story_item` as a step, so the reply and live comment draft are
  cleared (13-090). The text reply is cleared and its row closed only when the
  reply went out, and slow mode holds it like a sticker (13-091). The live
  Stars flow is one value (`StarsFlow`: `Idle`, `Box`, `TopUp`); the top-up
  carries the waiting comment and a refused top-up leaves the flow (13-092).
  Opening a panel closes the others (`StoryViewerUi::close_other_panels`,
  `ViewerPanel`, 13-125). The close-friends editor is `CloseFriendsStage`
  (`Closed`, `Editing`, `Saving { request }`) and Saving leaves with the answer
  to its call, also when the list was never loaded (13-141). Save media copies
  on a background task (`MediaSave`) and gives its result under the story it
  was for (13-142). The Stealth Mode box tracks its request
  (`StealthStage::Activating`) and shows a refusal with the button to ask
  again (`StoriesState::stealth_refusal`, 13-128). The cover picker's clip is
  `CoverStage` (`WaitingForClip`, `Probing`, `Ready`, `Unreadable`) and a clip
  that finishes downloading is picked up (13-121). The strip's wheel decides its
  axis per notch (13-146).
- **Per chat.** The screen-reader focus (`A11yFocus`: none, the unread bar, a
  row) and the pinned-messages list are `ChatUi` values, so another chat
  replaces them (10-100, 10-107).
- **Re-review of s3-stories (after the s12-money merge).**
  - 13-092: a top-up no longer carries the waiting comment
    (`StarsFlow::TopUp { mode, stars }`). The box sends the comment only when
    it opens from a send that needs Stars (`SendAction::NeedsStars`); a price
    edit never sends. A comment that went out (`live_comment_sent`) and
    "Remove Stars" leave the flow `Idle`, so a later box does not start on the
    old amount.
  - Composer answers by request: `StoryPostState::check` is `StoryChecks` and
    `outcome` became `outcomes: StoryPostOutcomes`, both keyed by `RequestId`
    (`canPostStory` / `postStory`), so a late answer of an earlier composer
    cannot take the slot of the current one. `Checking` and `Posting` also
    leave, to the composer with an error and Post to retry, when their
    request is gone from the registry and nothing answered it.
  - The close-friends editor is main's `story_extras::CloseFriendsStage`
    over `CloseFriendsList`, with `Saving { edit, request }`; the branch's
    own UI-level stage was dropped.
  - Story page: Create, Delete, Rename, Pin and the album Top / Remove
    buttons are disabled while a mutation of that page is in flight, instead
    of the press being ignored. Save media is kept per story item
    (`StoryUi::media_saves`: `Saving`, `Done(notice)`), so stepping away and
    back neither allows a second copy nor loses the result. The stealth
    refusal text is a lang key; a session going away also forgets the
    management calls on their way (`StoriesState::settle_account`).
- **Not done as the register says.** 13-125 keeps the panels' data in their own
  fields (`cover`, `privacy_edit`, the close-friends stage, four flags) and
  closes the rest through one seam instead of storing a single `ViewerPanel`
  value (about 150 readers); every open goes through that seam.

### Sweep fixes (s4-links-tdlib-1)

Minor entries, each proved against the code first.

- **Sent before recorded, recorded before sent.** A group bot's `getUser` that
  could not be sent leaves `asked_bot_users` (02-084). A mask set's request
  that could not be sent leaves `set_requested` (02-139). `setTdlibParameters`
  that could not be sent takes its `SetParameters` entry back (02-039).
  `request_close` puts back the logOut request `begin_close` cleared when its
  send fails (02-038). Edit and Remove of a proxy drop its ping result only
  once the change was accepted (02-189). The retry stash is written before
  `td_send` and the entry taken back when the send fails, so a fast 429 finds
  its request (02-231).
- **Answers matched to what asked.** A superseded username check's failure
  sets no profile error: only the check that is still current does (05-156).
  A failed `getChatMessageCount` drops the chat's entry, so the next open asks
  for all of them again (02-030). A lost export request (flood-retry re-send
  failed, TTL sweep) frees the export's link, so the worker is not left
  waiting out its timeout (03-046). Composer link previews never evict an
  entry that is still `Asked` (05-056). The sharing request is answered from
  `loaded_message`, so a request shown in a topic or other view works
  (02-196). The Translate card made with a style the server removed is asked
  again without it, like the Style tab (01-055).
- **Failures drawn.** A message statistics send that fails is `Failed`, which
  has Retry, not `Loading` (02-217). The Collect password box shows an error
  when its request could not be sent (04-043). A refused zoom is not recorded
  (02-219; `zoomed` stays keyed by the graph token, which TDLib makes unique
  per graph).
- **Window and account scope.** The GIF search asks with the chat of the
  window that searched (`GifPanel::search_chat`), not the main window's open
  chat (02-061). A log-out sweeps only the on-screen account's media scratch
  (`sweep_account_media_cache`), and `ensure_private_dir` makes a directory
  again when the OS removed it (03-071, 03-072).
- **Links and the picker.** A ninth queued external link is `Full`, not
  silently dropped: the poll turn says "Too many links opened at once" once
  (03-019; tdesktop opens each at once and has no queue, so this is the
  smallest safe message). Venue icons that failed or never answered are asked
  again when a picker opens (03-075). The address hidden while the pin moves
  is kept with its place and restored only there (03-086). A launch whose
  sender thread cannot be spawned reports `NotSent` and Quill starts
  unchecked, not "not responding" (05-012; not tested: a thread-spawn failure
  cannot be forced). Profile accent colour is refused while the own user is
  unknown instead of sending emoji id 0 (02-171). A failing call-audio send is
  no longer a change to redraw for (02-056).
- **Skipped.** 02-043: `resume_disk_paused_found` already puts unsent ids back
  and always saves. 02-071 and 02-178: the asked set and the request box are
  already written after the send (tests `a_grams_peer_request_that_never_left_…`
  and the private-sharing tests). 03-005: the wallet's explorer, rates and
  state are already `Fetch`-like stages with `cancel`/`fail`. 05-177: the add
  contact box reads its own answer (`ContactSave::Failed`, test
  `the_add_contact_box_closes_on_the_answer_and_shows_a_refusal`); the contacts
  notice belongs to the settings list.

### Sweep fixes (s4-misc-1)

- **Lost answers settle.** `canTransferOwnership`, the later-page and
  tab searches (`SearchMessagesMore`, `SearchPublicPosts`,
  `SearchPublicPostsMore`, `SearchPublicMessagesByTag`) and the recommended
  channels are sweepable lookups. A lost answer settles through the error
  arm each already had, so the transfer box and the search tabs end in an
  error with their retry instead of a state nothing ends (05-193, 05-202).
- **Answers matched to what asked.** A late `isProfileAudio` answer
  never overwrites a value a local save or remove already set: the check is
  asked only for an unknown song, so a known value is newer (05-194). A page
  of a post's public forwards lands only in the list that asked for it:
  `PublicForwardsList::asking` holds the request, `loading` is derived, and
  reopening a post whose older page is still out asks again (06-008). A
  late relay Ack from the document before a reload is ignored by sequence
  (`Relay::stale_through`) instead of ending the carrier (01-003). An
  import-contacts read and the file picker's answer carry tokens counted
  process-wide, so a closed box's read cannot land in the next box
  (09-001). Selection check fades are keyed by chat and message (11-p4-014).
  A purchase on its way in the marketplace box is not taken over by another
  chat's box (10-171).
- **Every window.** Auto-download progress of a chat a chat window draws is
  live there (`open_chat_media_downloads` records the chat that asked,
  05-195). Fast buttons mode rebuilds the rows of every window, from the
  Experimental page and the bot's own switch (07-024). The device-link
  answer is dropped with the link flow (`clear_device_qr`), so no stale
  "Device linked" shows on the next visit (09-X015, 09-028).
- **Owner and exits.** The Downloads panel's open flag is `ChatListUi`'s,
  so it starts closed when an account comes back (09-053). The folder zip is
  an `ArchiveJob` of the composer (`Idle` or `Building(slot)`), one at a
  time; an answer for another chat is dropped and its "Preparing" note goes
  (03-050, 09-055). The Quit's pending exit is `MainWindow::export_stop`,
  beside the window-level alert (09-098). The window-place save timer is a
  `StateSaveTimer` that holds its task and is `Idle` again whether or not
  the window was still there (09-117). The experimental page reads the
  clipboard when it opens and when the window is activated, not on every
  render (07-023). Autoscroll's loop is a `Task` inside `Active` (08-038).
  A refused fact check keeps its box and text (09-102). A replayed intro
  clip ends when the app locks (09-X051). A storage slider drag whose
  release never came is applied when the page is left (10-133).
- **Skipped.** 05-045: the conference's `invite_results` is already cleared
  by leaving or ending the call, by the call's own update and by logout.
  10-165: the tick runs from each window's `per_window_upkeep`, is scoped to
  that window, and `frame.window_active` counts chat windows.

### Sweep fixes (s4-groups)

- **Requests that never answer.** A swept or logout-settled request takes its
  waiting group-admin follow-ups with it and says so (02-074). A refused part
  of the business bot lookup clears the recorded query and drops late answers,
  so the same text asks again (02-032). A refused poll-votes request is a
  `Failed` chart with Retry, not an empty slot the next render asks for
  again (13-030).
- **Answers matched to what asked.** The join-request list keeps its newest
  request id after it answers, so an older page that lands later is dropped;
  "load more" asks the registry, not that map, whether a page is out; a stale
  load-more error says nothing (05-078, 05-084). A membership update forgets
  an administrator list asked before it, so its answer cannot put the old
  administrators back (05-079). A list error does not fail a list that
  already holds an answer (05-085). A hidden join request does not bump a
  searched total (05-080). Statistics carry the theme their request asked
  for in `Loading`/`Loaded`, and a poll drain asks again when the theme
  changed (13-032).
- **Saves in parts.** A permissions Save whose later part is refused returns
  `PermissionsSent::Partial { sent, refused }`; the box stays, names the part
  and a second Save sends only what still differs (02-168, 10-065). The
  upgrade-to-supergroup confirm is a `Submit` page: it closes on the answer
  or the upgrade event and shows TDLib's reason otherwise (10-043).
- **New group dialog.** A made plain group takes its dialog (and its creation
  row) away from a poll drain, like a community's chat (02-150, 05-087,
  11-p5-001). Editing the link no longer forgets a type save still out
  (11-p5-005). The subscribers step finishes only when the add went out; tdesktop
  closes at the send, so the answer is not waited for (11-p5-004).
- **Stage owners.** Add Members keeps the picked contacts when the add did
  not go out (09-051). Unban, kick and remove no longer refetch the page
  before the change applies: the request's `ok` marks the member list stale
  (10-061). A saved Edit box is dropped, not hidden (10-055). The Custom
  limit field clears its focus request when its content is drawn
  (`Rc<Cell<bool>>` shared with the content closure) (10-150). Welcome
  Retry only re-asks the load, so typed text stays (10-076). A blocked
  Public choice is not stored in the draft (10-083); the apply-to-links and
  username alerts name the box that asked (10-084, 10-086). The Share of a
  group link names the asking window (10-095). A chat window of an upgraded
  group is replaced from whichever window is live (10-044). A failed topic
  select forgets no scroll position (10-073). The revoked-links list opens
  only once its request is out (10-143); Requests on a link says when the
  driver refused (10-147).
- **Smaller.** One welcome message per chat in flight, not per account
  (02-132); revoked links wait for the viewer's id (02-151); one failed
  profile thumbnail download keeps the rest of the queue (02-186); a saved
  business time zone stays when TDLib's list lacks it (08-007); typing on
  past a debounced bot lookup reads as Loading (08-006); a Created
  community closes the identity box too (08-124); the monoforum ban item says
  Checking until the member status is known (09-063); a filter naming only
  gone administrators opens as all (03-047); the boost count buttons wait
  for the save in flight (02-124).
- **Skipped.** 09-052: the New community box is a `Submit`; a refused
  `createCommunity` lands in `Failed` with the reason and the box stays
  (`settle_create_community_dialog`). 10-094: the info panel target is a
  selection that draws from cached data; a failed fetch shows the error note
  and reopening asks again, so no stage can stick.

### Sweep fixes (s4-chat-list)

Twenty-four S4 entries. Each was read end to end against the integration
branch first; eighteen were real and fixed, five were already fixed by the S3
chat-list sweep, and 09-X059 duplicates 09-101.

- **A swipe runs what the panel showed.** `SwipeMachine<K, A>` carries the
  action the row offered at its latest event; the release stores it and `poll`
  returns `(key, action)`, so a chat that was muted meanwhile does not flip
  Mute into Unmute (02-007). `poll_swipe` runs nothing while locked and
  `close_for_lock` drops the gesture and every spring-back (08-113).
  tdesktop resolves the label per paint and per perform; the lock gate is
  Quill's own rule (`lock-is-a-render-state.md`).
- **Search.** A filter change bumps the generation and clears the results
  (`SearchState::restart_for_filters`), so the Recent answer of the old tab no
  longer lands under Channels (05-203). Picking a chat or a hit closes search
  through `ConnectDriver::close_search`, which clears the typing debounce
  (02-214). `getPublicPostSearchLimits` carries the search generation, is
  applied only for the open one, and the quota of the previous query is
  dropped when a new public-posts query starts (05-128).
- **Shared-media calendar.** `jump_shared_media_to_day` returns
  `SharedMediaJump::{Jumped, NoMedia, Stale}` and takes the calendar only after
  its checks: a day with nothing near it keeps the box open with a note; a
  calendar of another chat is dropped (02-232).
- **Notes are a queue.** `chat_action_error` is `Notes` (the queue the message
  notes already use): two refusals before the drain are both shown, oldest
  first (05-063). The logout settle keeps it with the other queues.
- **Suggestion box.** `hide_suggested_action` removes the action locally before
  it tries to send, so a dismissal that cannot go out does not reopen the box
  and add a toast per close (07-045).
- **Chat list drags.** A forward drag entering the list drops the file drag's
  probe, hover and one-second timer, and `choose_row_by_drag` refuses while a
  forward drag is seen (08-064). The freeze timer, which every drag update
  re-arms, ends the drag state too when no drag is active any more, and
  `drag_left` clears even when nothing was hovered, so the next drag of the
  same paths is probed again (08-065).
- **Rows not on screen.** The Contacts and Calls panes and search results clear
  `chat_list.items`, so key stepping and file drags never read a list that is
  not drawn (08-075).
- **Row animation per account.** `RowFxMap` moved from `FrameUi` into
  `ChatListUi`, so a switch starts it empty and a row snaps on first sight
  (08-098). It is bounded by one account's chats.
- **Contacts.** A failed send in `fetch_contacts` sets `contacts_error`, so
  Retry ends in the error line (09-002; `set_contact_sync`'s refetch uses the
  same path). The Sync switch is saved first and put back when the save fails
  (`ConnectDriver::set_contact_sync`, 09-X003).
- **Folders.** The copy button's check mark is a `CopyMark` enum cleared by a
  `spawn_screen` timer matched to its own copy (09-035). The join box for new
  chats sends and counts `selected` intersected with the current offer
  (09-101, 09-X059). The delete box asks `getChatFolderChatsToLeave` through
  `folder_chats_to_leave: HashMap<i32, Fetch<Vec<i64>>>`: Loading disables the
  confirm and says so, a failure draws the error with a Retry, and Delete stays
  possible (09-X058). A failed delete no longer moves the tab to All; the poll
  does it when `updateChatFolders` drops the folder (09-113).
- **Already fixed** by the S3 chat-list sweep (e38b4dae, 6b7156e4): 05-060 and
  09-X017 (the share box takes only the answer of its `LinkOp::Saving(request)`;
  `folder_share_error` is gone), 09-036 (`InviteStage::Checking(request)`), 09-X055
  (`InviteStage::Adding` is left by the matching answer, error or lost-request
  settle), 09-X056 (`folder_limit_hit` carries the asking request).
- **Not verified on screen.** The headless demo has no live driver, so the
  delete-box Loading text and the Retry row were checked by tests only.

### Sweep fixes (s4-accounts-2)

Twenty-six S4 entries; the code was read first and each is fixed, a
duplicate, or skipped with its reason.

- **Window answers.** `WindowViews` numbers every opening of a chat window
  (`opening_of`, never reused); a request carries the opening of the window
  whose view was in place (`PendingRequest::window_opening`), and
  `apply_payload_in_asking_view` applies it to a view only while that
  opening is the window's. A window closed and opened again for the same chat
  takes none of the old window's answers (06-006).
- **Forum topics.** The order of a new topic and of a topic that moves to the
  top is a saturating add (05-285; the `getForumTopic` refetch the entry asks
  for already runs from `possible_topic_refresh`). `replace_forum_topic`
  checks that the list is loaded before it merges drafts (05-286).
- **2FA password recovery.** `AuthState::password_recovery` is one enum
  (`Idle`, `Requested(RequestId)`, `CodeSent`) moved by the answer to the
  request that asked: the code step draws only after `ok`, a refused request
  goes back to `Idle` with the auth error on the password step, and auth
  leaving the password step resets it. `AuthUi::recovery_mode` is gone
  (08-043).
- **Already fixed.** 08-045 and 09-X014: the terms prompt is
  `Option<TermsPrompt { terms_id, stage }>`, replaced whole for another
  version and dropped when the terms go (f51f1abc, with a test).
- **Account box.** Log out from it goes through `close_account_lifecycle`,
  so the typed password and reason are emptied (07-018).
- **Data and Storage.** The per-network editor is one `StorageEditor`
  (`Closed`, `Open`, `Saving { request }`): Save keeps the draft until the
  session's `AutoDownloadAnswer` for that request id says `Saved` (closes) or
  `Refused` (reopens with the draft); the answer is drained by the poll
  (09-042). "Reset statistics" is `NetworkReset { Idle, Confirming }`, dropped
  by `leave_data_storage` (09-040), and kept with a page error when the send
  failed (09-041). "Clear selected" forgets the ticks only when the clear went
  out (09-044).
- **Video chats.** The Schedule box keeps the refused time with its message
  and draws it only while that time is the picked one (09-X026). Before
  leaving the call you are in, `check_create_video_chat` runs every refusal
  `createVideoChat` has except the call itself, so a request the driver would
  refuse (a start time too close, a closed connection) no longer ends the
  original call; the same for "Stream with..." (09-045).
- **Lock screen.** `LockNotice { None, Wrong, Flood { until } }` replaces
  `lock_error` plus `flood_until`: typing clears a wrong-passcode line, never
  the flood wait that disables Unlock (11-p7-006). The GIF gate is shut while
  locked and `set_locked` re-syncs it both ways (11-p1-003). Log out from the
  lock screen sets `PasscodeJob::Cancelled`; the passcode check and the system
  unlock drop their late answer and clear the master key (11-p7-008). The
  auto-lock time and system unlock change the screen only after the write
  succeeded, and say why otherwise (11-p7-009).
- **QR box.** A failed code draws nothing instead of the previous link's
  (11-p8-011). Copy or Share finishing in a box whose code or colors changed
  says so (`QrToast::Changed`, a new string) instead of doing nothing
  (11-p8-013). The share chooser is remembered as `QrShare::Chooser { window }`;
  another window's pick or cancel leaves the box alone (11-p8-010). The
  screen's `Farewell` removes a card file still waiting for its chat
  (11-p8-009).
- **Security.** The two-step dialog falls back to its status screen from the
  poll's drain (`drain_twofa_step`), not while it draws (13-171). The
  sessions layer is open only while `details_session()` resolves (12-097).
- **Security keys.** A login round gives the PIN only to the key that asked for
  it (14-080). On Linux a key that exists but cannot be opened ends the
  ceremony at once with `AccessDenied` and its own message (14-079;
  `hid::Found`).
- **Skipped.** 09-X025: the Waiting step has nothing to show; its answer is
  applied by the window that hosts the box, and a minimized window draws
  nothing either way. The answer's two effects (the box, an error toast) need
  that window, and it recovers when the window comes back, as the Asker seam
  intends.

### Sweep fixes (s4-media-1)

- **Emoji effects.** A lone emoji read while the effects pack loads is
  marked watched when it waits, so a failed pack load (or the queue pushing
  it out) puts it back as unwatched and a later read plays it (03-035). A
  tap that arrived more than the start wait before a window took it is
  dropped: `EmojiInteractions` stamps each effect with its arrival and
  `take_fresh` hands over the recent ones, so taps made while the app was
  locked do not play at the unlock (09-X038). The "watching" action carries
  the effect that started, not the first effect of its chat (09-084). The
  anchors, badges and effects noted by bare chat id go with the account on
  screen (`MessageUi`'s `Farewell`, 09-085).
- **Emoji sets.** Each unpack of a pack has its own temp and move-aside
  folders and swaps under one lock, so two installs of one set do not wipe
  each other (03-044). The installed set list for the `:` suggestions is
  asked from `EmojiSets::installed` (`Fetch<()>`): from idle or after a
  failure, and set to loading only when the request went out. The UI flag
  `EmojiBarUi::sets_asked` is gone (09-080, 09-X037).
- **Gallery profile tabs.** Stories and Gifts requests carry the tab's stamp
  (`begin_profile_fetch` restamps, `begin_profile_more` keeps it); an answer
  for another stamp, from before the gallery was closed and reopened, is
  dropped whole, and a request is only "already in flight" for its own stamp
  (06-004).
- **Reactions.** A refused reaction puts the old chips back only while the
  message still shows what the request made; with another change on top it
  takes back just its own reaction (05-191).
- **Media playback.** The shuffle's played list drops tracks that left the
  playlist (04-022). A swipe steps from the same armed value the arrow shows
  (03-097). A seek slider event is applied only for the slider of the active
  track (08-041). A frame ThorVG cannot draw fails its clip, so views rest
  and the frame is not asked again every tick (06-012). The idle watch
  thread starts through `spawn_guarded` and clears its flag when it panics
  (07-048).
- **Panels and windows.** The reaction and status selectors enter their mode
  through one function that also puts the panel on the emoji tab and syncs
  the GIF gate (11-p1-002). Closing a word's bar (typing on, blocking, the
  setting) resets the `EmojiBarUi` it shares with the suggestions, like the
  other ways of closing (09-081). Tapping a custom emoji again restarts its
  card timer (each tap has a count the timer compares, 09-019). The album
  pin label is kept per history revision instead of cloning the history on
  every chrome redraw (11-p1-007). The viewer's farewell switches the
  window's extended range off (10-099). A failed image decode is tried again
  after 30 s (10-112). A sweep that met a window mid-draw wakes itself once
  more (10-113). The demos report a failed check instead of panicking
  (09-029); a test no longer sleeps (07-050).
- **Skipped.** 02-141: the scan runs once per user click, not per frame, and
  an index would be state kept in step with every edit and delete. 03-039:
  the switch is already one `SetStage::Switching`; a replaced switch's task
  is dropped, so its continuation (which saves) never runs, and the load it
  ran is a read. 05-095: a cancel with no request of ours is only sent for
  a transfer the file map shows active, and TDLib's idle update for that
  transfer is the one that settles it (a paused one is not cancelled this
  way).

### Sweep fixes (s4-messages-2)

Minor entries of the messages area, proved one by one.

- **Sends and the composer.** The reader's saved scroll and the recent
  hashtags are noted by `note_message_leaving`, called where a send has
  passed every refusal (slow mode, rights, size, the paid box) and goes out,
  not when Send is pressed (08-137). Delete confirm dissolves the message
  only when the delete went out (08-138). A bot's Restart after Unblock
  sends `/start` only when the unblock request went out (08-010). The
  attach picker's answer waits for the unlock instead of filling the tray
  while locked (08-141).
- **Composer boxes are the chat's.** The link, date and Code Language boxes
  are fields of `ChatUi`, so a chat or topic change replaces them and Save
  cannot apply a range to another chat's draft (08-140, 08-154). "Send as a
  document" for an edit's replacement file is a field of the edit
  (`EditMode::replace_as_file`), so it ends with the edit and another edit
  starts without it (02-092).
- **ChatUi of a closed chat.** `sync_chat_ui` replaces the value when the
  reducer closed the chat (removed, left), so reopening it brings no old
  bars (08-106).
- **Menus and keys.** `open_submenu` reads the chat list's and the
  community's menus only in the main window, so an arrow key in a chat
  window's message menu does not close the main window's submenu
  (09-X004). The reactor tab a chip's menu asked for is dropped when the
  next menu opens, so it cannot outlive its menu (11-p3-010). Copy Option
  Link and Delete Item say when the request could not be sent (11-p3-012).
  Save to Saved Messages tells a connected account whose id or draft is
  missing from the demo (11-p3-009). A failed reactor tab or viewer list is
  drawn with a Retry row (11-p3-011; `menu_audience_page.rs` is split out).
- **Boxes and links.** The wallpaper file picker's answer applies only to
  the chat look box that asked (`LookTarget`, 08-097). An Add-bot or
  Share-game link does not replace an open profile box (Edit Contact,
  Birthday ...) and says to close it first (08-013).
- **Join requests.** "Show more" is disabled while a page is on its way
  and a send that cannot go out is a status note (08-060).
- **History.** `scroll_view_probe` resets with `scroll_top_probe` when the
  history key changes (09-005).
- **Emoji.** Closing the emoji packs box bumps the search keystroke, so a
  search waiting on its pause does not go out (09-078, 09-X039; the field is
  already cleared on open). Download progress notifies only while the
  Choose emoji set box is open (09-X044). Saves of the chosen set run in a
  chain, in the order the switches were applied (09-088).
- **Inline mode.** A failed first page is asked again by the next composer
  event instead of counting as fresh (10-117); closing the results panel
  bumps the query token (10-118). A sleeping inline player whose file no
  longer opens becomes `Rest::Stuck` and keeps its still, so no sweep opens
  it again (10-128).
- **Message effects.** The badge, replay and waiting registries are emptied
  when `OnScreen` is replaced (`EffectNotes` guard), so an account switch
  leaves no entry that could match another account's message (11-p2-016,
  03-096).
- **Skipped.** 08-136: a rich send's flag is dropped on its refusal
  (`apply_sent_text_answer`) and a voice or round video's is set only after
  the send went out; what is left is a later async send failure, which
  cannot be tied to the flag without a request-to-message map, and dropping
  it by slot would resurrect the drafts of later sends. 09-X011: tdesktop's
  `HistoryWidget::newItemAdded` scrolls to the end for any new outgoing item
  of the open history, not only for the view that sent it. 09-X007: the
  dismissed set would need a home every window's view reads; online,
  `deleteChatReplyMarkup` reaches every window's session through TDLib.
  10-131: the mark at the attempt is what stops a file outside the sandbox
  from being tried (and noted) every frame; a download wait plays through
  the intent. 10-129: `forget` already drops `on_demand` with the players;
  the other window's scale only sizes the decode. 10-120: the Started stage
  ends when the user leaves the bot's chat, and the scan is over the loaded
  messages only. 11-p2-011: already fixed by f86e9e7c (`MenuActions` keyed
  by chat and message). 11-p2-010: the reaction options and the paid offer
  are keyed by chat and message, and each menu opens by asking its own, so
  a stale value never applies to another message. 11-p3-003: already
  `EffectLookup` with `forget_dropped` (a lost answer is asked again).
- **Test repaired.** `an_old_bot_message_loaded_after_the_start_does_not_bring_the_user_back`
  (already failing on the integration branch) opened the bot's chat only after
  the start went, so a frame in between saw the user outside it and ended the
  trip; the chat is opened before the start, as in the app.

### Sweep fixes (s4-payments-1)

Minor entries (S4). Each was read end to end first; the ones that were
already fixed or cannot happen are listed at the end with the reason.

- **Earn / affiliate.** `JoinBox::joining` is `JoinStage` (`Open` or
  `Joining(RequestId)`): only the answer or error of that request acts on the
  box, and a late Connect ok fills "My Programs" but closes or covers no
  box opened since (03-027, 05-124). A click on a program is `Resolve`
  (`Idle` or `Asking(RequestId)`); only its answer opens a box, and leaving
  the page (`set_open_info_panel`, the programs box closing) returns it to
  `Idle` (03-027, 05-124). TON and Stars history pages apply and fail only
  for the request the list waits for, like the program lists (05-123; 02-048
  was already fixed the same way). "End program" closes its confirmation
  only when the send went out (02-051). Earn toasts, the Fragment URL and the
  setup page's "go back" of an account in the background are forgotten when
  it comes back on screen, with the other drained outcomes (03-028).
- **Suggested posts.** `SuggestBar::send` (`BarSend`) keeps the edit's bar up
  until its send answers: the `message` answer takes it down, an error returns
  it to `Idle`, and a second send waits (03-012). `busy` is cleared only by the
  approve or decline's own ok or error, not by an offer update or another
  request on the same post (06-041).
- **Limits, labels, files.** `getPremiumLimit` asks are kept as
  `LimitAsk::{Asked(request), Refused}`: a lost or transiently failed ask is
  asked again, only the server's refusal stays (04-049). `PsaLabels` keeps a
  `PsaAsk` per type; nothing is marked asked before its request exists, a
  failed send or error returns the type to asking (three tries per pack)
  (04-071). Files handed to Quill past the queue's size are counted and a
  toast says how many were not added (05-033). The Remove Fee box keeps an
  earlier save's slot (04-054). `StarAmount::label` no longer overflows on
  `i64::MIN` (04-051).
- **Sign-in and security.** The 2FA round trip is `PasswordFlight`
  (`Idle` or `InFlight(RequestId)`): an older request's late answer or error
  neither frees the newer one nor writes its error line (05-039).
- **Chats and links.** `setDefaultBackground` carries `for_dark_theme` in its
  purpose and files under it (05-067). A deep-link failure is stale only when
  no stage carries its generation (05-066). A handled community add-chat
  request is remembered until the next page is asked, so a page in flight does
  not bring it back (05-073).
- **Gifts.** An ok closes the flow it belongs to (`GiftFlow::answers`), not
  whichever flow is open; a transfer's ok closes the picker that sent it
  (05-118). The export password box is `ExportSend::{Idle, Out(RequestId)}`,
  and the temporary password, the URL and the error act only for the request
  it holds (05-119).
- **Names.** The name-color palette is kept per account and the renderer
  reads the account on screen (`set_active_account` in `put_on_screen`), so a
  background account's `updateAccentColors` paints nothing (06-064).
- **Skipped.** 04-054 (the slot was already matched by request, with a test;
  only the reset on open was changed), 05-069 (`take_chat_upgrades` drains the
  pairs every poll), 05-070 (`refuse_login_url_overlap` allows one login-url
  request at a time and a swept request has no pending to answer), 05-071
  (`background_answers` is keyed by request), 05-072 (a first page is not asked
  while a page is in flight), 05-075 (outcomes of a left screen are forgotten
  on return by design, like the other drained outcomes), 05-113 and 05-127
  (answers already matched by request).

### Sweep fixes (s4-misc-2)

- **Owner resets.** The side column's slide (`PanelReveal`) is replaced in
  `replace_screen`, so it never slides out an old account's panel (11-p7-007).
  The business-hours row's view is `BusinessHoursUi { view: Option<(user,
  HoursView)> }`: another profile starts folded in the viewer's zone
  (12-036). The send button's morph, the topic strip's reveal key and the
  self-destruct tick task are `ChatUi` fields, replaced with the chat: a new
  chat's Send button starts settled, its strip is scrolled to its selected
  tab, and a return to a chat holds one tick task, not two (04-088, 13-101,
  12-079). The send menu (and its card) closes in `replace_chat_ui`
  (12-104). Closing a chat window drops its flying sticker (`send_fly::
  window_closed`) and its toast entry (12-106, 13-117). The effect strip
  rests only while a card is shown, not after it was hidden (12-105). The Privacy page's
  password-check field is emptied by a derived upkeep step whenever Settings
  is not on the Privacy page (13-170).
- **Colors box.** Color-request failures have their own
  `UsersState::peer_color_error`; `PeerColorDialog::stage` is `ApplyStage`
  (`Editing`, `Sent`, `Failed(text)`) and replaces `sent`. Editing leaves
  `Sent` and `Failed`; with the box closed the failure is a note (11-p8-002).
- **Answers and effects only when they happened.** The reaction fly-up
  starts only when the request went out (12-063). A failed source listing
  forgets the call like an empty list (12-069). Report and Save as HTML end
  a selection with `end_selection`, which says nothing; only
  `clear_forward` says "Forward cancelled" (12-087). The dissolve starts
  only after `delete_selected` / `delete_confirmed` were sent (12-089), a
  waiting dissolve asks for no frames, and an emptied chat keeps its rows
  while the last message crumbles (14-012). Dust starts only from a row
  painted in its window's latest frame (14-013).
- **Dialogs and shortcuts.** `MainWindow::dialogs_closed` is a set of
  windows whose kit dialog an action already closed
  (`close_kit_dialog_if_done`); the shell then forgets it instead of closing
  the dialog now on top (13-053, hotspot: shell.rs). The story viewer blocks
  shortcuts of the main window only (13-055). The statistics chart cache
  compares the whole text, so a buffer reused at the same address parses again
  (13-024).
- **Skipped.** 11-p5-007: `close_add_contact_dialog` calls
  `forget_new_contact` (test `a_closed_boxs_import_opens_nothing_and_the_next_box_sends`).
  12-091: the first mouse move without the button ends the drag
  (`selection_drag_lost`), a window that lost focus ends it in the poll, and
  GPUI has no read of held buttons for the poll. 13-074: `ForwardStage` is
  one enum (round 18). 13-033: a pick removes the picker's flight in
  `begin_sponsored_report`, so a second click finds no picker (test
  `a_picked_report_option_closes_the_picker_until_the_answer`).

### Sweep fixes (s4-stories)

Twenty-two S4 entries; each was proved against the code first.

- **Answers matched to their request.** `setCloseFriends` carries its list
  on the request (`PendingRequest::user_ids`), so the `ok` of the first of two
  quick saves applies its own list and a failure of one never clears the other's
  (03-010; the `close_friends_pending` field is gone). The viewers panel and
  the public story search keep their page in one `PageFetch` (`Idle`,
  `Loading(request)`, `Failed(text)`); the story forwards list keeps it in
  `ForwardsPage` (`NotAsked`, `Loading { request, offset }`, `Ready`,
  `Failed`), which also knows whether the first page was ever asked and makes
  a first page replace the rows (03-007). Either way an answer or an error
  lands only when it is the request the page waits for, so a stale one from
  an earlier visit to the same story fills or fails nothing (05-279, 06-009),
  and a page whose request the registry no longer holds does not hold the
  next one back. `StoryViewersState`, `StoryInsightsState` and
  `StorySearchState` drop their `loading`/`error` pairs (baseline lines
  removed). A reopened insights panel that replaced another story's starts
  at `NotAsked` and asks its first forwards page again. A search page the
  sender refuses leaves the pages already found and a retryable error
  instead of clearing the panel (03-008).
- **Viewers panel (13-145).** The spinner shows only for `Loading`; a failed
  page shows its error with Retry (also for a failed first page), and a send the
  sender refuses is a `Failed` page, so scrolling does not re-ask every frame.
- **Post-as list (05-162).** `StoriesState::post_as` is its own
  `Fetch<Vec<i64>>`: its failure no longer lands in the eligibility check's
  answers (which made the composer stop waiting for the real `canPostStory`
  answer), and it replaces the `StoryPostState::post_as_error` field the s3
  round added for the same bug. The picker draws the error (the composer's
  status line no longer repeats it); reopening the composer asks again.
- **Live Stars (05-168, 03-079).** `LiveStoryState::committed` is one batch per
  commit request instead of a number. A failed add takes exactly its Stars from
  the batch that holds them (request ids only grow, so the add belongs to the
  batch whose commit followed it, or to the pending one); a failed commit drops
  only its own batch; the answer to a `getLiveStoryTopDonors` asked before a
  commit leaves that batch in the total. A pushed `updateLiveStoryTopDonors`
  still counts every batch: it carries no request id to compare, and TDLib sends
  it because of the commit (a snapshot older than the commit that arrives as an
  update is not told apart; it was the same before).
- **Comments toggle (03-078).** `show_from_pinned` opens the list the way the
  toggle does (marks the comments read), so the dot cannot come back for ones
  already seen.
- **Live call (13-100).** `Session::live_call()` is the story stream only when
  it is the watched story's call; the composer, the send price, the admin check,
  the sender and the Ban User check read it. The delete box bans only after the
  delete went out (`delete_live_story_comment_then_ban`), and says when the
  delete itself could not be sent (13-093).
- **Calendar (05-158).** `HistoryCalendar::paging` is `Idle`, `Loading`,
  `Failed` or `Exhausted`; a failed page stays retryable (moving to another
  month asks again) and only an empty page ends the paging. The baseline line is
  removed.
- **Mention search (05-240).** A failed `searchChatMembers` drops its own search
  (an older query's error is ignored), so the same query is asked again.
- **Custom emoji cache (05-283).** The cap clears the cache once, before the
  batch goes in, not in the middle of it.
- **Story page (06-037, 13-136, 13-143).** "Page loading" is read from the
  open page's chat's own ops (`StoryPageOps::loading`, one op per request
  since the s3 round), not from the label of a single status op that a
  mutation or another chat's page replaces.
  The album picker's grid asks like the archive section (`load_due`, the shared
  `archive_asked` guard, never in a demo session). The statistics and search
  lists reset their `Asked` guard and scroll handle when the story or search
  changes.
- **Per viewer / per chat (13-127, 10-104, 13-099).** The stealth toast lives in
  `StoryViewerUi`, so closing the viewer drops it. The fact-check expansion lives
  in `ChatUi` (per view, collapsed again on a chat change, as tdesktop's
  per-element `Factcheck::expanded`). A voice/video/sticker/GIF reply that
  finishes after the viewer moved on ends the reply row only when it is still
  the row for that story (`reply_row_ends`).
- **Skipped.** 10-102: `sharing_request_answering` already reads the in-flight
  `AnswerSharingRequest` request, so a second click (Accept then Reject) sends
  nothing; the test `accepting_a_request_names_it_once_and_a_refusal_frees_the_buttons`
  asserts it, and the error frees the buttons. 13-122: the passcode lock calls
  `release_story_viewer`, which drops the cover picker, and the strip loop checks
  the picker's serial after every frame, so the cutting stops within one frame.
  Minimising or pausing is not a lock; nothing private is on screen, so the cuts
  are left to finish.

### Sweep fixes (s4-calls)

S4 entries, each proven against the code first.

- **Calls engine and capture.** A late empty stream list no longer revives a
  `Failed` or `Ended` feeder (01-083). The macOS window monitor ends through
  `StreamShared::end()`, which swaps `stopped` once, as the Windows one does
  (01-065). A reaper thread that never started runs a hand-off on a thread of
  its own, never on the caller (01-079). The Windows hook thread of a start
  that timed out no longer installs its context over a newer hook (01-088).
- **Invite links.** `VideoChatShareLinks` holds one `ShareLink`
  (`Idle`/`Fetching`/`Got`) per link; "generating" is derived, so a failed
  listener request no longer re-sends a speaker request still in flight, and
  a listener send that fails frees the speaker request it never sent
  (01-093).
- **Call end card.** The diagnostics and call-log uploads are one
  `CallUpload` each (`Idle`/`Sending(request)`/`Sent`/`Failed`) in place of
  two bools and two errors: a second press while one is out is refused, and
  an answer or error applies only to the summary whose request it is
  (02-016, 05-027). A call that ends untracked no longer replaces another
  call's open rating card (05-226). Submitting a rating puts the draft back
  when the send is refused (08-083). The window that pressed Call is
  forgotten when its card is dismissed and when a call comes in (08-077).
- **Group call requests.** The screen-share pause toggle has its own purpose;
  a refusal puts your tile back to what the server has and raises no box
  error (02-014). An unwanted rejoin or screen offer is undone, so neither
  waits on a request that is gone (02-105, 02-106). A failed `addPending`
  after its commit takes the Stars off the committed total (02-116). Calls
  waiting for their chat are a list (02-112). The "join as" request is
  marked asked only once it was sent (05-031). `failed_creates` keeps the
  latest eight (05-053). The own-mute guard keys the in-flight toggle by
  participant (05-227).
- **Call UI.** A mute or connect cue over a ringing loop leaves the loop set
  and the ring resumes after it (08-086). Calls history keeps its loaded
  rows when a later page fails, with Retry for that page (08-079). The live
  PiP: a failed open restores the stream window and ignores a link that was
  replaced; the source check compares the link's account (08-090, 08-088,
  08-089). The story volume pill is released with its slider and with the
  viewer (08-057). The stream key reveal is part of the Settings overlay
  (`StreamOverlay::Settings { key_shown }`), so every way out hides it
  (10-029). Share Invite Link refuses under the lock before asking (10-037);
  Share to leaves a waiting deep-link open alone (10-039). The in-call chat
  and member levels reset with the call (10-040) and so does the composer
  text (10-052). A stream wish ends when the call fetch fails (10-050,
  10-033). A failed rename keeps its box (10-051).
- **Skipped.** 01-072 (PipeWire takes the fd even when the connect fails; a
  second close would hit an unrelated fd), 01-087 (a layout switch changes
  the XKB group, not the keycodes; only a remap does), 01-096 (the map is
  reset per call, bounded by its participants), 02-102 (a failing engine
  call is retried by design and its error is written once), 02-110 and
  02-117 (tdesktop closes the box and removes the comments at once, with no
  rollback), 02-115 (same: Undo is local), 08-056 (no dialing ticket is left;
  `PermissionAsk` owns the wait), 10-035 (the no-photo branch already drops
  the cached blur and its task).

### Sweep fixes (s4-links-tdlib-2)

Minor links, mini app, location and TDLib entries; each was proved first.

- **Answers matched to what asked.** The groups-in-common list is
  `ProfileChatsFetch::Loading(RequestId)` and its follow-up page is
  `CommonGroupsPaging::next_page: PhotoPage::Asking(request)`: a late page
  of a fetch that was dropped lands in neither (05-185). A story album
  answer merges into a list that was fetched, never starts one (05-198).
  A mini app's attachment-menu toggle goes on to its launch only for its
  own bot, and the three waiting launches carry the window that asked
  (`Awaited`, 14-042, 14-043). The latest launch asked is the current one
  (`mark_web_app_pending` drops older ones, whose answers are closed,
  14-041). Mini app render actions are a queue (14-040).
- **Rollbacks.** Every optimistic rollback that has a shown value restores
  only while the value still equals it (`restore_optimistic`), as the topic
  mute already did (05-201).
- **Stages with an exit.** The AI style preview has `Adding` and `Removing`
  carrying the request: the button waits, the answer or an error (or a send
  failure) leaves the stage, and `Added`/`Removed`/`Deleted` name the
  request so another preview's answer closes nothing (07-030, 07-031).
  The proxy editor is `ProxySave::Saving(request)` and closes on the
  answer through the `SaveWatch` (12-057). The mini app emoji-status grant
  is `Granting(request)` and ends when its request leaves flight (14-035).
  The main window's close is `MainWindowState`, and a raise makes it
  `Open` (10-162).
- **Asked once, but freed on failure.** A video userpic's full-info ask is
  released when the send fails (05-157; an error answer keeps it, since the
  ask comes from drawing). The AI compose emoji lookups use the session's
  own per-id stage (`custom_lookups.attempted`), so a failed lookup is
  asked again up to `MAX_FAILURES` (07-032); `emoji_asked` is gone.
- **Parsing.** `updateChatReplyMarkup` reads only the id and the markup of
  its message (06-047); `updateChatLastMessage` says `Unreadable` instead of
  clearing the preview (06-048); an unreadable `messageEffect` fails its
  request (06-051). The first frame's launch wait belongs to the client that
  asked for the page (06-063).
- **Small UI.** The link tooltip is in `ChatUi` with a stage that owns its
  timer (09-X046, 09-090); a right-click link note is dropped when its event
  ends (09-092); the map drag ends at the next move without the button
  (10-168); a map tile cache past its cap forgets one place per new one
  (05-188); an invite Join names its window before sending (09-X010).
  The IP-exposure warning is acknowledged per account (documented, 12-058).
- **Skipped.** 05-186: `refresh_chat_statistics` keeps the `Loading`
  entry while a request is out (`send_chat_statistics`), so no stale answer
  can land. 09-014: a lookup that cannot be sent already becomes
  `ShowText("Couldn't reach Telegram.")`; the remaining `InvalidRequest`
  returns are a shutting-down session and a share draft. 10-167: already
  fixed by 5ddbff65 (`Locating::drop` stops the manager). 09-X010's second
  half (drawing the invite box from `ChatsState::deep_link`) is left: the
  copy is only read by the box and cleared with the flow.

### Sweep fixes (s4-messages-3)

Thirty-four minor entries; each was proved against the integration branch first.

- **Stages with an answer.** The Replies "Block" box stays as `Submitting`
  (`RepliesBlockStage`, carrying the request id) until the answer: ok closes
  it and queues the "blocked" note, an error gives the choices back
  (11-p4-011). "Load full message" is one `Fetch<()>` per `ChatMessage` in
  the session: loading from the send, removed by the answer, `Failed` by the
  error, and the button waits while it loads (11-p4-012). A quick reply's
  text is held as `TextSave::Sending` with its request; an answered refusal
  puts the text and its edit back in the field (12-061). `mask_set_change_pending`
  is keyed by set, so one Add disables its own row only (13-013).
- **Answers matched to what asked.** A poll vote carries the option id and
  the driver looks the index up at send time (11-p4-010). A file picked for a
  poll option finds its slot by `OptionAttach::id`, not by row index (12-004).
  A sticker-set editor and a selection translation say goodbye through
  `Farewell`, so the encode stops and the job leaves the old session
  (13-008, 13-164). The report's details field is emptied where the flow
  ends, `close_message_report` (11-p3-008).
- **One place for a close.** Every message-menu close goes through
  `close_message_menu` (it also resets the reaction strip's request; the
  reaction selector alone keeps it, as it shows those options);
  `scripts/single_writers.py` rejects a bare `message_ui.menu = None`
  (11-p4-006).
- **Lost answers.** A translation job whose request is gone fails, so it no
  longer holds one of the 20 places (13-167); a widget public-data poll is
  swept like the other lookups and answers "poll again" (14-057); a retry of
  a failed thread open keeps the failed pane when it cannot be sent, and says
  so (13-107). Mini-app requests the bridge refuses (popup, custom method,
  clipboard read) get their failure reply with the id that asked; a request
  id over 64 characters is refused, not cut (14-077, 14-078).
- **Widgets.** A press or hover is released when the widget goes off screen,
  stops, fails or restarts; new content replaces the entry's content-bound
  part whole (`replace_content`); a widget starts at the scale of the window
  that shows it (`WidgetsUi::scales`); canvas reads and puts check their size
  (14-046, 14-051, 14-058, 14-056, 14-047).
- **Notifications.** A preview that has to download plays only while a sound
  picker is open: `pending_sound_downloads` is a set the UI empties
  (`forget_sound_previews`) on any poll that finds no picker open, with no
  clock in the reducer (11-p5-023). A sound finishing in a background
  account is not played. "Mark as read"
  waits, queued, while locked (11-p5-025). A toast replaced in the call slot
  is dismissed first (11-p5-022, partly). A Linux toast withdrawn while its
  `Notify` is on the bus is closed when its id lands (`Sending`), and the
  D-Bus connection is made again after the listener ends (11-p5-026,
  11-p5-027).
- **Smaller.** Element ids built from message ids use wrapping arithmetic
  (11-p4-016); the sticker creator's refusals are toasts, not silent notes
  (13-009); Reply in Another Chat keeps the reply when an agent may not open
  the chat (12-073); leaving or changing a thread closes its info card
  (13-106); the manager box puts the composer panel's tab back on close
  (13-012, the shared open flag stays; the tab goes back only if the box
  was open); a closed chat window drops its widget scale; a reaction picked
  from the menu strip is toggled before the menu closes, so a locked emoji
  still finds its Premium options; a demo session clears the translation
  of a chat whose bar is gone (13-166).
- **Skipped.** 12-098: the background poll already drains
  `take_finished_rich_html_exports` per account. 14-054: query tokens are
  process-wide (`next_query_token`). 13-165: tdesktop toasts the hide at
  once (the bar is optimistic here too, with `RequestRollback`), and the
  local hidden flag is already keyed by account.

### Sweep fixes (is-11)

Forty-two implicit-state entries; each stage that two or more fields kept in
step by hand is one enum now, and the baseline lost the lines of every field
converted (221 to 204, the rest belong to other bundles).

| id | change |
| --- | --- |
| 05-256 | `ProxyList` (`Unasked`/`Loading{request}`/`Fresh`/`Stale`/`Failed`) and `ProxySave` (`Idle`/`Saving{request}`/`Failed`) replace `loading`/`stale`/`mutating`/`error`/`fetch_attempted`; the IPv6 error is its own line; a system step keeps the user's refused change inside `Saving::kept_error`. |
| 05-274 | `KeyboardFetch` per chat (`Wanted`/`Loading{request}`/`Failed`/`Known`) replaces `by_chat` + `markup_message_ids`; a failed `getMessage` ends the fetch (test added). |
| 05-272 | `ShareSearch::{Idle, Searching{query, local, server}}`, each half `Waiting(request)` or `Done`. |
| 05-250 | Already `SponsoredReportFlow` on the integration branch (is-09); taken as is. |
| 05-268, 06-023 | The greeting stickers and `installed_loaded` were already `Fetch` on the integration branch; `SuggestFor::{Nothing, WaitingForSets, Asked}` replaces `suggest_for` + `suggest_waiting_for_sets`, and its baseline line is gone. |
| 05-269, 06-024 | Already `Fetch` + `GifList` on the integration branch (is-13); taken as is. |
| 05-284, 06-036 | Skipped: `StoryPageOps` already tracks one `StoryPageOpState` per request (the request is the entry's key, loads and mutations never share a slot). |
| 05-287 | `ForumTopicsMap` holds a list or a failure per forum. |
| 06-030 | `Translation::Pending{job}` is the only record of a job; a late answer of a replaced job no longer overwrites the newer one. |
| 06-014, 13-084 | `SharedMediaTabStatus::Ready{more: NextPage}` and `Failed(error)`; a failed next page shows a retry row (tdesktop shows "Load more" the same way); the `Asked` mark is gone. |
| 06-015 | `SharedMediaState::open()` derives from `chat_id`. |
| 13-051 | `GallerySearch::{Closed, Open(SearchField)}`; the debounce is a `Task` dropped with the field. |
| 06-017 | `ForwardsPage::{Idle, Loading(request), Failed}`. |
| 06-026, 06-027, 06-028 | `Stage` on a sticker clip, `PlayHead::{Waiting, Playing}`, `StripPull::{Collapsed, Revealing, Expanded}`. `Clip::delivered` stays: it marks the first frame handed out, not a stage. |
| 06-022 | `ComposerStage::Failed` holds the error line (`local_error` is gone); `ComposerMode` is the integration branch's `Edit(chat, story)` / `Repost(chat, story)`. |
| 06-038, 06-039 | `StoryPlayback::{Stopped, Running, Paused}`; `StoryViewerKind` carries the clip, the duration and the live call. |
| 06-040 | A suggested post is busy while its approve or decline is a pending request. |
| 06-035 | `AccentRead`; a lookup no longer starts a read, `refresh()` does. |
| 06-046 | `BridgeFeed::{Injected, Routed, Closed}`. |
| 06-042, 06-043 | `CallState::Ready(ReadyParams)`; the participant's video flags derive from the video infos. |
| 06-045 | `ChatDraft{body: DraftBody, reply: Option<DraftReply>}`. |
| 06-044, 06-059 | `MemberStatus` carries the administrator's rights, `can_be_edited`, the restriction and the owner's anonymity; the group payloads carry it instead of nine options. |
| 06-054 | `MessageReplyTo::{Message, Story}`. |
| 06-053 | `MessageTopic::{Forum, Thread, DirectMessages}`. |
| 06-052 | `MessageSendState::Failed{can_retry}`. |
| 06-058 | `SelfDestructClock::{NotOpened, Counting}`. |
| 06-056 | `ChecklistTask::completion: Option<TaskCompletion>`. |
| 06-055 | `LocalFileState::{Absent, Partial, Downloading, Complete, Uploading}`. |
| 06-057, 06-067 | `StoryReaction::{Emoji, Extra}`. |
| 06-060 | `PremiumSticker{animation}`. |
| 06-066 | `GiftOfferState`. |
| 06-068 | `CryptoStage::{Unbound, Bound}`; the request log is hidden exactly while bound. |

Review fixes: a refused save or remove of a GIF marks the saved list, never
the search grid (tested); a refused refetch of a stale proxy list is
`ProxyList::FailedStale`, which keeps `settled()` false so the rotation and
system-proxy planners wait (a failed answer settles as before; tested); a
newer reply-markup message replaces one not yet fetched (tested). The
bridge's `stopped` flag is documented as the feed's report from another
thread. The shared-media "load more" line comes from the lang table.
`CryptoStage` binds in one step. `LocalFileState::Uploading` wraps the state
the file was in, so a file being sent that is not complete keeps its path
and size (tested). Call parameters live in `CallState::Ready`: a hang-up in
flight starts no media, and a failed `discardCall` puts the state back, so
the next pass connects; `ready_at` is set from the update itself, as before.

Behaviour changes worth a look: a newer `updateChatReplyMarkup` replaces a
keyboard fetch still out (its answer is then dropped); a
refused system proxy step no longer wipes the user's error; the accent is read
once per `refresh()`; a hang-up in flight no longer shows the call's emoji
fingerprint, because the key lives in the `Ready` state the hang-up replaces.

### Sweep fixes (is-14)

Implicit-state conversions: each stage is one value, with a way out.

- **Folder boxes.** The share box's request and its failure line are one
  value (`LinkOp::{Idle, Saving, Failed}`), and the link awaiting delete
  confirmation is part of the list view (`FolderShareView::List {
  confirm_delete, copied }`, so the copy check mark leaves with the list
  (09-038); opening the form drops only a failure, never a request still in
  flight (09-115). 09-114: already `FolderLoad`
  (Loading, Failed, Ready, Merging, Saving); the form line and the picker are
  messages and a sub-page of `Ready`, not stages. 09-116, 09-037, 09-X018:
  already `InviteStage`, the `seeded`/`adding` flags and the session-side
  invite fields are gone.
- **Video chat.** The window is `CallWindow::{Closed, Opening, Open {
  handle, visible }}` (10-031, 10-048). The stream you asked to watch is
  `StreamWatch::{Idle, Wanted { .., joining }, Joining}` (10-032, 10-049): a
  new wish keeps a join already on its way (its timeout task, `joining_call`
  and its failure line), tested. The invite and share boxes are
  `CallBox::{None, Invite { call }, Share { call, .. }}`, opening one
  replaces the other and `keep_for` closes one opened for another call
  (10-038). `boxes_call` stays: it is the owner key of the rename, Settings
  and pin state that live in `GroupCallScreen`, not a stage of the box. The global push-to-talk hook keeps its listener and its
  permission re-check as tasks in `GlobalPtt`: dropping the task is the way
  out, so no stop flag or polling bool (10-036, 10-067); `ptt_capture` is
  one independent Settings mode. The stage volume slider is
  `SliderStage::{Made, Dragging}` (10-041). `createVideoChat` waits as
  `VideoChatCreate::{Idle, Sent}` and now also leaves on its own success,
  through `answered_creates` beside `failed_creates` (09-X024).
- **Storage.** The clear is `StorageClear::{Idle, Clearing, Freed, Failed}`:
  its failure line is in the stage, so a new clear or a reopened screen
  replaces it without resetting `data_storage_error` by hand (09-X028);
  `data_storage_error` is left for the page's other requests. The scroll to the
  usage rows is `ScrollStage` (StorageScroll::pending).
- **Boxes.** The checkout's payment method is one value,
  `PaymentMethod::{NoCard, Saved { id, card }, NewCard, NeedsPassword}`, and
  its page `PaymentPage::{AwaitingForm, Checkout, Country, CardForm}`
  (09-049, 11-p7-010); the form landing prefills the inputs and moves
  `AwaitingForm` to `Checkout`, so no `Prefill` flag exists. The poll box is `PollPage::{Opening, Form, Country, Deadline,
  ConfirmDiscard}`, and the first-focus request is the `Opening` stage
  (09-048, 09-X029, 12-009). An option's attach area is `AttachMode::{Idle,
  Menu, Link, AwaitSticker, AwaitEmoji}` (09-047, 12-008). The two earn
  history boxes are `TxBox` (09-062); the referral link's icon plays from the
  first draw of its link: `LinkBox::icon` (`LinkIcon::{Unseen, Playing}`)
  lives inside the box the session holds, so a box that goes away takes it
  along and the same link reopened plays again (09-061); a join box
  parked for the recipients lives in `Recipients::Loading`, so a failure or
  closing the profile drops it with the stage (09-X033). The layer fade is
  `LayerFade::{Open, Closing}` with the close count in the variant (09-072).
- **App-wide.** `Standing` joins the disk reading, its confirmation and the
  held-for-call mark (09-058); the draft timer's `running` was always
  `due.is_some()` (09-057); the file drag is `FileDrag` (09-060); the frame
  clock is `ClockLoop::{Stopped, Running(task)}`, so a stopped clock cannot
  read as running (09-X068, 09-118); layer focus is `LayerFocus::{Closed,
  Open}` (09-103); the lifted-drag marker is the presence of its global
  (09-X060); the emoji word bar is `WordBar::{Closed, Asking, Hover, Pinned}`,
  where `Asking` is "Suggest Emoji" waiting for its keyword answer and the
  open bars own the pointer-away wait, so closing the bar drops it (09-083);
  a hover opened while `Asking` takes the bar's place and ends the ask; a first-run clip is `Clip::{Writing, Ready, Absent}` (09-099);
  an emoji effect is `EffectKind` plus `Playback` (09-086); the emoji set
  box's startup read and file install are `Startup` and `FileInstall`
  (03-045, 09-089), with a dismissed file picker now leaving its stage, and a
  cancelled download is `DownloadError::Cancelled` instead of a string
  compare. The composer's draft is `DraftLoad::{Unshown, Shown, Restoring}`:
  the next-frame restore request carries what was shown (09-X061). The frame
  clock sits in a `Cell`, read by taking and putting back.
- **GIF panel.** The saved list is a `Fetch<()>` and the panel holds
  `GifList::{Saved, Search(GifSearch { query, fetch, request })}`: a new
  search no longer starts with the last one's failure, and a page or error
  is matched to the request the search waits for, so a late page of an
  earlier query cannot mark the new search ready (09-119, tested). A failed
  save or remove while the list loads keeps the fetch stage. Bid amount
  (09-120): `chosen` is the one stored value; the track is only a coarser
  view of it, written by `choose` alone and never read back.
- **Already fixed upstream, skipped.** 09-091 (`TooltipStage`), 11-p4-015
  (`StateSaveTimer`), 09-125 (`AuctionOpen`).
- **Left in the baseline.** `ActiveGroupCall`, `SettingsState` and
  `StickerPanel` fields in files this bundle touched are stage flags of other
  requests that span many files; they stay for their own bundles.

### Sweep fixes (is-13)

Implicit-state entries: each names a struct whose stages were kept in step by
hand. The shape is gone (one enum or `Fetch<T>` per stage), not commented.

| id | result |
|---|---|
| 08-099 | `PinStage { Idle, Pressed{chat,y}, Dragging(PinDrag) }`: the press, the drag and its list are one value on `ChatListUi::pin`. A mouse-down writes `Pressed` only from `Idle` or `Pressed`, so a press during a drag or its release slide does not cut it. |
| 08-100 | `PeekPreview { Idle, Pressing{chat,started,over}, Open(state) }`: opening replaces the press, a release ends a press and leaves an open panel; a new press keeps an open panel showing under it (`over`) until its own opens, as before. Esc closes only an open preview, not a press. |
| 08-101 | The checked set is the mode (non-empty = selecting), one value. Every frame's upkeep now prunes a chat that left the list (archived, left), not only when the last one does; the view change still clears it. Not keyed by list: a view change ends the selection. |
| 07-044 | Already fixed: `SidebarTab` (the bools are gone). |
| 08-093 | `WallpaperChoice { Keep, Installed, File, Remove }` in `ChatLookDialog`; picking one drops the others. |
| 08-107 | `InlineMode { Closed, Open{selected, wait: Idle \| Session \| Armed{.., timer}} }` (`inline_stage.rs`). The debounce timer is owned by the stage: closing or a newer query drops it, so the token counter is gone. |
| 08-115 | `HistoryMeasure { Seeded, Measured{width,layout,pane} }` in one shared cell; the paint probe replaces the seed whole. |
| 10-161 | Already fixed: `MainWindowState`. |
| 13-052 | Partly: the mailbox (`MainWindow::dialogs_closed`) is dropped when its window is forgotten, so a mark no shell will read does not stay. Deriving it from the kit layer (`has_active_dialog`) was tried and fails `an_action_that_closed_a_box_leaves_the_alert_opened_after_it`: the kit says whether a dialog is up, not which, so an alert opened right after the box closed looks like the box. `QuillShell::open_dialog`, the host map and the mailbox stay; a single per-window host value needs the kit to name its dialogs. |
| 08-112 | Already fixed: `SaveTimer`. |
| 08-147 | `ChatWindowView::closed` is gone: `closing` runs only while the app lists the view (`ChatWindows::lists_view`); the bookkeeping unlists it, so an account switch's earlier close, or a window of the same chat opened since, is not closed twice. |
| 08-148 | `clear_calls: Option<ClearCallsBox{revoke}>`; closing drops the checkbox. |
| 08-125, 08-159 | `SendOnce` is an argument of the send path (`submit_composer`, `submit_from_send_button`, `submit_rich_composer`, `composer_send_options(once)`, the paid and bot-thread waits keep their copy); `ComposerUi::once` and `with_send_once` are gone, so nothing is set and restored. |
| 08-120 | Skipped: `ban_wait` is a request pending beside any box (its warning opens only if the box that was open is still on screen), and `new_chat_for` is the target of a chat made in another dialog; neither is a stage of `open`. |
| 08-126 | `UndoId` replaces the `Instant` used as a token; `started` stays only as the countdown's clock. All three triggers already go through `perform_request_action`. |
| 08-130, 08-157 | Already fixed: `EditMode::replace_as_file`. |
| 08-139 | `FieldEcho { prev, revert }`: setting the field text drops a replacement made for other text. |
| 08-155 | Code Language box: `CodeLanguageStage { Opening, Editing{error} }` holds the first-frame focus and the language error in one value. The link box has no other stage (one field); its `BoxStage { Opening, Open }` is the kit's first-frame hook and stays, the focus must wait for the dialog to mount (a deferred focus from the open call lands before the dialog exists). |
| 08-156 | `draft_restore: Option<(ChatId, DraftTopic)>`: the slot that asked; a restore for a slot no longer open is dropped. |
| 08-158 | `DragOver { None, Probing(paths), Ready(paths, state), Fixed(state) }`; the demo's fixed zones are a variant. |
| 08-153 | `slow_mode_tick: Option<(ChatId, Task)>`: replacing it drops the old tick. |
| 09-007 | `TextLimitCount { Idle, Known, Counting{key, task, last} }` in a `Cell` (taken, read and put back, never borrowed in render). |
| 09-008 | `ConnectStage { Connecting, NeedCredentials, Blocked(failure), Shown(auth) }` on `AuthDemo`; `ConnectUiStatus`, `ConnectionUi::status` and `demo_state` are gone. The "cycle auth" debug click moves a synthetic pane to `Shown`. |
| 09-009 | The current note and its wake-up deadline live in `ToastQueue` with the toasts. |
| 09-003 | `UsersState::contacts: Fetch<Vec<i64>>`: the ask goes out as `Loading`, a failed send or answer is `Failed` with Retry, a contact change drops it to `Idle`. |
| 09-004 | `MenuFocusStage { Closed, Open{previous} }`. |
| 09-X005 | `schedule_picker` is the popup: `Some` is open; hiding it drops the picker (the share box's own step stays). |
| 09-X006, 09-006 | `WindowAnchor { Unsynced, Anchoring, Anchored }` with key, epoch and the landed jump. `HistoryUi::aim: ScrollAim { Free, Goal(ScrollGoal), Landing(Landing) }` replaces `scroll_goal` and `landing`: a newer aim replaces the older, reaching a goal frees the aim, and the reader taking over ends only a landing (a goal waiting for older pages stays). |
| 09-020 | `ChromeDue { Idle, Due(rows), Notifying(rows) }` in one `RefCell`. |
| 09-018 | `CustomEmojiCard { Hidden, Asked{emoji,tap}, Shown{preview,tap} }` per window; the session's preview is only the answer. |
| 09-011, 09-X009, 10-163 | `LinkFlow { Idle, Running{origin: FlowOrigin{window, by_agent}}, Asking{invite} }` and `ResolvedLink { window, step }`: the agent origin lives in the running flow, taken with its state and put back only while it goes on. `ChatsState::deep_link_from_agent` and `request_deep_link_info_from` are gone. |
| 09-022 | `DemoUi::next_seq(last)` numbers the stream's updates through the demo's one counter, never behind the session's. |
| 09-032 | `GifPanel::saved: SavedGifs { Unloaded, Loading{items,stale}, Ready{stale}, Failed }` replaces the list, `loaded`, `loading`, `failed` and `stale`; an `updateSavedAnimations` during a request keeps it `Loading` (spinner) with a stale mark, so `needs_load` sends a new request at once. The search flags are a second flow and stay on the baseline. |
| 09-X016, 09-027 | `DeviceLink { Idle{notice}, Scanning, Confirm, Linking }`; the answer is applied only in `Linking`, and `is_busy` (any stage but `Idle`) guards both entry points and hides the page's buttons, so a pasted link cannot replace a pending answer. |
| 09-039, 11-p5-006 | `NewContactStage { Form, Sending{request,name}, NotJoined{name} }`. |

### Sweep fixes (is-15)

Forty-two implicit-state entries. Each field list became one value, with the
data of its stage inside and a way out of every stage; the baseline lines of
the converted fields are gone (`ResaleSearch` x3, `HistoryUi::anchor_pending`,
`HdrViewer::in_flight`, `ChatUi::inline_query_waiting`,
`InlineMembersUi::admins_asked`, `MembersView::loading`, `PaymentsState`
`form_loading` x2 and `verification_started`).

| Entry | Result |
|---|---|
| 10-019 | `GiftMarketUi::bid` is gone: the slider (and its custom field) sits in `MarketBox::AuctionBid { slider }` and goes with the box. A rebuild of the slider after another bid moved its bounds carries the open custom field over. |
| 10-017 | Already `AuctionOpen { Idle, Waiting }` (and the session's auction fetch). Skipped. |
| 10-018 | `CraftStage { Idle, Chosen, Forging { primary, since } }`. |
| 10-025 | `ResaleSearch::stage: SearchStage { Idle, Loading { request, loaded }, Ready, Failed }`; an answer applies only to the search waiting for that request. |
| 10-020 | `GiveawayPage::Confirming` replaces `confirming`. |
| 10-045 | `Sweep { Idle, Marked }`, `IdleTrigger { Foreground, Left }` and `GlyphCounts` for kept/seen. |
| 10-030 | `key_shown` is the whole data of the `Settings` variant: no second field moves with it, and the box that reveals it is a variant of the same enum, so every way out hides the key again. Skipped. |
| 10-034 | The hide timer already is `Option<Task>` alone. The photo is `StreamPhoto { None, Blurring { path, task }, Blurred }`. `title_pressed` is one `Option<Point>` (a press position), not a stage. |
| 10-059 | The group Edit box's pack buttons read the `Fetch` of the two installed lists, begun at the send, not request purposes. |
| 10-057, 10-058 | `SaveStage::TitleInvalid`, `GroupEditBox::sub: EditSub { None, Type, History }`, and the title emoji panel is `PanelMode::GroupTitle`, so the panel's mode is its owner. |
| 10-066 | Already `BoostsStage::Loading(RequestId)`; the tab key is carried with the list. Skipped. |
| 10-081 | The thumb's keyed state is `Thumb { Dragging, Synced(value) }`. |
| 10-082 | `LinkCheck { Idle, Typed { version }, Refused { version } }`; choosing Public or Private ends a refusal. |
| 10-072, 10-111 | `ForumColumnUi { Hidden, Column, Peek }` in `HistoryUi`, laid out in `forum_column_layout`; `HistoryWidthSource { Seeded, Measured }`. `window_active` is an observation of the window, not a stage; skipped. |
| 10-071 | `InfoCard { Closed, Topic, Thread, Both }` in `ChatUi`: a thread's card opens over a topic's and the topic's is back when the thread closes, as with the two flags. |
| 10-077 | An edit of a welcome message that left the list is dropped (`settle_welcome_dialog`). |
| 10-105 | The probe's background job records its own answer (and a panic counts as unreadable), so `Probing` always ends. |
| 10-096 | `Decode { Idle, Decoding { key, showing }, Sdr, StandardRange, Hdr }`, `Edr { Off, On, Lost }` and `HeadroomPoll::Running(Task)`; an answer is matched by its key. A bigger re-decode keeps drawing and counting the frame it replaces (`Decoding { showing }`), so the window stays in extended range; `Edr::LostOff` keeps the loss for good off Windows after the layer is switched off. |
| 10-101 | `HistoryFocus { Away, Focused, ArmedForFirst(chat) }`; focus leaving drops the wait. |
| 10-106 | `PillStage { Hidden, Showing { since, last_scroll, hide_after, wake } }`. |
| 10-108 | `Anchor { Idle, Pending { key, epoch }, Anchored }` replaces `key`, `window_epoch` and `anchor_pending`. `ends`, `landing`, `last_highlight` and `highlight_fade` stay: a landing is its own struct, the others are a de-duplication key and a clock. |
| 10-109 | `ChromeRefresh { Clean, Due, Notifying }`. |
| 10-110 | `live_location_tick: Option<(ChatId, Task)>` (`self_destruct_tick_chat` was already the task). |
| 10-134 | `IndexSliders` moved from `PreferencesUi` into `OnScreen`. |
| 10-122, 10-114 | `InlineResults { Closed, Open { selected } }`, `InlineQuery { Idle, Waiting, Armed { username, query, token } }` and `BotAlert { None, Typed, Switch }` in `ChatUi`; closing sets both. |
| 10-125 | `AdminsFallback { NotAsked, Asked(chat) }` (paging was already `Paging`). |
| 10-126 | `MembersStage { Loading, Ready, Failed }` in the view. |
| 12-065 | `lock`, the discard row, "Play once" and the preview are fields of the `Capture`, as `RecordMode`, `DiscardRow` and `Delivery`; they end with it. |
| 10-132 | The recording tick is `RecordingUi::tick: Option<Task>`, cleared by the task. |
| 10-130 | `GifPlayback { Idle, Waiting, Playing { playing, clip, started } }`. |
| 10-127, 14-011 | `Presence { InView, OffScreen { since }, Asleep, Stuck }` and `Audio { Muted, Sounding { paused }, Seeking }` for inline players; `SlotStage` for userpic players. `sweep_due` and `history_listed` are a frame's own marks; skipped. |
| 10-135, 10-138 | Already `DemandStage` and `InlineStage`. Skipped. |
| 10-148 | `CustomBox::Limit { input, focus: LimitFocus }`: `Requested` moves to `Done` when the dialog is built, not in drawing code. |
| 10-151 | `Checkout { Idle, LoadingForm { request }, Open, Verifying, Paid }` replaces `form`, `form_loading`, `form_request`, `form_paid` and `verification_started`. |
| 10-154 | `KeyCapture { Idle, Armed { id, kept }, Rejected(Refusal) }`: arming a row keeps another row's refusal (`kept`). `keybindings_applied` is the process keymap's validity mark and stays. |
| 10-158 | Taken from main (is-04): `LanguageStage` and `LanguageEvent` replace the four fields. |
| 10-174 | `LayerStage { Idle, Ticked, Redrawn(at) }`. |

Review fixes (is-15): the installed sticker and emoji lists go out through
`sticker_request` / `emoji_set_request` at every sender (media panel, message
menu, sticker creator, composer suggestions), so one `Fetch` per list tracks
every send and the group sticker box shows "Loading" instead of asking twice;
`emoji.installed` is gone (`emoji.lists.installed` is the one). The craft info
box never replaces a running forge (`CraftStage::choose`). Choosing Public or
Private in the Type box leaves a refused link line as it was.

### Sweep fixes (is-16)

Implicit state: each of these was a set of bools, `Option`s used as flags or a
side list kept beside a stage. Each is now one value whose variants carry their
data, with a way out of every stage. A task that waits is held inside the
stage, so dropping the stage cancels it.

| Id | Fix |
| --- | --- |
| 10-164 | `HistoryUi::live_location_tick` is `Option<(ChatId, Task)>`: a screen change or another chat drops and stops the task. |
| 10-169 | `LocationPickerUi`: `Search` (closed / open with input and `Wait` debounce), `MapPhase` (idle / settling / resolving), `Locate` and `Geocode` (idle / task, cleared by their answer). The search field closes only through `location_search_changed(None)`, so it is open exactly while `picker.search_open()` (test). The pointer `drag` stays an `Option<Drag>`: it is the hand, across several settle restarts. |
| 10-173 | `MenuStage` (closed / open with the highlight) replaces `open` + `highlight`. |
| 10-170, 11-p8-003 | `PaymentUi::marketplace` is `MarketplaceBox` (closed / kept / open with its draft: chat, privacy, error). The purchase is is-04's `GiftPurchaseStage` (it carries the `RequestId` of its quote and its send and matches each answer to it); is-16's `GiftStage` was dropped in the merge. Log-out forgets the box. |
| 11-p8-004 | `PasswordField` holds the input and one `FieldStage` (away / waiting / typing) fed by `Showing` (off / sending / idle), one value in place of the `showing` + `idle` pair; the field takes the keyboard on entering `Idle` only. |
| 11-p1-005 | `ViewerUi::reorient` is `Reorient` (idle / decoding with target and task): a newer press, a step or a close drops the decode, so no generation counter. |
| 11-p1-004 | `ViewerChrome` (`viewer_chrome.rs`): shown (with the hide task) / hidden (at) beside the idle clock. |
| 11-p1-006, 11-p2-002 | `LoopPause` (free / held by the window); the pause is cleared when the clip stops, so a stale one never resumes the next clip. |
| 11-p2-001 | `SessionTick` (off / running task). The tick no longer stays flagged when its window is gone. |
| 11-p2-003 | `ViewerSource::Profile(PhotoOwner)`: the owner is in the source. |
| 11-p2-009, 11-p3-013, 11-p4-005 | `MessageMenu` (`message_menu_state.rs`): closed / open with selection, link, item, page (audience tab inside the page) and strip. Closing replaces it whole. |
| 11-p3-006, 11-p4-007 | The report box is derived from the session's report flow (`Session::messages::message_report`) as a session-wide box: `open_message_report` names the asking window with `open_box(DialogKind::MessageReport)`, so one window shows it and nothing resets windows by hand. Only the sticker set box is local (`StickerBox`); a report no longer replaces it, and it is still there when the report ends (test). |
| 11-p2-015 | `ChatPeek` (idle / pressing with the timer task / open): a release drops the timer instead of matching `(chat, Instant)`. |
| 11-p3-004 | `game_scores` holds `Fetch<Vec<GameHighScore>>`; a refused request lands in `Failed`, which the panel draws with Retry (test). |
| 11-p3-014 | `PollOptionMenu::properties` is `Fetch`; a refused lookup lands in `Failed` (test). |
| 11-p4-001 | `SelectionDrag` (none / active with the pointer-poll task): ending the drag stops the poll. |
| 11-p4-013 | `Revealed` has separate spoiler and quote sets; the `1 << 63` key trick is gone. |
| 11-p5-003 | `TitleCheck` is the name Confirm judged (`Rejected(text)`, or `Sent(text)` for the create TDLib answers with `NO_CHAT_TITLE`); the error shows while the field still holds that text, so typing ends it with no reset. |
| 11-p6-009, 11-p6-010 | `Hold` (waiting for input / lingering) replaces `waiting_for_input` + `hide_at`; `Popup::hiding()` reads the opacity target; `Clock` (stopped / running task) replaces `ticking`. |
| 11-p6-013 | `ScreenClient` (empty / filled / goodbyes due) replaces `client_filled` + `goodbyes_due`. |
| 11-p6-014, 11-p7-013 | `PriceSource` (default / channel / chosen) replaces `seeded` + `touched`. |
| 11-p7-015 | `Layout` (size only / held with its bytes) replaces `paragraph` + `bytes`. |
| 11-p7-012 | `LockScreen` (entry / offer the system prompt / confirm log out) replaces `suggest_system_unlock` + `logout_confirm`. |
| 11-p7-014 | `DeleteEdit` carries a press number, so a press's deferred cleanup clears only its own snapshot. |
| 11-p7-011 | `form_loading` is derived from `form_request`; `CheckoutStage` (open / paid / verifying) replaces `form_paid` + `verification_started`; `SubscriptionsBox` holds the open box and its cancel confirmation; `star_subscription_edit` is the request id of the edit in flight, cleared by its own answer. |
| 11-p8-014 | `Gesture` (none / stroke / crop / item) replaces `current`, `drag` and `item_drag`. |
| 11-p8-015 | Set-mode `corners` and the keep-ratio choice live in `EditorPurpose::Set { edit: SetEdit }`; `base_item` is the purpose. |
| 12-005 | `seek_scrubbing` was `seek_preview_secs.is_some()`; the bool is gone. |
| 12-006 | `PlaybackTick` (off / running task with what it showed). |
| 12-011 | `PlaylistFlow` (closed / menu with its page / confirm remove). |
| 12-012 | Skipped: `show_stats` is a setting fixed when the dialog opens (voters or stats) and never switches; `selected_option` is independent of it. Restored as the bool. |

- **Skipped (already one value).** 08-103 and 11-p5-011: the mute panel is
  already `MuteBox` + `MuteSection` (s4-settings); what is left beside it is a
  lazily made input entity (`mute_time`), the auto-delete picker and the
  defaults sound picker, which are other boxes. 11-p8-001: the box already has
  `ApplyStage`. 11-p6-011: the full, chat-list and conversation redraws owed
  by `PolledRedraw` are three independent obligations with their own intervals
  (the type says so), not stages of one thing.
- **Left as they are on purpose.** `LocationPickerUi::drag` (the pointer),
  `PasscodeUi::shake_started` (an animation clock).

### Sweep fixes (is-02)

Implicit state in the calls, chat-swipe, composer, account and settings
files, each rebuilt as one value per stage.

- **Calls.** The live-video slot is a `Feed` (empty, unshown, shown) and a
  `Waker` (idle, armed, pending) instead of `unshown`, `wake_pending`,
  `waker` (01-095). The muted-speech hint is one `HintStage` (idle,
  listening with its tester and due time, showing, done) (01-091).
  Push-to-talk is `Idle`, `Held`, `Releasing { mute_at }` (01-090). The
  global key hook is `Off`, `Active { key, handle }`, `Failed { key,
  state }`, `RetryDue` (01-089). The stream playout is `Priming` or
  `Playing` (01-097). "Stream with..." is `Fetching { request }`,
  `FetchFailed`, `Ready { info, revoke }`; a failed first request and a
  failed revoke are taken once by the UI (`RtmpFailure`), and an answer
  counts only for the request it asks (01-092). The box view is
  `Rows(KeyRow)` or `Confirm { which, back }` (09-046, 09-X027).
- **Call driver.** The level tap is `Tap::{Closed, Opening, Open, Failed}`
  with the read time inside `Open` (02-010). The call microphone is
  `Mic::{Off, Active, Failed, Refused}` (02-011). The driver's shared
  screen is `Share::{Idle, Picked, Running, Ended}`, and a group call's
  screen share is `ScreenShare::{Off, Pending, On, Paused}` instead of
  `screen_sharing`, `screen_share_pending`, `screen_share_paused`
  (02-012). A 1:1 call sends `LocalVideo::{Off, Camera, Screen}`: the
  camera and a screen share replace each other in one place (02-036). The
  first device listing is `DeviceListing::{Cold, Listing, Listed}`, so
  `call_devices_warmed` is gone (02-036). The call window is
  `CallWindow::{Closed, Opening, Open}`, the clock `CallTick`, and the
  ended call's linger `CallEnd::{Running, Ended, Lingering}` (08-035,
  08-055).
- **Recent calls.** One `RecentCalls` stage (`Idle`, `Loading { request }`,
  `Failed`, `Clearing { request }`) replaces `recent_calls_request`,
  `recent_calls_error` and `recent_calls_clearing`; `can_clear` takes it,
  and a refused clear ends only for the clear that was asked (02-003,
  05-218, 02-017).
- **Chat swipe.** The gesture's orientation includes `NoDirection`, the
  reach bounce is `Reach::{Below, Reached}`, and the poll task lives in
  `SwipePoll::Running` (02-008, 08-114).
- **Composer.** `GifMode::{Off, Gif, SoundRemoved}` replaces `gif` and
  `sound_removed` (02-094). `PhotoPrep::{AsIs, Jpeg, FileOnly, Rendering}`
  holds the JPEG, the file-only mark and the render in flight, with what
  the picture was kept for a failed render (02-093). A Send held for a
  render is `TraySend::Held`, made only by `hold_send`, which refuses
  without a render (08-160). The draft clock is `Idle` or `Armed` (02-096).
  A repeat period lives in `ComposerScheduling::SendAtDate`, so no repeat
  period sits beside a different schedule (02-095). The inline query of a
  chat window is `InlineQueryStage::{Idle, Waiting, Armed}`; closing the
  panel or a newer keystroke leaves `Armed`, which cancels the timer
  (02-085).
- **Account and settings.** The new-login review hands its outcome to the UI
  as `ReviewStage::Reviewed`, taken once (02-076, 05-018). Terms are
  `TermsState::{None, Offered, Accepting, Failed}` (02-077, 05-021). The
  2FA request is `PasswordFlight::{Idle, InFlight, Failed(line)}`, so an
  older request's late error neither ends a newer one nor keeps a line
  (02-079). Change number is `ChangeNumber::{Idle, Sending, Ready,
  Resending, Checking, Failed}` with the code that was valid kept through a
  refused send (02-078, 05-220). The birthday card is `BirthdayCard::{Shown,
  Hiding { request }, Hidden}`; a refusal brings it back only for the
  request that hid it (02-006). Time zones, connected websites, the default
  auto-delete timer and the account TTL are `Fetch` values. Sessions,
  websites and star subscriptions use `ListOp::{Idle, Mutating(request),
  Stale}`: the change is matched to its request, and staleness is taken
  when the refetch goes out, so a change that lands during the refetch is
  not lost (02-034, 05-219, 05-023 partly). Folder specs are `FolderSpecs`
  with `Fresh`, `Stale` and `Failed` per folder (02-021).
- **Review fixes.**
  - Recent calls (02-003, 05-218, 02-017): a reopen of the tab or a scroll
    is refused while a `Clearing` is out (`fetch_call_history` and the page
    fetch return without touching the stage), so Clear all cannot be
    offered again before its answer.
  - Terms (02-077, 05-021): `TermsState::offer` keeps `Accepting` or
    `Failed` when the pushed terms have the same id.
  - Held send (08-160): `PendingSend::end_render` owns the release. A
    render that lands hands the held Send out once no render is left, a
    render that fails drops it; no caller clears it by hand.
  - Change number (02-078, 05-220): `Sending`, `Resending` and `Checking`
    carry their `RequestId`; a resend under a check is `Checking.resend`.
    Answers and errors act only for the request they answer, so the old
    "a resend failure leaves a check alone" guard is gone.
  - Gifts (05-219): `PremiumHub::gifts_op` is a `ListOp`
    (`Idle`/`Mutating(request)`/`Stale`) and replaces `gift_mutating` and
    `gifts_stale`; an older request's late answer leaves a newer mutation
    locked.
  - Two "code sent to" fields (05-023): one `CodeWait` (`None`,
    `Recovery(to)`, `LoginEmail(to)`) in `TwofaFlow`.
  - Time zones (02-033 nit): the failure line lives in the `Fetch` only;
    the Business screens read it through `BusinessSettingsState::refusal`.
  - A refused microphone with nothing wanted keeps its error for the pass
    it arrives in. A sessions or websites refetch whose send fails gives
    the stale mark back. A finished login review's follow-up is kept
    until the UI takes it: a new press is refused meanwhile.
  - Merge: the group call's screen share and transport are is-04's
    `GroupScreenShare` and `GroupTransport`; the paused mark stays the
    `screen_share_paused` bool beside it (the driver and the error arm
    write it with the share's stage).
- **Skipped.** 02-002: `CollectibleLookup` already is one `LookupStage`.
  05-222: `folder_invite_done` is gone. 02-018: the call privacy asks are
  already per-setting `PrivacyAsk` matched to a request. 02-097:
  `ComposerDoc` is a value built whole and normalized, with no stage.
  02-033: links, bot and the chat-link save are `Fetch` and an enum;
  `business.saving` is the set of saves in flight (independent kinds, not
  one stage) and `business.error` is the last refusal line, taken once by
  the UI. 02-034: `own_info_error` holds only a failure reason; whether the
  info is in or on its way is read from the cache and the request table,
  so no second field is kept in step. 02-021: `folder_remove_queue` is a
  work list of intents (data); each folder's stage is `FolderSpecs`
  (`Fresh`, `Stale`, `Failed`) plus the request table. 05-023: `refetch`
  is the driver's take-once upkeep trigger, read by the pump in the same
  pass; it is not a stage of the code wait. 02-036: `muted` is the user's
  mic switch (a setting), and `call_sound_watch` is the driver's
  diagnostics watch keyed by call; neither is a stage of the other.

### Sweep fixes (is-17)

Implicit-state entries: each stage that was kept in step by hand is one
value now, with a way out of every stage. Where an entry's fields were
already one value, or are independent, it is skipped and says why.

| id | result |
| --- | --- |
| 12-010 | Skipped: already fixed (`ClipSlot` holds the file id with its player or failure). |
| 12-024 | `GramsHub::history: PageStage` (request id and page kind in `Loading`, the error in `Failed`); the Settings probe is `probe: Fetch<()>`; `peers_dirty` is `peers_checked`, how many rows have had their peers checked. |
| 12-025 | `withdraw_url` / `withdraw_too_small` are one `withdraw_outcome: Option<WithdrawOutcome>` the window drains. |
| 12-021, 12-022 | `PremiumHub::tx` and `gifts_page` are `PageStage` (`premium_hub/page_stage.rs`), shared with the Grams history. The cursor (`tx_offset`, `gifts_offset`) stays: it is data of the list. A mutation failure line is `gift_notice`, apart from the load error. |
| 12-023 | `gift_view: GiftView { List, Detail, ConfirmConvert, Mutating }`. The in-flight lock outlives the panel (a second request would pay twice), so `Mutating` carries the panel. |
| 12-027 | `premium_box: Option<PremiumBox { page, start_ref, requested }>` (closing drops all of it); `premium`, `premium_stickers` and `business_features` are `Fetch`, asked again after a failure. |
| 12-028 | `limit_notice: Option<LimitNotice { Box { kind, extra, toast }, Toast }>`. A toast raised over an open box waits in the box (`toast`) and the window still drains it once; a box raised over a toast not yet shown keeps that toast; closing the box leaves a waiting toast. |
| 12-029, 12-046 | `PrivacyState::layer: PrivacyLayer` (page, editor, exceptions with its picker, block picker, Unblock question, last-seen warning). `websites_open` / `websites_confirm` are `WebsitesScreen`. |
| 12-030 | Skipped: already one enum, `ReadDateStage`, with the request ids. |
| 12-031 | `FileOpenConfirm` holds `remember`; session details, rename and the terminate question are `DeviceLayer`. |
| 12-033 | `UsersState::username_check: ProfileUsernameCheck { Idle, Checking, Checked }` (request id and text in `Checking`). `profile_edit_error` stays, and is independent: it is the only cell of its kind (no flag or stage beside it), a message line that every starter clears and the window drains as a note, like `peer_color_error`; the in-flight edits are tracked by their own request purposes. |
| 12-034 | `rating_box: Option<RatingBox { user_id, mode }>`. |
| 12-037 | `ProfileModal::fade: ModalFade { Open(Glide), Closing(Glide) }`: the glide is inside the variants and `close()` is the one transition that runs it to hidden, so there is no stage next to a glide set by hand. |
| 12-041 | Skipped: `PendingGallery` is one `Option` carrying the request's keys, dropped on answer, failure or owner change. |
| 12-043 | `open_profile_dialog` sets the dialog and its window together. |
| 12-047 | Skipped: `CommonGroupsUi::search` is one value carrying its owner id; `common_groups_paging` is gone. |
| 04-050 | `CameraDraft` is `CameraStage { Idle, Starting, Live, Saving, Taken, Stopped }`; `PhotoSources::busy` is `Saving`. |
| 12-059 | The link box's stage is `ProxyState::link_check: LinkCheck { Review, Warn, Checking(RequestId), Done(PingStatus) }` in the session; `ProxyLinkBox` holds only the draft. The link ping has its own purpose (`PingLinkProxy`) and `LINK_PING_ID` and its slot in the ping map are gone: only the request `Checking` carries can settle it, and a new link, a close or a connect puts it back to `Review`, so a late answer of an earlier link is ignored (test). `ip_warning_acked` is a remembered consent, not a stage. |
| 12-060 | `PullUi::route: Route { Unfound, Found(Next), Jumping { next, task } }`; leaving the history drops the jump with the route. |
| 12-062 | `QuickRepliesDialog::pane: ShortcutPane { editing, save, delete: DeleteStep, error }`, replaced whole on every screen change. |
| 12-064 | `MessageUi::menu: Option<OpenMenu { at, page, target: MenuTarget }>`. The window position, the page the menu shows and what it was opened over are one value, so closing the menu (`menu = None`) drops all three; `menu_target` and `menu_ui.page` are gone. |
| 12-066, 11-p4-004 | `RequestShare::stage: ShareStage { Picking, Confirm, Locating(task), Located }`; closing the box drops the lookup. |
| 12-067 | `schedule_popup_open` is gone: the popup shows while `schedule_picker` is set. |
| 12-068, 12-080 | `ScreenShareUi::chooser: Chooser { Closed, Listing, Open, PrivacyBox }`. A window made after the chooser closed is removed. A second ask while the chooser is listing or open retargets it (the old `for_call`). |
| 12-070 | `PasteNotice { Idle { last }, Showing { at, field } }`. |
| 12-071 | `ChatUi::rich: RichComposer { Field, Editor, Discard, Choice(plain), EditorChoice(plain) }`: one value with the transitions `set_editor`, `ask_discard`, `ask_choice`, `dismiss` and `take_choice`; the box and the editor are no longer two cells set by hand. Opening the editor dismisses the discard question (its draft moves into the editor). |
| 12-083, 12-093 | `TwoStepUi::stage: TwofaStage { Closed, Open { view, notice, confirm } }`. Skipped in `TwofaFlow`: the recovery code and the login-email code are two flows with two emails (a test holds both). |
| 12-084, 12-096, 12-111 | `DeviceLayer { None, Details, Renaming, Confirm }` and `DeviceLink { Idle, Scanning, Consent }`. `sessions_mutating` is `sessions_op: ListOp { Idle, Running(RequestId) }` (`state/fetch.rs`): only the answer of the request it carries ends it. `sessions_stale` stays: a refetch mark that a notice can raise while a request is out. |
| 12-085 | Skipped: `top_view` is the rule state, reset with the search; `top_toggled_at` is a render clock and `top_drag` a pointer gesture. |
| 12-090 | The pointer poll is held by the drag (`DragSelect::poll`); dropping the drag stops it. |
| 12-092 | `PasskeysScreen { Closed, List, ConfirmDelete, About }` and `PasskeyRun { Idle, Waiting, Showing }` (the ticker is in it). |
| 12-095 | `connected_websites: Fetch<Vec<ParsedWebsite>>` (fetch and disconnect errors land in it, like `sessions`); `WebsitesScreen`; `websites_mutating` is `websites_op: ListOp`. |
| 12-101 | `TrayReads` holds one `Reads<T>` per kind: a path is `Reading { job }` or `Done`. |
| 12-103 | `SendMenu::list: EffectList { Strip, Expanded }` and `card: Card { None, Shown }`. |
| (review) | A paged list kept failing its follow-up page: `PageStage::invalidate` keeps the error line of a failed page and only makes the first page due again. `gift_notice` blocks the received gifts' auto-paging like `gifts_error` did. |

### Sweep fixes (is-08)

Forty-two implicit-state entries; each stage that two or more fields kept in
step by hand is one value now, with the data inside the variants and a way
out of every stage.

| id | what it is now |
| --- | --- |
| 04-017 | Skipped: `ThemeGifts` is already `GiftsStage` (First / FirstFailed / Listed(GiftsRest)); `requested_at` and `serial` are data of the request. |
| 04-018 | `Backdrops::stage`: `BackdropsStage::{Idle, Loading{id, at}, Ready{at}}`. Only the answer or error of request `id` leaves `Loading`; an answer of another request is dropped. A request that never answers expires after `BACKDROPS_LOAD_SECS`, so the next box asks again. A failure goes to `Idle`. |
| 04-019 | `PinReorder::stage`: `Reorder::{Dragging(id), Dropped(id)}`. `pin_reorder_archived` was already gone (`PinDrag`). |
| 04-024 | `PlaybackClock::clock`: `Clock::{Paused{at}, Playing{from, since}}`. An `Ended` stage was not added: the end is the position reaching the duration, read at `now`, and the owner pauses the clock. |
| 04-023 | `ShuffleState` is one path (`back` up to the current track, `ahead` for what Previous walked over) for one `chat`. A move in another chat starts a fresh round. |
| 04-028, 07-006 | `PollDraft::options` is `Vec<DraftOption{text, media, correct}>`; the deadline is `Option<PollClose>` (`Period` or `Date`), and `PollSend::deadline` carries the same value; `send_poll` maps it to `open_period` / `close_date`. |
| 04-037 | `PremiumHub::tx`: `Listing<TxList>` (`Idle` / `Loading{request, append}` / `Ready` / `Failed{append}`), list inside. |
| 04-038 | `PremiumHub::received`: `Listing<GiftList>`; `gift_mutation`: `GiftMutation::{Idle, Sending, Failed}`. A confirmed mutation invalidates the list (`Idle` with the old list shown), which abandons a page in flight instead of letting it overwrite the fresh list. Starting or ending a mutation does not touch the list's stage (a failed first page stayed `Failed`, not `Idle`, so the tick does not re-send it); `gifts_error()` prefers the mutation's line. |
| 04-039 | `premium`, `premium_stickers`, `business_features` are `Fetch<_>`; a failed request is `Failed` and the Business screen keeps its fallback rows. |
| 04-042 | `GramsHub::page`: `Listing<TonList>`; `balance`: `Fetch<i64>` (a failed Settings probe is not asked again by the tick); `peers`: `PeerSync`; the withdraw outcome is `GramWithdraw::{Done(url), TooSmall}` taken once by the UI. |
| 04-068 | `ProxyFetch::{NotAsked, Idle, Stale, Loading, Failed}` replaces `stale` and `fetch_attempted`; a send or answer error lands in `Failed`, which only the dialog's open / Refresh leaves (the tick no longer re-sends an owed refetch). `ProxyMutation::{Idle, InFlight(id)}` replaces `mutating`, and only the change in flight unlocks. `error` stays the box's line (three sources). |
| 04-070 | `ProxyState::rotation`: `Rotation::{Watching, Disconnected, Probing{id}}`. Only the rotation's own probe holds it; the edit box's link ping no longer does. |
| 04-079 | `ProxyState::carrier_failed` is gone: the row asks `Carriers::failed_for(id)`; a refused carrier (no host) already shows through `can_enable`. |
| 04-072 | `PsaAbout::toast`: `Hidden / Waiting(chat) / Showing`; `save`: `InSync / Unsaved`. With no file path the flag stays `Unsaved`. `wait_for_load` never replaces a `Showing` box. |
| 04-073 | `PullMachine::stage`: `Idle / Pulling(Pull) / Jumping{pull, until}`. A jump that never lands lets go after `JUMP_HOLD_MS`. The indicator and hint flags are read off their tweens. |
| 04-033 | `JoinedLines::lines` counts the pushed lines. |
| 04-034 | Skipped: `MarkupBlocks` and `PreviewBlocks` are memos of one input, private, with `parse` / `update` as the only writers, and fuzz tests compare them with a fresh parse. |
| 04-031 | `Toggle::{Settled(bool), Easing{target, from, since}}`. |
| 04-081 | `AccountRegistry::entries`: `AccountEntry::{Listed, Removing(key), WipeOnNextStart}`; `active` is repointed in one place. `accounts.json` keeps its keys (`RegistryFile`). |
| 04-085 | Skipped: user preferences. `theme`, `day_theme`, `night_theme`, `auto_night` and `follow_omarchy` are separate settings the user edits one at a time; none moves through stages, and the file format and about 150 readers rest on them. |
| 04-086 | Skipped: `suggest_emoji` (tdesktop's) and `highlight_emoji_words` (Quill's) are two settings under their own keys; the dimmed switch deliberately remembers the choice. |
| 04-089 | `ChatWindowsState::answer`: `RestoreAnswer::{Unasked, Yes, No}`; a stored `restore` without `asked` reads as `Unasked`. The file keeps `asked` / `restore`. |
| 04-090 | `Writer::{Idle, Running{in_flight}}` and `Queued::{Ready, Failed}`; `set_writer_running` is the one place the writer starts or stops. |
| 04-095 | `FirstRun::{NotStarted, Started, Done, Upgraded}` read by the pure `first_run_stage`; `first_run_pending` makes the writes. |
| 05-011 | `Heartbeat` holds `Beats::{NotRunning, Running{count}}`. |
| 05-001, 13-060 | `DictManager::download`: `DictDownload::{Idle, Running{download, task}, Failed{code, message}}`; the rows read one `RowDownload`. `Progress` is one `AtomicU8` stage (running, cancelling, finished) that only moves forward: a cancel after the finish is a no-op. |
| 05-009 | `SpellChecker::words` holds the app words, the ignores and the verdict cache under one lock; a change to the words clears the verdicts in the same step. |
| 05-024 | `ActiveCall::ready`: `Option<CallReady{params, since}>` replaces `ready` and `ready_at`. |
| 05-025, 08-082 | `CallRating` and `CallUpload` gain `NotNeeded`; `need_rating`, `need_debug_information` and `need_log` are gone. |
| 05-028 | `ActiveGroupCall::need_rejoin` is gone; readers use `reconnecting()` (`rejoin != Idle`). |
| 05-030 | Taken from the integration branch (is-04): `GroupTransport` and `GroupScreenShare` live in `state/group_call_stage.rs`; this bundle's duplicate conversion was dropped in the merge. |
| 05-029 | As 05-030: the integration branch's `GroupScreenShare`. |
| 05-032, 05-229 | `ChatOrderSync::batch`: `BatchOrder::{Live, InBatch{resort}}`; the driver's duplicate `batch.open` reads `Session::in_update_batch`. No guard type: both begin and end are the driver's, and an unbalanced end is a no-op. |
| 05-041 | `ChatSummary::my_member`: `OwnMember::{Unknown, Other(status), Administrator(AdminRights)}`; a new status drops the old rights whole. Read through `member_status()`. |
| 05-040 | `ChatSummary::my_restriction`: `OwnRestriction::{Unknown, NotRestricted, Restricted}`. |
| 05-042 | `ConferenceCall::stage`: `ConferenceStage::{Running, Upgrading, Upgraded{from}, SharingLink{from}}`. |
| 05-037 | `AuthState::change_number`: `ChangeNumber{code, send, check}` with `ChangeStep::{Idle, InFlight(id), Failed}`; a late answer of a superseded send is dropped. A check may still run during a send. `code_sent` takes the `RequestId` itself (an answer with no pending request never reaches it). |
| 05-038 | `PasswordFlight` gains `Failed(line)`; `password_op_error` is gone. |

Outside the bundle's files: `src/ui/…` readers of the changed fields, the
tests that set them, and `scripts/implicit-state-baseline.txt` (every
converted field's line dropped).

### Sweep fixes (is-12)

Thirty-nine implicit-state entries. Each stage is one value now; the baseline
lost the fields it replaced.

| id | result |
| --- | --- |
| 07-016, 09-X050, 09-097, 09-059 | `ExportBox<Draft, Progress>` (`export_state.rs`) is `Options(draft)` or `Progress { seen }`. "Hidden" is `ExportView` on the export in the session, now also on `ChatExport` (a hidden chat export used to reappear after an account switch because its flag lived in the screen). The calendar page is `ExportPage::Calendar { field, month }` inside the draft. |
| 07-019 | `LifecycleStage { Closed, Open, ConfirmDelete, ConfirmLogout }`: at most one inline confirm is armed. |
| 07-020 | `AccountOp { Idle, Loading(id), Mutating(id), Failed(line) }` for the account TTL and Delete Account (answers settle by request id); `default_auto_delete: Fetch<i32>`. A TTL write can no longer start while the read is out. `AuthState::password_state_loading` is already `PasswordFlight`. |
| 07-021 | Skipped: already `OpenNotice` + `AgeCheck`. |
| 07-022, 09-095, 09-X049 | The scroll hook is set on every open of the page (setting it again replaces it), so `event_log_hooked` is gone. `AdminUi::event_log_page: Option<EventLogPage>` is the page last opened with the timers that belong to it: the poll task, the search debounce task and the autoload mark. Opening it for another chat replaces it, which stops the first chat's timers (the old `EventLogPoll` was a chat id the detached task reset itself, and a second chat overwrote the first's mark). A newer search edit replaces the debounce task, so the keystroke counter is gone. |
| 10-090 | `TextPromptKind::CustomTitle { chat_id, user_id }` and `CommunityName { community_id }` carry their owners; `UsernameDialog` has no `chat_id`. |
| 10-091 | `GroupConfirmAction` variants carry their own chat (`DeleteChat { chat_id }`, `BlockUser { chat_id, block }`, ...); the chat-less ones have none, and no `ChatId(0)` placeholder is left. The dialog slot stays one per account (the shell shows one box at a time). |
| 01-029 | `DebugConfirm { Closed, Asking { logs_on } }`. |
| 07-037 | Skipped: already `ComposerStage`; `local_error` is a form validation line, not a stage. |
| 07-038 | `CustomThemes { Unread, Reading { task }, Read(list) }`. |
| 07-039 | `Omarchy { Unprobed, Absent, Found, Watching { state, _task } }`: the watcher is owned by its stage, so leaving it (`use_omarchy_dir`) stops it and a second one can no longer run beside it. |
| 07-046 | `Standing { Current, Stale, Failed }` replaces `failed` + `stale`; `Fault { None, Seen, Unseen }` replaces `error` + `start_failed`. An untold start failure stays until `take_start_failed` tells it: a later success, resume or lost player no longer clears it (`Fault::worked`, `Fault::seen`). `track`, `building` and `sim_clock` stay: the old sound plays on while a seek builds, and the simulation is the demo backend. |
| 07-047 | `DeviceLink { Down { failed_at }, Opening, Up { device, watch } }`: the watch thread is a `WatchHandle` owned by the device's stage. Taking the device drops the handle, which stops the thread at its next look under the lock; a panicking watch marks its device unwatched so the next use starts another. No hand reset in `look` or `WatchEnded` is left. |
| 08-039 | `ActiveTrack { None, Voice(id), Audio(id) }` and `Scrub { Idle, Dragging(secs) }`. The per-track data (`track_length`, `seek_slider`, `seek_hover`, the bar's chat and meta) is still separate and is cleared in `clear_playback_state`. |
| 08-040, 12-007 | `PlaybackTick { Idle, Running { shown, task } }` and `BuildsWatch { Idle, Watching { task } }` hold their tasks. |
| 08-046 | `AuthUi::registration: RegistrationStep { Idle, Asking(terms), Accepted(terms) }` replaces the loose `accepted_registration_terms`; `follow_auth` builds it from the authorization state on every poll, so it exists only in `WaitRegistration` and a tick holds only for the terms it was given for. |
| 08-044 | Skipped: already `TermsPrompt` + `TermsStage`. |
| 08-023 | `BotText`/`BotSave` in the session (integration branch) and `EditStage { Loading, Editing(SaveCheck) }` in the box. The 150 ms fill loop is gone: the first frame after both texts are `Ready` (frame upkeep of the window hosting the box, like the Business box) fills the fields once and moves the stage to `Editing`. |
| 08-017 | Skipped: already `BotStopStage`; `can_stop` is what the bot reports. |
| 08-027 | The driver marks the similar-bots section `Loading` as it sends; Loading draws a label, not the fetch button. |
| 08-004, 08-032 | `BotLink { Loading, Empty, Connected(id) }` and `BotNotice { None, Warning, ConfirmLeave }`. |
| 08-031 | `BusinessDialog::screen: Screen`, one value: `Greeting { draft, picker }`, `Away { draft, picker }`, `Hours { draft, sub: Week / Day { day, rows } / TimeZone }`, `Location`, `StartPage { draft, sub: Page / Stickers }`, `ChatLinks { edit, confirm_delete }`, `Chatbots { draft, picker }`. A draft is `Draft::Waiting` until the full info arrives. A sub-screen is part of the section it hangs from, so the picker, the day rows, the sticker panel and the link box cannot outlive it, and `View` is derived. `local_error` and `note` are one `Line { None, Error, Note }`. |
| 08-085 | `Loop { Off, On { sound, hold, phase } }` with `LoopPhase { Playing, Due(at) }`. |
| 08-084 | `CueStage { Ringing, Connected, Ended }`. |
| 08-080 | `RecentCallsPage { Idle, Loading(id), Failed }` and `RecentCallsClear { Idle, Clearing(id) }`. |
| 08-087 | `PipWindowStage { Opening, Open(window) }`. |
| 08-058 | `SlowModeTick { Idle, Ticking { chat, _task } }`: the stage owns the tick task; a tick for another chat replaces it. |
| 08-051 | Skipped: already `AppearanceStage`. |
| 08-063 | `DropOnRow { hover, seen: Dragged { Nothing, Files(FileDrag), Forward }, freeze: Freeze { Thawed, Frozen }, preview }`. |
| 08-066 | A topic list request that cannot be sent marks the forum failed in the session set the chooser already reads; `DropTopic::load_failed` is gone. |
| 08-069 | Skipped: already `SidebarTab`. |
| 08-070 | `PinGesture { Idle, Pressed, Dragging }` and `ChatPeek { Idle, Pressing, Open }`. A press on another pinned row during the release slide waits in the drag (`next_press`): the slide goes on, and that row's drag starts under the pointer (it used to start at the drag threshold). |

Tests: `an_older_account_request_does_not_settle_a_newer_one`,
`a_hidden_export_stays_hidden_until_the_box_is_asked_for_again`,
`a_press_during_the_release_slide_starts_its_drag_under_the_pointer`,
`a_watch_ends_with_its_device`, `a_second_chats_page_replaces_the_first`, and the
existing ones updated to the new shapes (similar bots now assert that the
driver, not the UI, marks Loading).

### Sweep fixes (is-03)

Implicit-state entries: each struct's stages (bools, `Option`s used as flags,
a side list beside a stage) became one value. Baseline lines of the converted
fields are dropped (220 to 159); three more (`CraftSession::candidates_*`,
`GifPanel::*`, `GramsHub::*`) went with their structs.

- **Paged lists: `PageStage`** (`src/state/page_stage.rs`). `Idle`,
  `Loading { request, more }`, `Ready`, `Failed(line)`. The request sits in
  the stage, so only its own answer or failure acts (a changed filter or tab
  drops the older page). The resale search (02-067), the Grams history
  (02-072) and the craft box's "your gifts" list use it, replacing
  `loading`/`loaded`/`error`/`request_id` kept side by side. A send that never
  left ends in `Failed`, which the list draws. `loaded()` is true for
  `Failed`, so a guard that means "nothing to ask" reads `is_ready()`: the
  Grams tab ask (`set_grams_filter`) went through `loaded()` and sent nothing
  when the same tab was tapped again after a failure; it asks again now. The
  craft box's "your gifts" list draws its error with a Retry
  (`craft-own-retry`) and `preload_craft_lists` asks again from `Failed`
  (opening the box offline, then reconnecting, no longer leaves it empty).
  The gifts list (02-064) is a `PageStage` too (`gifts_page`), with the
  change's refusal in `GiftMutation::Failed`; `gifts_error()` reads both.
- **`Fetch<T>` and small stage enums.** Archive settings (02-022), contacts
  (02-029, the invalidation is `Idle`), saved tags and the Grams balance probe
  are `Fetch`. The saved sublist list is `SavedTopicsLoad` (`Idle`, `Started`,
  `Retry`, `Exhausted`); the reconnect path calls one
  `retry_after_reconnect` (05-172, 05-209, 02-059). `SavedTagSearch::loaded`
  was written and never read, so it is gone (05-210). The award members list is
  `MembersStage` (03-057, 02-070, 10-028); the gift value lookup is
  `GiftValueStage { Idle, Loading(id), Failed, Ready { info, open } }`
  (02-068, 04-044, 10-023; offline, where it cannot be asked, it goes to
  `Failed` instead of staying on `Loading(0)`); the giveaway info lookup
  carries its request in `GiveawayInfoStage` (10-021, 05-116 for the
  lookup). The emoji pack change is `SetMutation { Idle, Changing { request,
  .. }, Failed }` (02-055): only the answer of its own request settles it, a
  new change or a new box replaces a failure (`started`, `for_new_box`), and
  the two hand resets are gone. GIFs are `SavedGifs` and `GifSearch`
  (02-060), both with the request they wait for in the stage: an older
  search's page or error, or a list answered after it was dropped, changes
  nothing.
- **`Fetch` carries its request.** `Fetch::Loading { stale, request }` with
  `sent`, `answered` and `failed`: contacts (02-029), archive settings
  (02-022) and the Saved Messages tags take the answer of the request that
  asked and drop any other (a contacts answer that lands after a contact
  change dropped the list no longer brings back what the change removed).
  `load_saved_tags(0)` begins the fetch, so a send that never left lands in
  `Failed` and the pump waits for the connection instead of asking at every
  pump. A refetch of the invalidated contacts that cannot start (chats path
  not active) shows the error and Retry again.
- **Matched to what asked.** The gifts list's one change in flight is
  `GiftMutation::Running { request }`, settled only by its own answer, so a
  late answer of a reopened list no longer unlocks it (02-064, 10-026). The
  Fragment export's unlocked step is `ExportSend::Unlocked` in the transfer
  stage, not a flag the driver takes after each ingest (02-064).
- **Earn.** The withdraw password step is `Password { error }` or
  `SendingPassword` (09-X040, 02-050); the setup page's save is a `BoxSend`;
  the join box parked for the recipients lives inside
  `Recipients::Loading { join }` and goes with that attempt (02-049, 05-122);
  toasts, a URL to open and "leave the setup page" are one `EarnEffect` queue
  (`setup_closed`, 02-050, 05-122); the UI's four boxes are one `EarnBox`
  opened by `open_earn_box` (09-X041); the link box carries the time it
  opened, so its first render plays the icon (09-073).
- **Queues and work.** The poll batch is `Batch { Closed { owed }, Open { dirty } }`:
  a pass leaves the set only after it ran, so a send error in the media pass
  no longer loses it (02-040). The composer draft save is `DraftSave { Idle,
  Waiting { pending, started_ms } }`: the pending save and its quiet-window
  clock are one value (02-027). `DiskPressure` replaces the held bool beside
  the room (05-096); the persisted disk pauses are compared with what was last
  saved, not flagged (05-096); `AvatarDue { Everything, Some(set) }` replaces
  `avatar_rescan` (05-096, 05-247 in part). The sticker category is chosen by
  its `EmojiCategorySource`, so a reloaded list cannot name another (10-172).
  The deep link's `ResolvingInfo` carries the link text, and the flow's
  origin lives in the flow slot (`DeepLinkFlow::Active { from_agent, state }`,
  02-042): born with the flow, taken with its state, `Idle` has none, so no
  terminal state leaves one behind. The draft save is `DraftSave { Idle {
  last_token }, Waiting { pending, started_ms }, Failed { pending } }`
  (02-027): the quiet-window id is inside it, and a save that failed to go
  out is a stage of its own, not `started_ms: None`.
- **Already fixed on the integration branch.** 02-069, 05-116 (options),
  10-022 (`GiveawayOptions` is two `Fetch`es), 02-044 (`DiskSearch`),
  02-057 (`InviteStage`), 02-108 (`ShareLink` per link), 09-074 (sliders
  keyed by bot, `ensure_setup_sliders`).
- **Review round.** 05-247: a pause is `Phase::Paused` of the request (not a
  bool beside the phase) and the chat that asked an automatic download is its
  origin (`DownloadOrigin::AutoChat`), so `open_chat_media_downloads` is gone
  and leaves with the request. The `DownloadExtras` index (request id to file
  id and back) is a lookup of the registry, and `avatar_claims` is the
  per-frame demand of the rows on screen; neither is a stage. 02-035: the
  closing set keeps one `Slot` per account (`Closing(client)` or
  `Unconfirmed`), not an entry list beside an `unconfirmed` set; dropping the
  set never waits on the dropping thread (`close_all_within` is the one wait,
  the quit paths call it). `close_held_until` is the deadline of one hold and
  `restart` is a per-client policy set by the caller and changed by user
  actions; neither is a stage kept in step with another. 02-113: the
  provisional engine key is `ConferenceKey::Joining`, not `-1` compared by
  hand, and a join whose offer failed ends the session's pending join, the
  crypto, the offer wait and the engine instance in `abandon_conference_join`
  (the driver tests with the mock engine cover the exits).
- **Skipped, with the reason.** 05-242: the four values are not one
  conversation of the main window. `threads.thread` is keyed by its own chat
  and read by chat windows (`thread_for_chat`), `direct.open` is the
  conversation remembered for each chat after it is left (tdesktop
  `activeSubsectionThread`), the Saved sublist belongs to Saved Messages, and
  `open_topic` is the main window's. One enum on the session would have to
  drop the others when one window changes, which loses a window's thread. The
  clash is read in one function (`open_topic_id`, with `open_draft_topic`
  beside it), pinned by tests. 09-121: the `focused` bool is the per-frame
  edge memory of "take the keyboard when the box becomes idle", which needs
  the `Window`; the stage transitions run in the driver without one. It is
  the UI's memory of what it did with its own widget, not a request stage.
- **Merged with the integration branch.** Other bundles converted the same
  fields before this one landed, so the integration branch keeps their value
  and this bundle's version of these is gone: the paged lists and the gifts
  change (`PageLoad`, `PageStage` of the premium hub and `ListOp`, not
  `state::PageStage`/`GiftMutation`), Earn (`EarnEffect`, `PasswordSending`),
  the draft save (`Debounce<HeldDraft>`), the deep link origin
  (`DeepLinkFlow` with `start_deep_link`), the GIF panel (`GifList`) and
  `SetMutation::Running`. What stays from this bundle: `Fetch` carrying its
  request (contacts, archive settings, saved tags), the gift value and
  giveaway info stages, the poll batch, the disk and avatar stages, the closing
  set, the conference key and the craft box's "your gifts" error with its
  Retry (`craft-own-retry`), which asks again only from that button: the open
  preload does not re-send a failed list.

### Sweep fixes (is-07)

Implicit state: stages that were bools, options and counters kept in step
by hand became one enum each. Behaviour is the same unless a line says
otherwise.

| id | where | now |
| --- | --- | --- |
| 03-053, 03-054, 03-055, 03-056, 09-124, 09-126, 10-027, 09-X064 | gift market | the integration branch converted the same fields (`PageLoad`, `BuyPhase`, `CraftRun`, the auction was `AuctionFetch`); is-07 merged onto that version and dropped its own parallel enums, so the market has one stage type per list, buy and craft |
| 09-X066 | `CraftSession` | a failed "your gifts" list draws its error with Retry (`retry_craft_candidates`) instead of "none" |
| 03-082 | `link_handler` | one `Cell` (generation + `Stage` Empty/Known/Reading); the three statics are gone, and the Appearance page's polling watcher with its `WATCHING` flag is replaced by `wake_when_read` (a waker the read runs once; the page owns the waiting task, `PreferencesUi::link_handler_watch`) |
| 03-077 | `CommentsTracker` | `has` is read off `CommentsState`; the toggle does nothing while there are no comments (its button is not drawn then) |
| 03-084, 03-083, 03-076 | location picker | `Address.stage: AddressStage` (Idle, Postponed, Wanted, Resolving, Shown, HiddenWhileMoving); `VenueSearch.input: SearchInput` (Closed, Blank, Typing, Settled) replaces the query, shown and debounce fields. An address that lands while the pin is lifted waits for the pin's return (it was drawn under it) |
| 03-088 | `BoostState` | enum Idle, Armed, Boosting |
| 03-098 | `WheelNav`, `SwipeNav` | the trackpad gesture is one value: `SwipeNav` gains `Spent`, set by the viewer when `WheelNav` moved or zoomed in it, and handed to `WheelNav::feed` |
| 03-094 | `EffectWatch` | one bounded map of `EffectStage` (Unwatched, Watched) with one eviction |
| 03-093 | `MessageEffects` | `by_id` folded into the per-effect `EffectLookup` (adds `Ready`) |
| 03-099 | `MediaFacts` | `file: MediaFile` (Remote, Downloading, Local) |
| 04-007, 14-050 | widgets | `Lifecycle` carries the stop streak (`Waiting`, `Loading`, `Stopped`) and the runtime with everything of it (`Live`: queries, theme, frame ask, cursor, `Pointer`); the selected text stays on the entry until the content changes; a stopped, released or restarted widget starts with none of it |
| 04-008 | crash handler | `IN_HANDLER` and `REPORT_DONE` are one `REPORT` atomic (Idle, Writing, Written) |
| 04-003 | `Coalesce` | one `AtomicU8` (Idle, Running, RunningDirty) |
| 04-005, 04-004 | `NewChatStage` | `Creating{progress: CreateProgress}` and `Type{save: TypeSaveStage}` carry the request state; `StageFacts` is gone, `Session::settled_new_chat` folds the answers in, and the poll drain stores the result; `NewChatView` holds one `ViewState`. A type Save is matched by the request ids in `Saving` (`TypeSave` Sent, Done, Failed); a request gone without an answer lands in `Refused`, and the chat-wide `info_edit_errors` no longer decides it |
| 04-010 | `BackgroundAccounts` | `clients` is changed only through `clients_mut()`, whose guard publishes the combined unread when it ends: no other writer of the badge |
| 08-047 | `BackgroundAccounts` | `prepare: PrepareSet` replaces `starting` + `preparing`; `poll: Option<Task>` replaces `polling`; `local: LocalData` replaces `swept` + `wiping` |
| 08-048 | `BackgroundAccounts` | a client that could not start (`Failed`, no client) is replaced by the next `ensure_background_accounts` pass |
| 04-067 | `notify_focus` | one `Mutex<Dnd>` (Unknown, Known, Refreshing{last}) |
| 04-059 | `PaidReactions` | `offered` is `MessageReactionOptions::offers_paid` (the picker's own state); `senders` is a `Fetch` per chat, asked again after a failure; `note` is the error `add_paid_reaction` returns (`PaidAddError`) |
| 04-064 | passkey sign-in | `LoginStage` (Idle, FetchingOptions, Ceremony, Asserting, Checking, Failed) with request ids, `config: Fetch<()>`; `Ceremony` and `Asserting` carry the options and when they were fetched, so `LoginState::pending` is gone (reuse is `reuse_options`); a lost request ends the stage, the sign-in screens going away resets it |
| 04-063, 05-147 | `PasskeysState` | `list: PasskeyList` (Unknown, Stale, Asking, Known, Failed); `Asking` and `Failed` carry a `Basis` (Unanswered, Answered) so a re-ask or a failed re-ask keeps the answered list and the Privacy row usable; `removing: Option<Removal>` matched to its request. The registration was already `PasskeyAdd` |
| 12-081 | `CeremonyUi` | `run: CeremonyRun` (Live{sheet, worker, focus_pin, qr}, Ended(reason)) |
| 04-015 | `FreeState` | `ui: FreeUi` (Idle, MenuOpen, Requesting{request}) |
| 04-016 | `PeerQrData` | `UserLinkFetch::Failed(Option<reason>)`, taken once by the box |
| 04-013 | `PeerQrBox` | `save: SaveStage` (Idle{failed}, Out{serial, choice}) replaces `saving` + `failed` |
| 11-p8-006 | `PanelMotion` | enum Idle, Opening, Closing |
| 11-p8-005 | `OpenQr` | `export: Export` (Idle, Rendering(task)) and `clock: Option<Task>`: closing the box ends both |

Skipped, with the reason:

- 14-052: `Presence` (Shown/Hidden) already replaced `visible` and `hidden_at`.
- 14-053: `WidgetsUi::lander` already holds the task.
- 12-082: the ticker `Option<Task>` is the task handle the owner holds (as
  `lander` is). Busy is not stored: `passkey_creation_busy` and
  `passkey_login_busy` derive it from the ceremony on screen and the
  driver's `PasskeyAdd` / `LoginStage`, so no second copy of the stage
  exists.
- 03-099: the entry's `downloading` and `local` half is `MediaFile` now (in
  the table above). Only `favorite`, `gif_saved` and `in_profile` are
  skipped: each is one three-valued fact loaded later, not a stage kept
  beside another field.
- 03-076 (`moving`, `places_in_area`): the pin being dragged and the
  "search this area" offer are independent of the address and the query.
- 04-008 (`INSTALLED`, Windows `IN_FILTER`): a one-time install guard and a
  re-entry guard, not stages of one thing.
- 04-013 (`chosen`, `served`): the pick mode and the data it follows, not
  request stages.
- 08-047 (`deleting`, `closing`, `clients`, `screen_preparing`): each is its
  own stage set (a folder being deleted, a client closing) and was left as
  it is.

Guards: the implicit-state baseline lost the fields converted here
(`LoginState`, `MediaFacts`,
`PanelMotion`, `StageFacts`, `NewChatView`), `MediaFacts::in_saved_messages`
is listed in the allow file (a fact about where the message lives).

### Sweep fixes (is-05)

Implicit-state bundle: lists that were an `Option`/`Vec` beside `loading`,
`loaded`, `error`, `stale` and a request id are one value each. Two shared
seams carry most of it: `Paged<P>` (`src/state/paged.rs`; a paged list whose
page in flight names its request, with `Outdated(Option<P>)` for "a change
landed, ask again") and `Fetch<T>` (now with `Outdated(Option<T>)` and a
sibling `Change` for one mutation at a time: idle, busy, failed with its
line). A change that outdates a list drops the page asked before it, a first
ask with nothing held included (`Loading { stale: None }` becomes
`Outdated(None)`, drawn as loading by `shows_loading`), so a late answer never
replaces the refetch: `Fetch::answer` and `answer_failed` drop what lands on
an outdated list. A refetch whose send fails ends in `Failed` with its line,
not in `Ready` (which would lose the outdated mark).

The integration branch converted several of the same fields in parallel
(star subscriptions, quick replies, emoji icon lists, invite-link members,
the sponsored report, recommended channels, `Seed` for storage limits, the
debounce token). Where both sides made an enum for the same fields this
branch kept the integration one and moved its readers onto it.

| ids | Fix |
| --- | --- |
| 02-181 | Passkeys: `ListStage` (never / asked / known / stale) replaces `known`, `requested_at`, `stale`; asking again for a known list keeps it `Known` (the row keeps its label), `Asked` only from never or stale; `PasskeyRemove` carries the removal's request |
| 02-154 | Subscriptions: integration's `StarSubscriptionsList` (this branch's `StarSubscriptions` dropped) |
| 02-172 | Profile, chat and background emoji icon lists and the wearable gifts are `Fetch<Vec<_>>` (integration's asks-from-the-transition form) |
| 02-175 | Premium hub: Stars history and received gifts are `Paged` (a filter switch resets the list and drops the old filter's page), Premium, business features and premium stickers are `Fetch`; drawn as loading while a first ask or a refetch of nothing is due |
| 05-182 | Invite-link members: integration's `PageStage` |
| 02-185, 06-016 | Playlist: one `Paged<PlaylistData>`; a failed change outdates it |
| 02-188 | Proxy: list is `Paged<ProxyList>`, the mutation lock is `ProxyMutation::InFlight(request)` (an error with no request to match still ends it), a ping slot is `Checking(request)`; a change during the first `getProxies` outdates it, so the pre-change answer is dropped and the list is asked again |
| 02-197 | Next-channel preload: `HistoryPreload` per chat; pinned messages are `Fetch<Vec<_>>` per chat (loading / ready / failed); a failed fetch is not sent again by every preload tick, the chat opening asks again |
| 13-034 | Sponsored report: integration's `SponsoredReportFlow`; hiding ads no longer overwrites a report in flight |
| 02-191 | Quick replies: integration's load values |
| 02-193, 02-213 | Typed search: the quiet-window `Debounce` lives in `SearchState` and `ChatSearchState`, and every new generation (close, clear, filters, a query started) drops it, so no hand `clear_*_debounce` is needed for those and it cannot outlive its search |
| 02-216 | `Leg::Paying { request, price }` for the confirmed paid public-post search; the box carries the query and price it showed (`SearchConfirm::PaidPostSearch { query, price }`) and the driver refuses when either differs from the open query or TDLib's reported price |
| 02-215, 05-205 | Recommended channels: integration's `Fetch` |
| 02-205 | Sessions, websites, account TTL and default auto-delete: `Fetch` plus `Change`; a successful terminate/disconnect outdates the list; `set_account_ttl` clears the load's error line |
| 02-208 | Storage limits: integration's `Seed` (unseeded / in flight / seeded), persisted from the enum |
| 02-200, 05-016, 13-020 | Gift Stars: `open` folds into `GiftStage::Closed` |
| 02-218 | Public forwards: `Paged<PublicForwardsPage>`; an empty answer is an answer |
| 02-226 | Sticker batch: `StickerBatch` (idle / running / done) |
| 02-227 | GIF panel: `saved: Fetch<Vec<_>>` and `GifSearchStage` (off / loading / failed / ready) |
| baseline | `SettingsState::saved_notification_sounds` is a `Fetch` (never, loading, ready, outdated, failed); `saved_sounds_loaded`/`saved_sounds_stale` are gone |

Already one value on the integration branch, no change: 02-156
(`BankCardStage`), 02-176, 05-253 and 09-X019 (`PrivacyData::network` is a
`Fetch`; errors are per control), 02-182 and 05-183 (`MorePage`), 06-034
(`SystemStepState`), 02-204 (`read_date_stage`), 02-229 (`StickerSetStage`),
03-011, 05-277 and 13-103 (`StoryViewersState::page`), 05-161 and 06-021
(outcomes and checks are keyed by request), 06-019 (`StoryManageState`
settles per request).

Skipped: 02-230. `pending_live_story_join` is an `Option` that carries the
group call and the `getGroupCall` request, and it leaves on ready, on the
request ending, on a new join and on `leave_live_story`;
`stories.live` (the viewer's content) and `calls.story_stream` (the call
slot) are different owners, not stages of the join.

Not done: the in-chat search's `status` and `generation` stay two fields.
`status` is already the one `SearchStatus` value and the older-page state is
`ChatSearchPaging`; `generation` is a monotonic id that stale answers are
matched against, not a stage. The search timers stay detached tasks: a
`gpui::Task` cannot live in the core session state, and a stale fire takes
nothing because its token is no longer the armed one.

Two bools stay with a reason in `scripts/implicit-state-allow.txt`:
`pending_attention` is a dock-bounce event the notifier drains with `take`.

- **Merged with the integration branch.** Bundles merged earlier converted
  these fields first, so their value stays and this bundle's is gone: the
  sessions, websites and account changes (`ListOp`, `sessions_op`), the passkey
  list and its removal (`PasskeyList`, `Removal`), the proxy list and its
  pings, the GIF panel, the saved sounds list and the gifts and premium-hub
  paged lists. What stays from this bundle: `Fetch::Outdated` (a change that
  lands after an answer keeps the old answer and asks again), the `Paged`
  lists of search, shared media, playlists and stats.

### Sweep fixes (is-10)

Forty-two implicit-state entries. A fix means the shape is gone: one enum per
stage, every stage with a way out (the answer, the error, a cancel, the owner
going away), and the baseline line of each converted field dropped.

- **Settings** (05-140, 05-141, 04-047). Each fetched resource is one
  `Fetch<T>`: storage statistics, account TTL (also the `setAccountTtl` and
  `deleteAccount` in flight, so the surfaces disable while the value loads),
  connected websites, default auto-delete, the auto-download presets and the
  cache clear (`Fetch<i64>`: in flight, the bytes freed, or why it failed).
  `data_storage_error` was one slot shared by five requests; it is gone, each
  request keeps its own failure (`data_storage_error()` joins them for the
  screen). A write against a list is `ListEdit { Idle, InFlight { refetch },
  Refetch }` (the sessions and websites lists, with the "refetch on the next
  ingest" need: a list that went stale while a write was out stays owed
  when the write fails), a `setAccountTtl` or `deleteAccount` waits only
  for another such write, not for a `getAccountTtl` in flight, the device link is `DeviceLogin { Idle, InFlight, Done(result) }`
  taken once, and the read-date setting is `ReadDate { Unknown, Getting,
  Known, Setting, Failed }`, so the value and the request that owns it are
  one value.
- **Stickers** (05-150, 05-151, 05-152, 05-153, 05-154, 05-238, 09-079). The
  set box's edits are `StickerSetEdits { Idle, InFlight, NeedsRefresh,
  Deleted }`; `files_requested` moved into the `Ready` stage of the set view
  (a refreshed set starts unasked). The emoji panel's install is
  `EmojiMutation`, the status picker's action `EmojiStatusStage`, and the
  tab a search returns to is part of `EmojiSetTab::Search { back }`, so the
  picker no longer keeps a second field in step. The GIF panel has
  `saved` and `search` as `Fetch<Vec<_>>`; a refetch owed (an
  `updateSavedAnimations`, a save or remove) is `Fetch::Stale`, a variant of
  its own (the answer shows, no request is out), asked once the panel is
  open, and a failed list waits for the panel to reopen; the saved
  notification sounds use `Stale` the same way. The masks panel keeps the
  integration branch's shape (`Fetch` per list, `MasksWanted`, a `Fetch`
  per set); the manager's commit is a queue of `CommitStep`s. The status
  picker's `EmojiStatusStage::Changing` carries a line that arrives while
  the change is out (a failed list fetch, a refused send) and shows it
  when the change ends.
- **Stories** (05-164, 05-165, 05-260, 05-282). `ActiveStoriesPaging` is
  `Idle | Waiting | Exhausted` (a late failure no longer turns an
  exhausted list back into a wait), the picker's custom emoji fetch is
  `CustomEmojiFetch { Idle, Backoff }` (an answer ends the failure count),
  and one `StoryFetches` map holds where each story's `getStory` stands
  (`Asked { wait } | Gone | Retry`; an ask after a wait is `Asked` and
  carries the wait, so the next failure still doubles it), replacing the
  asked set, the gone set and the retry map.
- **Threads and users** (05-170, 05-171, 05-173, 05-174, 05-175, 05-271). The
  direct-messages topics keep `TopicsLoad { Asked | Failed | Exhausted }` per
  chat (a failed first page counts as not loaded: opening the chat asks
  again, an update about the chat does not); the forum topic lists are one
  `Fetch<Vec<ForumTopic>>` per chat in `ForumTopicsMap` (`forum_row` is
  gone; a failed refetch keeps the topics); contacts are `Fetch<Vec<i64>>`;
  the New Contact box's import is `NewContact { Closed, Sending, Opening,
  Done }` (`Opening` carries the joined user until the driver opens the
  chat, so `user_chat_to_open` is gone; the support chat opens from its
  own answer); the three emoji icon lists and the wearable gifts are
  `Fetch`es (an empty answer is now an answer, not "not loaded").
- **History** (05-176, 05-179, 05-180, 05-241). A window's two ends are
  `WindowLoad { older: Open | Failed | Complete, newer: Open | Failed |
  Exhausted }`, replaced whole with the window. The `viewMessages`
  bookkeeping is one `ViewBook`: a stage per message (`Due | InFlight(request)
  | Done`) and whether rows were reported, so a trimmed message leaves every
  stage at once. The dropped-history memos carry their own order. Closed chats and
  topics are one `Retention { Releasing, Closed }` per key in `Retained`
  (the order lives in the value), replacing the LRU list and the release
  queue, so a chat is never both queued and ranked. A history page carries
  the window epoch it was asked for (`PendingRequest::window_epoch`), so a
  page for a replaced window reads as stale without a session-wide id set.
- **Session** (05-189, 05-190, 05-211, 05-212, 05-216, 05-224, 05-237, 05-239,
  05-245, 05-259, 05-273). A set's library load is `SetLoad { Loading(request)
  | Ready | Failed }`; the picker's Star offer lives in the reaction options;
  Stop on a streaming bot reply is `BotStopStage::Stopping(request)` (the bar
  no longer reads the request table), and an error settles only the Stop it
  answers; the window a request was asked in is one `AskingView` (the
  window and its opening); `getRepliedMessage` registers its request and its
  `Loading(request)` row together; the "boosts to bypass restrictions" save
  carries its request, and an older answer does not settle a newer save; the
  report flow's steps are one list; the next page of a reactions tab is
  `Audience::Paging`; forwards are `Forwards` (one flight per destination and
  a queue of outcomes, bounded, so two forwards finishing before the UI
  polls both show, and a log-out clears the queued flights too); the main
  window shows one `OpenView { Closed, Chat, Topic }`, so a topic cannot
  outlive its chat (a chat window's `SessionView` carries the same
  value); a group call the user left is `GroupCallRejoin::Left` (it
  replaces `left_group_call` and the `need_rejoin` copy, ends any rejoin in
  flight and is ended by TDLib's not-joined update); shown notifications
  are `NotifyShown { Shown | ClearDue }` per chat.
- **Skipped.** 05-166: `CloseFriendsList` already matches the load to its
  request, and the sent list rides the request (tested in `stories_more.rs`).
  05-167: `extras_requested` is gone, the three extras are `Fetch`es.
  05-181: the unread bar (`unread_anchor`) and the read hold
  (`opened_with_unread`) are separate in tdesktop too (`_unreadBarView` and
  `_firstUnreadView`) and take all four combinations. 05-213: the optional
  key fields of `PendingRequest` are written once at registration and never
  change; they are keys, not a stage. 05-252: the privacy counter is gone
  (`privacy_asks` derives loading and failed). Left as they are:
  `HistoryPruning::holds` (the UI's claim, replaced whole at every update
  by `set_history_holds`: an input, not a stage of a history) and
  `keep_all` (set once by the screenshot demos for the whole session).
  05-151 and 05-152, the `open` of the masks panel and of the stickers
  manager: both are session-held dialogs that the farewell closes
  (`close_mask_sets`, `close_stickers_manager`; `scripts/session-dialogs.txt`
  lists `MaskSets`), the screen's own flag for the manager is
  `settings.sticker_settings_open`, and the core driver reads the session's
  copy to decide what to download, which it cannot do from the UI's
  `OnScreen`. They are written only by the open/close calls
  (`set_dialog_open`, `set_sticker_manager_open`).

- **Merged with the integration branch.** Bundles merged earlier converted
  the same fields. The integration forms stay for sessions, websites and gifts
  (`ListOp`), the login stages, the GIF panel and the emoji packs; this
  bundle's `Fetch::Stale` became `Fetch::Outdated`, and its `OpenView`,
  `DeviceLogin`, saved-sound `Fetch`, media-library set stages and
  Data & Storage save stage are kept. `ListOp::MutatingStale` keeps a refetch
  that went stale while a change was out, also when that change is refused.

### Sweep fixes (is-01)

Implicit-state entries: each names a struct whose stages were kept in step by
hand. The stages are one value now; the baseline lines of the converted
fields are dropped (`Relay::started`, `StyleExamples::loading`,
`ChatListState::archive_settings_loading`).

| id | What it is now |
| --- | --- |
| 01-001 | The webview carrier loop owns `Phase { LockingDown, Loading, Closing { at, wait_for_browser }, Gone }`; `armed` stays only as the copy the web view's callbacks read. `Relay::closing_at` is gone. |
| 01-002 | `Relay` holds one `Document { started, finished, page: Page }`, `Page { Silent, BridgeUp, Talking, Listening { in_flight } }` plus `TalkingUnbridged` and `ListeningUnbridged` for a page heard before the new document's hello; replaced whole by `load_started`. A late `Receiving` from the previous document waits for the hello (as `bridge_seen && receiving` did) before anything is delivered. |
| 01-004 | The mini-app helper shell is `Stage { Booting(queue), Ready, Capturing { at } }`; a top-level navigation back to the shell page (`ShellNavigated`) starts it over at `Booting`; a full queue says so in the debug log. |
| 01-028 | `access.rs` keeps the lock flag, the lock count and the switch generation in one `Gate` under one lock. |
| 01-027 | `activity.rs` keeps the connections, their `Stage { Live, Revoked }` and the refused agents in one `Roster`. |
| 01-007 | Skipped: the `alive` bool is gone already; liveness is read off `Waiting::{Open, Dead}` under the one pending lock. |
| 04-002 | `Link::gateway` is one `Gateway { None, Unresolved(ip), Resolved(ip, hardware) }` (the `gateway` and `gateway_hardware` pair is gone); `Path::settled` and `carried` read that stage and no longer compare against the tracker's neighbours. `Tracker::neighbours` is `[Neighbour; 2]` (`Unasked`, `Asking(gateway)`), only which gateway was already asked. `link::Resolving` is a per-IP-version try-lock guard, not a stage. |
| 01-010 | `agent/subscription.rs`: `Subscription` (`None`, `Watching { token }`) replaces the 0 sentinel at its three call sites. |
| 01-021 | `request_access` builds one `Request { Contacts, AllChats, Chat }` that yields the prompt and the answer text. |
| 01-019 | `CallScope::begin(start, agent)` saves the previous call and agent and restores them on drop. |
| 01-043 | Screenshots travel in `ToolOutput::images`; the thread-local side channel is gone, and a lock still drops them. |
| 01-041 | Skipped: the command already has one `Stage { Queued, Running, Injected, GaveUp, GaveUpAfterInjecting }` under one lock; `cancelled()` reads it. |
| 01-042 | `DriverClock { drive, real_input }` holds `Option<Instant>`s; no 0 sentinel. |
| 01-059 | The input guard is one thread-local `Guard { mode, refusals, step, open }`. |
| 01-057 | `Facts::hold` is `Hold { None, Paused { reason, since_ms }, Stopped }` (a Stop ends a pause, a pause cannot start under a Stop), and `Session::run` is `Run { Running, Ended { at_ms } }`. |
| 01-047, 07-029 | `StyleExamples` holds `ExampleFetch { Idle, Asked { token, at } }`; `loaded(token)` and `failed(token)` act only for the request awaited. The preview keeps the example shown. |
| 05-097 | The lookup and example answers are `StyleReply { Running, Answered }`; `styles_received` had no reader and is gone; a save answer carries its request. |
| 07-033 | `StyleSave::Sent { request }` lives in the editor and is matched to the answer's request; the session only holds the newest answer. A late limit error of a save whose editor was left now toasts the limit as well. |
| 01-049 | `EmojiPlays` is one `HashMap<MessageKey, Play { generation, state: Played or Replaying }>`. |
| 01-048 | `ChatListState::archive_settings` is `Fetch<ArchiveChatListSettings>`; a failed reload is `Failed` with the cached copy still readable. |
| 01-050, 02-123 | `BoostPhase::Working(BoostWork)` with `Slots`, `SlotsReady(slots)` and `Boosting { reassign }`; `reassigning`, `groups.boost_intent` and `boost_slots_by_chat` are gone, so closing the box or an error ends the wait. A Boost press with no open box, or while working, sends nothing. |
| 08-016 | Same change. The status and details caches are data, not a stage, and stay. |
| 01-068 | Skipped: `CallLinkStage` already holds the confirm, leaving and offered stages; `calls.call_link` is the invite hash used by the later join, not a stage. |
| 01-073 | `MicProcessor::denoise` is `Denoise { Off, On(state) }`. |
| 01-070 | `FeederMode { Forwarding, Paused, Stopped }` in one atomic; a stopped feeder never forwards again. The sink's own `closed` flag belongs to the queue. |
| 01-069 | `Health { Stopped, Running, Failed }` shared with the stream callback (`MicHealth`), replacing the error slot. |
| 01-060 | The PipeWire handler has `StreamPhase { Connecting, Streaming, Paused }`. |
| 14-076 | The macOS scanner is stopped when `events` is `None`; `stopped` is gone. |
| 01-062 | `CaptureStage { Running, Stopped }` on the Windows capture's shared state, used by `stop` and `end`; `stop` is safe to repeat. |
| 04-077 | `CarrierStage { Starting, Ready, Failed, Closing }` in one atomic, each transition one step. |
| 04-078 | `Folder { NotConfigured, Claimed(generation), RetryAfter(time) }`. |
| 01-061 | `ShareVisibility` in `screen.rs` is the one transition the X11, macOS and Windows loops use. |
| 01-084 | `StreamsSync { Idle, Asking { request, waiters } }`: a time waiter cannot outlive its request. |
| 01-082 | `GroupJoin` carries the mode (`AnswerApplied { mode }`, `Connected { mode }`), and `PresentationStage { Offered, Connected }`. `camera_id` is what is wanted and `camera` is the source running: two things, kept. |
| 01-078 | Presentation stage as above. The two offer maps hold different values (a call's, a screen's), so they stay two. |
| 06-029 | Skipped: `is_joined` and `need_rejoin` are TDLib's own `groupCall` fields; `group_call_joined_or_joining` is the one place that derives the stage. |
| 01-075 | `Conference` keeps `Link { Unattached { pending, unsent }, Attached { handle, pending }, Closed }` behind the media lock. |
| 01-076, 01-081 | `P2pStage { Waiting { pending_signaling }, Live { handle } }`; the hung-up stage is the call leaving the table. |
| 01-077 | `CallMediaConfig::video` is `VideoSource { Off, Camera(device), Screen }`, so the camera and a screen share cannot both be set. The native camera and screen handles stay on the call (resources, not choices). |

One shared-file change: `session_apply_error.rs` restores the archive settings
through `Fetch::data_mut` (the rollback has to follow the field's new type).

Not compiled here: the Windows, macOS and Linux-only capture files
(`screen_win`, `screen_mac`, `ble_macos`) were changed by reading, and the
tgcalls tests need the native library, which this machine did not run.

### Sweep fixes (is-04)

Implicit-state entries: each struct that kept its stages as bools, `Option`s
and side lists now holds one value per stage, with an exit for the answer,
the error and the owner going away. Every field converted left
`scripts/implicit-state-baseline.txt`; two TDLib-reported facts went to the
allow list with a reason.

| Entry | Fix |
| --- | --- |
| 02-107 | `ActiveGroupCall::join_as_options` is a `Fetch<Vec<MessageSender>>`: begun only when the request left, failed by its error arm, never re-asked from `ingest`. |
| 02-109 | The call link stage already was one `CallLinkStage`. The hash (`call_link`) is now set only once `getGroupCall` left and dropped when it fails. The "Discarded end screen keeps `active_call` Some" claim no longer holds: `end_active_call` takes it on the terminal update. |
| 02-100 | `ActiveGroupCall::transport` is `GroupTransport` (`NoTransport`, `AnswerReady`, `Connected{error}`, `Failed`) and `screen_share` is `GroupScreenShare` (`Off`, `Starting{answer}`, `Sharing`); a rejoin resets both together, and a stray screen answer with no start in flight is dropped. |
| 02-125 | The three emoji icon lists (chat photo, profile photo, background) are `Fetch<Vec<StickerItem>>`; a failure is asked again at the next open, an empty answer is not. |
| 02-131 | The welcome pack per chat is one `Fetch<Vec<_>>` (the cached pack lives in the stage). |
| 02-130 | The join-request list carries its query and the request it waits for (`Loading{query, request}`, `JoinRequestList{query, more}`); the three maps are gone. |
| 02-129, 05-092 | `InviteLinkMembersState` and `LinkRequestsState` hold a `PageStage` (the stage `BoostsListState` already had). |
| 02-127, 05-234 | `BoostsListState` already had `PageStage`; nothing left to convert. |
| 05-235 | `InviteLinkFetch::Loading(RequestId)`; an admin's two link lists take only their own answer. |
| 02-134, 05-143 | `LanguageStage` (`Idle`, `Fetching`), `SwitchStrings` for the two string sets and a `LanguageEvent` queue (`Install`, `Patch`, `Reload`, `SwitchFailed`) replace `switching` and the four one-shot fields. `packs_loading` no longer exists (`packs` is a `Fetch`). |
| 10-159, 05-115, 02-137 | `GiftCodeStage` (`Checking`, `Known`, `Applying`, `Applied`, `Failed`, `ApplyFailed`); a failed apply can be tried again. |
| 10-160, 05-144 | `LanguageLinkStage` (`Looking`, `Known`, `Failed`). |
| 05-091, 05-192, 02-166 | `OwnershipState{check: Fetch<CanTransferOwnershipResult>, transfer: TransferStage}`. |
| 09-030, 02-135, 03-087 | `GiftPurchaseStage` (`Loading`, `Quoted`, `Sending`, `Sent`, `Failed`); the answer is matched to the request, the note is derived. |
| 09-031 | `SettingsState::storage_stats` is a `Fetch<StorageStats>`. |
| 02-138 | `MasksPanel`: `Fetch` per list, `MasksWanted` for dialog/strip, `sets: HashMap<_, Fetch<_>>` for the strip. |
| 02-147 | One `forward_flights` list; no special first slot. |
| 02-149, 03-041 | `WorkerLink::handoff` (`Idle`, `Handed`, `Awaiting`) replaces `pending` + `asked`. The cancel flag and the ended channel stay: they are the signals shared with the worker thread, and `Progress.stage` already is the run's stage enum. `Option<Export>` means "an export object exists", not a stage. |
| 02-143 | `send_snapshot`, `send_animation`, `send_sticker`, the album and the paid-media send have `_to(place)` forms; the old names read the place at the click. `open_chat`/`open_topic` stay: they are the main window's selection, not a stage. |
| 02-192, 04-032 | `RichHtmlStage` (`Running`, `Done(outcome)`) in the shared slot. |
| 02-169 | One `EventLog` per chat (query, filter, users, fetch, waits). |
| 02-170 | One `MemberAsk` per list (first request, query, `MoreMembers`). |
| 02-165, 05-049, 08-020 | `BotText`, `BotSave` per field; `BotUsernameCheck` and `CreateStage` (the submission rides in the stage). The 150 ms poll task is gone: `drain_bot_create` runs from the existing poll drains. |
| 02-163, 04-055 | `TrustFile` (`NotLoaded`, `Clean`, `Dirty`, `SaveFailed`); a failed write is shown. |
| 02-161, 05-015, 12-026 | `StarPacksState::options` is a `Fetch<Vec<StarPack>>`; `open` stays a view flag. |

### Sweep fixes (is-09)

Forty-one implicit-state entries. Each thing that moves through stages is now
one enum (or `Fetch<T>`) whose variants carry the data, with a way out of
every stage. Fields that were gone before the sweep reached them are listed
as "already fixed".

| Id | Result |
| --- | --- |
| 05-048 | `game_scores` holds a `Fetch<Vec<GameHighScore>>` per open panel; a refused request removes the panel. |
| 05-052 | `ActiveGroupCall::screen_share` is the `GroupScreenShare` from is-04 (the answer lives in the stage that waits for it), next to `GroupTransport`. is-09's own `ScreenShare` enum, `join_payload`, `transport_ready` and `transport_error` were dropped in the merge. |
| 05-050 | `CallsState::progress` is `CallProgress { Idle, Answered, HangingUp(HangUp) }`; a failed hang-up goes back to what it replaced (`HangUpFrom`). |
| 05-051 | `recent_calls_stage` is `RecentCalls { Idle, Loading(request), Failed, Clearing }`. A reopen during "Clear all" waits for the answer. Privacy was already one `PrivacyAsk` per rule. |
| 05-059 | `folder_limit_hits` is a queue (two hits between polls both show). `folder_link_saved` and `folder_share_error` were already gone (`DialogAnswers`). |
| 05-058 | Already fixed: the add-folder dialog is `InviteStage` plus `DialogAnswers`. |
| 05-064 | `main_paging` and `archive_paging` are `ListPaging { More, Retrying, Exhausted }`. |
| 05-057 | `archive_settings` is a `Fetch`; "loading only when no copy is held" is `begin` on an empty fetch. The box-open flag stays (a box, not a stage). |
| 05-068 | Already gone: `chat_look_oks` is not in the tree. |
| 05-065 | `ChatsState::deep_link` is `Option<DeepLinkFlow { stage, from_agent }>`; the lookup's link text is inside `ResolvingInfo { original }`. The UI passes the origin to `continue_deep_link`. |
| 05-074 | Skipped: `notification_sends` and `new_community_photo` are keyed by the request that asked and removed on ok, error, lost request and failed send; `outcomes` is a queue the UI drains. |
| 05-081, 05-094 | The pending boost and its slots live in the boost box phase (`BoostWork::Slots`, `SlotsReady`, from is-01). is-09's `groups.boost_chain` was dropped in the merge. |
| 05-076 | Already fixed: `BoostsListState::stage`. It is now the shared `PageStage`. |
| 05-077, 10-144 | Invite-link members and join requests use `PageStage { Loading(request), Ready, Failed }`. |
| 05-090 | `PublicLinkStage::RevokingFailed` replaces the `failed` bool. |
| 05-089 | Already fixed: `UsernameCheck` is an enum; `check_generation` is a counter. |
| 05-093 | `member_asks` (`MemberAsk { first, query, more: MoreMembers }`, from is-04) replaces the queries, next-page bases and failed marks; a new first page resets the next page. is-09's `MemberPaging` was dropped in the merge. |
| 05-098 | Skipped: the stop stage is `BotStopStage`; `can_stop` is TDLib's capability flag. |
| 05-101 | `message_link` is one `MessageLinkAnswer { link, is_public }`. |
| 05-102 | `instant_view_requests` is one table of `InstantViewAsk { url, from_agent }` by request. |
| 05-099 | `forward_flights` is one list; `last_forward` is the outcome the UI takes. |
| 05-100 | `sponsored_report` is `SponsoredReportFlow { Idle, Asking, Picking, Done }`. |
| 05-121 | `PageLoad` for resale and craft candidate pages; `CraftRun { Idle, Crafting, Done }`; `BuyPhase` for the purchase. The auction was already `AuctionFetch`. |
| 05-125 | `GramsHub::load` is a `PageLoad`. |
| 05-111 | `checkout` is `CheckoutForm { Closed, Loading, Ready, Verifying, Paid }`. The order info was already an enum. |
| 05-112 | `star_subscriptions` is `StarSubscriptionsList { data, load, edit }`; an edit landing while a page is out makes that page's answer stale. |
| 05-149 | `recommended` is a `Fetch`. |
| 05-148, 05-232 | `date_jump` is `DateJumpAnswer { Idle, Resolved, NotFound }` (jump and note). The pending jump stays on the per-window view. |
| 05-159, 10-153 | Already fixed: `HistoryCalendar` pages with `CalendarPaging`. |
| 05-208 | `SearchState::surface` is `SearchSurface { Closed, Recents, Query }`. `SearchState::public_limits_exceeded` stays a bool: it is TDLib's quota fact from the `searchPublicPosts` answer, independent of any stage. |
| 05-206, 05-230 | The in-chat search is open exactly when its status is not `Closed` (`is_open`); the page and jump stages were already enums. |
| 05-207, 06-031, 09-X022 | `TopicHistory::older` gains `Complete` and `newer` is `NewerEnd { AtLatest, More, Failed }`; `ThreadsState::topic_load_failed` was already gone. |
| 05-145 | `time_zones` is a `Fetch`; the Business extra state was already `Fetch`. |
| 05-146 | Quick reply messages are a `Fetch` per shortcut; `shortcuts_load` is a `Fetch<()>`. A failed load waits for the dialog to open again. |

- **Also converted** (baseline fields in the files touched): `ResaleBuy`,
  `InvoiceWatch`, `ThreadWindow`.
- **Behaviour changes with a reason.** A page answer for an older query is
  dropped by the `MemberAsk` of its list. A failed first page of a gift
  list, Grams or the star subscriptions is `Failed` and is not asked again by
  render. `fetch_call_history` refuses while "Clear all" is out.
- **Baseline.** 221 lines at the start; 162 after merging with is-01 and is-04.

### Sweep fixes (is-19)

Thirty-five implicit-state entries; each stage that was a set of bools and
options is now one value. Behaviour is the same unless said below.

- **Boxes and windows.** Two-step box: `TwoFaBox { Closed, Open { view,
  confirm, notice } }`; moving to a view or closing replaces the whole
  value (13-169). What's New window: `WhatsNewWindow { Closed, Opening,
  Open }` (14-045). Pump scan: `PumpScan { Fresh, Scanned { .. } }` (13-168).
  Wallpaper preview: the `bg/` link lookup is a `PreviewStage::Looking`
  carrying its request; the preview reads its answer or failure from the
  session once and owns the failure line, instead of reading the
  session-wide `background_error` on every draw (14-022).
- **Mini apps.** One `LaunchFlow { Idle, Searching, FetchingAttachBot,
  Adding, Confirming }` holds the launch's wait and its box (14-039); a
  newer launch replaces an older wait, and an answer for another bot is
  dropped. The window has `HelperState`, `WriteAccess` and `DataSend`
  (14-038); `closing_confirmation` is the app's own setting and is in the
  allow list. `WriteAccess { Idle, Checking(req), Asking, Allowing(req) }`
  holds the request each wait is for, and the session's answer now carries
  its `RequestId` (`WriteAnswer`), so only the answer to that request moves
  the window; a refused or failed send leaves `Idle`. `DataSend` is gone:
  the window closes in the same handler that sends the data, so the window
  itself is the once-only guard. The emoji-status lookup is matched by the request id it sent,
  not by the purpose, so another batch's lookup no longer keeps it alive
  (14-036). The invoice watch is `Attempt { None, Sending, Failed, Pending }`
  (14-037).
- **Video editor.** `EditorStage { Reading, Unreadable, Ready, Encoding }`
  holds the info, the strip's selection, the player, the note and the serial
  of the probe or encode in flight (14-005). The strip drag is
  `StripGesture { Idle, Held { grab, shift, seek } }` and the cover bubble is
  `CoverBubble` (14-006); with reduced motion the bubble now shows at full
  strength at once (`Shown`, no ease-in). `EditorStage::reading()` returns
  the serial with the stage.
- **Players.** The live feed's off-screen policy is `Sight { InView, Unseen,
  Quiet }` (14-016). `ControlsFade` has `Mode { Window, Full { shown_until }
  }` (14-017). The PiP window has `Gesture { Idle { dragged }, Pressed,
  Dragging }`, `LineDrag` and `FeedLeft` (14-019). A player's life is
  `Life { Awake { awaiting }, Failed, Asleep }` (14-027); the loop's seek is a
  `Stage::Landing(state)` of the transport machine itself (14-029), so a
  sleep, pause or failure replaces it and nothing resets it by hand; a
  swapped-in PiP player is told about a left feed because the told marker
  is the player it was told for, not a flag (`told_for`, 14-019). The cover avatar is `Cover { None, Playing, Failed }`
  and no longer retries a failed file on every frame (14-015).
- **Viewer.** Handing the clip to Picture-in-Picture clears the OS Now
  Playing entry when no audio is playing, since the viewer's tick goes with
  the player. `ViewerClip { None, Playing, Gif, Failed }` holds the item, the
  player, the media-session tick (a stored task, so it ends with the clip)
  and the error (14-031); the dead `video_path` is gone, and `video_layer`
  stays a separate drawing entity reused across clips. A photo turn is
  `Reorient { Idle, Decoding, Ready }` with a token (14-032). Controls are
  `ControlsVisibility { Shown { idle_task }, Hidden }` (14-030). The photo
  editor and its deferred open are one `PhotoEditorSlot` (14-033).
- **Caches and ticks.** The wallpaper render board is one map of
  `Stage { Queued, Running, Done }` by key (14-025). The update pill is
  `Fill` and `Phase` (14-009); a vanishing message is `GhostStage { Waiting,
  Crumbling }` (14-014).
- **Core.** The install receipt records `Outcome`, and old receipts with
  `failed` still read (14-067). `Extraction` is one value under one lock for
  a cancelable ffmpeg run, shared by the worker and the UI that cancels it
  (14-066); the round-video capture is `Capture { Recording, Stopped,
  Finished }` (14-065). The decode clock is `Run { Paused, HeldAfterSeek,
  Running }` (14-064); the decode thread's flags are `Decoding { Running,
  Seeking, Ended, Failed, Stopped }`, so a seek on a failed clip tries the
  file again (14-062); the player's picture stage is `Presenting`
  (14-063, 14-072). The voice pause handshake is one atomic with a `Failed`
  stage (14-070); the encoder thread answers a pause with `Flushed` even
  when a resume and a new pause both fit inside one receive timeout and a capture is `Source { Live, Fixture, Closed }`
  (14-071). A passkey ceremony is `CeremonyState`, CTAPHID reassembly is
  `Assembly { Idle, Reading }` and the Windows cancellation is
  `Cancellation { Idle, InFlight, Cancelled }` (14-073, 14-074, 14-075).
- **Files outside the bundle.** The sticker player's `Cancel` and the WebM
  decode take the new `video::Extraction` (14-066).

### Sweep fixes (is-18)

Implicit state in the settings, sign-in, stickers, stories, suggested-posts,
wallet and Touch Bar files: a stage kept as bools, flags beside an `Option`
or a side list becomes one value whose variants carry their data.

| Id | Fix |
| --- | --- |
| 12-109, 13-082 | `SettingsUi::boxes: SettingsBox { None, LogOut, Stickers }` replaces `log_out_confirm` and `sticker_settings_open`; closing one box never closes the other. The Account box's `confirm_delete` / `confirm_logout` pair is `AccountConfirm { None, Delete, LogOut }`. |
| 12-108 | `KeybindingUi { capture: Capture { Idle, Waiting(action) }, refusals: Refusals, keymap: Keymap }` replaces `keybinding_capture`, `keybinding_error`, `keybindings_applied`. `Keymap { Stale, Applied(AccountKey) }` names the account whose overrides the keymap carries, so an account change makes it stale with no reset by hand. Refusals are kept per row: another row's capture or reset leaves a message alone, and only the row's own try, reset or success ends it; `SystemAccent { Unread, Read(Option<u32>) }` replaces `system_accent` + `system_accent_probed`. |
| 13-079 | `Arrival::stage: Stage { Seeking, Washing { landed, centered_at } }`. |
| 13-080 | `SettingsSearch::stage: Stage { Live { selected }, Finished }` replaces `pending_clear`, `selected` and the `query` mirror: the query is read from the field, the highlighted row lives with the search it belongs to, and a result opening clears the field at once (it has the window). Only closing Settings (no window) leaves `Finished` for the next render. |
| 12-110 | `ChatListState::archive_settings: Fetch<ArchiveChatListSettings>` replaces the loading bool and the `Option`; a refused fetch now ends `Failed` (it used to leave the box "still loading" in `None`). The open flag stays in the session: it is the registry's single predicate and `Farewell` closes it. |
| 13-081 | `AgentPageUi::picker` holds its fingerprint (`BuiltPicker`). `focus_chat` is the card's chat and `scroll_to` a one-shot scroll request: independent. |
| 12-112 | Skipped: `reset_confirm` is the dialog registry's one open predicate (`AgentResetToken`), not a mirror of anything. |
| 13-083 | `Mount { Mounting, Shown }` (`ui/mount.rs`) lives in the stage it belongs to: `ForwardStage::Picking` and `ReplyChooser` start `Mounting` when they open (nothing sets it by hand) and `take_first_draw` hands the focus request out once. The contact box keeps its own `Mount`, made with the box it belongs to; `link_text` and `forward_result` are single values with their own exits (`esc_stack`, chat change), not two fields in step. |
| 13-063 | Skipped: already `chat_list.tab: SidebarTab`. |
| 13-054 | `SidebarReveal::mode: Mode { Docked, Hidden(Peek) }`: the pointer's peek exists only while hidden. |
| 13-047, 13-048 | `ui/signin_clock.rs`: `StepClock { None, Code, Email }`, `ErrorClock { None, Seen { banner } }` (a different error shows its banned box again), `Ticker { Idle, Armed(task) }`. `PhoneStep` already carried the phone stages; `picker_open`, `touched` and `hint` are independent. |
| 13-059 | `ComposerSpelling { underlines: Underlines { found, text }, check: Check { Idle, Checking(task) } }`: ranges and the text they refer to are one value. The check returns to `Idle` when its answer lands. |
| 13-035 | `StartFiles { Idle, Choosing { files } }`; closing hands the files back, and `end_start_files` is the one place that also clears the search. |
| 13-038 | `Watch { Healthy { kept }, Recovered { kept } }` replaces the `Cell<bool>` and the kept-window slot; `recover()` takes the window and moves to `Recovered` in one call. `Recovery` is only the decision function's input, read off the watch. |
| 13-025 | `ZoomedDay::fetch: Fetch<Rc<StatChart>>`. |
| 13-043 | `RangeSelector` and `PieZoom` own their subscription. |
| 13-031 | Skipped: the lists already use `ForwardsPage` (matched to a `RequestId`). |
| 13-014 | `UiClip::opening: Opening { Pending, Ready(timeline), Failed }`; the sticker pump and closer are `Upkeep { Idle, Running(task) }` (set through `try_borrow_mut`, as the Touch Bar's `SyncStage` is). |
| 13-004 | `SetConfirm { None, Rename, DeleteSticker(item), DeleteSet, RemoveSet }` and `PickWait { None, Downloading(pending), Decoding(pick, file) }` replace six fields. `reordering`, `create`, `encoding` and `pick` stay: separate flows. |
| 13-011 | `StickerPanel::installed_loaded()` reads `lists.installed`; the masks panel has `installed_fetch` / `archived_fetch` (`Fetch<()>`) instead of `installed_loaded`, `archived_loaded`, `failed`; `ManagerView` derives a `ListStatus`. The masks strip reads the same fetch (the twin screen). |
| 13-017 | `StickerPanel::greeting: Fetch<Vec<StickerItem>>`: marked when answered, asked only from `Idle`, `Failed` when refused or not sent. |
| 13-018 | Skipped: the picker's status is already per-tab `Fetch` plus `StickerSetStage`; `search_has_more` is a paging cursor. |
| 13-147 | `StripPull { Collapsed, Revealing, Expanded }`. `context` is a last-seen token and `bounce` the kit's output cell. |
| 13-120, 13-149 | Skipped: `CoverStage` and the `RequestId` of the call are already separate things on purpose: the call outlives the panel (`close_other_panels`), so its answer still settles. |
| 13-137 | `ComposerMode { New, Repost, Edit }` replaces `edit_target` / `repost_source`; the press stage was already `ComposerStage`. |
| 13-134 | Skipped: already `AlbumView` / `AlbumDelete`. |
| 13-102 | Skipped: `extras_requested` is gone. |
| 13-095, 13-094 | `AfterStars` in the Stars box; `LiveJoin { Idle, Joining, Joined, Refused }`. |
| 13-098 | `ReplyRow { Closed, Typing, Panel }`: the panel exists only in an open row. |
| 13-109 | `SuggestedPostsUi` is a base box (price, accept or decline) and one box over it (time or Grams Needed). |
| 13-110 | `SuggestBar::kind: BarKind { New, Edit { of, send } }`: only an edit has a message and a send. A Change-mode price box whose bar is gone applies nothing (it used to make a New bar). |
| 13-118 | `SyntheticKind::{ImageLoading, ImageLoaded}`. |
| 13-116 | `Showing::timer: Timer { Running, Paused }` and `ToastQueue::pointer: Pointer { Away, Over }`. |
| 13-114 | `MoneyPage::view: MoneyView { Overview, Lookup, Tx, Nft }`; `Paged` carries a `Fetch<()>`. `hint_open` is an overlay over the view, kept. |
| 13-119, 13-154, 13-155 | `PickerFill { Unfilled, Shown, Waiting }`, `SyncStage { Idle, Scheduled(task) }`, `SlideRun { Still, Since }`. The pinned panel's `touch` is already a `PinTouch` and its `timer` handle is its own armed state. macOS only: not built in the Linux gate. |
| 13-161 | `close_after_choose` is dropped: a chooser with no box under it (no source) closes after a pick. |

### Sweep fixes (is-06)

Thirty-seven implicit-state entries. Each struct was read with every reader
and writer of its fields; a field pair that formed one stage became one enum
whose variants carry the data, with a way out of every stage.

| ids | what |
| --- | --- |
| 03-017, 06-013 | `ThreadStatus` is the whole open sequence: `Resolving`, `SwitchingChat`, `Starting`, `LoadingHistory(request)`, `Ready`, `ReloadNeeded`, `Failed`. `needs_chat_switch`, `reading_started` and `reload_needed` are gone. The resolve answer is still matched by the thread's origin chat and message. `LoadingHistory` is entered only once the first page's send returned its id; a failed send ends the thread `Failed` (drawn with its retry), so no spinner is left with nothing in flight. |
| 03-006, 07-009 | `Paged<T>` is `Idle`, `Loading { request, append, shown }`, `Ready`, `Failed { error, shown }`. A page lands only when it carries the request the list waits for; `TonWalletPurpose::{Transactions, Nfts}` lost `append`. A request that cannot be sent leaves the list `Failed` with the rows it held (`Paged::send_failed`, called by `fetch_wallet_history` and `fetch_wallet_nfts`). |
| 07-008 | `MoneyPage.detail` is `MoneyDetail { Overview, Tx { tx, hint }, Nft, Lookup { request }, LookupFailed }`; `selected_tx`, `selected_nft`, `tx_lookup` and `hint_open` are gone. A tab switch returns to the overview. |
| 03-015 | `ConnectDriver.handshake` is gone: `parameters_sent()` reads the session (the `SetParameters` request is pending, or the authorization left `WaitTdlibParameters`), so nothing sits beside `session.auth`. |
| 03-016 | `Debounce<T>` owns the token counter and the value. The typed searches and the draft save (`HeldDraft::{Quiet, Kept}`) use it; the draft clock comes from the stage. |
| 03-003 (part), 06-025, 14-034 | `WebApps.grossing` is `Fetch<Vec<i64>>`; a failed list says so on the Apps tab and the next open of the tab asks again. |
| 03-018, 09-X020 | `storage_stats` is a `Fetch<StorageStats>`; a failed one is drawn with its error and a Retry instead of "No storage data yet". The seeds of the auto-download presets and of the cache limits are `Seed::{Unseeded, InFlight, Seeded}`, stored as the bool the account folder always held. `auto_download_presets_loading` and `storage_limits_seed_sent` are gone. |
| 09-043 | `StorageScroll` is `Idle`, `Requested`, `WaitingSettled`, `Placed`. |
| 03-037 | `DiskStage` is `Healthy`, `Low`, `Doubt`, `Full`, `FullHeld`; `confirmed` is a method of it. `screen_refused` and `start_refused` stay, for a reason: they are not stages of the disk but records of two different clients that were refused (the on-screen one, the background accounts), they can be set together and in any disk stage, and each leaves by its own retry (`reconnect_after_disk`, `RetryStarts`). Folding them into the stage would need a variant per combination. |
| 03-031, 03-030, 03-021, 03-022, 03-029 | `EarnLimits.app_config` is a `Fetch<()>` (a failed one is asked by the next panel to open, never by the per-frame upkeep: `forget_failed_app_config`); `Load::Unavailable` replaces `ton_unavailable`; `WithdrawStep::{Password, PasswordSending(request)}` (an answer is taken only by the step that sent it; the password step is entered once the send went out); the parked join box lives in `Recipients::Loading`; `EarnState.effects` carries `SetupDone` to the UI. |
| 03-032 | `SetMutation` (settled by its own request), `StatusPicker` (profile or chat appearance) and `StatusChange` (the `setEmojiStatus` in flight). `trending_stale` is gone: an update of the shown trending tab leaves `lists.trending` `Idle`, and the refresh asks from that. |
| 03-036 | `EmojiAnimations` holds one `Pack` stage; the messages that wait live only in the stages that still expect a set, and a failed load drops them. |
| 03-038 | The startup read (`Startup`) and "Install from file" (`FileInstall`) are stages; `started`, `file_task` and `file_error` are gone. |
| 03-051, 10-024 | `SendGiftState.stage` is `Idle`, `Sending(request)`, `Sent`, `Failed`; the stage is entered once the request is out. A failed gift list is `GiftListFetch::Failed(line)`, not a `SendStage`, so it can no longer overwrite a send in flight. |
| 03-052, 10-016 | `GiftManageState.events` replaces the one-shot `deleted`, `transferred` and `export_url`; `CollectionsFetch` replaces `collections_loading` and `collections_stale`. |
| 05-117 | `GiftMutation::Busy(request)` replaces `gift_mutating` and is freed by that request (or by an answer that names none); `export_unlocked` is `ExportSend::Unlocked`. |
| 13-124, 13-096, 13-148 | `StoryViewerUi.panel` is one `ViewerPanel`: none, reaction picker, reply row (with its media panel), viewers, stats, report, share, close friends (its stage inside), privacy editor, cover picker. A step to another story keeps only the close-friends editor. |
| 13-126, 13-097 | `StoryTick` owns the 100 ms task; the stealth box owns its tick. |

- **Skipped.** 03-009 and 06-018: the insights forwards and the story search
  were already `ForwardsPage` and `PageFetch`. 13-150: the statistics
  panel's forwards are that same `ForwardsPage`; `Asked` is a scroll
  throttle. 07-010: the three wallet requests were already `Fetch<()>`.
  03-020: `JoinStage` and `BoxSend` already carry the request.
- **Left as they are, on purpose.** `menu_open` mirrors the kit menu's own
  visibility and can be up over any panel. `cover_request` and
  `privacy_request` are two calls that can be out together and outlive their
  panel. 03-003: `MiniAppWindow.data_sent` (a bool beside the stage) is
  deleted, since the window closes in the same step that sends. The session
  slots `open_result`, `write_access`, `emoji_access`, `data_sent`,
  `attachment_menu_toggled` and `found` are answer mailboxes, not stages:
  the flow that asked already has its own stage (`awaiting_search`,
  `PhoneFlow`, the window), and the slot only carries the reducer's answer
  across to it, keyed by the bot or request that asked and taken once.
  `phone_share` carries its `RequestId`, and `poll_phone` takes it only when
  it equals the `PhoneFlow::Sending` request; `finish_phone` clears it so a
  timed-out send's late answer is nobody's. `WebApps.pending` is a list of
  independent launches. 03-032: `EmojiPanel.open` is the box's visibility,
  and `tab` is also read by other consumers while it is closed (the profile
  photo sources), so the two are not one stage; `status_note` is deleted: six paths wrote
  it and nothing read it. 03-037: see the row above. 03-038:
  `SetState` is a pure function of `SetStage`, the active set (a setting)
  and the sets on disk (an inventory); it is computed per draw and never
  stored, so there is no copy to keep in step.
- **Behaviour that changed.** Opening a story panel replaces the open reply
  row and reaction picker too (they used to stack with the other panels).
  A failed apps list on the Apps tab shows "Couldn't load apps".

### Sweep is-11, merged with the integration branch

- **Integration forms kept** where bundles converted the same state: the call
  `ready` stage and its parameters, `ProxyFetch`/`ProxyMutation`, the forum
  topic map, the GIF panel lists. This bundle's versions of those were
  dropped, with the call and proxy files that only followed them.
- **Kept from this bundle:** the envelope accessors (`LocalFileState`,
  `ParsedChatMember`, `StickerItem::premium`, `ChatDraft`), the reply target
  enum, the share search, the sponsored report, the reply keyboard fetch,
  the receive bridge stop report.
- **A refused refetch of a stale proxy list** lands in
  `ProxyFetch::FailedStale`: the tick does not ask again, the list stays
  busy (never planned on), and Refresh is the way out.

### Sweep fixes (r2-26)

Round 2, files `earn_state` to `fragment_password` (ids are the register-r2
ids `09-NNN`).

| id | result |
| --- | --- |
| 09-081 | Fixed by the integration's `EarnLayer { Closed, Open { back, restore, fade } }` (`earn/layer.rs`, the fade stage owns its timer); this branch's second version is gone. |
| 09-082 | Fixed: `EarnUi::learn: Option<LearnBox { diamonds }>`; closing the box drops the count. |
| 09-083 | Fixed: `MediaEdit::image: EditImageFetch::Downloading { file_id }` lives with the edit; `edit_image_upkeep` opens the editor when the photo lands or says it failed. The stage keeps no `RequestId`: the download tracker owns the request and reads the end per file (`downloads.ended_without_file`), because `downloadFile` answers at once and can share a transfer already under way. `stage-enums-allow.txt` says so. |
| 09-084 | Fixed: `BarOwner { Free, Suggest, Word }` on the one `EmojiBarUi`; `restart` for a new list, `release` by its own owner only (no closer repeats the "unless suggestions use it" rule). |
| 09-085 | Fixed: a failed installed-sets list is asked again from a new list opening (`retry_suggestion_emoji_sets`), not from every rebuild. |
| 09-086, 09-087 | Fixed: `SetsView { tab, typing: Debounce }` replaces `emoji_set_return_tab` and `emoji_set_debounce`; the integration's `Debounce` (the timer lives in the value) is kept and the tab lives beside it in the one value, so opening, closing or choosing a tab drops a search that has not gone out. |
| 09-088, 09-089 | Fixed: `Sheet::Loading { _decode }` holds the decode, which retries a failed read after 1 s and 2 s; `Failed` after six reads (1, 2, 4, 8, 16 s: a disk that stalls for half a minute still gets its sheet); the atlas going away cancels it. A sheet that is corrupt stays on the system glyphs until the set is installed again. |
| 09-090 | Fixed: no data folder puts `SetStage::Failed` on the row. |
| 09-091 | Fixed: `Startup::Reading` is set only when the task exists; `Done` comes from the task. |
| 09-092 | `open` skipped: it is the box's presence for `DialogSpec` (one source, nothing mirrors it). `Switching { id, task }` now carries its set and the answer is applied only while it is the stage in place. |
| 09-093 | Fixed (review round): `WordBar::Asking { request: RequestId }` is settled only by that request. `KeywordLookups::outcome(id)` says how it ended (the last 64 are kept): answered opens the bar; failed follows the retry out for the word, asks once more from the transition (`Settle::Again`), or gives up with the note. A later lookup of the same word, a different request, cannot open the bar. "Suggest Emoji" opens at once only for an answer that was already cached, and notes the failure when the lookup was turned away. |
| 09-094 | Fixed by the integration's `RightPress` (a link, a task or a poll option, with its clearing `Task`); this branch's `RightClickedLink` is gone. |
| 09-101 | Fixed: `FolderPick::Choosing` holds the picker task inside the draft. |
| 09-102 | Fixed: closing a draft never hides a running export; asking for the box drops a draft over a run. |
| 09-103 | Fixed: `QuitGate { Open, Asking { exit } }` ends with the exports as well as the alert (`quit_gate_upkeep`). |
| 09-104, 09-105 | Fixed: `FirstRun { Writing { _write }, Playing, Leaving, Notice, .. }`; a replay or skip drops the write task. |
| 09-106 | Fixed: `JoinStage { Choosing, Joining { request } }`; the box closes on the offer going, or returns to choosing with the ticks kept. |
| 09-107 | Fixed (rebase round): the integration's `ChatOverlay` is the one value of what floats over the window, held in `MessageUi::menu` (a chat window has its own): `RowMenu`, `ArchiveMenu`, `FolderTab`, `Message(OpenMenu)`, and the long press and peek. Opening any of them replaces whatever floated before, so no opener closes the others by hand (`FolderUi::tab_menu` and the separate message menu slot are gone); `close_menu` closes any menu and leaves a peek, `close_message` only a message's. Left as is: a message menu replaced by another menu no longer resets the session's reaction options at once (they are keyed by chat and message, and the next message menu asks for its own). |
| 09-119 | Fixed by the integration's `ChatListView { filter, folder }` (written only by `ChatListUi::show_view`, which ends the selection and pinned drag); this branch's enum is gone. |
| 09-117 | Fixed: the success note of a topic action is shown from the answer (`TopicAction::Waiting`, `Session::topic_action_answered`). |
| 09-120 | Fixed by the integration's `ForwardStage::Picking.share_at` (with `ticks`); this branch's `at_time` is gone. |
| 09-123 | Allowed: `DestRow::saved` is a per-row fact. |
| 09-121 | Fixed: a lifted message drag is read from the active drag's payload (`App::active_drag_is`, a one-method addition to the vendored gpui). |
| 09-124, 09-125 | Fixed: `PasswordView { Idle, Sending, Failed }`; the field's own `FieldStage` is `Away`/`Typing` (a try out or the box off screen both mean the keyboard is taken again when the box is idle; `Disabled` was the same as `Away`). |

Rebase round (r2-26): merged `sweep-r2-38` (the follow-up with r2-31 and
r2-38) and `sweep-r2-41`. Where both sides fixed an entry the integration's
shape stayed (`EarnLayer`, `ChatListView`, `RightPress`, `Picking.share_at`,
`Debounce`, `SuggestStage::Live`, the `ReplyBar`/`ReplyPanel` split, the
`PasswordView` caller's stage kept); this branch's fixes the integration had
nothing for were rebuilt on its shapes: the word bar owner on the suggest
stage (`BarOwner`, restarted when rows appear, released when they go), the
chat list's menu with the folder tab's and the message's in `ChatOverlay`,
`LearnBox`, `SetsView` around `Debounce`, and `PasswordView` for the grams
withdraw password. `emoji_access` is a `Vec` of `EmojiAccessAnswer` (r2-41's
oldest-first list with the follow-up's answer type).

Review round (r2-26): merged `sweep-r2-followup` (its `SavedToastStage`,
`TrendingSets`/`Fetch` list stages and `FolderShare` shapes kept; the sets
panel reads `lists.{installed, trending, search}` loading itself, as the
guard asks). Also: `process_folder_new_chats` hands back a request already in
flight for the folder instead of `Ok(None)`, so a second Join waits on it and
closes or returns to choosing with it (nit, no silent dead end); register
02-052 (`fetch_folder_share` sends both halves independently) was already
fixed on the integration branch. Fresh review of every file in the bundle:
no new findings (the remaining `detach` calls end finished tasks the stage
had moved past, `later`, `pending` and `skippable` are facts of the thing
shown).

Also: the emoji style download no longer polls (the open row asks the frame
clock for frames), the sets and open-link boxes use `Cell`, the sprite and
preview cells use `try_borrow`.

### Sweep fixes (r2-31)

Twenty-eight re-sweep entries in twenty files. A file in the bundle was read again
from the top after its entries; what that found is in the `new-` rows.

| ids | what |
| --- | --- |
| 12-C006 | The chooser's window is a stage of `Chooser`: `Opening { sheet, _open: Task }` (the task makes the window and moves the stage on) and `Open { sheet, window }`. A chooser closed while its window is made drops the task, so no window is made for it; `OpenChooser.window: Option` is gone. |
| 12-C007 | `ScreenShareUi.demo_active` is `Option<DemoShare { source, with_audio }>`; `demo_active_audio` is gone. |
| 11-035 | The timer picker closes through its one closer (`close_ttl_picker`, the integration branch's) only once the send went out: a refused send keeps the picker, so the pick can be retried, as the sound and mute pickers do. |
| 12-D005 | The two-step buttons that send (reset, recovery code, resend, the confirm box's yes) say "request not sent" when the driver refuses, and the confirm box stays open on a refusal. The screens without a form draw the notice line too. |
| 12-D006 | `WindowSelected.messages` is a map by document order: a text that holds none of the selection lets go, and a read with no selection resets the record. The one message the copy names is the first text in document order that holds the selection. |
| 12-D007 | `SELECTED`, `OWNERS` (selectable text) and the send-fly tables are `Cell`s taken and put back (`with_taken`); `SelectionFx` and the history row bands use `try_borrow`. `selection_bar.rs:25` is not in this bundle. |
| 12-D009 | `DragSelect.pending` is gone: the release reads the message's send stage (`message_still_sending`), and a message that is gone counts as not selectable. `selection_press_row` and `selection_row` lost the `pending` parameter. |
| 12-D010 | "Save Selected" has a `SaveSelection { Idle, Picking(Task) }` stage in `ChatUi`: one picker at a time, dropped with the chat. A refused download ask says so. |
| 12-D011 (and 02-008) | `ChatUi.pending_delete` is gone: `confirm_delete` takes the `DeleteConfirm` the box captured. |
| 12-D012 | `delete_selection` keeps the selection when the send was refused. A refused report or "delete all" stops the delete and keeps the box (the integration branch's `refused` form). |
| 12-D016 | The flight is one `Flight { Idle, Waiting(Fly), Flying { fly, started } }`; `Fly.started` is gone. It ends with its own landing, its window closing, the account leaving the screen (`end_account_surfaces`) and the lock (`close_for_lock`). A refused sticker send forgets the noted cell. |
| 12-D017 | `SystemAccent` is `Unread`, `Reading { color, _wait: Task }`, `Read(color)`; `system_accent_poll` is gone. |
| 12-D018 (and 14-003) | The banner's dismissal is `ChangelogBanner::{Shown, Dismissed}` inside `UpdateState::Installed`, as the integration branch has it; this branch's `update_banner_dismissed_for` was dropped in the rebase. |
| 13-005 | The ticked destinations live in the box's stage (`ForwardStage::Picking { ticks }`, beside the integration branch's `share_at`), so every way out of the box drops them; `ShareUi.selection` is gone. What was typed in the box goes once neither it nor the reply chooser (which types in the same search field) is open (`sync_share_inputs`, per window and frame), and a box that opens starts with nothing typed. |
| 13-006 | The forward result banner is the source chat's (`ChatUi.forward_result`): it is set only where the window shows that chat, and leaving the chat takes it along. A window on another chat gets the status note only. `ShareUi.forward_result` is gone. |
| 13-013 | `MediaMenuFacts.downloading` and `.local` are one `MenuFile { Remote, Downloading, OnDisk }`. |
| 13-017 | The gallery's selection becomes the forward through `ForwardStage::begin_selection` (a box open for an earlier forward goes with it, ticks included), then `open_forward_picker`, which closes the reply chooser (`ChatUi`, as the integration branch has it). |
| 13-016 | What floats over the chat list is one `ChatOverlay { None, RowMenu, ArchiveMenu, Pressing, Peek }`: the integration branch's `ChatListMenu` (a row's menu or the archive row's) and `PeekPreview` folded into one value, so opening a menu ends a peek and the other way round, and a close ends only its own (`close_menu`, `close_peek`). |
| 13-018 | The Shift+click range anchor is `ForwardDraft.range_anchor`: it ends with the selection, so a menu "Select" no longer starts from an earlier selection's row. `ChatUi.selection_anchor` is gone. |
| 13-012 | The shortcuts page reads the keymap with `try_borrow`. |
| 13-007 | `PhoneLookups` is gone. The IP default is `AuthState.guessed_country: Fetch<String>` (asked once, a refusal or an error leaves it `Failed`, matched by its request); the list was a `Fetch` already. |
| 13-008 | `PhoneStep::Sending` holds the `RequestId` of `submit_phone`; its answer is read from `session.acks` as the integration branch has it. |
| 13-014 | A refused profile-list send leaves the list `Failed` (it was removed, so the strip asked again every render). |
| 04-061 | A cancelled download is `DictDownload::Cancelling { _task }`: the task goes on to see the worker out and read the dictionary folder, so a cancel that came too late to stop the install still shows it. |
| 13-021 | `QUIT_ASKED` is `QuitStage { Normal, Asked { at } }`: refused by the app (`note_quit_refused`), or over after `QUIT_GRACE`. `QUIT` and `GPU_GONE` are `Cell`s. |
| 13-020 | A statistics refresh that cannot go ends the panel `Failed` (it stayed `Loaded` in the old theme, so the poll asked every pass); `drain_statistics_theme` reports progress only when a request went out. |
| 13-025 | The export waits for a poll post's votes chart while it is loading or never asked, and for a post not looked up yet. A failed chart is exported without it. |
| 13-023 | A refused `getStatisticalGraph` leaves the graph `Failed` (Retry), not absent (`graph_unsent(extra, message)`, as the integration branch has it). |
| new-01 | The three timers in these files that the owned-timers baseline held are explained in `owned-timers-allow.txt` (the self-destruct badge clock, the held press's pointer, the dictionary progress counter): each has no event, and its task is its stage's. The self-destruct task is let go of after its update, not detached. |

- **Not changed, on purpose.** `share_state.rs`'s `link_text` is the link chooser's
  one value (the text and the box are the same thing), closed by its own exits.
  `search_share_chats`, `refresh_message_menu_actions` and `open_similar_chat`
  are sent and not waited for: nothing holds a stage for their answers, and a
  refusal changes nothing the user sees.
- **Rebase onto the integration branch (r2-followup).** Where both sides fixed the same
  entry the integration shape stayed (see the rows above): `ForwardStage::Picking.share_at`,
  the request-matched sign-in answers, the changelog banner and the refused-delete stop.
  This branch's own fixes the integration branch has nothing for stayed, built on its shapes.
- **Files outside the bundle that changed** (the smallest change each): `copy_inline_text.rs`,
  `chat_ui_state.rs`, `history_a11y.rs`, the lock and account closers, and in the core
  crate `composer/actions.rs`, `auth/state.rs`, `profile_panels.rs`, `stats_pages.rs` and
  `groups/mod.rs`.

### Sweep fixes (r2-41)

Files: `translate_ui/mod.rs`, the video player's transport, backends, pacing,
controls and Picture-in-Picture, and the mini-app launch, invoice and emoji
status flows. Rebased onto the r2 follow-up: where both sides fixed an entry
the integration's shape stays and this branch's second version is gone.

| id | fix |
| --- | --- |
| 14-008, 14-017, 14-026, 14-027 | Integration's shape: `Awaited` holds the send's `RequestId`; `LaunchFlow::take_*_for` take only their own answer; a newer launch replaces the older wait. |
| 14-018 | `WebApps::emoji_access` is a list of `EmojiAccessAnswer` (the request and its `EmojiAccessResult`, the integration branch's type), so two answers in one poll are both kept; the grant takes only the answer whose request is its `AccessStage::Granting` request, not the permissions panel's. |
| 14-015, 14-016 | Integration's `CheckoutForm` owns the checkout. The verdict of an invoice's last `sendPaymentForm` is recorded where its answer is applied (`PaymentsState::outcomes`: `Paid`, `Refused`, `NeedsVerification`; a new checkout or payment of the invoice starts it over), and the mini app's watcher reads it (`PayStage { Untouched, Sending(request), Settled(verdict) }`) instead of diffing 30 ms looks, so a payment refused between two looks is seen. `InvoiceWatch::Attempt` and the `CheckoutView` bools are gone. |
| 14-033, 13-065, 14-011, 14-012 | The PiP slot is a `Cell` hand-over and the offer cache uses `try_borrow`; the web-app confirm body uses the integration's one-shot content. Baseline borrows pruned. |
| 13-060 | Integration's `ToastStage` owns the toast timer. |
| 13-061 | `TranslateDialog` holds one `TranslateStage` (`Box`, `Choosing { under }`, `SkipList { note }`). |
| 13-062 | `new_translate_dialog` drops the open box's selection job before replacing the dialog. |
| 14-080, 14-044 | `BackendStatus::health` is one `EngineHealth` (`NotOpen`, `Failed`, `Open(Flow)`). |
| 14-045 | The user's intent lives in `Stage` (`At/Landing(state, Intent)`); `wants_play` is read from it. |
| 14-046 | `LiveStatus::end` is `FeedEnd { Live, Ended, Failed }`. |
| 14-081 | Integration's `LiveInput`. |
| 14-083 | `LiveOverlayClock::armed` holds the due time and its timer together. |
| 14-053 | Integration's `SliderDrag`. |
| 14-054, 14-055 | `WaitingGate` keeps its timer inside the stage that needs it. The Space hold is r2-38's `BoostState<Task<()>>` (11-007), whose `Armed` stage holds the timer; the `SpaceHold` wrapper this bundle made for the same finding went at the merge. |
| 14-028 | The PiP's last-drag answer is read once by the click that ends the press. |
| 14-074 | Each PiP line is one value (`SeekLine`, `VolumeLine`). |
| 14-041 | `request_lead` computes "on the grid" from what it is asked about. |
| 14-050 | The decoder load is one global stage (`DecoderLoad`). |
| 14-071 | `Surfaces::pool` is one `PoolState` (macOS only; not built on Linux). |

`MiniAppWindow::closing_confirmation` is allowed in
`implicit-state-allow.txt` (the app's own setting), so its duplicate baseline
line is gone. `settings_button`, `in_attachment_menu`, `looping`,
`Engine::{ready,running,at_end,keeps_up}` and `AvPlayer` buffer flags are
independent readings or settings, not stages.
### Sweep fixes (r2-32)

Round 2, bundle 32: the statistics rows, the story state files, the suggested
posts, toasts, threads, userpic clips, vanish and the video editor.

| id | result |
| --- | --- |
| 06-018, 13-024 | `StatsState::posts` is `PostFetch { Loading{request}, Ready, Missing, Failed }`: a recent post's `getMessage` is recorded when sent (`post_requested`) and settled only by its own request (`accept_post`/`post_missing` compare it). A send the sender refuses is `Failed`: render does not ask again, and opening the panel afresh (`reset`) asks again. `PostLookup::Loading(request)` replaces the missing entry. A graph whose send failed is `GraphFetch::Failed` (its Retry exists), not a removed entry that the next render asked for. |
| 13-033 | The cover and privacy calls are `StoryUi::manage_calls`, one `ManageCall{kind, story, request}` each. Opening an editor or stepping to another story drops none; `story_frame_upkeep` settles each by its own answer, shows a failure for the story it was sent for and closes the editor only when it is still open on that story. |
| 13-039 | `MediaSave::Saving(Task<()>)` holds the copy's continuation; the finishing continuation lets it go (`finish_media_save`). |
| 13-045 | `AlbumDelete::Deleting(RequestId)` is the delete stage, inside the album view; `PageWait::Deleting` is gone, so leaving the view drops the stage and its request together. |
| 13-046 | `close_for_lock` closes the stealth box, which owns its redraw tick. |
| 13-050, 13-051 | `SuggestedPostsUi` is one `Flow` of the legal boxes (price, price+time, price+Grams Needed, accept, accept+Grams Needed, decline, time, Grams Needed). A Grams Needed answer never replaces an open time box: it says the need in a note instead. The dialog builder clones the price box instead of taking it out and putting it back. |
| 13-054 | `Drawn::leaving` is gone: `Drawn::target` is where the glide heads (a number) and `leaving()` reads it. `Presence` also holds its swap (`from` and `since`) as one value. Baseline lines removed. |
| 13-055 | The synthetic photo row owns the timer that ends its loading (`ImageLoading(Rc<Task>)`). |
| 13-052 | `ThreadView::asker` replaces `ChatWindows::thread_asker`: the window that asked lives on the thread, is set by `open_thread_for` and ends with it. A retry keeps the asker. |
| 13-053 | `advance_thread` drops the jump-to-root goal when the older end failed. |
| 13-058, 13-059 | `ToastQueue::pointer` is gone (the hover is the visible toast's `Timer::Paused`, handed to the next toast by `dismiss`); the wake-up is `Wake::Scheduled{deadline, task}` held in the queue, so a click, a hover or an empty queue cancels it. |
| 14-064 | The session's recovery flow (`TwofaFlow`: the emailed code a step awaits, the finished step's notice) is written by the core's answers, so it stays the session's; the box only ever forgets it. Every path that drops the box runs `twofa_forget_flow`: `twofa_close` for the box's own close, `replace_screen` (account switch, logout, a replacing connection: the whole `TwoStepUi` is dropped, `Farewell` for it is empty) for the rest. `twofa_open` also starts clean. Test: `replacing_the_screen_forgets_a_half_finished_recovery_step`. |
| 14-039 | A clip whose file entry is not known yet downloads (`should_download` handles the unknown file). |
| 14-006 | `failed` remembers the file a clip failed on; a different file gets another try. |
| 14-019 | `begin_vanish` skips a message that already has a ghost. |
| 14-052 | `main_window_media_left` (hide, minimize, lock) drops the cover player and holds the main window's userpic clips. |
| 14-056, 14-069 | `Ready` and `Encoding` hold a `Look` (quality, GIF, turns, flip, crop) beside the timeline; the kept range and cover exist only in the timeline, and `edits()` builds the `VideoEdits` from both, so no stale range can be read. `edits_mut()` returns the `Look` and is `None` while encoding, so nothing edits during an encode. The `Task::detach()` calls of the finishing task (`toast_queue`, `stories_state`, `video_editor_timeline/frames`, `end_reading`) carry a comment saying why. |
| 14-057 | `CoverFrame{at_ms, path, still}` is replaced whole by each cut; a cut whose picture will not decode clears the still. |
| 14-058, 14-070 | `Looping { Off, Playing, PausedByUser }` with the playhead is in `Ready`; `user_paused` and `playback_ms` are gone. The probe's task is held by `Reading`. |
| 14-009, 14-047 | `Drag { Idle, Strip, Pressed, Dragging, Quality }` is the one live pointer drag; `QualityPointer`, `StagePress::moved` and the separate strip/press fields are gone. |
| 14-048 | `Frames { Idle, Cutting{width, frames, task}, Done }` with `StripFrame::{Empty, Cut, Failed}` (in `video_editor_timeline/frames.rs`); the cancel flag is the task. |

Skips from the fresh review: `userpic_videos` `sweep_due` is cleared by the
`cx.defer` closure that always runs, `history_listed` at every frame start;
`public_shares(loading, error)` are two reads of one `ForwardsPage`.
Two `RefCell` body slots (stealth box, video editor) are `Cell`s.

### Sweep fixes (r2-33)

Round 2, files: the media viewer's state (`viewer_state.rs`, `viewer_parts.rs`),
the wallpaper box, cache and source, What's New, the widget bubble, state and
driver, the window title and window control, the AI compose box (box, styles,
style preview) and three files of the main frame (`frame_upkeep`, `render`,
`status_toast`). Every `RefCell` borrow in these files is gone (`try_borrow`,
`Cell` take and put back, or `get_mut` where `&mut self` is held).

| id | result |
| --- | --- |
| 14-059 | `reset_viewer_item_state` drops a timestamp start for any item but the current one (the open or quality change that armed it keeps it) |
| 14-060 | `ViewerUi::pointer: Pointer { Away, OverControls, Panning(anchor) }` replaces `drag` and `over_controls`; a step ends a pan, a close ends both, a move over a control never ends a pan |
| 14-061 | `Reorient` owns the shown orientation in every stage (`Idle(o)`, `Decoding { orientation, target, shown, _task }` (the stage holds the decode task, so a newer turn, a step or a close drops a superseded decode and no token is needed), `Ready { orientation, rotated }`); `ViewerUi::orientation` and the code in `Rotated` are gone, so nothing compares them by hand |
| 14-020, 14-066, 14-013, 14-014, 14-085, 14-032 | no panicking borrows; the dialog body slot is `Rc<Cell<Option<_>>>` |
| 14-036, 14-037 | `Lookup::stage: LookStage { Settled, Rendering, JustMade, FadeStarted }` replaces `pending` and `fade_started` (baseline lines dropped) |
| 14-040 | a crossfade holds one old picture per window, each painted in its own window |
| 14-030 | `BackgroundSearch::{Found, Failed}` carry the request; the preview takes its answer by the `Looking` stage's request, and a search that is another request fails the preview |
| 14-031 | Share is disabled while `Sharing`, `share_wallpaper_preview` refuses a second send, and Blur drops `Sharing` back to `Editing` |
| 14-049 | `Rendered::{Whole, FillOnly}(Picture)` replaces `complete: bool` |
| 14-087 | opening What's New refreshes the notes; `Notes::Loading { stale, _fetch }` shows the old ones meanwhile, a failed refresh keeps them, and the fetch task is held (baseline line `WhatsNewUi::opening` was stale and is dropped) |
| 14-007, 14-062 | `Pointer { hold, contact: Contact { Away, Over, Pressed(button) } }`: `hovered` and `pressed` were one stage; `hold` is the widget's own claim (`Gesture`), kept apart |
| 14-063 | `Lander::{NotStarted, Running(Task)}`; a finished loop (`Task::is_ready`) is started again by the next runtime |
| 14-075 | a `Request` from a runtime that is gone sends nothing to TDLib |
| 07-040 | the language search clears when the box opens |
| 07-041, 07-042, 07-043 | with no connection and no demo session a request fails its box, an `addstyle` link opens no preview and Delete Style keeps the editor, each with the failure toast; a demo still answers through its session |
| 07-049 | `cover_request` and `privacy_request` hold the story they were sent for; the answer closes the panel only when it is that story's (`cover_settled`, `privacy_settled`), and a failure names that story. Touched outside the bundle: `stories_state.rs`, `story_albums/{mod,cover_strip}.rs` |
| 07-050 | a Premium preview fetches nothing while locked. The other steps are teardown or pauses and already lock-aware; `sync_video_avatar` no longer opens a clip while locked (`video_avatar.rs`) |
| 07-051 | `ToastQueue::wake` is `Wake::{Idle, Armed { deadline, timer }}`; the timer is an app task held in the queue, so it survives an account switch and is dropped with its deadline. Touched outside the bundle: `toast_queue.rs`. Baseline line `detached:status_toast_visible` dropped |
| 07-059 | the toast surface uses `try_borrow_mut` |

Review fix round (file-complete rule; the integration branch `claude/sweep-r2-followup` was merged first and its shapes kept):

| item | result |
| --- | --- |
| review 1 (`widgets_state.rs`) | `Poll::Asking(RequestId)`: `poll_widget_public_data` returns the id of the poll that is out (the one already in flight, if any) and `WidgetAnswer::PublicData` carries `request`; an answer for another request (a replaced widget, a restart) is dropped |
| review 2 (`whats_new.rs`) | `WhatsNewWindow::Opening(Task<()>)` holds the deferred open, so the app going away cancels it |
| review 3, viewer timers (`navigation.rs`, `actions.rs`) | the swipe timeout is a held `SwipeTimer::{Idle, Armed(Task)}` re-armed one wake at a time (no loop, no detach, dropped on close); the controls' idle clock re-arms from `viewer_idle_check` into the visible stage's timer instead of looping |
| review 3, allowed clocks (`playback.rs`, `hdr_image/viewer.rs`) | `spawn_viewer_session_tick` (OS media-key queue with no wake signal) and `poll_viewer_headroom` (the OS posts no EDR change) are in `owned-timers-allow.txt` with what ends them; both tasks were already held by their stage |
| review 3, `stories_state.rs`, `story_more.rs` | `MediaSave::Saving(Task)` holds the copy task; the answer replaces it with `Done` |
| review 3, `story_albums/cover_strip.rs` (13-038) | `CoverStage::Probing { _probe }`, `Ready { cut: PreviewCut, _strip }`: probe, strip and preview tasks are held by the stage that waits for them; the serial is gone (a closed or replaced picker drops its tasks) |
| review 3, `Reorient::Decoding` | the token is gone (see 14-061 above); no allow line |
| review 3, `apply_chats_payload` | the Backgrounds arms file their answers under the pending request (`answered(extra, ..)`, `background_answers.push(id, ..)`, `Found(id, ..)`); `check-request-match.py --list` reports no hit for the function after the merge |
| nit: `bare:Phase::Loading` | pruned from `stage-enums-baseline.txt` with the other stale lines (all four baselines pruned) |
| nit: Share answer after Blur | `Answers::push` keeps at most `KEPT` answers, so the answer is bounded and the next drain drops it |
| nit: `Wake::Armed.timer` | `None` only between `rewake` and `arm` in one call; no frame can see it |
| 11-006 | `VideoControlsState::drag: SliderDrag { None, Seek(at), Volume, Speed(v) }` replaces `seek_preview`, `volume_dragging` and `speed_preview`; a clip stop ends it |
| 11-014 | the per-item and per-open resets are two methods beside the fields (`ViewerUi::new_item`, `ViewerUi::closed`), no longer two lists in `navigation.rs` |
| 11-015 | the "Share at" position is `ForwardStage::Picking { share_at }`; leaving the box drops it (the three hand clears are gone) |
| 11-016 | the viewer panel copies only the item's files and download intents (`FileMap::only`, `DownloadIntents::only`) instead of both whole maps |
| 13-033 | `leave_item` carries `cover_request` and `privacy_request` to the next story, and reopening an editor no longer clears them; Set is disabled while a cover call is out |
| 13-042 | a picker save whose adds went out and whose removes did not send marks the adds done (`AlbumPicker::mark_added`), so the retry sends only the removes |
| 13-058 | `ToastQueue::pointer` is gone: the shown toast's `Timer::Paused` is the hover, and `dismiss` hands it to the next toast |
| 14-052 | the window-activation observer pauses the playing cover clip when no Quill window is in front (`hold_video_avatar`) |
| 10-083 | not a bug: a failed HDR decode draws the SDR picture, as a file with no HDR does; a re-decode of the same key from render would loop, and each open or step resets the stage, which retries it |
| 10-081, 10-082, 11-002, 11-004, 11-008, 11-009, 13-038, 13-039, 13-059 | already fixed by the worker commits or `claude/sweep-r2-followup` (held tasks in `Decode::Decoding`, `Shown`, `Reorient`, `SavedToastStage`, `FadeOut`, `Wake`), and by the items above |

### Sweep fixes (r2-36)

Round 2 of the re-sweep, one bundle of box files. Every entry in these files
was fixed or proven false; the files were then read again from the top.

| id | file | outcome |
| --- | --- | --- |
| 09-012 | `conversation/conversation_part1.rs` | The render no longer asks for the first page: `select_chat_open` / `open_chat_window` ask when the history opens, and `fetch_history_for` ends an empty history in its failed stage (`Session::fail_history_load`) when the send itself is refused (`ConnectSendError::Native`), so render only draws the skeleton, the Retry row or the empty pane. `maybe_auto_load_older` (scroll) keeps the `load_failed()` gate from r2-04. Test: `a_first_page_that_cannot_be_sent_ends_in_the_failed_row_and_is_not_asked_again`. |
| 09-016 | `conversation/mod.rs` | `reported_visible` is `ReportedVisible { chat, open, ids }`; `open` is the new `ViewBook::epoch()`, bumped by every `restart`, so a reopened chat reports its rows again. |
| 09-029 | `dialogs/account_notices.rs` | `TermsStage::Terms(AgeLine)`: `Unticked` / `Ticked` / `Missing` replace the `age_ok` + `age_error` pair. |
| 09-027, 09-028 | `dialogs/checklist.rs` | The two switches are one `OthersMay` value (baseline lines for `ChecklistDialog` dropped); the refusal clears on any edit through subscriptions the dialog holds. |
| 09-026 | `dialogs/contacts.rs` | One `AddContactStage` (`Form`, `Importing`, `NotJoined`, `Saving`, `Failed`) replaces `stage` + `save`. `ContactSave` stays for Edit Contact. |
| 09-032, 09-033 | `dialogs/data_extra.rs` | `NetworkReset::Resetting(RequestId)` holds the `resetNetworkStatistics` request and `settle_network_reset` (in `settle_box_submits`) ends it on the answer; a send that did not go out stays on the confirmation with the failure in `data_storage_save` under `StorageSaveKind::ResetStatistics` (the integration branch's per-kind failures), so no other save's `ok` wipes it. |
| 09-043, 09-044, 09-045, 09-046 | `dialogs/folder.rs` | The copy mark's timer and the link request (`LinkOp` inside `FolderShareView`) are the integration branch's (`CopyTimer`, `view.op()`); this bundle adds the editor's refusal as `FolderLoad::Ready { error }` and the link form's name field and ticks in `FolderShareView::Edit { form, op }` (`LinkForm`). `FolderLoad::Loading` and `ContactNote::Waiting` wait on keyed per-folder / per-user fetch entries whose asks are deduped, so they hold no request of their own: `stage-enums-allow.txt` says so. |
| 09-038, 09-039, 09-040, 09-041 | `dialogs/group_call_create.rs` | `join_as` is read off the step (`Confirm { join_as }`, `ChooseJoinAs { selected }`); a second `createVideoChat` is not sent while the first is still in the registry (`video_chat_create_in_flight`, also used by the RTMP start); the dialog body is held in a `Cell`. 09-040 skipped: `Waiting` leaves on the answer, on the error (create as yourself), when another Start replaces the flow, and when the session no longer holds its request (`abandoned`); tdesktop's `ChooseJoinAsProcess` also draws nothing while its list loads, so a box with a Cancel would be new behaviour. |
| 09-037 | `dialogs/local_storage.rs` | `StorageDrag::Sent { account, requests }` holds the `setOption` requests the release sent (`set_storage_limits` now returns them) and ends when the session no longer holds any of them: on the limits TDLib holds if `StorageSaveKind::Limits` failed, settled otherwise (a rounded or clamped limit no longer keeps the stage). Test: `a_limit_release_settles_on_the_answers_of_its_own_requests` (both answer orders). |
| 09-047 | `dialogs/member.rs` | `searched` is written when the request went out, not before. |
| 09-048, 09-049, 09-050, 09-051 | `dialogs/poll.rs` | `PollDialog::options` is a list of `PollOptionRow { input, attach, correct }`; the panel's recipient is one `PollPick` (by slot id) in `PickerUi`; `Preparing` owns the read task and is answered by slot id; the focus of a fresh box is an `Option<Entity>` consumed by the mount (`focus_on_mount`, the way `checklist_box` does it) on both the poll box (`PollPage::Opening` is gone) and the Add an Option panel (`AddOptionStage` is gone). |
| 09-052, 09-053, 09-054 | `dialogs/profile_panels.rs` | `ContactNote::Unavailable` ends the "Loading" wait when the full-info ask could not be sent or failed; the birthday refusal is kept with the typed fields and drawn only while they match; the chat pickers use `ChatStep` / `AddBotStep` instead of `target: Option<i64>` (the bot's rights live in the confirm step). |
| 09-055 | `dialogs/restrict.rs` | The refused custom time is a `TimeRefusal` with the pick it refused and is drawn only while that pick is the shown one. |
| 09-042 | `dialogs/rtmp_stream.rs` | The dialog body is held in a `Cell`, no `RefCell` borrow. |
| 09-057 | `dialogs/storage_editor.rs` | Taken from the integration branch (`AutoDownloadAnswers`, taken by the request in `Saving`); this bundle's own `dialog_answers` version was dropped in the merge. |
| new-01 | `dialogs/account_notices.rs` | The same `RefCell` body cell became a `Cell`; "No" on the new-login banner opens its box only after the request went out; a refused `deleteAccount` send from the terms box says so (`account_delete.fail`). |

Guards: `scripts/check-dialog-answers.py` (new purpose), `view-state-not-chat.txt`
(`reported_visible` entry), one `window-scope` note on the checklist's
subscription. Behaviour that changed: with "Allow Multiple Answers" turned off,
the quiz keeps the first marked row in row order (it kept the first marked in
click order).

Review fix round (merge of `claude/sweep-r2-followup`, integration shapes kept):

- The group-call schedule countdown is one timer owned by the step (`schedule_refresh_timer`, re-armed by its own wake while the step is current), not a loop.
- Every one-shot dialog body in the files this bundle touched uses `shell::one_shot_dialog_content` (a `Cell`), so no `RefCell` borrow is left there.
- `Session::apply_ok`, `settings/state.rs` and `session_apply_ok.rs` equal the integration branch after the merge, so their ratchet hits are not this bundle's. `open_folder_delete`'s demo seed goes through `Fetch::begin` / `sent` / `answered`, and `bots_extras::Submitted`, `StorageScrollStage::Requested`, `ContactNote::Waiting` and `FolderLoad::Loading` have an allow line with the reason.
- A fresh review of every touched file found the two items below (new-02, new-03) and nothing else.

| id | file | outcome |
| --- | --- | --- |
| new-02 | `connect/settings/storage.rs` | `set_storage_limits` returns the requests that went out (the release stage's keys) instead of `()`. |
| new-03 | `folders/save_folder_editor.rs` | The demo seed of the "chats to leave" list used `Fetch::succeed` (an answer set without a request); it now goes through `begin`, `sent` and `answered`. |

### Sweep fixes (r2-37)

Re-sweep bundle for the welcome editor, the earn pages, folders, group
admin panels and boxes, the forum column and sharing requests.

| ids | what changed |
| --- | --- |
| 09-056 | The welcome editor's row, its Save and the edit text are the integration branch's `WelcomeEdit { Idle, Row { welcome_id, save } }` (merged from the round 2 follow-up; this bundle's own `RowEdit` is gone). Opening another row is refused while the open row's Save is out. |
| 09-069/070/071, 10-053/077/078 | `ui/dialog_body.rs::one_shot_body` hands a dialog body over through a `Cell`; the `RefCell` takes are gone (and their baseline lines). |
| 09-072/073 | `EarnUi::layer` is one `EarnLayer { Closed, Open { back, restore, fade, refused } }`; the close timer is the `Task` inside the fade's `Closing` stage. |
| 09-074 | A refused app-config or balance ask leaves `Failed`, so the per-frame upkeep does not ask again. |
| 09-075 | A refused page load is `refused` in the layer; Monetization, Setup and Join draw the error with Retry. |
| 09-076/077 | The boost box shows Retry while its link is empty; the locked "Switch Off Ads" click asks for the boost level again until it is known. |
| 09-078, 09-080 | "End anyway" and the withdrawal password Submit are disabled while a save or send is in flight; a second Submit keeps the typed password. |
| 09-113 | A second `addlist` link keeps an Adding box; a replaced or closed Checking takes its request out of the registry (and its arrived answer), so a late answer is noted by nobody. |
| 09-114 | `FolderLoad::Ready { error }` carries the line; `FolderEditorDialog::error` is gone. `FolderLoad::Loading(RequestId)` carries the `getChatFolder` request: the stage ends when that request is gone (the spec, or the error, or `Failed` with Retry); with no connection the open ends in `Failed`, not in a load that never ends. |
| 09-116 | `FolderUi::deletes` is the set of `deleteChatFolder` requests out, keyed by request: a second delete of the same folder is a second entry that settles on its own answer. The set lives in the account's `OnScreen`, so a switch drops it. The screenshot demo's "no chats to leave" seed moved to `apply_demo_no_chats_to_leave`. |
| 10-037 | An unasked invite-link, join-request or administrator list draws Retry, not a spinner. `AdminListFetch::Loading(RequestId)`; the administrator and invite-link answers and errors land only while the list waits for that request (or has no entry), so an older answer is dropped in both orders (`answer_administrators`, `answer_invite_links`, `fail_administrators`). The revoked-link list is open only while its fetch exists, so its answer lands only on a list that still waits for that request (`land_revoked`). |
| 10-061, 10-075 (guard) | `EditBoxState` was never stored: it is derived from `SaveStage` and the registry on each call. It is renamed `EditBoxView` (a derived view); `SaveStage` is the stored stage and holds the request ids. |
| 10-038 | `GroupsState::chat_boost_status` is `Fetch<(level, count)>`: the old level stays beside a refetch, a failure draws Retry, answers match their request. |
| 10-060 | `PermissionsSubmit::Partial { sent, reason }` keeps tracking the part that left. |
| 10-061, 10-075, 10-076 | Delete/Leave/Remove wait while the Edit box's Save is out; Remove from list tracks its request like Delete; unsent bulk removals stay selected. |
| 10-063 | `open_member_dialog_on` opens on the wanted tab: one ask. |
| 10-064 | An unknown supergroup or basic group draws its name alone; its id is never used as a chat id. |
| 10-074 | `HistoryUi::forum_peek` is only the user's choice (`ForumPeek`, keyed by the forum); the column layout is a pure function, nothing is written from render. |
| 10-088 | Accept/Reject are disabled while the answer is out; the driver already ignored a second press. |

Review round (r2-37 fix round):

- `GroupsState::boosts_per_gift` is `Fetch<i32>` (asked once, answered and failed by request, `Failed` asks again); the TON explorer and the earn app config fetch record their request with `sent` and take only their own answer or error. A push (`updateChatWelcomeMessages`) and a confirmed unpin-all replace their list in `welcome_pack_pushed` and `pins_cleared`, and the stale sticker fetch cancels go through `ConnectDriver::drop_in_flight`: each of the three is an allow line whose reason says why no request id applies.
- `schedule_idle_image_trim` sleeps once to the due time and re-arms itself from the wake-up when newer activity moved it (no loop); the `Task` stays in `idle_trim`.
- The chat-appearance and group-settings dialog bodies use `one_shot_body` / a `Cell`.
- Left as it is: Remove from list with nothing sent (`Ok(None)`) keeps the Edit box open, because `None` means the same request is already out and its answer closes the box. The administrator, invite-link and join-request sections draw "could not refresh" only for a list never asked; every page is entered through `set_group_settings_view`, which asks before the next draw.

### Sweep fixes (r2-34)

Round 2, files `window_slices`, theme editor/omarchy, appearance pickers and wallpapers, audio engine/tempo, background calls/closing, business screens, calls.

| id | result |
| --- | --- |
| 07-055 | `ConversationFrame {Absent, Replayed, Rendered}` replaces the two `Cell<bool>`s. |
| 07-060 | The theme editor body is built inside the dialog content closure; the `RefCell` take slot is gone (same for the call confirm and swap boxes). |
| 07-058 | `Omarchy::Watching` exists only while following Omarchy (`sync_omarchy_watch`); the poll is allow-listed (no file-watch dependency). |
| 07-057 | `KeybindingUi::{stop_capture, reset_row, refuse_chord}` are the only exits; `capture` is private. |
| 07-053 | `AccentSelect` moves the Custom field to the applied theme's accent when drawn. |
| 07-056 | `LinkHandlerRecheck::Rechecking {_task}` owns one 1/3/8 s re-read task; dropped on a new click or when System is left. |
| 07-054 | The wallpaper delete confirm draws only for a listed, removable wallpaper, is re-checked at confirm time and cleared on leaving Appearance or switching account. |
| 07-061 | The list is asked on arrival (`ask_for_wallpapers`), not per draw; a send that cannot leave is `Failed` ("not sent") with Retry. |
| 08-001 | `Landing {Playing, Held}`; `set_landing` sets it and the old sound's pause together. |
| 08-002 | `Backend::{Device, Simulated(SimRun)}` holds the simulated clock and stall stamp. |
| 08-003 | `state()` reads `Standing::Stale` as Paused, so a seek on a track that ran out is not ended by the tick. |
| 08-004 | The shared video output is cloned out with `try_borrow`; no borrow is held across the output's mutex. |
| 08-005, 08-006 | `Stretch {Starting, StartingInputEnded, Streaming, Draining, Done}` replaces `done`, `inner_done`, `prev`. |
| 08-012 | `RingPrompt` and `RingNotice` (`Posted {tag}`, set only once the system took the notification). Two independent channels, not one stage. |
| 08-014 | `after_closing` tasks are held in `CloseWaiters` and give up as `Never` after about a minute; `closed` is capped likewise. |
| 08-034 | The save stage is `LinkSave` in `LinkEditDraft`, matched to the request id; the session keeps only the latest answer (`ChatLinkAnswer`), so no hand resets. |
| 08-029, 08-030 | The chatbot draft is filled by frame upkeep from the fetch (no poller, no stop on Idle); the username debounce timer is owned by the draft (`LookupDebounce`). |
| 08-032 | The time zone screen draws a failed list with Retry. |
| 08-035 | `business_show` / `sticker_pick_left` are the one seam that closes the start page's sticker panel. |
| 08-051 | A redial dismisses the summary in `send_dial`, once the dial is accepted (`SummaryAfterDial`). |
| 08-059 | `PipWindowStage::Opening {_open: Task}`: the deferred open is owned by the link, so an older open never attaches to a newer link. |
| 08-048 | `CallTick::Running { _task }`. |
| 08-052, 08-053 | The rating draft is `CallRating::{Editing, Sending{request, draft}, Failed{error, draft}}` on the summary; the comment input stays until the editor opens again. |
| 08-058 | `LiveInput::{HostFailed, Nothing, Expected}` (touches `group_call_tiles.rs`, `live_view.rs`, `live_overlay.rs` constructors). |
| 08-060 | `VolumePill::{Closed, Held, Closing {_timer}}` owns the close timer. |

Review fix round (r2-34, after merging `r2-followup` and `r2-28`):

| item | result |
| --- | --- |
| review 1 | This entry: one row per id above, each fixed; none skipped as not real. |
| review 2: `call_types.rs` | `CallSummary::busy` and `ActiveGroupCall::loaded_all_participants` are allow-listed with their reasons (a fact about how the call ended; TDLib's own report) and leave the baseline. `CallRating::Sent` and `CallUpload::Sent` are the end states after the answer of their `Sending(request)` (nothing in flight); `CallTransport::Connecting` ends on the engine's next report: all three are allow-listed with that reason. |
| review 2: dialog bodies | `build_call_confirm_dialog`, `build_call_swap_dialog` and the theme editor already build the body in the content closure; `chat_look_ui.rs` now uses `shell::one_shot_dialog_content`, so no `RefCell` take is left in these files. |
| review 2: `keybindings/mod.rs` | `apply_custom_bindings` reads the keymap with `try_borrow` and leaves the keymap as it is when it cannot (a partial list would drop the kit's bindings). |
| review 2: `ensure_call_tick` | Allow-listed: the elapsed-time clock and the ringtone loop change with the passing second, which no update announces. `CallTick::Running` owns the task and it ends itself on the first beat that `call_tick_wanted` is false. |
| review 2: `after_closing` | No clock any more. A waiter (`CloseWaiter`: its wait, its follow-up, its cap task) is looked at by `settle_close_waiters` when something it waits on changes: after each background pass that drains the closing set (TDLib's `authorizationStateClosed` pokes that pass), when a deletion ends and when a worker preparing the screen's connection ends (the worker's claim tells its end, `begin_screen_worker`, so a worker whose connection was replaced still wakes the waiters). The cap is one 60 s timer per waiter and is the way out (`then(.., false)`). `closed()` (the connect screen) uses `after_closing` with a oneshot answer. |
| review 2: stage lines | `Capture::Waiting` (the user's keystroke), `BotLink::Loading` (the session's connected-bot `Fetch`, moved on by `business_bot_upkeep`), `Found::Loading` (a view computed per draw, never stored) and `Phase::Opening`, `LiveOverlay::Opening` (derived per draw from the player's `PlayState`) are allow-listed with those reasons. `settings_window/search.rs` `Stage::{Live, Finished}` is not an in-flight stage (the guard does not list it) and holds its selection beside the text it belongs to. |
| review 3 | r2-28 is merged. `KeybindingUi::capture` stays private; push-to-talk goes through `toggle_ptt_capture`, `end_ptt_capture` and `ptt_armed`, and the tests of this branch that set `group_call.ptt_capture` use them. The `ptt_release` timer baseline is gone with r2-28's shape. |
| new-01 | `group_call_join_sent` asked the request registry by purpose (`check-request-match`). The join is now `GroupTransport::Joining { request }`, set where the join is made (`start_group_join`) and ended by the answer (`store_answer(request, ..)`), by the error of that request (`join_failed`), by an offer that was never sent or is no longer wanted (`join_unsent`, from `undo_offer`) or by a reset. Another request's answer or error leaves it alone. The panel reads `call.transport.joining()`. |
| new-02 | `fetch_installed_backgrounds` on the per-theme fetches of the follow-up: a send that cannot leave is that theme's `Fetch::fail("not sent")` with Retry (it was `cancel`, which asked again from the next arrival), as on this branch before the merge. |
| new-03 | Merge repairs: the escape and lock tests use r2-28's `open_call_box` seam; the inline "load more" refusal sets `InlineMore::Failed { offset }` on the follow-up's slot; `EntryStage::Loading` (image budget) is allow-listed (the entry's `Load` handle sits beside it and ends with it). |

### Sweep fixes (r2-40)

| id | outcome |
| --- | --- |
| 12-C011 | `PasteNotice::Showing` holds its 9 s timer `Task`; closing the notice drops it. |
| 12-C019 | `RichPreview::blocks` takes both cells with `try_borrow_mut`; the last parsed blocks live in a `Cell` beside them, so a frame that finds the cell busy draws those, never an empty frame. |
| 12-C009, 12-C010 | The two search debounce timers are held in `SearchUi::debounce`; the next typed query replaces (cancels) the one before it. |
| 12-C008 | The "From:" picker filters members with `SearchUi::member_input`, a field of its own (swapped per window with the chat window's slot). The message field never holds a member filter, so opening, picking or closing the picker no longer empties it and `restart_chat_search_field` is gone; the member field starts empty each time the picker opens. tdesktop does the same (`ShowSearchFromBox` has its own field). |
| 12-C017 | The ready-search-in-chat seeder returns instead of `assert!`. |
| 12-C012, 12-D004 | `refresh_active_sessions` and `refresh_connected_websites` in the driver own the reset: a fetch already in flight (`Fetch::is_loading`, which holds its request) is the refresh, so its stage is not overwritten with `Idle`. No purpose lookup. |
| 12-D002 | The PIN focus ask is gone from the stage: the ticker focuses the field when the `NeedPin` event it just applied opens the sheet. |
| 12-D001 | The delete confirmation no longer resets itself from the dialog builder: `QuillApp::passkey_confirm` answers `None` once its passkey left the list. |
| 12-D003 | `once` in `passkeys.rs` (and the sticker set box's one-shot body cells) use `Cell::take`, no `RefCell`. |
| 12-D015 | `Preview::stage` is one `PreviewStage` (`Shown`, `Playing`, `Hiding(since)`; `Shown` waits for no request, paint moves it on); `begin_playing` is the one transition paint makes. |
| 13-001 | `SettingsUi::nav` is one `SettingsNav` (`Closed`, `List`, `Page`, `Sub`): a subpage carries its parent and a closed window has no page. |
| 13-002 | `Arrival::Seeking` gives up `TOTAL` after it began, so a row that shows up much later does not scroll the page. The time comes from the frame clock (`cx.background_executor().now()`) passed in, so a test advances it. |
| 13-003 | A send that fails for the exceptions list lands in `Fetch::Failed` (Retry), so the page render no longer re-sends it every frame. |
| 13-004 | The activity filter drops a chat, agent or tool whose entries are gone (`effective`); rows and buttons read the same value. |
| 13-015 | The sidebar's grace wake is held by `SidebarReveal` in its `Leaving` mode (`set_mode` is the one place the mode changes and drops it). |
| 13-030 | The sticker player's upkeep slots use `try_borrow` / `try_borrow_mut`. |
| 13-031 | See 12-D003. |
| 13-026 | The set box's `Farewell` also clears the session's viewed set. |
| 13-040 | `RowState::pending` is gone: the Add button asks the session where it draws. Baseline lines removed. |
| 13-038 | `CoverStage::Probing` and `Ready` hold their probe and strip tasks, the preview cut holds its own; the serial and `NEXT_SERIAL` are gone because a closed picker drops its tasks. |
| 13-042 | A save whose adds went out and whose removes did not send waits as `SavingPickerPartly`; once the adds are answered the picker counts them as held, so a retry does not resend them. |
| 13-044 | A wait's answer acts only when the picker or view still is the one that asked (`Creating`, `SavingPicker { album_id }`, `Deleting { album_id }`). |
| 13-034 | The picker asks for thumbnails once per candidate count, so later pages get theirs. |
| 13-049 | Skipped: hosting is `ReplyRow::MediaPanel` inside the one `ViewerPanel` value, not a bool; the panel counts as the story's only while it is also open. |
| 13-063, 13-064 | The touch bar's sync and followed composer are `Cell`-held stages (`SyncStage`, `Follow`); the observation lives in `Follow`. No `RefCell` borrow is left in them. |

**Review fix round (r2-40).**

| item | what changed |
| --- | --- |
| 1 (12-C008) | See the table above: own member field, no hand-emptying. Test: `member_field_tests`. |
| 2 | Both stickers manager dialogs (`build_stickers_manager_dialog`, `build_mask_sets_dialog`) use `shell::one_shot_dialog_content`; no `RefCell`. |
| 3 | `send_effects` cell rects: `try_borrow` / `try_borrow_mut` (strip, panel, `choose_send_effect`); a busy cell skips one frame's rect. |
| 4, 5 | Already `Cell`-held on the integration merge (`media_roots_frame`, `BLOCKED`); no borrow left. |
| nit | Preview keeps the last blocks; `Arrival` takes the frame clock (test `the_wash_ends_when_the_frame_clock_passes_its_length`). |
| nit | 12-C009/C010: the debounce token is the driver's check that a fired timer is the newest, the `Task` is the timer; the two are not stages of one thing, so they stay apart. |
| guards | `check-request-match` flagged the two `refresh_*` purpose lookups (fixed above); `check-stage-enums` flagged `PreviewStage::Waiting` (renamed `Shown`, it waits on no request); baseline lines for gone entries pruned. |

### Sweep fixes (r2-19)

Round-2 entries for the auth, bots, calls, communities, groups and messages
domains and the shared-media, sponsored, sticker/GIF, story, topic-history and
view-book state files. Every one of those files was read again from the top
after its entries were done.

| id | file | what changed |
| --- | --- | --- |
| 06-026 | `sponsored_state.rs` | The unused `SponsoredReport` machine is deleted; `SponsoredReportFlow` is the only one. |
| 05-015 | `bots/error.rs` | Already `Fetch<Vec<i64>>` on the integration branch (r2-18, 08-021): `Loading` holds the request. This branch keeps its test (an older request's error and answer leave the newer one alone) and drops its own `SimilarBotsFetch`. |
| 05-016 | `bots/commands.rs` | `bot_users: HashMap<i64, Fetch<()>>` replaces `asked_bot_users`; a refused `getUser` is `Failed` and is asked again when the group's commands arrive again, never from upkeep. |
| 05-017 | `auth/state.rs` | `ShutdownPhase::{LogoutRequested(RequestId), LogoutRefused(TdError)}` replace `logout_request` and `logout_refused`; a quit replaces the stage, a refusal outside Ready becomes a close. |
| 05-018 | `calls/state.rs` | `CallLinkStage::{Offered{offered, link}, Joinable(link)}` hold the invite hash; `calls.call_link` is gone. |
| 05-019 | `calls/state.rs` | `RecentCalls::{List(Paged<CallPage>), Clearing{request, list}}`: rows, offset and stage are one value. |
| 05-021 | `communities/apply.rs` | The add-chat request list is `Loading` with its request; a page of another request is dropped. |
| 05-022, 05-023, 05-026, 05-028, 05-038 | `groups/apply.rs`, `groups/error.rs`, `groups/group_type.rs`, `messages/error.rs` | `ChatStatisticsFetch`, `AdminListFetch`, `AdminRightsFetch`, `PollStatsFetch`, `PollVotersFetch`, `PublicChatsFetch` carry the request in `Loading`; `SupergroupMembersFetch::Loading` stays the integration branch's unit variant, its request is `MemberAsk::first` in `member_asks` (the basic group's one list is the `Recent` key: `GroupsState::basic_members_wait_for`); the invite-link lists, pinned messages and boost status are matched by request too. Answers and errors of older requests change nothing. A member who is no administrator ends the rights lookup (`Failed`). |
| 05-024 | `groups/apply.rs` | `chat_boost_status` is `Fetch<ParsedBoostStatus>`; `chat_boost_details` and `boost_level_moved` are gone. A level change or a boost marks it `Outdated`, it stays on screen, and a failed refetch is `Failed` with the held answer kept (no re-ask from render). |
| 04-014, 05-025 | `groups/new_chat.rs` | The integration branch's `CreationStage::{Out, OutClosed, Made, MadeClosed, Settled}` already replaces `CreationAsker` and the copied bools (r2-18); this branch's own stages are dropped. Kept from it: a create whose dialog was closed and whose request is lost removes its row (the lost-request error hook, and `abandon_new_chat` when the request is already gone), with its test. |
| 05-027 | `groups/private_sharing.rs` | Skipped: tdesktop's `actionTaken` is "the next item is a toggle" (docs/decisions/noforwards-requests.md); Quill matches it. |
| 05-029 | `groups/state.rs` | `invite_link_error` and `community_error` are `Notes` queues (the integration branch kept `Option<String>`; the entry is two failures between two polls, which needs more than one message at a time). `member_action_error` is a per-chat banner the user dismisses (not a drained slot), `member_list_stale` a queue of chats, `add_members_failed` a counter: they are not stages. |
| 06-022 | `shared_media_types.rs` | A replaced list drops ticked rows that are no longer listed. |
| 06-024 | `shared_media_types.rs` | `SharedMediaView::{Closed, Gallery, Playlist{user_id, under}}`. |
| 06-027 | `sticker_gif_types.rs` | The integration branch's `StickerBatch::{Idle, Running, Finished}` (`begin(total, sent)`, `finish_item(request, ok)`); this branch's second type is dropped. |
| 06-028 | `sticker_gif_types.rs` | The stale bools are `Fetch::Outdated` on the lists and on the saved GIFs; a refused refetch send keeps the mark. |
| 06-030 | `sticker_gif_types.rs` | `AttachedStickers::{Idle, Asking(RequestId), Answered}`. The box's own `loading(0)` view is the box's stage. |
| 06-031, 06-032 | `sticker_gif_types.rs` | The integration branch's `GifSearch` holds results and `asked_offset` (the panel keeps `search_chat`); the duplicate `GifSearch::request` is gone: the page request is the one in `fetch` (`Fetch::sent`, `waits_for`). |
| 06-033 | `sticker_gif_types.rs` | `SuggestFor::Asked{emoji, request}`: matched by request, and an error returns to `Nothing` so the next keystroke asks again. |
| 06-034 | `story_types.rs` | `StoryManageState` is a `ManageLine` and one list of `ManageCall::{InFlight, Saved, Failed}(RequestId, ..)`; going away ends a "Saving" line. |
| 06-016 | `topic_history.rs` | The blacklists (`stale_pages`, `gone_topic_pages`) are gone: a window move forgets the page requests, so their answers find none. Threads do the same. |
| 06-041 | `view_book.rs` | `Phase::{Newest, Reported{on_screen}}`. |

Merge with r2-18 (`claude/sweep-r2-followup`): every overlap kept the integration branch's shape (sticker batch, poll voters, new chat creation, GIF search, supergroup members, community lists, `ShutdownPhase::HoldingClose`, the `contacts_notice` clear). The four shape guards pass with no new hits; their baseline lines for code that is gone were pruned.

Fresh review of each touched file found nothing more beyond what the table lists: `ManageCallState::InFlight` (the stage guard's one new hit) became `ManageCall`, which carries its request in every variant, and the stranded comment in `bots/error.rs` sits above its arm again.

Files outside the bundle changed only to follow these types: `info_types.rs`
(request ids in the `Loading` variants), the connect drivers that send the
requests, and the UI readers.

### Sweep fixes (r2-17)

Round 2, file-complete bundle: `premium_hub/ton.rs`, `ton_withdraw.rs`,
`pull_next/mod.rs`, `settings/accounts.rs`, `appearance.rs`, `media.rs`,
`state/call_types.rs`, `chat_types.rs`, `composer_link_preview.rs`,
`composer_targets.rs`, `fetch.rs`, `group_call_stage.rs`, `info_types.rs`,
`ownership_types.rs`.

| id | result |
|----|--------|
| 04-029, 04-030 | `GramsHub::peers: BTreeMap<PeerKey, PeerAsk>`; `PeerAsk::Asked(RequestId)` / `Missing`. The four sets and the `ChatId(user_id)` smuggled in the pending request are gone; a refusal marks only the peer its own request asked. The rows and their peers cursor are one `TxRows`, so a cleared list restarts the cursor by construction. |
| 04-031 | The balance probe holds its `RequestId` in `Fetch::Loading` and takes only that answer. A failed probe is asked again on the next Settings open (`settings_closed` from the tick, never a re-send from the tick while Settings is open); a probe that never left is a failure, not a re-send every poll. |
| 04-032, 04-033, 02-063 | `GramWithdraw::{Password{error}, Sending(RequestId), SecurityCheck, Finished(WithdrawOutcome)}`: one value for the box and its outcome. The window takes the outcome once (`take_withdraw_outcome`); `GramsHub::close` drops an outcome nobody acted on. |
| 04-046, 04-047, 04-048 | Pull gesture: `Stage::{Idle, Pulling, Released, Jumping}`; `Reach::{Stretching{dwell_due}, Ready{peak}}` inside the pull, so the dwell timer and the peak live and die with their stage; `Expand::{Settled, Running(Tween)}` replaces `ExpandRun` beside the tween. Review round: `Pull::holding` is deleted: the indicator reads `PullMachine::edge == Edge::Held` (the stage of the edge) instead of a copy written in `handle()`. `Step::started` is the one-shot answer of a wheel call (a pull began in this call), returned and merged per call, never stored: it moved from the baseline to `implicit-state-allow.txt` with that reason. |
| 04-051 | `rename_account` goes through `update_registry` like every other writer. |
| 04-055 | `ignore_battery_saving` is tdesktop's own user setting (keep animations while the battery saver is on), independent of `power_saving`; moved to `implicit-state-allow.txt` with that reason. |
| 04-056 | `WallpaperSource::{Theme, Color(rgb), Telegram}` in the prefs; the file keeps `wallpaper_rgb` and `telegram_wallpaper` through a serde adapter (Telegram wins when both are set, as before). |
| 04-057 | Proven independent, with one coupled write made one call. `theme` (manual night choice), `auto_night`, `day_theme`, `night_theme` and `follow_omarchy` are separate user settings: every combination is meaningful and distinct (a day slot kept while following Omarchy, a manual choice kept under auto-night, high contrast over both), and each has its own writer (window toggle: `theme`; Appearance page: `auto_night`; editor: a slot). The only action that wrote several, `choose_app_theme`, is now `AppearancePrefs::choose_theme` (pure, tested). The old skip reason ("about 150 readers") was effort; this one is the meaning of the fields. |
| 04-058 | `WorkMode::{WindowAndTray, TrayOnly, WindowOnly}` in the prefs (both-off cannot be stored), file keys `show_tray_icon` / `show_taskbar_icon` kept by serde. `minimize_to_tray` and `start_in_tray` are tdesktop's `closeBehavior` and `startMinimized` settings, independent of the mode; whether they apply is the tray availability gate at read time. |
| 04-059 | `MediaGrouping::{AlwaysGrouped, Remembered{grouped}}`; file keys kept. |
| 05-005 | `CallSummary::end: CallEnd::{Ended, Declined, Failed}`; `busy()` derives from it. Baseline line dropped. |
| 05-006 | `GroupScreenShare::{Off, Starting{request, answer}, Sharing, Paused}`: pause exists only on a connected share and ends with it; `screen_share_paused` is deleted. |
| 05-007 | `ActiveGroupCall::my_video: MyVideo::{Off, On, Paused}`. |
| 05-008 | `ActiveGroupCall::rtmp: Option<RtmpInfo>`. |
| 05-009 | The `HangingUp` copy is gone: the hang-up lives in the call as `CallAction::{Idle, Answered, HangingUp(HangUp)}` (replaces `CallProgress`'s old hang-up copy, so it ends with its call), and `ActiveCall::phase()` says `HangingUp` while it is in flight; `HangUp::resume` is deleted. Review round: `ActiveCall::{state, ready}` are one value, `progress: CallProgress::{Signaling(CallState), Ready{ready, state}}` (`state/call_progress.rs`): the Ready record (key parameters, clock start) exists exactly from the Ready update on, is kept through a later hang-up state, and a state without its record (or the reverse) cannot be built; `update` is the only writer. `transport` stays a field of its own, by its writers and readers: it is the engine's report (`pump_call_engine`, `record_start_error`), written before Ready (a failed start) and after TDLib's terminal update (the engine closing), and read only through `started()` / `state()` / `error()`; no reader combines it with the TDLib state, so no combination of the two is invalid. |
| 05-010 | `ChatSummary::last: Option<LastPreview>` built by one constructor; the preview, style, sender, thumbnail and message identity cannot disagree. |
| 05-011 | `LinkPreviewFetch::Asked(RequestId)`, settled in the reducer (`settle_composer_link_preview`) by the request: the side map `composer_preview_urls` and the connect-core captures are deleted. A swept request settles the chip as `Failed`. |
| 05-012 | `ComposerReplyFetch::Loading{request, attempts}`; answers, errors and sweeps match the request. |
| 05-061 | `GroupScreenShare::Starting{request, ..}`; `store_answer` and `fail_start` take only that request. |
| 05-062 | **Open (partly fixed).** Sticker lists, greeting stickers, emoji icon lists, live story extras, and in this review round the emoji installed and search lists, the sticker category strip, the Premium sticker preview, the default auto-delete timer, the account TTL, saved sounds, storage statistics, active sessions, connected websites, the chat peek preview, the pinned list, the "Post as" list, the folder leave and folder-share chat lists, the "join as" list, the earn app config and the TON explorer URL record their request (`begin` + `sent`) and take `answered` / `failed` with it, in every file the bundle owns or touched; each family has a both-orders test (`state/tests/matched_answers.rs`). Three calls in those files take no request on purpose and are in `request-match-allow.txt` with the reason (the unpin-all local effect, the pushed live-story donors, the pushed welcome messages). The callers in other bundles' files stay in `request-match-baseline.txt` (the guard's own count, which only shrinks): the entry stays open for them. This branch's second guard (`check-fetch-answers.py`, `fetch-unmatched-baseline.txt`) is deleted; `check-request-match.py` counts the same callers. |
| 05-064 | `Loading(RequestId)` in `PollStatsFetch`, `InviteLinkCountsFetch`, `AdminRightsFetch`, `AdminListFetch`, `PollVotersFetch`; `ChatStatisticsFetch::Loading{is_dark, request}`. Every answer and error is applied only to the stage that waits for its request. `SupergroupMembersFetch`, `InlineQueryFetch` and `ChatEventLogFetch` keep the shapes of `claude/sweep-r2-followup` (merged in, which rebuilt the member list, inline query and event log: `MemberAsk`, `InlineMore`, `EventLogWait`, `OlderEvents`); the request of their first page lives there. `InlineBotResolve::Resolving` keeps its `generation` ticket (the request carries the same one in `ResolveInlineBot`), in `stage-enums-allow.txt`. |
| 05-065 | **Open after the merge.** The event log is the follow-up's shape (`ChatEventLogFetch::Loading` beside `EventLog::wait: EventLogWait`, `OlderEvents` in the page); this bundle's own `LoadedEventLog` was dropped for it, as the reviewer's note says. The first page's request is still in `EventLogWait::page` beside the bare `Loading`: left as the follow-up built it (its bundle owns the rebuild); `bare:ChatEventLogFetch::Loading` and `bare:SupergroupMembersFetch::Loading` stay in the stage-enum baseline. |
| 05-066 | `TransferStage::Sending{request, transfer}`, `OwnerLookup::Loading(RequestId)`, the check in `Fetch::Loading{request}`; `fail_ownership_transfer` ends only the transfer waiting for its request (it overwrote `Idle` and `Done`). |

**Found on the second read (new-NN).**

- new-01: `GramsHub::close` is the one place the screen closes (`open`,
  `selected`, `withdraw`).
- new-02: the pull edge was `finger_down` beside `spring` (never both):
  `Edge::{Rest, Held, Returning{from, since}}`.
- new-03: `ChatSummary::can_send_basic_messages` was a second copy of
  `permissions.can_send_basic_messages`, written beside it and rolled back
  beside it; it is a method of `permissions` now (`RequestRollback` loses
  `previous_can_send`).
- new-04: a `getChatMember` answer for a rights lookup of a member who is no
  administrator wrote nothing, so the rights editor said "still loading" and
  a refresh kept the stage; it lands in `Failed` now.
- new-05: a late "load more" voters page could overwrite a fresh first page
  that was loading; a next page joins only a `Loaded` list.
- new-06: lookups whose stage draws "Loading" ended only by an answer; they
  are sweepable now (`is_sweepable`) and settle as failures: link preview,
  reply bar, poll voters and stats, chat statistics, invite link counts,
  admin list and rights, member pages, event log, owner lookup, inline query.
- new-07: the "request already in flight" guards beside the stages of the
  admin list, admin rights and member list are gone: the stage holds the
  request, so a refresh that drops it and asks again is not answered by the
  request it replaced.

**Review round (r2-17).**

- `ActiveGroupCall::stream_mode` and `stream` are one value now:
  `media: GroupMedia::{Rtc, Stream{mode, state}}` (`state/group_call_stage.rs`).
  A WebRTC call has no stream state to read stale (`stream_state()` reads
  `Connecting`, as the default did); the feeder's state is written only into a
  stream (`set_stream_state`), and the mode is set once after the native
  connect (`GroupMedia::connected`). `GroupCallMode` itself is unchanged.
- `ActiveGroupCall::is_joined` and `rejoin` are independent, by their writers:
  `is_joined` is written only from TDLib's `groupCall.is_joined`
  (`apply_group_call_fields`; the screenshot demos set it directly), and
  `rejoin` is Quill's own series (`want`, `attempt_*`, `leave`, `rejoined`),
  which reads `is_joined` only as the not-joined to joined edge at the update.
  Every combination is meaningful: `is_joined` with `Left` is "the user left
  and TDLib has not said so yet", `Wanted` with `is_joined` is a drop TDLib
  reports with `need_rejoin`. `loaded_all_participants` is TDLib's own report
  mirrored as is (the allow file has the line; the baseline line is dropped).
- `ActiveCall::ready` is folded into `progress` (05-009 above).
- The stage-enum baseline lines of the bundle's files that are no stage have
  reasons in `stage-enums-allow.txt` (derived marks, answered markers, the
  engine's own transport report, the TDLib withdrawal state, the
  `InlineBotResolve` ticket); the member list and event log ones stay in the
  baseline with the follow-up's shapes (05-065 above).
- Merge overlaps with `claude/sweep-r2-followup`: the event log, member
  lists and inline query take its shapes; the request-carrying `Loading`
  stages of the other families, the ownership stages and the `GroupScreenShare`
  pause stay; `begin_ownership_check` is called after its request exists.
### Sweep fixes (r2-30)

Round-two re-sweep, twenty-one files, every entry of the register for them.
A file is done only with nothing left in it, so the extra readers of each
field were fixed too (ids `new-NN` below were found on the fresh read).

| ids | what |
| --- | --- |
| 11-044, 11-045 | `ApplyStage::Applying { waiting, refused }` holds the requests Apply sent and settles each on its own answer (`Session::acks`); `Sent` only when all were accepted, `Failed` with the first refusal. `UsersState::peer_color_error` (one slot shared by three request purposes) and the `has_purpose` busy check are gone. A box closed meanwhile forgets its requests, as the chat look box does. |
| 12-A013 | `photo_edit::render` returns a zero-size picture unchanged (the crop sums underflowed). |
| 12-A017 | `PhotoEditor::turn(Turn::{Quarter, Flip})` rebuilds items, orientation, preview, crop and strokes in one place and ends the gesture. |
| 12-A016 | `Gesture::Item { index, drag }` names its item; `drag_item` reads only the gesture, and `delete_selected` ends a drag whose indexes shifted. |
| 12-A002 | The profile crop's render continuation applies only while `photo_target()` still equals the target that asked, for the result and for its failure note. |
| 12-A015 | The dialog body cells of the edit-profile, color, crop, request-share, schedule and password boxes are `Cell<Option<_>>` taken with `take()`; no `RefCell` borrow is left in them. The same pattern in `polls`, `premium_ui` is those files' own. Review round: the emoji-packs, open-link and save-to-rows boxes use `shell::one_shot_dialog_content` too (the emoji-packs body now sits in the shared scroll region). |
| 12-A005 | `Debounce { Idle, Waiting { query, _timer } }` holds the search timer of the GIF and emoji-pack searches: a keystroke, a closed panel, a tab change or the screen going drops it. The keystroke counters are gone. Review round: every keystroke cancels the waiting search before the empty-field check, so clearing the field in the pause no longer lets the old search run over the restored list (`clearing_the_field_before_the_pause_drops_the_waiting_search`). |
| 12-A003 | `ActiveTrack` is `None`, `Voice(Track)` or `Audio(Track)`; `Track` owns the message, chat, length, meta, row slider, per-window bar sliders, scrub and hover. Stopping replaces it whole, so `track_length`, `seek_slider`, `scrub`, `seek_hover` and `PlayerBarState::{chat, meta, sliders}` are gone. `stop_voice_playback` and `stop_audio_playback` stop only their own kind. After the merge with the follow-up branch the active track takes its shape (`Track { id, chat, meta, length, slider, scrub, hover }`; the bar sliders stay in `PlayerBarState::sliders`); the per-window bar sliders therefore still go with `note_track_started`. |
| 12-A004 | `GifPlayback::{Waiting, Playing}` holds the `GifDecode` guard (dropping it cancels the decode); the `gif_decodes` map is gone. A late answer from an earlier decode of the same GIF is told apart by the guard's flag and dropped. Replaying while a decode runs starts a new decode (the old optimisation of reusing the first picture went with the map). |
| 12-A021 | `Volume::{Level, Muted { restore }}`: the restore level cannot go stale beside the level; unmute never lands on silence. |
| 12-A006 | `PlaylistDrag` carries the dragged track's `FileId`; `reorder_playlist_track(dragged, onto)` resolves both rows at the drop. |
| 12-A012, 12-A014 | The premium sticker paint notes and the played set are one `Notes` per account (cleared on an account change, pruned also while no chat is viewed), reached through `try_borrow_mut` so a paint that lands inside another borrower skips its note. |
| 12-B009 | `hide_check_password_suggestion` resets the card only once the request is sent; a refused send keeps the card and the box says so. |
| 12-B010 | `SectionOp::{Idle, Sending(request), Failed(reason)}` on the edit-profile dialog settles each username or photo operation on its own answer; `UsersState::profile_edit_error` and its eight hand resets are gone. |
| 12-B012 | `ModalFade::Closing { glide, _timer }` holds the close timer; a reopened modal drops the old timer. |
| 12-B015, 12-B016 | The slide is `PullUi::slide`, not a process-wide cell. `Route::Found { next, window }` carries the window of the gesture that found it, and `Route::Jumping` the task; `PullUi::begin_jump` and `restart` change the machine and the route together. A jump whose window closed still ends (`end_jump_without_window`). |
| 12-C013 | `OpenMenu::first_tab` carries the tab a chip asked for; `drain_wanted_reactor_tab` asks for it once the audience begins, and a menu closed first never does. `MessagesState::wanted_reactor_tab` is gone. |
| 12-C003 | `CaptureStart::{Idle, Asking { id, _wait }, Opening { .., _opening }}` is the whole start of a recording: the permission prompt and the capture opening are one stage that holds its task, so a cancel drops the wait. A press during the prompt or the opening is ignored. |
| 12-C014 | `ReplyPanel::{None, QuotePicker, Chooser(Mount)}` is one value in one owner, `ReplyBar::panel` on `ChatUi`: the picker and the chooser cannot both be open, and both go with the reply, a chat change or an account change. `ChatUi::quote_picker` and `ForwardStage::ReplyChooser` are gone. The chooser still draws the share box's destinations, so `show_reply_panel(Chooser)` ends the forward box and `open_forward_picker` ends the chooser (`the_forward_box_and_the_reply_chooser_are_never_open_together`, `the_quote_picker_and_the_reply_chooser_are_never_open_together`). |
| 12-C015 | `RequestShareKind` carries its own stage per kind (`Phone`, `Location { stage: LocateStage }`, `Users { pick }`, `Chat { pick }`); the location box is never opened when the lookup is refused. |
| 12-C018 | `RoundPreview` is a field of the recording (`Capture::round`), not a `RefCell` in `RecordingUi`; its picture retires as an `OwnedImage`. |
| 12-C016 | The window's own mouse-up handler ends every seek (`InlineVideos::end_all_seeks`), so a drag ends with the button even when the ring is no longer painted. |
| 12-C004, 12-C005 | `OnConfirm::{Keep, SendNow { effect }}`; `ScheduleFlow::{Closed, List(ScheduledDialog), Picking(picker)}` (in `scheduled_flow.rs`) replaces `scheduled_dialog_open`, the follow-up branch's `ChatUi::scheduled_dialog` and `schedule_picker`: the list's load stage (`Loading(request)`, `Ready`, `Failed`) lives in the flow, closing the list or opening the picker forgets a load still out. |
| new-01 | The color box says "Loading icons" only while the icons are loading; a request that cannot go fails the fetch. |
| new-02 | `check_remembered_password` leaves the card `Failed` when the send cannot start. |
| new-03 | `round_seek`'s shade cache and the premium notes use `try_borrow_mut`. |
| new-04 | The two "save a copy" prompts (`privacy_extra`, `playlist_manage`) run through `spawn_scoped`. |
| review 2 | `chat_window::slot`: `swap_slot` and `chat_frame_starts` use `try_borrow_mut`/`try_borrow`. A busy slot makes `set_window_scope` report false with nothing moved (the next call, the next frame's, swaps), and the frame start is skipped untouched, so the next call of that frame does it. `send_effects::choose_send_effect` reads the measured areas with `try_borrow` (a busy cell puts the card at the menu's position). |
| review 2 | Sleeps in tests: `the_bar_follows_consumed_frames_not_the_wall_clock` runs on the engine's manual clock; `PullUi::now` takes the executor's clock (`background_executor().now()`), so the pull test advances the dwell with `advance_clock`. |
| review 2 nits | `PullMachine`'s jump stage and `Route::Jumping` stay two values: the machine is the core crate's and knows nothing of the route; `PullUi::begin_jump` and `restart` are their only writers. |

- **Behaviour that changed.** Opening the schedule picker closes the
  scheduled-messages dialog (they were two flags). A press of record while
  the system prompt for the microphone is up is ignored. Replaying a GIF whose
  first decode is still running decodes again. A color or profile request
  that fails after its box was closed no longer raises a note.
### Sweep fixes (r2-39)

Files: notifications, notify windows, os menus, passcode, peer QR, polls,
premium UI, privacy radio rows, profile panels, profile photo sources and
proxy. Every entry of the bundle is fixed or shown false:

| id | what changed |
|---|---|
| 09-118, 11-031 | `ChatListUi::show_view` is the one mover of the folder tab and category. It ends the selection and a pinned drag, and the poll's "folder deleted on another device" fallback uses it too. |
| 11-033 | `NotifyView::actions` is gone: the Reply button is derived from `hovered`, `reply` and `can_reply` when drawn. `reply` holds the field entity itself, and the manager's `Stage::Replying` is its own owner of the hide timer, fed by `ReplyOpened`. |
| 11-054 | `MenuSync::registry` is `RegistryRead::{Never(default), Read{registry, at, writes, clients}}`. |
| 11-051 | `PasscodeUi::panel: Option<PasscodeBox{view, error}>` replaces `open`, `view` and `error`. Closing drops it whole, so a late job error has no box to land in. |
| 11-052 | `SystemUnlock::{Off, On{held_key}}` replaces `system_unlock` and `system_unlock_key`. The key exists only inside `On`. |
| 11-053 | `LockGate` and `PasscodeUi` share one `Rc<Cell<LockState>>`: one flag, written by `set_locked`. |
| 11-046 | `ToastStage::{Hidden, Showing{toast, _timer}}`: the timer task is the stage's own, so a newer toast or closing the box cancels it. The clock stays an owned `Task`. |
| 11-047 | `PanelMotion::Closing` settles to `Idle` (`settle`, called from `qr_refresh`) and `visible(now)` reports `None` once a close has run, so a closed gifts panel holds no list. |
| 11-048 | `OpenQr::swap` is dropped by `qr_settle` after `SWAP_MS`. |
| 12-A007 | `PollDialog::rows: Vec<PollOptionRow{input, attach, correct}>` replaces `option_inputs`, `attach` and `quiz_correct_rows`. Removing a row removes all three. |
| 12-A008 | `PollDialog::close: Option<PollClose>` (None is off) and `countries: Option<Vec<String>>`. Turning a switch off forgets its value. |
| 12-A009 | `set_media_panel_mode` ends every `AwaitSticker`/`AwaitEmoji` when the panel leaves the compose mode (`end_poll_option_waits`). |
| 12-A010 | `OptionMediaSlot::Preparing{path, read: ReadId}`: a file read answers the slot holding its `ReadId`, never the first one with the same path. |
| 12-A018 | `GramWithdraw::Done(WithdrawOutcome)` carries the outcome in the stage (the `withdraw_outcome` slot is gone). `grams_outcome_window` names the window that takes it: the box's host, else the Grams screen's, else main when that one closed. |
| 12-A019 | A limit toast is claimed by the window of the last press (`PaymentUi::limit_toast`, per account) and shown in that window's next frame, not at a later press. |
| 12-A020 | A form that names a gift lives in that gift's panel: `GiftView::Form{id, form: GiftForm::{Upgrade{keep_details, ask}, Resale{ask}}}` (`premium_hub/gift_view.rs`). `GiftFlow` keeps only the flows that name no shown gift (the collection forms, and the transfer picker, which is its own box `DialogKind::GiftTransfer` with a request of its own). Selecting another gift, `restart` (another collection, a new owner), `close`, the upgrade answer and the delete answer all end the form by replacing the view; `form_sent`, `form_refused` and `close_form_answered_by` match the answer by `RequestId`. `PaymentsState::select_gift` is gone: nobody writes a selection next to a separate `flow = None`. Tests: `a_form_belongs_to_its_gift_alone`, `only_the_form_that_sent_a_request_closes_on_its_answer`, `a_gifts_form_ends_with_its_panel_on_every_path`. |
| 12-B019 | `mutate_privacy_detail` says whether it sent; the exception picker closes only then. |
| 12-B017 | `ProfileDialog::owner()` (kind and person) is compared when the photo picker answers, so another person's Edit contact is not mistaken for the box that asked. |
| 12-B018 | `user_photos_request_ended` ends a user's pending gallery when the first page's request is gone, with a note. |
| 12-B011 | `drop_profile_dialog` is the one way a profile box goes; replacing a box (`open_profile_dialog`, the birthday privacy link) forgets its `Saving` watch or its `ContactSave` answer. A saving box takes no click (`block_while_saving`). `ProfileDialog::Saving` (answers in `saves`) and `ContactSave::Saving` (answers in `dialog_answers`) stay two stages: they wait on different answer channels, each carrying its `RequestId`. |
| 12-B014 | `GradientEditor::hsl()` is derived from the selected colour; the only stored extra is where the sliders were last dragged, valid only for that exact colour. |
| 12-B013 | `PhotoSources::stage: SourceStage::{Closed, Emoji, Camera(CameraDraft)}` and one `close()`. `CameraStage::Starting` and `Saving` hold their tasks, so the serial is gone. |
| 14-068 | `video_encode_failed` is the one failure transition of the video editor (tray, edit box, avatar): ready again, with its player back. |
| 12-B005 | `ProxyNotice::{Hidden, Showing{text, _timer}}`: the line under the list hides itself. |
| 12-B006 | `remove_proxy` is sent first; the system-proxy fallback is set only when it went out. |
| 12-B007 | One owner: `ProxyState::link: LinkBox::{Closed, Review(draft), Warn(draft), Checking{draft, request}, Available{draft, ms}, Unavailable(draft)}` in the session's proxy state; `ProxyUi::link` and `ProxyLinkBox` are gone. Opening a link (`open_link`), Check (`warn_link`, `link_checking`), Connect (`take_link`) and closing (`close_link`) are transitions of that value, and `settle_link` settles only `Checking` under its own request, so a check cancelled by closing the box does not touch the next link's box. `ping_link_proxy` reads the draft from the stage. The demo session holds the box too. Tests: `the_link_box_is_one_stage_that_holds_its_draft` and the connect test of two link checks answered in either order. |
| 12-B008 | The sign-in screen's own state (`AuthUi::proxy_list_open`) holds the box; the Ready transition (`follow_auth`) ends it. The per-frame sync is gone. |
| review nits | `scripts/dialog-hosts.txt` and `scripts/session-dialogs.txt` name `auth.proxy_list_open` (was `proxy.list_box`). |
| 12-A008 (nit) | Turning Limit Duration off forgets the chosen deadline on purpose: `close: Option<PollClose>` is the one value, and re-enabling starts from the default period like a fresh box. Remembering it would be a second field kept beside the stage. |
| 12-A007 (nit) | Turning multiple answers off in a quiz keeps the first marked row by position. The box stores no click order, and the tdesktop `CreatePollBox` source is not in the sandbox to say which row it keeps; this is the smallest safe choice. |
| fresh review | Re-read from the top after the fix round, with the re-sweep criteria (implicit stages, renames, answers not matched by `RequestId`, detached tasks, re-asks from render, crash risks): `premium_hub/gift_view.rs`, `gift_manage/state.rs`, `state/domains/payments/{state,gift_manage}.rs`, `connect/gift_manage.rs`, `ui/{gift_manage_ui,gift_owner_actions,premium_ui/mod}.rs`, `proxy.rs`, `state/session_proxy.rs`, `connect/proxy.rs`, `ui/proxy/{mod,build_proxy_edit_dialog}.rs`, `ui/settings_state.rs` (the link box closes in the account's farewell). 0 findings left in each; no new `unwrap`/`expect`/indexing, no new `RefCell` borrow, no timer, and the four shape guards pass with no new hits and nothing baselined. |

### Sweep fixes (r2-05)

| id | what changed |
| --- | --- |
| 10-058, 10-059 | `send_supergroup_members` (one seam for fetch and refresh): a refused send puts back what was there with how it was asked (a page stays shown, a failed list keeps its Retry), and a list that had nothing becomes `Failed(send_failed_line)` under the refused query, so no picker reads "Loading" for a request that is not out. The Add exception / Remove picker's Failed arm has a Retry. |
| 10-066 | `check_group_username` records `UsernameCheck::Failed` when the send is refused; the status line says the check failed and Save asks again. |
| 10-067 | The private Type box's link block matches `InviteLinkFetch`: Failed shows the reason with Retry, Loaded without a primary link says "No invite links." |
| 10-068 | A refused link-count send is `InviteLinkCountsFetch::Failed`. Save with a changed join approval waits while the count is still loading (the button is drawn busy), as tdesktop waits for `requestInviteLinksCount`; a count that failed saves without the question, as tdesktop's `fail` handler does (`done(0)`). |
| 10-069 | The pause before the server check is a `Task` held in `LinkCheck::Pausing` (kept by `Refused`): the next keystroke or closing the box drops it, so no version counter is needed. |
| 12-B001 | The quick reply poll ends when nothing is awaited: no accepted text (`TextSave::Accepted` waits for the shortcut id) and behind the lock. |
| 12-B002 | The text save settles on its own request through `acks` (expected when sent, forgotten when the screen or box goes); a request that left flight with no answer gives the text back. |
| 13-043 | `LiveJoin::Joining { target, request }`: when the `getGroupCall` request is gone with no tracker and no pending join, the join is `Refused` and "Join live" shows. Retry of a failed stream joins through the same path (a refused retry is `Refused` too), so no join is left without a request. |
| 13-047 | A refused `getStory` send starts the story's retry clock, so the pending open sees a failed ask and is dropped with the note (or the repost's gone box). |
| 01-002 | `take_turn` returns a `Turn`; the new-person, chat and any-chat slots are kept only on `commit()` after the send or write went out, and a dropped turn gives them back. Tests book on a `State` of their own with a given `now`; none sleeps. |
| 01-022 | `BoostWork::Slots` and `Boosting` carry their `RequestId`; an answer or error applies only when the box waits for that id. A press adopts a slots read still out by its id. |
| 01-023 | The note lives in the phase (`Main { note }`, `Reassign { note }`), so it leaves with it. |
| 01-034, 01-035, 01-036 | `ShareLink::Fetching(RequestId)` is entered only once the request went out, so a refused send leaves the slot idle. `answered` and `failed` apply to the slot that waits for that request, so a revoke's late answer is dropped even when a newer click is already fetching. `ShareLink::Unavailable` is recomputed at each click (a member promoted to admin gets the speaker link asked). |
| 02-003, 02-004 | One rule, `ComposerAttachment::sends_as_files`, for the checkbox, the next drop and the send options; a file kept as a file counts neither way. |
| 03-B003 | A file call that times out is `Skip::Unavailable`, not an aborted chat, and an unanswered release of a file already saved leaves the cache copy as a leftover. An error mid-chat is `ChatOutcome::Partial`: the messages saved so far get their page, and the row and summary count them (`partial_chats`, with a line in the finished box). |
| 03-B004 | `AccountStage::ChatsList { found }` and `Chats { index, count, name, chat }`; `Stage::Collecting { read }` and `Writing { done, total }`. The next chat starts from `Starting`, so no stale writing counts. |
| 03-C001 | A handler read that dies is stamped now (or "could not read" with no answer), so the page does not start another read at each draw. |
| 10-065 | Skipped: tdesktop shows no result for an unban or a kick in the picker, a refusal lands in the member action error, and no stage is stuck. |
| 12-B004 | Skipped: the asked step is drawn on the button until the pane is replaced, so a later press is the user answering the question they see. |
| 13-048 | Skipped: a waiting open can coexist with an open viewer (a repost press pauses it), so the two are independent; the Option is the whole stage. |
| 01-010 | Fixed with 01-002. |

### Sweep fixes (r2-06)

| Id | Fix |
| --- | --- |
| 03-C010 | A map tile whose ask failed (or could not be sent) is asked again up to three times in a row (`TileState::Failed(count)`), so a view left alone does not stay blank after one lost answer; after that it waits until the map rests after a move (`move_ended`) or its size changes, which drops the failure. |
| 03-C012 | `moving` and `places_in_area` are one `Pin { Resting, Moving, RestedAway }`. |
| 03-C011 | Skipped: `venues_failed` shows the empty-list text; tdesktop's venue list has no separate error row either, and the retry is the next move or search. |
| 03-D007 | The admin delete box sends the moderation plan first; a send that fails keeps the box open, deletes nothing and notes "could not delete" (the steps are idempotent, so a retry repeats them harmlessly). The community ban waits for the plan. Server refusals of a step are not matched back to the box (no UI test can install a failing live driver). |
| 06-035 | Opener threads are `WorkerLife` too (`State::openers`, `Thread::{Draw, Opener}` in `WorkerEnd`); a clip an opener decodes is `Stage::Decoding { opener }`, and when that opener ends the clip fails. With no opener left `workers_decode` flips so the draw workers decode the waiting clips. `worker_gone` is gone: `wait_idle` reads the per-thread lives. |
| 06-036 | Skipped: `Clip::delivered` is a one-way "first frame handed out" latch, not a stage: nothing waits on it and no exit is missing. |
| 07-052 | The AI answers are one queue `MessagesState::ai_answers` of `AiAnswer { request, window, chat, result: Text / Blocks / Failed }`, replacing the two single-slot `Option`s and matched by the request. `AiAsk::Asked { request, markup }` applies only the answer of its own request; a failure answer returns the ask to `Idle`; a newer ask makes the older answer stale. |
| 08-016 | A background client that died or failed to start is restarted by the poll after a delay that doubles from 5 s to 5 min per failure in a row (`background_accounts::restart::Restarts`, pure and tested with explicit instants); reaching `Ready` forgets it. The poll loop sleeps until the due time instead of ending. A client that is due while nothing may start it (the app is locked, or its old client is still closing) is put off by the first delay (`Restarts::put_off`, not a failure), so it never keeps the poll at its fastest pace. |
| 10-044 | The lock closes the invite/share box (`close_call_box`). |
| 10-072 | `begin_topic_editor` keeps an editor whose Save is `Sending`. |
| 10-073 | `TopicEditor::original_emoji` deleted (never read). |
| 10-079 | The one-shot body cell uses `try_borrow_mut`, so it cannot panic. |
| 10-081, 10-082 | `Decode::Decoding` owns its task (a replaced decode is dropped); the headroom poll task lives in the `Shown` frame, so `HeadroomPoll` and the self-reset are gone. macOS and Windows only; not run on Linux. |
| 10-083 | Skipped: a failed HDR decode is stored as `Sdr` on purpose, the SDR picture is what shows, and not retrying stops the render from re-decoding a file that cannot be read. |
| 11-002 | `Reorient::Decoding` owns the decode task; the token and `is_decoding(token)` are gone (a stage the viewer left drops the task). |
| 11-004 | `SavedToastStage { None, Shown { toast, _hide } }` replaces the `Option` and `saved_toast_gen`. |
| 11-009 | `ControlsVisibility::Hidden { fade: FadeOut::{Fading { _end }, Gone} }`: the fade-end redraw is a scoped task the stage owns, and render reads `is_gone()` instead of `Instant::elapsed`. |
| 11-030 | A reply typed into Quill's own toast that is not sent is kept in `unsent_replies` (`keep_unsent_reply`, shared with the platform path) for the chat that opens. |
| 11-034 | `Shown::unlink(now)` sets the stage and fades the window out together. |
| 12-B003 | `drain_show_or_premium` settles on the `Saving(request)` id through `Submit::view`, not on the privacy key's state. |
| 13-022 | Still picks check the download send like the video path (`ask_pick_file`); a refused send ends with a note. |
| 13-056 | `touch_bar_reorder` maps `Ok(None)` (a reorder in flight, or nothing to send) to `reorder_refused`. |
| 14-024 | Sound is wanted (`options.want_audio`) only once the reopen succeeded. `Player::open` returns at once and fails only when its thread cannot start, so no test can make it fail. |
| 14-025 | A seek, step back or reopen drops the pending stepped picture. |
| 14-042 | A failed message translation is dropped when the chat is shown again (a new pump scan), so it is asked once more; not on every pass. The scans of chats no window shows are dropped at the start of every pass, before the early returns, so closing the chat or leaving it for one that is not translated counts. |
| 14-043 | `ToastStage { None, Shown { toast, _hide } }` replaces the `Option` and `toast_seq`. |

### Sweep fixes (r2-08)

| id | result |
|---|---|
| 08-042, 08-041 | `MicTestRun::Failed(reason)`: the watchdog moves a device that errored to `Failed` (the test is over; the reason shows and the next click on "Test microphone" starts a new test) instead of the render ticking at 30 fps before its error return. The watchdog `Task` lives in `MicTest`, so stopping the test drops it and a restart cannot leave two. `mic_test_starts` stays: it is the id generator of `Opening(id)`, not a stage. |
| 09-024, 09-025 | `DeviceLogin::{InFlight(RequestId), Done { request, result }}` and `DeviceLink::Linking(RequestId)`: the answer is taken only by the request that asked (`take_result_for`). `clear_device_qr` no longer resets the session's `device_login`; leaving the Devices page, or locking the app, keeps a link that is out (`leave_device_link`), so its answer is shown and no second link is sent beside it. |
| 09-108 | `apply_force_reply` returns whether it armed; the pending target is removed only then, so a force reply that arrives while a reply is in the composer waits instead of being lost (and the keyboard is not dismissed for nothing). |
| 10-041 | Esc in the voice chat window also closes the share and invite boxes, topmost first. The full `CallWindowBox` enum was not built: privacy, Settings and rename live in other owners. |
| 10-062 | `boosts_view`: a failed first page draws its error and Retry, a loading one draws "Loading", "No boosts yet" only for a page that really came back empty. |
| 10-092, 10-095, 10-096 | A failed `@bot` lookup is dropped and asked again on the next composer event (and goes with the trigger); a failed inline-query send lands in the slot as `Failed` so the panel shows it; a pick with no connection keeps the typed query and says so. |
| 10-093, 10-094, 10-097 | Skipped. 093: the session slot is already matched by `generation` on every answer, and a window copies only settled answers (an immutable username fact); the re-ask is the designed recovery. 094: `inline_query` is written only when no query of that chat and bot is in flight (`has_inline_query_in_flight`), so its answer cannot belong to another request. 097: a once-acknowledged latch beside the ask stage (`bot_alert`), not a stage of one thing. |
| 11-001 | The sticker manager is marked open only after `open_mask_sets` succeeded; a refused open returns before the box. |
| 12-A001 | `MediaEdit::render: EditRender::{Idle, Rendering(n)}`: the render of a message edit's photo lands only when the edit still waits for its number; a cancelled edit takes the stage with it. |
| 12-C001 | `take_voice_capture` stops the record preview, then takes the capture; both Sends use it. |
| 13-009 | `Host::Asked { window, waits: AskWaits }`: an ask ends when the requests in flight once the asking code has run have settled and the flag is clear. A box is asked for before its request is sent, so a live ask starts `AskWaits::Unread` and the next dialog sync reads the requests then out (`read_ask_waits`); the request that opens the box is one of them. Requests made later neither hold it nor end it; a demo's ask never expires. |
| 13-010, 13-011 | Skipped. 010: folding the host map, `QuillShell::open_dialog` and `dialogs_closed` is a shell-wide redesign, beyond a local change. 011: both borrows are statement-scoped with nothing called while held. |
| 13-035 | A statistics send that fails puts the story's insights in `StoryStatsFetch::Failed` (`story_statistics_not_sent`), so the panel shows its error with Retry. |
| 14-079 | `AgeCheck` moved into `UpdatesSync::age_verification` (`AgeVerification { params, check }`): it ends when the server lifts the request and a repeat keeps it. `AccountBoxes::age_check` is gone. |

### Sweep fixes (r2-12)

Round 2, 22 files of the agent tools and driver, the calls engine and
capture, the composer and the connect bots and backgrounds. Every file ends
with no entry left; each id below is fixed or proven not to be a bug.

| id | what |
| --- | --- |
| 01-014, 01-015 | `Asked` is one enum (`Off`, `Reserved{options, slot}`, `Skipped(note)`). `shot::Slot` gives itself back to the limiter on `Drop`; `keep()` is for a picture that was taken, so every early return releases it. |
| 01-017 | Skipped: the composer clears only the text the send delivered, and the match on chat and text is that guard, not a correlation. A retyped identical text was delivered too. An id would have nothing stored on the window side to match against. |
| 01-018 | `pin_uploads` runs at the paced-queue turn inside `common::send` and before the direct `td_call` writes: it refuses the data folder again and rewrites each local path to the file it resolves to now. |
| 01-019 | `set_draft` reads the chat's draft first and refuses to replace or clear a different non-empty one unless `replace_existing` is true. |
| 01-020 | `guard::Injecting` is a scope guard (depth, restored on `Drop`) replacing `set_injecting(bool)` in the driver; `Commands{Closed{latest}, Open{step, count}}` replaces the `open` count and the `step` slot. `set_injecting` stays for tests only. |
| 01-021 | The revival of an ended session is `Session::revive_unnamed`, one named transition. |
| 01-026 | `DeviceThread::opened()` keeps its answer (`Opened::{Waiting, Answered}`): asking again returns it. |
| 01-027 | A PipeWire `Unconnected` ends the share from any phase (`StreamPhase::Gone`, once). |
| 01-028 | `WindowMonitor` no longer joins its thread on drop: it checks the stop right after the ScreenCaptureKit wait. |
| 01-029, 01-030 | A Failed or Ended feeder sends nothing (`NotReady` / `ResyncNeeded`, time answers 0). `Syncing` is no longer set by the time request; `StreamsSync::Asking` is the one record of a request in flight. |
| 01-031 | `StreamQualityTracker` is deleted: the engine's own `stream_quality` is the record, so a width reported before the engine has the call is reported again. |
| 01-032 | `CreateJoinAs.stage` is `JoinAsStage::{Asking, Answered, Failed}`; `accept` and `fail` take only the request that asked. |
| 01-033 | `GroupCall::playback_muted` is deleted; the reopen reads `SoundSlot::muted()`. The viewer's stream volume is a global preference, the slot's volume the per-call applied value, so not copies. |
| 01-037 | `RtmpStart::revoking` returns whether it recorded; the driver sends no second revoke while one is out and refuses one with no box. |
| 01-038 | `peak` lives in `HintStage::Listening`. |
| 01-039 | `SampleRing` loads `read` before `write`, and a gap over capacity counts as empty. |
| 01-040 | Keycode span uses `saturating_sub` and `saturating_add`. |
| 02-008 | `confirm_delete(confirm, cx)` takes the box's `DeleteConfirm`; `ChatUi::pending_delete` is deleted. |
| 02-012 | `ReplacementProbe::Probing{job}`: only the answer of that job settles it. |
| 02-021 | `BotsState::login_url: LoginUrlStage::{Idle, Asking{request, context}, Answered{info, context}}`; recorded after the send, answers matched by request. |
| 02-023, 02-024 | `InstalledBackgrounds` holds one `Fetch` per theme, begun once, `sent(extra)`, answered by request. |
| 02-025 | `InlineQueryFetch::Loading{request}` and `Loaded.more: InlineMore::{Idle, Asking{request}, Failed{offset}}`; the slot changes after the send. |
| 02-026 | `ComposerSnapshot.content: SendContent::{Text, Single, Album}`. |
| 02-027 | `GifMode::HeldAsFile` remembers the animation while "Send without compression" is on and returns it. |
| 02-028 | `AttachmentExtras.video: VideoPrep::{Untouched, Edited(VideoEdit{edits, start_timestamp, source})}`. |

Outside the bundle's files, the smallest changes: `src/ui/agent_driver.rs`,
`src/ui/selection_mode.rs`, `src/ui/chat_ui_state.rs`, `src/ui/inline_grid.rs`,
`src/ui/video_editor.rs`, `src/ui/send_prep.rs`, the connect drivers for
revoke, stream tile width and the login URL, and the tests that built the old
shapes. `connect/messages/media_send.rs` still takes `album().len() - 1`
unguarded; it belongs to another bundle.

### Sweep fixes (r2-09)

Round-2 re-sweep of 20 files (archive_mute, battery_signal, chat_bottom_bar,
chat_swipe, chatlist_menu, chatlist_suggestions, checklist, child_reap,
collectible_info, corner_buttons, data_settings, icon_badge,
invite_link_form, launch_lines, launch_prewarm, local_path, local_time,
main, marketplace, media_capture). Every file ends with no entry open.

| id | result |
| --- | --- |
| 01-024 | `ArchiveReload` is `Idle`, `Suggested`, `Reloading(RequestId)`, `Failed`. `note_settings` and `note_reload_failed` take the request, so a fetch that predates the suggestion does not end the wait; a suggestion sends a fresh fetch even while an older one is in flight (the older answer no longer matches the `Fetch` stage). |
| 01-025 | The listener's end runs from a drop guard (`ListenerEnd`), so a panic that `spawn_guarded` catches also returns `STATE` to not listening and the watcher polls again. |
| 01-043, 01-042, 02-009, 02-010 | Not stages: a fact derived per render (`bot_start_pending`), facts about the chat (`is_saved`, `is_saved_messages`) and a TDLib permission (`others_can_mark_tasks_as_done`, five structs). Moved from the baseline to `implicit-state-allow.txt` with the reason. |
| 01-041 | `Gesture::key` and `Gesture::action` are one `target: Option<(K, A)>`. |
| 02-007 | `PhoneSuggestion { Absent, Asking, NotMine }` replaces `PhonePrompt` beside the `actions` set: the phone action carries its own stage and `answer_phone_no()` is the only way to `NotMine`. |
| 02-011 | A child no thread could reap is parked with its notice sender; every call into `child_reap` polls the parked ones, and the notice goes out only after the child is reaped (it used to disconnect at once, read as "helper gone"). |
| 02-006 | `LookupStage::Unsent` is gone: the UI builds a `LookupSpec`, and `request_collectible_info` constructs `CollectibleLookup` as `Loading(request)`. |
| 03-A001 | `FastShareFacts::sent` is `send: SendState` (a local id counts as `Sending`). |
| 03-A002, 03-A003 | `Seed` is `Unseeded`/`Seeded` only (the persisted bool). The presets request is the `Fetch` stage `auto_download_presets` (`sent(request)`, answered by its request id, failed by its error arm), which the UI reads for "loading"; the page also gets a Retry. A refused storage-limit option leaves the seed `Unseeded`, so the next Ready sends the (idempotent) options again. |
| 03-C003, 03-C004, 03-C024 | One `BadgeStage` (`Idle`, `Pending`, `Delivered`, `Failed`) replaces `LAST_SENT`, `INVALIDATED` and `Launcher` on every platform. The Linux worker handle is held in a static and restarted when it finished or could not be spawned; its end (drop guard) hands a pending count back. The Windows overlay uses `try_borrow_mut`. The slot test uses a channel handshake, not a sleep. |
| 03-C014 | `InviteLinkForm::kind: LinkKind { Plain { approval }, Subscription { locked_stars } }` replaces `request_approval`, `subscription` and `locked_stars`. |
| 03-C006 | The line-layout slot writes only the first account that opened; a second account (`Mixed`) or a logged-out one (`Forgotten`) ends the write for the launch, so two accounts' chat names never share a file. |
| 03-C005 | `GlyphSave { Pending, Saving, Saved }`; a writer that cannot be started goes back to `Pending` and the next drawn frame tries again. |
| 03-C007 | The webm sticker worker and the file-thumbnail worker read their scratch folder before they are spawned; `sweep_account_media_cache` takes the account that logs out. The on-screen scope stays for UI-thread producers. |
| 03-C008, 03-C023 | A display check that began before a sweep keeps no verdict (`SWEEP_GENERATION`); the sweep test joins the sweep thread. |
| 03-C019 | `utc_offset_at` uses `try_borrow_mut` and asks the OS directly when the cell is taken. |
| 03-C025 | The start-in-tray reveal is `tray_reveal::StartupWatch` held by the tray's chrome loop (one pass per look, a 100 ms due while it waits), not a second detached polling task. |
| 03-C002 | `accept_result` and `fail` act only in the stages they leave. |
| 03-C020, 03-C021, 03-C022 | A device listing that did not come back is an error ("Couldn't list the cameras and microphones"), not an empty list; the audio backend is cached only after a probe answered; the probe test injects `Patience` instead of a real timeout. The probe still runs on the thread that starts the capture, bounded by its 3 s limit. |

### Sweep fixes (r2-03)

Round 2, files `connect/{call_screen, composer, gift_manage, premium_hub,
profile_tabs, ton_wallet}.rs`, `connect/core/{mod, pumps}.rs`,
`connect/group_calls/live_story.rs`, `connect/groups/group_type.rs`,
`connect/messages/mod.rs`.

| id | result |
|---|---|
| 02-015 | A microphone, level-tap or screen-share failure is put on the call by `pump_call_media`. The three are compared one by one, so a failure that still stands does not hide a new one of another device. A joined group call takes only a new failure, in its `group_call_error` line (the user dismisses it). The 1:1 call's `ActiveCall::media_note` follows the devices: the newest failure, else one that still stands, and nothing once they work again. |
| 02-029 | The installed-sets answer replaces `suggest_for` only when it is `WaitingForSets`; `Asked(emoji)` stays. |
| 02-030 | Both sends call `cancel_pending_draft_of(chat_id)`: another slot's waiting save (and its dirty mark) is left alone. |
| 02-045, 05-030, 02-046 | `CollectionsFetch::Stale` stays until the send went out (`Loading` is set there); a refused first ask in `begin_gift_manage` leaves `Stale`, so the next ingest asks again. |
| 02-056 | `ConnectDriver::claimed_owner` is `OwnerClaim { Unclaimed, Saved(user), Unsaved(user) }`. A failed owner-file write is retried by the next ingest with `account_data::write_owner` only: a second `claim` would take the new user's own files for leftovers and drop them. The failure is printed once, not on every ingest. |
| 02-067 | Undo takes the pending Stars only after the removal went out. |
| 02-070 | `UsernameCheck::Checking` carries the check's `RequestId` (the generation counter and its purpose payload are gone); only that request's answer or error ends it, and a refused second check leaves the first one answerable. |
| 02-081 | An older page that failed on a window that holds messages is no longer asked for from the list's paint; Retry, a new message, a scroll away from the top edge (`preload_history_by_scroll`) or opening the chat again (`prepare_history_window`, for a window too short to scroll away from its top) clears it. |
| 02-087, 02-049 | `PremiumHub::close_gifts` drops owner, list, page, notice and a stale mark in one place (`reset_gifts_list` is shared with opening and switching collection). A change in flight keeps its lock until its own answer. |
| 02-088 | A refused remove or reorder marks the playlist outdated (`playlist_change_unsent`), so it loads again. |
| 07-013, 02-108 | `open_money_page` runs every ask and returns the first error: a refused explorer, state or rates send no longer skips the history page. |
| 02-013 | A refused pause send puts `screen_share_paused` back, as the error arm does, so the next edge goes out. No automatic resend: an error answer reverts the same mark and a resend would loop on a server error. |
| 02-016, 02-017 | `Share::Waiting { pick, since }` replaces `Picked { wait: EngineWait }`; the wait has a deadline in `next_deadline`. The level tap's 5 s reopen is not added: the pump does not leave a failed tap when no call wants it, so a deadline would spin. |
| 02-018 | With no call left, a pick or an ended share's error is dropped. |
| 02-033 | `data_storage_dirty` is cleared only after the save returned Ok. |
| 02-034 | A refused send in the post-ingest pumps or boost is returned at the end of `ingest`, after the Ready kickoff and paging. |
| 02-035 | The AI composer answers are queues; a window takes its own, answers of closed windows are dropped. |
| 02-068 | `LiveBursts::{Unasked, Requested(RequestId), Loaded, Failed}`, asked from its own step in `maybe_join_live_story`; a failure waits for the next Join instead of every ingest. |
| 02-074 | A commit that did not go out puts the Stars back as pending with a new Undo window, and no burst effect is queued for them. |

- **Not done:** 02-014 (the capture's visibility and what Telegram was told
  are two facts; 02-013 makes the told one honest), 02-048 (needs the gift
  id inside the shared `ListOp`, used by other lists), 02-050 and 02-096
  (whole-dialog enums over the send-gift and hub dialogs; no bug beyond what
  is listed above).

### Sweep fixes (r2-04)

| id | result |
| --- | --- |
| 03-D001 | The Shared Media viewer asks the older page only while the tab can load more on its own (`can_load_more`: `NextPage::Idle`); a failed page waits for the panel's Load more row. |
| 03-D002 | `PhotoPage::Failed(error)`: a failed profile or group photo page is not asked again from the poll. A step in the viewer (`PhotoPage::retry`) lets it ask once more. |
| 09-014 | `fetch_history_for` stops on `OlderLoad::Failed` for a window with rows too. The main history draws the older Retry strip (`OlderRetryFor::Chat`) that calls `retry_history`. |
| 09-015 | The newer end gets its own Retry strip (`NewerRetryFor` Chat, Thread, Topic) calling `retry_history_newer`, `retry_thread_newer`, `retry_topic_newer`. `TopicHistory::retry_newer` has its caller. |
| 09-030 | The per-network editor draws the auto-download failure. |
| 09-031 | `data_storage_save` is `StorageSaveFailures`: one failure line per kind of save (auto-download, limits, statistics reset). An `ok` clears nothing; a new save of the kind, or arriving on the page, does. |
| 09-035 | `DataStoragePrefs::seeded` is `PresetsSeed` (`Unseeded`, `Loading(RequestId)`, `Failed(why)`, `Seeded`); `Seed::InFlight`, `auto_download_presets` and the `has_purpose` check are gone. The page draws the failure with Retry. |
| 09-036 | `auto_download_answer` is `AutoDownloadAnswers`: the editor takes the answer of its own request, and answers of several saves in flight no longer overwrite each other. |
| 09-034 | `leave_data_storage` resets the scroll stage; `open_local_storage` asks for the scroll only once the page is up. |
| 09-109, 09-110, 09-111 | `FolderShareView::List` and `Edit` each carry their `LinkOp`. Edit and delete wait while a link request is out. The form's Cancel and a new form drop the stage with the view. |
| 09-112 | `CopyMark::Copied` owns its timer task; replacing or dropping the mark cancels it. |
| 09-115 | A folder fetch refused before any request (`chats_path_active`) ends in the error the Share, Delete and Add to folder boxes draw with Retry. |
| 08-113 | A rich send whose chat view is replaced moves to `ComposerSends` (`LeftSend`, per account, with its slot). Its refusal puts the text in that slot's draft; its ack no longer leaks. |
| 08-114 | `AiAsk::Asked { request, markup }`; AI answers carry the `RequestId`, and the window applies only its own ask's answer. A newer ask drops the older answer, and an error (`ai_failed`) ends the wait. |
| 08-115 | The open box holds its list's load (`ScheduledLoad`: `Loading(RequestId)`, `Ready`, `Failed`; see 08-091 for the box) in place of `scheduled_dialog_open`; the dialog draws loading, failure and Retry. |
| 14-067 | `PhoneFlow::Unblocking { since, request }`, finished by that request's answer or error (the 15 s timer stays as the backstop). |
| 14-076 | One `close_refused_launch` for every early return of `spawn_mini_app_window`. |
| 14-077 | `custom_requests` and `CustomRequestReply` carry the launch id; a window takes only its own launch's answers, and closing a window forgets its entries. |
| 14-082 | `closing_confirmation` is a mini app setting (allow list); its baseline line is dropped. |
| 09-017 | The swipe-reply press is cleared when the button is released outside its row. |

- **Not changed, on purpose.** 08-116: tdesktop removes a scheduled message
  from its list as the delete goes out, as Quill does. 14-078: the answers
  are per bot and the flows wait for one bot each; a repeat answer for the
  same bot is the same content. 14-086: the 30 ms poll stops by itself when
  no window or flow is left; a wake from the helper's reader thread is a
  separate redesign.
- **No new CI guard.** The shapes (a failed page asked again from render, a
  stage matched by bot id) are about which state a function reads; the
  tests walk each exit instead.

### Sweep fixes (r2-01)

| id | result |
| --- | --- |
| 07-025 | `submit_delete_account` sends first: only an accepted send clears the password and reason and leaves the armed banner. A refused send keeps both and the banner, with a "Couldn't delete the account" note. |
| 07-026 | `submit_account_ttl` and `submit_default_auto_delete` note a refused send. |
| 07-034, 13-066 | the one-shot body goes in through `shell::one_shot_dialog_content`, a `Cell` (`take`, no borrow that can panic), used by every dialog builder this bundle owns or touched (account box, Money, Translate, the checkout and Subscriptions boxes, Manage Members). A later call of the kit's `Fn` closure draws nothing, as the builder runs again with the next render. |
| 08-047, 12-C002 | `PermissionAsk::Asking` owns the wait task (dropping the stage ends it). The wait is screen-scoped (`spawn_screen`): the answer ends the stage by its id whichever window asked, and the recording starts through the asking window's own handle only while that window is still there. There is no fallback through the main window. |
| 09-095, 09-097 | a first page of the event log that cannot be sent (path inactive, sender closed) is `ChatEventLogFetch::Failed` with the Retry the page already draws; Save filter / search failures land there too. |
| 09-096 | an older page that was refused or never sent is `OlderEvents::Failed(line)` on the loaded page (it replaces `has_more`: `Exhausted`, `Available`, `Failed`). The list draws the line with Retry in place of the "Loading earlier actions" spinner, and the automatic load asks only while `Available`, so a failing server is not asked in a loop. Asking again clears the line. |
| 09-098 | the id list is a `Cell<Vec<_>>` taken out before the list is reset, spliced or remeasured: no borrow at all. |
| 09-099 | the poll tick does nothing while locked. |
| 09-100 | the search box belongs to its `EventLogPage` (`page.search`, with the typing subscription): another chat's page has a box of its own, built with that chat's stored query. `shows` and the per-frame resync are gone; upkeep only builds a missing box. |
| 10-040 | the poll tick ends the timer when the window is inactive (the page's `poll` is `None`); a window waking (`follow_windows` gate) restarts it with an immediate poll (`resume_event_log_poll`). |
| 10-012 | the Gift Premium or Stars box draws contacts Loading, and Failed with Retry (`retry_gift_recipients`). The frequent-contacts fetch stays best-effort: failing it only drops that optional section, as in tdesktop. |
| 10-043 | `open_group_call_invite` returns while locked, like the share opener. |
| 10-055 | the invite picker draws a failed contacts fetch with Retry and a loading line, not "No contacts to invite". `fetch_contacts` itself lands a list it cannot ask for (chats path down) in the failed stage, so the picker never draws "Loading contacts..." for good. |
| 10-051 | a repeat Join press while a conference join is pending is ignored, and the Join and Decline buttons of the invitation row are disabled while `pending_conference` holds (the driver's stage; nothing new to keep in step). |
| 10-052 | skipped: tdesktop closes the rename box on send too. |
| 10-057 | a member page that cannot be sent is `SupergroupMembersFetch::NotSent` (supergroups and basic groups), drawn like `Failed` with Retry, not a removed entry read as Loading. It is its own variant: only TDLib's `Failed` refusal switches the inline list to the Administrators fallback, so a transient local refusal keeps the Recent list and its Retry. |
| 10-117, 10-118, 05-032, 02-092 | the checkout is one value: `PaymentsState::checkout: CheckoutForm` (`Closed`, `Loading { request, invoice }`, `Failed(line)`, `Ready`/`Verifying`/`Paid(Checkout)`), where `Checkout` carries the form, invoice, order info, shipping choice and note. The five fields `request`, `order_info`, `shipping_id` and `note` are gone: closing the checkout takes all of them, the driver's `send_payment_form_request` replaces the whole earlier one before sending, and the UI resets nothing by hand and drops the old dialog when the send is refused. A form answer or order-info answer applies only to the stage whose request asked; `sendPaymentForm` records the form only while the checkout shows that invoice. |
| 11-018, 11-019 | the game scores panel draws `Fetch::Failed` with Retry; the driver opens the panel as `Loading { request }` and the answer or error applies only for that request. |
| 12-A011 | `confirm_unpin` takes an `UnpinOrigin`; only the selection bar's unpin clears the forward stage. |
| 02-002, 12-D014 | the tray reads (media check, file thumbnails, sound track) and their give-up timers run as app work (`spawn_app`), so they settle `TrayReads` whichever window or screen asked; the thumbnails follow-up is a `defer_screen`. The worker and the timer are held in the stage (`Read::Reading { job, _tasks }`): forgetting a file, starting it again or finishing it drops them, none is detached. |
| 13-019 | `install_spell_engine` and `recheck_spelling` go through the disabled gate (`sync_spellcheck`), so turning spell check on later checks the draft. |
| 07-014 | a failed first Money page draws Retry. |
| 14-010 | the grossing apps list is `Loading { request }` and Failed with a Retry; a refused send is Failed. |

Guards: none added (the shapes are per-feature stage choices that `check-implicit-state.py` cannot see).
Tests: refused first event-log page, a failed older page and its Retry stage, refused member fetches and the Recent list kept after a local refusal, a contacts fetch with the path down, a newer checkout replacing the whole older one (note, shipping and order info included), a checkout that cannot be asked dropping the old dialog, an older game-scores answer ignored, the permission stage helpers, the poll stopping while inactive, the search box following its page.

### Sweep fixes (r2-07)

| id | outcome |
| --- | --- |
| 08-018 | `open_bot_create` resets the create state only when no `createBot` is in flight; the new box adopts the `Creating` stage and `drain_bot_create` reports the answer. |
| 02-005, 08-107 | `open_collectible_info` opens the box only when the lookup was sent; a refused send posts the error note. |
| 10-021 | `GiveawayState::launch` is `launches: GiveawayLaunches`, one `GiveawayLaunch` stage per boosted chat (the variants no longer carry `chat_id`); answers find their chat by request id. |
| 10-022 | A giveaway price list whose send fails is `Fetch::Failed` (`send_failed_line`), and the form draws Retry (`giveaway-options-retry`) for any failed list. |
| 10-070 | A welcome-message load that cannot be sent leaves the pack `Failed`; an `Idle` pack is drawn as loading, not as the empty list. |
| 10-071 | `WelcomeDialog::editing` + `save` are one `WelcomeEdit { Idle, Row { welcome_id, save } }`. Cancel and Edit are refused while the row's Save is out. |
| 11-038, 11-039 | `ConnectDriver::open_ownership` / `request_ownership_candidates` mark every request that cannot be sent `Failed`. The Confirm step, the Leave box and the pick list draw loading, failure with Retry, or empty from the real stage. |
| 11-040 | The password field is emptied only when the transfer was sent; an empty password says so. |
| 02-001, 12-D013 | The tray video sound read runs as an app task (`spawn_app`), so a closed window or a switched account cannot drop its answer, and gives up after `GIVE_UP_AFTER` like the thumbnail and media reads. |
| 12-D008 | The photo header size is read by the media-check worker and stored in `TrayReads::photo_sizes`; the render thread only looks it up. |
| 13-032 | The composer stays open while its Check, Post or Save is out (`ComposerStage::in_flight`); the saved edit ends it (`end_story_composer`). |
| 13-041 | `StoryComposer::open` is gone: `ComposerMode::Closed` is the closed stage and `is_open()` reads it. |
| 14-001 | `check_for_updates` refuses to start over `Installed` (button and timer alike). |
| 14-003 | `UpdateState::Installed(release, ChangelogBanner)` carries whether the banner is dismissed; `update_banner_dismissed` is deleted. |

Review round and file-complete pass (round 2):

| id | outcome |
| --- | --- |
| review 1, 14-001 | `UpdateControl::ChangelogPending` (from `Installed`) is drawn as a disabled "Check for updates" and does nothing when pressed; it stays out of the title bar. `check_for_updates` asks the control, so the button, the timer and the guard agree. |
| review 2, 12-D008 | The High quality offer sizes the picture a photo goes out as: `TrayReads::photo_sizes` is a `Reads<PhotoSize>` keyed by the photo's source path and holding the file it read (the converted JPEG once one is made). A photo with a preset JPEG is read too; a size read for another file is read again. A timed-out media check makes the photo a file, so a late size has nothing to offer. |
| 08-108 | A tap on a collectible lookup that is still out asks again (the newer request replaces it, the older answer no longer matches), so a lost answer cannot block later taps; `request_collectible_info` returns the `RequestId`. The box opening over a later screen matches tdesktop, and closing the box clears the lookup. |
| 08-018 | The second managed-bot press while a create is out is refused with a note (`Q_MEDIA_STILL_CREATING_A_BOT`), keeping the first create's stage and share; it no longer adopts it. Tested. |
| 02-005, 10-071, 08-018 | Tests: a refused lookup stores nothing (connect), `WelcomeEdit::cancel`/`open` are refused while a Save is out, the second press. A Save that is out also keeps its row when the message is deleted elsewhere. Other rows' Edit is drawn disabled while a Save is out. |
| story composer (review) | Close, Esc and the backdrop while Check, Post or Save is out say why the box stays (`Q_MEDIA_STILL_POSTING_STORY`); opening another composer over it is refused the same way. The status line still shows the upload. |
| welcome load | `fail_welcome_load_send` stores a `Failed` pack only for a supergroup. |
| 10-023 | `GiveawayLaunches::for_form` keeps `Verifying` as well as `Launching` (closing the form on the provider's page used to reset it at once). The form shows "Waiting for the payment" with "Start over" (`end_verification`), the user's way out, and no Start. |
| 10-024 | One reader (`giveaway_notice`: a refused field, else the failed launch) and one dismissal (`dismiss_giveaway_notice` clears both) on every edit. |
| 10-025 | The confirmation page is no longer reset by hand to follow the launch: it stays while the launch is out (its Start shows busy) and after a failure (the line shows, Start asks again). `page` is navigation, the per-chat stage is the session's. |
| 05-035 | `GiveawayLaunch::Launching { request: RequestId }`; no `0` for none. |
| 05-034 | The Premium and Stars price lists are answered and failed by the `RequestId` their `Fetch` waits for (`Fetch::sent`/`answered`/`failed`), not by chat id or purpose: a late answer to an earlier request (a Retry) is dropped. |
| 06-044, 06-045 | `StoryComposer::open` is gone (`mode`); the link URL and reaction text are read from the inputs when used (`areas_json(link, reactions)`), no mirror is kept. |
| 10-080 | The dialog body hand-over is a `Cell` (`Cell::take` cannot panic on re-entry), in `groups_welcome`, `collectible_info_ui`, `ownership`, `send_box_options` and `bot_create_dialog`. |
| 11-040 | Already fixed in round 1; tested now (the field keeps what was typed when nothing was sent). |
| 12-D013, 12-D014, 02-002 | A tray read (`Read::Reading { job, _tasks: Held }`) owns its worker and give-up tasks: leaving the stage (answer, give-up, forget, supersede) drops them. Sound, media, photo-size and thumbnail reads use it, as `spawn_app` tasks (the reads are the app's). |
| price box | `PriceDialog::too_high` is `PriceVerdict { Typing, TooHigh { text } }`: the line shows only while the field holds the refused text, so editing clears it. |
| 13-037 | `StoryComposer::id` (never 0, new per opening); the file dialog's answer applies only to the composer that asked (`take_story_media_asked_by`). |
| 14-002 | The timer's `recheck_for_updates` leaves `InstallFailed` for the button. It no longer leaves `DownloadFailed` for it: superseded by the r4-12 round 3 and round 4 tables (the timer checks again from `DownloadFailed`). |
| 14-004 | `UpdateState::Checking(Job)`, `Downloading(.., Job)` and `Installing(.., Job)` own their tasks (`updater::Job`): replacing the stage cancels the check, the download wait and its redraw timer. The 4-hour loop is the app's own timer and ends with the app. |
| 14-005 | A community ban waits as a `PendingBan` that holds its `RequestId` (from `fetch_community_member_chats`, or the one already in flight for the same member); a newer ask for the member is followed, and any wait whose ask answered settles (one still out no longer holds up another). A wait ends with the box it was pressed from (`drop_bans_over`, from the dialog close and the shell's replacement of the box), and, as before, when another box is open at the answer. Nothing is banned for a forgotten press. |
| 14-004 | `take_draft_clear` no longer falls back to any clear of the chat. A pending `DraftClear` holds its send's messages (`Asked(RequestId)` from the send, `Made(MessageId)` once the answer to that request names the temporary message: `SendAnswered` in `MessagesState::sent_answers`). The ack (`SendAck { chat_id, slot, old_message_id }`) finds its clear by `old_message_id`, so a reply from a supergroup's main composer, acked under the replied-to message's topic, still clears the draft of the slot it was typed in (the clear returns that slot). With no message match only the clear of the very slot the ack names is taken, never a neighbour's. Tests: `a_reply_from_the_main_composer_is_acked_under_another_topic`, `a_send_answer_names_the_temporary_message_of_its_request`. |
| 14-005 | `Installing` holds a 30 s watch: a quit that does not land ends in `InstallFailed` (`restart_did_not_land`). |
| 14-003 | Already `Installed(release, ChangelogBanner)` from round 1. |

- **Not fixed here.** 02-092 (`connect/payments.rs`: the checkout invoice
  `payments.request` beside the `CheckoutForm` stage, 29 readers and writers in
  the payments domain's own files) and 09-056's remaining text buffer (the edit
  row's text is the `Textarea`'s own value, written when a row opens). The
  `giveaway_pick_ui.rs` entries (10-026, 10-027) are in a file this round did not
  touch.

### Sweep fixes (r2-10)

Round 2, bundle 10: the files `media_tools`, `message_effect_watch`,
`message_effects`, `network_usage`, `new_chat`, `notify_accounts`,
`notify_volume`, `paid_messages`, `paid_messages_guard`, `paid_reaction`,
`poll`, `premium_limits`, `profile_photo_camera`, `proxy` (and its new
`proxy_ops`), `psa_about`, `screen_reader` and `search_filters`.

| id | outcome |
| --- | --- |
| 03-D003 | `candidates` caches only a found answer; a missing ffmpeg is looked for again on the next ask, so installing it is seen without a restart. |
| 03-D006 | `BoundedStages` queues `(key, ticket)`; eviction removes a key only when its ticket is the live one, so a re-set key's stale entry no longer evicts it. |
| 03-D008 | `EffectLookup::Loading` holds its `RequestId`; `fail` and `forget_dropped` match by request, so a dropped ask's late error is ignored. |
| 04-001 | `call_seconds` adds with `saturating_add`. |
| 04-015 | `settled_new_chat(kind, stage)` moves a supergroup or channel to `Type { Editing }` when its create is answered and the dialog lets the create request go (`settle_new_chat_stage`); `Creating { Created }` is a group's only. |
| 04-003 | The unread memo lives in a `Cell` and is taken out while it counts: no `RefCell` borrow spans the walk. |
| 04-004 | The volume writer uses `spawn_guarded`. |
| 04-007 | `PaidMessages::trust` and `trust_file` are private; `remember_trust`, `needs_confirmation`, `trusted_prices`, `trust_to_write`, `trust_loaded` and `trust_written` keep them together. |
| 04-008 | A save for a target already out answers with that save's request and sends nothing; `PaidSave::begin` runs after the send and never replaces a `Saving`. `clear` stays only for the offline demo. |
| 04-016 | `GateFetch::Asking` holds the `RequestId` and the Premium state of its ask; `accept_answer`/`ask_failed` match by request, and the answer is stored under the Premium it was asked under. |
| 04-017 | `price_steps` and `step_for_price` saturate (`abs_diff`). |
| 04-005 | `PaidGuard` uses `lock_or_recover` in `screen` and `set_state`: a poisoned lock still screens. |
| 04-006 | `HeldSend::total`, `PaidConfirm::total` and the guard's total saturate. |
| 04-009 | `PendingPaid::stars` is derived from `taps`. |
| 04-010 | Star totals and the commit time saturate. |
| 04-011 | A commit that cannot be sent and a refused undo leave the entry `PaidStage::Stuck`: TDLib still holds those Stars, so they stay reserved, are not shown and are not re-sent, and the next tap on the message joins them into the waiting entry (the toast shows what TDLib would commit). An answered commit or undo also settles a stuck entry of that message. A refused commit releases its Stars as before: TDLib takes the pending Stars out of the message when it sends the commit and unlocks them on the server's error (`MessageReactions::send_paid_message_reaction`, `SendPaidReactionQuery::on_error`), so keeping them would lock balance that is free and show the next tap a total TDLib does not commit. |
| 04-041 | `validate_at` adds with `saturating_add`. |
| 04-035 | `LimitStore` is one `HashMap<PremiumLimitKind, LimitState { Asked, Refused, Answered }>`. |
| 04-036 | `CameraStream`'s stage is one atomic `StreamStage` (`Running`/`Ended`/`Stopping`); the thread reaps ffmpeg when it ended by itself, and an owner's stop is never read as an end. |
| 04-037 | `PingStatus::Checking(RequestId)` in the one `pings` map; `proxy_ping_requests` is deleted. A probe the rotation waits on times out (`PROBE_TIMEOUT_MS`), and a probe that could not be sent counts as failed. |
| 04-038 | `ProxyFetch` and `ProxyMutation` are one `ProxyListOps` with private fields and every transition as a method (`proxy_ops.rs`); `ProxyMutation` is deleted. A change's success outdates a fetch in the one `change_succeeded`. |
| 04-039 | `LinkCheck::Available(ms)`/`Unavailable` replace `Done(PingStatus)`. |
| 04-042 | The PSA reader and writer use `spawn_guarded` through `Job`. |
| 04-043 | `Stage::Reading` holds the `Job`; a worker that died ends the stage with empty flags. |
| 04-044 | One `Stage { Unread{waiting}, Reading{job, waiting}, Ready{save, toast} }`; `restore_psa_about` resolves the waiting chat. |
| 04-045 | `Save::Writing { job, written }`: `InSync` only when the write succeeded and carried every flag, `Unsaved` (written again) when a flag came in meanwhile, and `Failed` when the write did not succeed. A failed write is not started again by a sync, which runs on every pump and would start a worker each time for a folder that cannot be written; the next new flag saves them all. The tests join the worker instead of sleeping. |
| 04-049 | The Linux watcher's `EndGuard` ends the watch when the thread ends, also by a panic. |
| 04-050 | `GlobalSearchFilters` stores the `SearchTab` (plus the Chats tab's chat type, date and archive); scope, media and chat type are read off it. A scope or a Channels chip is a tab change (`in_scope`, `with_chat_type`). |
| new-01 | `search_filters` date math, `proxy` last-used date and `poll_answer_for_tap` index use checked or saturating conversions. |
| new-02 | `message_effects` revision and `premium_limits` options: revision wraps; options are a map. |

- **Tests.** Request-matched effect failures, ticketed eviction, supergroup
  type step, gate answers by request, trust round trip, save target guard,
  poisoned guard, stuck Stars joining the next tap, premium-limit stages,
  camera pump stages, probe timeout, `ProxyListOps` transitions, the PSA
  stage with a failing write, the watcher guard and tab-derived filters.
- **Behaviour that changed.** A second Remove Fee or Direct Messages press
  while the first is out sends nothing. After a failed or refused Star
  commit, the next tap on that message shows (and commits) the held Stars
  with the new ones.

### Sweep fixes (r2-15)

Files: the connect groups, settings, stories, messages and moderation
drivers listed below, `earn/state.rs`, `emoji/mod.rs`, `export/handle.rs`,
`export/loaded_files.rs`, `gift_manage/state.rs`, `gift_manage/transfer.rs`.

| id | result |
| --- | --- |
| 02-078 | `GroupsState::community_full_infos` is `CommunityFullInfos`, one `Fetch` per community: the pack, its request and its failure are one value. `CommunitiesState::full_info_failed` and the in-flight purpose probe are gone. A refetch after `setCommunityName` keeps the old pack on screen (`Outdated`); the community chat list shows a failure line when the fetch failed. |
| 02-076 | `refresh_forum_topics` marks the list `Outdated` (an ask in flight predates the change) and asks again; `ForumTopicsMap` takes an answer or error only from the ask that waits for it. |
| 02-077 | The `getMe` fallback is `UsersState::my_id_probe: Fetch<()>`: asked once, a failure or a refused send is final for the session, so the poll pump does not re-send it. |
| 02-079 | `NewChatCreation` is one `CreationStage` (`Out`, `OutClosed`, `Made`, `MadeClosed`, `Settled`); the photo travels inside the stage, `CreationAsker` and `outcome` are gone. |
| 02-082 | The full-rich-message load holds its `RequestId` from the send; the answer or error of an older ask no longer ends or fails a newer load. |
| 02-085 | A basic-group add that stops midway counts the unsent users in `add_members_failed`. |
| 02-104 | `PrivacyKeyState::Loading` and `Failed` carry the rule held before; a refused refetch keeps it (`fail_privacy_load`). |
| 02-102 | A refused send of the exceptions fetch leaves the scope `Failed` (not removed), so the page does not ask again from render; answers and errors are matched by request. |
| 02-097 | `DeviceLogin` holds its request: `InFlight(request)`, then `Done { request, result }`, which the screen that sent it takes once (`take_result_for`). The integration branch keeps this shape from the earlier merge instead of this branch's `device_login_result`. |
| 02-103 | A refused `getActiveSessions` send fails the list instead of returning it to `Idle`, so the calls section does not ask again from render. |
| 02-098 | `ToneUpload::Uploading` holds its `RequestId`; only that request's answer or error ends it. |
| 02-106 | Skipped: not a bug. The box holds its press while its own request is out (`StealthPress::Wait`) and a box opened after a closed one asks for its own answer (a test pins it); the doc comment that claimed a dedupe was wrong and is corrected. |
| 03-A004 | `Load::Loading` holds its `RequestId`; TON and Stars stats answers and errors are matched by it. `Load` stays apart from `Fetch` for its fifth outcome, `Unavailable`. |
| 03-A005 | `WithdrawStep::Checking` holds the request; `Withdraw::request` is gone. |
| 03-A006, 09-079 | `SetupPage` is one `SetupStage` (`Editing`, `Confirming`, `SavingInBox`, `Saving`); `confirm` and `saving` are gone. |
| 03-A010 | The trending emoji sets are `TrendingSets` (`Idle`, `Loading{from, request, shown}`, `Ready`, `Stale`, `Failed`); the four paging fields and the `Fetch<()>` are gone. |
| 03-A011 | `KeywordEmojis` and `AnimatedSuggestion` are stages holding the request that asked; a replaced ask's answer lands nowhere, a refused search is `Failed` so the panel does not ask again. A failed `getAnimatedEmoji` ends its own `Asking` in `Failed { emoji }`: nothing is suggested and the same emoji is not asked for on every keystroke. |
| 03-A012 | Skipped: `EmojiPanel::open` is the visibility of the box (section 8, 03-032); the host registry holds only which window shows it. |
| 03-B001 | Skipped: `ExportView` is what the person chose for the progress box (Hide), independent of the export's stage (every stage can be shown or hidden); only the Hide button writes it. |
| 03-B002 | A worker call that times out tells the link (`gave_up`), which frees the request slot instead of waiting for an answer nobody takes. |
| 03-B005 | `LoadedFiles` uses a poison-recovering `Mutex`, not a `RefCell`. |
| 02-047 | `CollectionsFetch` holds the request (`Loading`, `LoadingStale`); only its own answer settles it and applies the list. |
| 03-B007 | `GiftFlow` forms hold `FlowAsk::Sent(request)`; a form closes only on the answer of the request it sent, and a refusal puts it back to editing. |
| 03-B008 | `GiftManageState::deleting` is gone: the `deleteGift` request carries the gift id (`subject_id`). |
| 03-B009, 05-037 | `SendGiftState::open` is gone: `SendStage` is `Closed`, `Idle`, `Sending`, `SendingClosed`, `Sent`, `Failed`, so a send in flight with its box closed is one stage. |
| 03-B006 | `TransferStage::Probing` holds the probe's request; its answer or error moves the picker only when it matches. |

- **Behaviour that changed.** A community pack refetched after a rename
  stays on screen until the new one lands. A failed community fetch shows
  "Couldn't load the community info." in the community chat list instead of
  a spinner.

### Sweep fixes (r2-16)

Files: gift market (`mod`, `state`), giveaway (`mod`, `members`), `lang/store`,
location picker, MPRIS (`media_session/linux`), media viewer (`transform`,
`pip/panel`), `notify`, `passcode`, `passkeys`, `payment_card`
(`field_edit`, `validate`), `peer_qr` (`free`, `state`, `theme`) and
`premium_hub` (`mod`, `gift_value`).

| Ids | Fix |
| --- | --- |
| 03-B010 | `acquired` / `previews` record their request (`Fetch::sent`) and take only that request's answer or error. |
| 03-C015 | `ResaleSearch::page` and `CraftSession::candidates_page` are gone; the purposes lose `page`. Whether a page appends is the stage's (`PageLoad::appends(request)`). |
| 03-C016 | `ResaleBuy` is `{ listing, recipient, pay_in_grams, stage: BuyStage, for_craft }`; `BuyStage` is `View`, `Send`, `Confirm`, `Changed(price)`, `Sending { request }`. `BuyPhase`, `step` and `price_changed` are deleted. Going Back during a purchase leaves it out; its answer still lands. `pay_in_grams` is the buyer's currency tab, kept as a choice that outlives a refusal. |
| 03-C013 | `MembersStage::Loading { request }` replaces the generation counter; `restart` leaves the stage `Idle`, `begin_page(request)` enters it, `accept` and `fail` match it. `PaymentsPurpose::GetAwardMembers` is a unit variant. |
| 03-C017 | Skipped: winners/months and option/winners are user selections validated against the offered options. Every edit goes through `edit_giveaway`, which settles the draft again (`settle_premium`, `settle_stars`); there is no stage and no request. |
| 03-C018 | `LOCAL` holds `Local::{Shared, English, Pack}` instead of `Option<Option<Arc<Snapshot>>>`. |
| 03-C009 | `VenueBot::Resolving { request }` is set by `request_sent`; `venue_bot_resolved` / `venue_bot_failed` take the request id and ignore a lookup a new picker dropped. |
| 03-D005 | MPRIS `Shared` is `Option<Playing { info, at }>`; the seek offset from D-Bus adds saturating. |
| 03-D004 | `flip_rgba` / `rotate_rgba_quarter_turns` check the byte count with `checked_mul` (`is_rgba_image`). |
| 03-D009 | `PipPosition::bounded` clamps a position read from `video_pip.json` to `SAVED_LIMIT`, so the window math cannot overflow. |
| 04-002 | `NotificationOutcome` is `Dismissed` or `Picked(action)`. |
| 04-018 | `UnlockState` keeps the wrong-try count of the process; the larger of it and the file's count applies, so a failed write does not switch the limit off (tdesktop keeps its count in memory only). |
| 04-012, 04-013 | The list error is `PasskeyList::Failed { error }`, the removal's is `RemoveStage::Failed`, the add's is `PasskeyAdd::Failed`; `PasskeysState::error`, `removing: Option<Removal>` and `generation` are deleted. The list answer is matched by the request in `PasskeyList::Asking`, so a look older than a removal is dropped without a counter; `list_in_flight` also checks the request is still pending. |
| 04-022, 04-023 | `Validation` is `Invalid`, `Incomplete`, `Valid`, `Finished`; `FieldResult` holds one `FieldCheck`. Baseline lines `FieldResult::finished` and `Validation::finished` dropped. |
| 04-020 | `FreeUi::Requesting { request, serial }`; `FreeState::begin_request` / `settle_request` / `drop_request` are the only writers. `last_serial` is an id generator. |
| 04-019 | `PeerQrBox::theme: ThemeStage` is `Following`, `Picked { failed }`, `Saving { serial, choice }`; `chosen`, `save` and `SaveStage` are deleted. |
| 04-021 | `GiftsStage::FirstAsking(GiftsAsk)` and `GiftsRest::{More, Loading, Waiting}` carry the serial, send time and next offset; `ThemeGifts::{serial, requested_at, offset}` are gone (`last_serial` is the generator). |
| 04-034 | `GiftValueStage::Known(info)` / `Open(info)` replace `Ready { open }`. The upkeep that drops a lookup whose gift box is gone is the owner-gone exit, in one place. |
| 04-024 | `stars_open` and `tx_selected` are `StarsView::{Closed, List, Detail}`; `gifts_open` is `GiftView::Closed`. The box and its selection are one value, so a closer cannot miss one. `GramsHub::open` is in `premium_hub/ton.rs`, which bundle r2-17 owns. |
| 04-025 | `PremiumHub::received: ReceivedGifts` holds owner, rows, total, cursor, page, notice, view and op; `opened_for` / `restart` replace it whole (a change in flight keeps its lock). |
| 04-026 | `PremiumHub::stars: StarsHistory` holds view, filter, rows, cursor and page; `switch_filter` replaces it whole. `balance` is the account's and stays on the hub. |
| 04-027 | `premium_state: Fetch<PremiumStateInfo>`, asked with `sent`, matched and failed by request, asked again after a failure or when the Premium status changed (`is_premium` option). `premium` and `business_features` match their request too. |
| 04-028 | A preview request not yet opened is dropped by the Payment farewell when the account leaves the screen. The main window already takes a request whose chat has no window of its own. |
| new-01 | `PeerQrBox` kept `color`, `emoji` and `gift` beside `ThemeChoice`; it holds one `shown: ThemeChoice` with `color()`, `emoji()`, `gift()` readers. |
| new-02 | `peer_qr` expiry arithmetic saturates. |

Tests: `sweep_payments_2` (acquired gifts, both orders), `gift_market::tests`
(buy stage), `giveaway::tests`, `sharing_tests`, `passkeys::tests`,
`passcode::tests`, `transform`, `settings::video_pip`, `premium_hub::tests`,
`state::tests::premium_hub`, `ui::premium_ui::preview_tests`.

### Sweep fixes (r2-13)

Round 2 re-sweep, `src/connect/` files business_extra to payments (28
entries). Every answer is matched to the request that asked, every refused
send settles its own part, and the four-field checkout is one value.

| id | outcome |
| --- | --- |
| 02-022 | `BotLookup` parts are `Fetch` values (`exact`, `mine`, `global`): a refused search is `Failed` and the Chatbots screen draws its error with Retry; the same query asks again after a failure. |
| 02-019, 02-020 | `SpeakingTracker` records a send only through `sent()` once it left; it holds the speaking mark itself, so `CallAudio::speaking` is gone and resets only through `reset()`. |
| 02-038 | The close hold is `ShutdownPhase::HoldingClose { until }`, so `chats_path_active()` is false while it holds; `close_held_until` is gone. |
| 02-036 | Community fetches carry the `RequestId` (`Loading { request }`) and answers and errors go through `answered` / `failed`. A lost request settles through the existing lost-request error path. |
| 02-057 | `ChatsState` owns the flow: `take_deep_link_step`, `cancel_deep_link_invite`, `next_deep_link_generation`; the UI no longer takes and puts back the flow every poll. The invite stays the session flow's stage and the box holds only the generation it is up for (`InviteBox`, 09-021), which Join and Cancel pass back; `deep_link_seq` is a monotonic id source, not a stage, so it stays. |
| 02-066 | "Mark as read": the two halves are independent; the call fails only when neither left. |
| 02-041 | `WithdrawStep::Checking(RequestId)`; `Withdraw::request` is deleted. |
| 02-042, 02-052, 02-064, 02-086 | The halves of the join page, folder share, craft lists and message audience are each sent whatever the other did; a refused half is `Failed`. |
| 02-043 | `join_affiliate_program` returns the refused send. |
| 02-044 | `Load::Loading(Option<RequestId>)` on the revenue page; only the open's own answer or error acts on it. |
| 02-054 | `TrendingPaging` (Idle / Asking / More / End / Failed) replaces the fetch and four offset fields; the answer is matched by `RequestId` and a failed page keeps its offset. |
| 02-053 | The sweep drops flood attempts and retained JSON of requests the registry no longer holds. |
| 02-051 | Not a bug: the read's own `ok` clears the badge (`clear_topic_marks`); the refetch only double-checks. Test added. |
| 02-055 | GIF search results and both offsets live inside `GifSearch`; a new search replaces them whole. |
| 02-065 | A refused `closeGiftAuction` leaves the follow as it was; the next call closes it before opening another. |
| 02-062 | A refused prices send is `Failed` and the form draws a Retry. |
| 02-080 | A refused later send of a language switch points TDLib back at the shown pack. |
| 02-073 | A gift code check is refused while an apply is out, and a refused check keeps the box. |
| 02-083, 13-028 | `SetFiles::Asked` only after the downloads left. |
| 02-084 | The emoji pack title is not asked again while its request is out. |
| 02-090 | One `createBot` at a time, refused by the driver. The stage matches by its own generation, which each create bumps once and a username check does not touch. |
| 02-089 | A paid price is trusted only after the send left. |
| 02-092 | `CheckoutForm` carries the invoice, order info and shipping choice (`Checkout`); `PaymentsState::{request, order_info, shipping_id}` are gone and a new form replaces the checkout whole, with its note, once its request went out (`send_payment_form_request`; the UI writes nothing before asking). A new form is refused while `Verifying`, which lasts until the poll opens the verification page and closes that checkout: a form started in between would be the one that close hits. |
| new-01 | `ClosingClient::warned` is one `CloseWait` stage (`Since` / `SlowLogged`). |

### Sweep fixes (r2-06b)

| Id | Fix |
| --- | --- |
| 03-C011 | `VenueList::Failed` is its own list state (a failed lookup or bot lookup), drawn with "Couldn't load places." and a Retry (`LocationPicker::retry_venues`, `LocationSharing::retry_venues`, which also asks for the bot again when that was what failed). A missing bot is still "No places found" (`venues_unavailable`). |
| 03-C012 | `AddressStage::HiddenWhileMoving` is gone: `Pin::Moving` is the one source for "the pin is lifted", and `address_line` hides the kept address while it holds. |
| 03-C010 | Not a bug: `TileState::Failed` is a stage with exits (the map resting after a move, a resize), and `tile_to_request` skipping it is what keeps a dead connection from being asked on every poll. Tests: `a_failed_tile_is_asked_again_*`. |
| 06-035 | Already fixed by r2-06: `State::opener_ended` fails the clips `Stage::Decoding { opener }` held. |
| 06-036 | The first-frame mark lives in the open stage: `Stage::{Ready, Drawing}` carry `Open { info, first_frame: FirstFrame::{Awaited, Handed} }`, so `Clip::delivered` is gone and a re-opened clip (a new `Clip`) starts at `Awaited`. `State::workers_decode` (a copy of "every opener is gone") is derived. |
| 10-072, 10-073 | Already fixed by r2-06 (Sending editor kept; `original_emoji` deleted). |
| 10-079 | The manage dialog's body is built in the content closure (`forum_manage_body`), so there is no one-shot cell. |
| 10-081, 10-082, 11-002, 11-004, 11-009, 11-030, 11-034, 14-024, 14-025, 14-043 | Already fixed by r2-06 (see its table); re-read here and found closed. |
| 10-083 | `Decode::Failed(key)` is distinct from `Sdr`: no badge, no frame, not re-decoded from render, dropped by `reset` so the next open asks again. |
| 14-042 | Turning translation on from the bar also starts a new pump scan, which drops the failures and asks again. |
| new-01 | `FfmpegBackend::{image, surface}` (an image xor a surface, kept in step by hand) are one `OnScreen { Nothing, Image, Surface }`. |

### Sweep fixes (r2-04b)

| id | result |
| --- | --- |
| 08-116 | The row still leaves as the delete goes out (as in tdesktop), but the delete is now a `ScheduledDelete { request, chat_id }` on the chat view. A refusal says so and loads that chat's list again, so a row still scheduled comes back. A chat change forgets the waits. |
| 08-115 | Verified (r2-04): the scheduled box carries the load stage and its Retry (`ScheduledLoad`, inside `ScheduledBox::Open` since 08-091). |
| 09-017 | r2-04's `on_mouse_up_out` on every row ran in the capture phase before the pressed row's own release, so it also cleared a real swipe. The press is now `SwipeReply` (`Idle`, `Pressed { chat, message, x }`): the row's release takes it, the history's release listener ends the gesture after the rows saw it (deferred), and it lives in `ChatUi`, so a chat change drops it. |
| 09-014, 09-015, 09-034, 09-035, 09-036, 09-031 | Verified against the current code: older and newer pages stop on failure and draw Retry; `leave_data_storage` resets the scroll; `PresetsSeed`, `AutoDownloadAnswers` and `StorageSaveFailures` stand. |
| 09-030 | Also closed the early returns: `set_auto_download_settings`, `set_less_data_for_calls`, `set_storage_limits`, `clear_storage`, `fetch_auto_download_presets` and `maybe_fetch_storage_statistics` refused before any request now end in the failure line / Retry the page draws. |
| 09-109, 09-110, 09-111 | The form's Cancel while its save is out goes back to the list carrying that `LinkOp::Saving(request)` (`FolderShareView::cancelled_form`): the list waits, and the answer's error line or quiet success lands there, instead of being dropped with the form. |
| 09-112, 09-115 | Verified: the copy mark owns its timer; refused folder fetches end in the error with Retry. The dialog body's `RefCell::borrow_mut` is a `try_borrow_mut`. |
| 14-076, 14-077, 14-082 | Verified. A helper that starts but never says ready (`HelperState::Starting { since }`) is given up after 30 s: the window closes and TDLib hears `closeWebApp`. |
| 14-078 | `searchWebApp`, `getAttachmentMenuBot` and `toggleBotIsAddedToAttachmentMenu` answers carry the `RequestId`; `Awaited` holds the request it sent and the flow takes only that answer. The slots are mailboxes (`Vec`), so two answers landing in one poll no longer overwrite each other. |
| 14-086 | The mini-app poll is `run_poll_loop` on a new `wake::MINI_APPS` signal. The helper's reader thread pokes it per event and on exit, and the session pokes it when a mini-app answer lands. Stages that watch other state (a suggested emoji lookup, the phone and payment flows, a starting helper) tick every 250 ms while they last; the 5 s ceiling is the safety net. A hidden or idle app costs no wake-ups. |

- **No new CI guard.** The shapes are about which state a function reads;
  the tests walk each exit (`two_searches_of_one_bot_keep_their_own_answers`,
  `a_refused_delete_asks_for_its_chat_list_and_leaves`, the `SwipeReply`
  tests, `storage_refused`, `cancelling_the_form_keeps_the_save_it_sent`).

### Sweep fixes (r2-11)

File-complete bundle: the agent log and tools, the spell engines, the story
composer, close-friends editor and story page, suggested posts, the system
accent, the tone upload, the tray worker, video crop and voice capture.

| Ids | Fix |
|---|---|
| 01-001 | `Action::outcome: Outcome { Done, Failed, Declined, Undone }` replaces `ok`/`declined`/`undone`; the log file still reads and writes the three flags (`ActionRecord`), and Undo lands only on `Done`. |
| 01-003 | `Pending::conn: Option<u64>`; `current_conn()` is `None` off a connection, so `withdraw_connection` has no sentinel. |
| 01-004 | `live_inputs` reads the takeover once (`takeover::snapshot`), so Hold and Phase come from one lock. |
| 01-005 | `list_chats` and `list_unread` page the list (`getChats` wider, `loadChats` on a short page, 404 ends) up to 2,000 chats and say when they stopped (`truncated`). |
| 01-006, 01-007, 01-008, 01-016 | `td::client_closed` drops the client from `CLIENTS`, `events::forget_client` drops its folders, messages and send outcomes, `tools::forget_client` its cached account and chat classes. The message and outcome buffers are also emptied when Quill locks and when the last agent connection ends (`ClearBuffersWhenEmpty`). |
| 01-009, 01-011, 04-054 | Tests wait on channels, `Duration::ZERO` or the manual clock, never on the wall clock. |
| 01-012 | A revoked connection's process (`peer_creds().pid()`) stays refused under any other name the client declares (`activity::refused_from`); Allow again clears both. Where the socket gives no pid the name alone decides, as before. |
| 01-013 | `grants::for_account` uses `read_or_recover`/`write_or_recover`, one write-locked load. |
| 04-052 | `SpellChecker::is_correct_now` / `suggestions_now` and the backend's `judge_now` / `suggestions_now`: the spelling menu never parses or waits for a parse (`Ready::Pending` gives no verdict). `Entry` has a `Load::Parsing` stage and a condvar for the background askers; the parse runs outside the lock under `catch_unwind`. Touched outside the bundle: `spellcheck/mod.rs` (the `_now` methods), `ui/spellcheck_ui.rs` (two calls). |
| 04-053 | The COM thread skips a job whose caller gave up (`abandoned` flag), counts queued jobs, and the UI-thread calls answer nothing while it is busy. Windows only: not compiled here. |
| 04-060 | `DownloadStage { Running, Cancelling, Done, Cancelled }` behind one accessor; a cancelled finish reads `Cancelled`, a completed one `Done`. |
| 06-044 | `StoryComposer::open` is gone: `ComposerStage::Closed` is the closed composer (`is_open()`), and `set_error` on a closed one does nothing. Edited `esc_stack`, `story_viewer/mod`, `app_render/mod` and tests for `is_open()`. |
| 06-045 | The link and reaction mirrors are gone: `areas_json(link, emojis)` and `link_url_error(link)` take what the inputs hold at send time. The inputs stay live while `canPostStory` runs, so the link is checked again when its answer lands, before the areas are built. `as_chat_id` is folded into `ComposerMode::{New, Repost}` as `post_as`; an edit has none. Edited `ui/story_composer.rs`. |
| 06-046 | `CloseFriendsStage::Loading`/`Failed` are replaced by `Awaiting`; the panel draws loading or the retry from the session's `CloseFriendsList` (one owner of the load). Edited `ui/story_more.rs`. |
| 06-047 | `StoryPageOp` carries a typed `StoryPageOpKind` (no label text) and `Option<ChatId>`; `begin` drops the same call's finished outcome on the same page and caps the list (oldest finished first, else oldest). Callers settle on page leave as before. |
| 06-048 | One capped `failures` queue (`SuggestedPostFailure::{Note, BalanceLow}`, newest 16) in failure order; the drain waits while locked. The drain stays at the toast host (`status_toast_visible`), which is the hand-off to the toast queue that any window's toast must run. |
| 06-052 | `AccentRead::Failed { retry_at }`: a read that found nothing is tried again 30 s later by the next `refresh()` (Appearance open), not never. |
| 07-009 | `TopPerson::kind: TopPersonKind { SavedMessages, Chat { muted, online } }`; baseline line dropped. |
| 07-017 | `ToneUpload::Uploading { request, .. }`; the answer and the error settle only the matching request, a new upload starts only from `Idle`. |
| 07-020 | The tray worker's `stopped` flag is set by a drop guard owned by the worker, so a panic or a thread that never started reads as stopped and `poll` restarts the item. |
| 14-034 | `Crop::mirrored` clamps first and subtracts with `saturating_sub`. |
| 14-084 | `RunClock` lives in `Live` / `Fixture`; `Live::pause`, `resume` and `preview_ready` move the clock, the encoder handshake and the microphone in one place. A closed capture's `VoiceFinish` holds one `Ending { Fixture, Encoder, NoEncoder }` instead of a `fixture` and a `worker` option. |

Tests added: `activity` (only a done write is undone; flags round trip),
`events` (closing a client; waiter test without a sleep), `chats`
(`next_step`), `activity` (revoked process), `spell_dict` (UI calls never
wait; background asker waits for the parse), `spell_download` (cancelled
finish), `story_composer` (closed stage, post-as), `panel_tests` (awaiting
editor follows the session list), `story_page` (supersede and cap),
`connect/tests/suggested_posts` (failure cap), `system_accent` (retry),
`connect/tests/tone_upload` (another request's answer), `tray_sni`
(panicking update), `video_edit` (mirror outside the frame).

### Sweep fixes (r2-22)

| Ids | Fix |
|---|---|
| 07-021 | `Tray::last_shown` is written only after `set_icon` returned `Ok`; a failed set (or an icon that did not build) is tried again on the next tick. |
| 07-022 | The tray cell is borrowed with `try_borrow_mut` (a re-entrant call skips the tick) and read with `try_borrow` (`false` when busy). |
| 07-010 | `AppearancePrefs::work_mode: WorkMode` replaces `show_tray_icon` and `show_taskbar_icon`. `WorkMode` (de)serializes as the two old keys (`#[serde(flatten)]`), so existing prefs files load unchanged and a file with both off reads as window only. `toggle_tray`, `toggle_taskbar` and `skip_taskbar` take and return the mode. |
| 07-019 | The folder picker's answer carries the key of the draft that asked (the draft's slider entity) and `apply_account_export_folder` applies it only to that draft. |
| 07-024 | The check lives with the request it answers, in the session (`AgeVerification { params, check }`, `SyncState::set_age_verification`): a repeat of the same request keeps a start, a different request or lifting it starts over, so nothing is cleared by hand and the screen holds no copy. |
| 07-027, 07-035 | `AccountsDialog::{Closed, Open { row_edit, error }}` replaces `open`, `row_edit` and `error`; open and close replace it whole, `ask` moves to the next row question and drops the last one's error. A removal that fails after the dialog closed goes to the status line (`fail_accounts_dialog`). The one-shot body is a `Cell`, not a `RefCell`. |
| 07-032 | The older page is `OlderEvents::{Available, Asking, Failed(line), ..}` on the page (r2-01, 09-096): a send that was refused and a page that failed both land in `Failed`, which draws the line and a Retry instead of the spinner, and the automatic ask does not repeat for the same list length. |
| 07-033 | `event_log_ids` is a `Cell<Vec<i64>>` taken out while the list is touched. (09-098 is the same fix in r2-01.) |
| 07-030, 07-036 | Turning Agent access off, from the master switch or the connected strip's Revoke (`turn_agent_access_off`, the one place), closes the consent and the reset question (`close_agent_boxes_for_access_off`), so they do not come back when it is turned on again; the one-shot body is a `Cell`. |
| 07-029 | `Driver::Running { id, _task, waiting }` holds the command's task. A command whose window is gone calls `agent_command_abandoned`, which answers the waiting commands with the "window not open" refusal and leaves the stage. |
| 07-037 | The blocked-dialog queue is a `Cell<Vec<Blocked>>` of `(ScreenEpoch, window, Opened)`: only that window's shell closes an entry, and entries of an earlier screen are dropped. |
| 07-028, 07-048 | `takeover.wake` is `Wake::{None, At { at, _task }}`: a nearer wake replaces (drops) the pending timer, and the app owns it. |
| 07-038 | The style editor lives in the view: `AiComposeView::StyleEditor(editor)` and `StyleIcon(editor)`; leaving the view drops it. |
| 07-039 | The AI button's blink start is `ComposerUi::ai_button_since`, per window, set and cleared as the button draws; the main-window upkeep that mirrored it is gone. |
| 07-044 | `AiStylePreview::begin_adding`, `begin_removing` and `show_again` move the stage in place; no placeholder `LookingUp { token: 0 }` is written. |
| 07-045, 07-046 | The animation layer's frame is `Frame::{Prepainting(items), Painted(items)}` and an item is `Slot::{Registered, Painted { inline, seq, opacity }}`. Frame and occluders are read through `try_borrow`; `prepaint` and `paint` copy the items out first, so nothing is held while GPUI is called. |
| 07-047 | `media_roots_frame` is a `Cell<Option<Rc<..>>>`, taken and put back. |
| 08-015 | The attach-icon cache is keyed by path and modification time: a file replaced at the same path reads again, a missing or unreadable file is not remembered. |
| 08-007 | `ActiveTrack::{None, Voice(Track), Audio(Track)}`: the track owns its chat, bar text, length, seek slider, scrub and hover. Stopping the voice note does nothing while a music file is active. |
| 08-011 | `Session::cancel_password_recovery` beside the other recovery transitions; the reducer paths use it too. |
| 08-009 | `OpenNotice::LoginPrevented(places)` is the one slot of "which account notice is open"; `AuthUi::login_prevented` is gone. A prevented login replaces an open freeze notice (the banner opens it again). |

Skipped, with the reason:

- **07-018** (`ExportView` on `AccountExport`). It is not a second copy of the
  work stage: `AccountProgress` says how far the export is, `view` says whether
  its box is on screen, and all four pairs are real (a settled export that is
  hidden is cleared with a note, a settled one that is shown waits for Done).
  Its owner is the export in the session on purpose (07-015): the screen is
  replaced on an account switch and a hidden export would reopen over the
  account when the user came back. The box's own stage is `ExportBox`.
- **07-031** (`EventLogSearch::shows`). The one search `Textarea` is filled by
  two surfaces, the info panel (`show_event_log_query` for the panel's chat)
  and the Group Settings page, so which chat's query its text holds is a fact
  about the widget that neither `EventLogPage::chat` nor the dialog's chat id
  says. The refill runs from the window's own upkeep because setting the text
  needs the `Window`.

### Sweep fixes (r2-18)

Round 2, `src/state` session files. Every register-r2 entry of the 23 files is
listed; the rest of the bundle's files hold no other entry.

| id | result |
| --- | --- |
| 05-082 | one exhaustive match: the permission, pin-order and archive-settings arms moved into `restore_optimistic(rollback, chat_id)`; `apply_error` calls it once and the wildcard is gone |
| 05-071, 06-013 | `SavedPage` deleted: the request of a Saved Messages page is `OlderEnd::Asking(RequestId)` in the history's own older end (`ask_older`, `settle_older`); a view left and entered again drops the old answer |
| 05-069 | `ChatSearchState` holds `ChatSearchSurface::{Closed, Open(OpenChatSearch)}`; opening on another chat and closing replace the value whole (chat, query, phase, results, paging, sender, media, member picker); the types moved to `chat_search_types.rs` |
| 05-070 | `SearchState::clear_results` drops a `PaidPostSearch` confirmation with the results it was asked about |
| 05-083 | `BotStopStage::Running { can_stop }`; `PendingBotMessage::can_stop()` is derived, the second field is gone |
| 05-074 | merged with r2-16's shape: `VenueBot::Resolving { request }` (`None` while the lookup is being sent); `venue_bot_resolved` and `venue_bot_failed` take the request and ignore any other |
| 05-080 | Kept as `pending_attention: bool` with its allow line: a one-shot event the notifier takes (`std::mem::take` in the drain), not a stage of any request. A list of chats read only for "is it empty" was the same flag in another type, so it was not kept; the stale baseline lines are gone |
| 05-072 | merged with r2-08's shape (02-025): `InlineQueryFetch::Loading { request }` and `InlineMore::{Idle, Asking { request }, Failed { offset }}` inside `Loaded` replace `more_failed`; answers and errors apply only to the request the slot holds (a late page of an older query no longer appends to the new list) |
| 05-073 | `PollVotersFetch::Loading(RequestId)` and `VotersMore::Asking(RequestId)`; a late page after `updatePoll` dropped the entry is dropped |
| 05-081 | kept: `loaded_all_participants` is TDLib's own report that the list is complete, replaced whole by every `updateGroupCall`, no request of ours in it; already allowed in `implicit-state-allow.txt`, so the duplicate baseline line is removed |
| 05-085 | `apply_local_search_filter` goes through `begin_recents` / `begin_query` and the `accept_*` calls |
| 05-077 | `MentionSearch::members` and `FromPicker::members` are `Fetch<Vec<_>>` (request in `Loading`, previous list as `stale`); the two `Option<RequestId>` fields are gone; a failed picker request lands in `Failed` and may be asked again |
| 05-078 | `forget_screen_outcomes` clears `contacts_notice` |
| 05-075 | `PingStatus::Checking(RequestId)`; `proxy_ping_requests` deleted (the same value as 04-037; the slot shows `Checking` only once the ping went out) |
| 05-076 | kept: `proxy.error` is the user's last failure and deliberately outlives the stages (a system step or a background refetch does not wipe it: `a_system_step_succeeding_keeps_the_users_failed_change_error`), so it is a note, not part of one stage |
| 05-079 | `ReplyTarget::Loading { request, attempts }` and `Retry { until, attempts }`: five failed asks end in `Missing`; a refused send keeps the count |
| 06-006 | `recent_inline_bots` is `Fetch<Vec<i64>>`; the error arm lands in `Failed` and the next "@" asks again |
| 06-012 | `date_jump_pending` deleted: `ChatSearchJump::Loading { message_id, then: JumpLanding }`; the date jump leaves with the jump stage (`clear_date_jump_ui` makes it a plain jump) |
| 06-007 | the send-as list carries its request in `Loading`; answer and error go through `Fetch::answered` / `failed`, and an answer after the lists were dropped is ignored |
| 06-004 | `begin_sponsored_hide` leaves a report in flight alone |
| 06-010 | `StickerBatch::{Idle, Running, Finished}` replaces the four batch fields; `Running` holds the request of each change that went out, and only the answer of one of those counts |
| 06-011 | kept: `archived_offset` and `trending_offset` are written after the `has_purpose` guard that allows one request at a time, so the offset read when the answer lands is the in-flight request's own |
| 06-014 | `fail_story_report_send` leaves a flow with a request out alone |
| 06-005 | `RowForum::topics` is `RowTopicsView::{Loading, Listed(..)}` (a view derived from the session for each row, not a stored stage); the `loaded` bool and its baseline line are gone |
| 06-003 | `leave_open_chat` closes a thread opened from the chat left as well as one living in it |
| 06-023 | `begin_fetch_at_day` clears the tab's query with the results it replaces |

- **New (new-01).** `finish_history_around` ends a search jump that has no
  chat to load into (`Missing`) instead of leaving "Loading message..." up.
- **Outside the bundle's files**, the smallest changes: `session_apply_error.rs`
  (the rollback call), `domains/*/error.rs` and `apply.rs` (the new request
  matches), `location_picker/mod.rs`, `proxy.rs`, `info_types.rs`,
  `sticker_gif_types.rs`, the connect drivers and the UI readers of the
  changed fields.

### Sweep fixes (r2-21)

| Ids | Fix |
| --- | --- |
| 06-042 | `FramePlan` holds one `SlotClock { Empty, Running { base, last } }`; the first picture fixes both at once. |
| 06-043 | The sticker decoder library is cached only on success; a failed load is tried again on the next sticker. |
| 06-053 | `system_proxy::Follow { Off, On { asked, seen } }` per driver replaces the process-wide `WANTED` flag; the follow runs for background accounts too (above the background early return). The network watcher no longer reads the OS proxy: it counts the change (`network_changed`) and pokes the poll loops, and each client that follows reads once per change (`Follow::step` returns whether to). With nobody following nothing is read. |
| 07-006, 07-007, 06-061, 07-008 | Out-of-range TDLib integers saturate or skip instead of wrapping (`sat_i32`, `i32::try_from`, `saturating_add`); a scheduled date past 2038 sends `i32::MAX`, never a past date. The birthday `as u8`/`as i32` casts in `parse.rs` are `try_from` too. |
| 07-004 | `RequestLogGate` is `Gate { Shown, Hidden { calls, saved } }`; a failed level read stores TDLib's default when the first call hides. |
| 07-002 | `LaunchChats` is one `Launch { Starting, Claimed, Asked, In }` behind one mutex; a new claim starts over for its own client. |
| 07-003 | `TdHub` has one `RunStage { Running, Died, Stopping, Joined }` per receive loop. The bridges' report cell is written only by the loop's own transitions (`Run::begin` on start, `died`); a replacement hub starts its own `Run` over the same cell, and dropping the old hub touches only the old stage. |
| 07-005 | `name_accent::forget_account` runs from `account_data::forget`: the palette and the screen's active key go with the account. |
| 06-049 | `ParsedGroupCallParticipant::video_enabled()`/`screen_sharing_enabled()` derive from the video infos; the two bools are gone. |
| 06-050 | `CallState::Ready(ReadyParams)`; `ParsedCall::ready` and the `ActiveCall::fresh` argument are gone. |
| 06-051, 07-001 | `ParsedGroupCall::loaded_all_participants` and `ChecklistSend::others_can_mark_tasks_as_done` are facts TDLib reports and a send option, not stages: moved from the baseline to the allow list with their reasons. |
| 06-058 | `InlineResultKind` and `InlineQueryResultsButtonKind { StartBot { parameter }, WebApp { url }, Unknown }` replace the kind strings. |
| 06-056 | `PollRequest { Any, RegularOnly, QuizOnly }` replaces the two bools (both set reads as a quiz, as before). |
| 06-057 | `Thumb { None, Still, Clip }` (with the size inside) replaces `thumb_file_id`/`thumb_width`/`thumb_height`/`thumb_still` on `AnimationContent`, `VideoContent` and `AnimationItem`. |
| 06-059 | `DiceState { Rolling, Landed { value, .. } }`: the rolled number exists only once landed, so the label and the animation read one stage. |
| 06-060 | `PaymentReceiptKind { Regular { .. }, Stars { star_count } }`. |
| 06-062 | `StoryContentView::Video::file_id` is `Option<FileId>`. |
| 06-054 | `PasswordState::pending_email: Option<PendingRecoveryEmail { pattern, code_length }>`. |
| 06-055 | The made-up scope of the `getScopeNotificationSettings` answer is gone from the payload; the request that asked holds it. |
| 07-011 | The wallet's `explorer`, `state_fetch` and `rates_fetch` call `Fetch::sent` after each send and settle with `answered`/`failed(request, ..)`, so an older request's late error fails nothing newer (tested in both orders). |
| 07-012 | `TonWalletState::money` is `Option<MoneyPage>`; `MoneyPage::open` is gone. |
| 07-015 | `ProfileAddress::{Loading { request }, Found, NoWallet, Failed}`: a failed lookup is asked again when the profile opens; answers and errors match the request; a late error after the wallet toggled lands nowhere. |
| 07-016 | `set_can_use(false)` replaces the whole wallet state (keeping the explorer, the app-config rate and the chosen currency), so the notice and the rates go with the Money page. |
| 07-023 | `TranslatePrefs::skip` dedups with a seen set, so a repeated or aliased language toggles off. |

- **Behaviour that changed.** An MP4/GIF-format animation thumbnail in the
  inline results grid plays as the preview clip (only `thumbnailFormatMpeg4`
  did before). A video or GIF whose thumbnail file does not parse draws no
  tile instead of a tile with no file.

### Sweep fixes (r2-23)

Round 2, bundle 23: the sign-in, bot, boost and 1:1 call UI files (auth, autoscroll,
battery watch, boost box, bot edit/permissions/stream/verify, bots, bottom action,
bubble theme, add people, call panel, backdrop, sounds, call state).

| id | Fix |
| --- | --- |
| 08-008 | `PhoneStep::Sending` carries its `RequestId`; `sync_phone_step` settles it from `acks.take(request)` (answered, refused, or the request gone with no answer), not from `has_auth_submit` and the shared `last_auth_error`. `PhoneStep::settled` is the one transition. (`signin_ui.rs` edited minimally for the variant.) |
| 08-010 | `auth_submit_busy` guards every `submit_*`, `resend_code` and `request_qr_login`: a second Enter while a submit is out sends and notes nothing. |
| 08-013 | `Active::pressed_at: Option<Instant>` becomes `Button::{Held { since }, Toggled}`; the layer root also hears the middle release, so a hold released off the history ends (a hold scrolls the pointer off it). |
| 08-028 | The Linux battery probe rests (`PARKED`) while no window is in front (`probe_wait`); window activation pokes `wake::BATTERY` (main and chat windows). |
| 08-017 | merged with r2-05's fix of the same box (01-022): `BoostWork::Slots { request }` and `Boosting { request, reassign }`; the slots answer, the `boostChat` answer and both errors act only when `BoostBoxState::waits_for(pending.id)`. A reopened box takes over the still-open slots read by its id. |
| 08-019 | Readers of the bot edit box use its own stage only (`loaded = Editing`); `fill_bot_edit` is the one transition from the session's two `BotText::Ready`. The upkeep runs in the window hosting the box, so a box that is never drawn is never seen either. |
| 08-027 | `emoji_access_shown` is added to only after a send that went out, when the switch is turned off, removed when turned on, and cleared when the info panel opens another target or closes. |
| 08-025 | `StreamReveal::Finishing` is keyed by `ChatMessage`. |
| 08-026 | `Appearing` is `WidthSweep`, `HeightOpen`, `Sweeps::{Fresh, First, Later}` and `Pace`: the loose tweens, targets, `fresh`, `finalizing` and `start_width` fields are gone. |
| 08-020 | `Step::Sending { request, chat, act }` is settled by the request's ack (`pump_bot_verify`, in the hosting window's upkeep); a refusal lands in `Step::Refused` with its reason; Back and Close forget the ack. |
| 08-021 | `similar_bots` is `HashMap<i64, Fetch<Vec<i64>>>`: `Loading` holds the request, a refusal is `Failed` with its reason (drawn, press asks again), a late first answer leaves a newer request waiting. `SimilarBotsFetch` is deleted. |
| 08-022 | Back is disabled (and its handler returns) while the Add-bot send is out. Closing the box during the send still drops the start payload with the box (the owner went away). |
| 08-023 | `BarFacts::bot_start_pending` becomes `StartState::{NotStartable, Offered, Starting(RequestId)}`, `Starting` read from the pending `SendBotStartMessage`; the bar shows `BottomBar::Starting` (button off). Baseline line removed. |
| 08-024 | Unblock and Restart: `BotRestart::Unblocking { request, chat }` in `ChatUi`; the `/start` goes when the unblock's ack is `Done` (`pump_bot_restart`), nothing on a refusal; the chat leaving forgets the ack. |
| 08-046 | `bubble_theme::forget_history_area` runs at the start of every window's upkeep, so a history area never outlives the frame that reported it. |
| 08-043 | `SearchField::{Opening { initial }, Open(field)}` replaces `Option` plus `initial_query`. |
| 08-044 | `end_call_add_people` is the one place that takes the box and clears the search; close, finish, stale call, lock and the `CallBoxes` Farewell all use it. |
| 08-045 | The sessions list is asked for when the Calls page opens (`fetch_call_settings`), not from the section's render. |
| 08-036 | `call_window_target` returns `CallWindowTarget::{None, Active, Ringing, Ended, Busy, Invite}` (the sync code moved to `call_window_sync.rs`). |
| 08-037 | `call_status` returns `CallStatus::{Steady, Connecting}`; `CallSnap::connecting` is gone, baseline line removed. |
| 08-038, 08-049 | `CallEnd::{Running, Lingering { call_id, _timer }, Busy, Over}`: the stage holds the timer task, so a new call or an account switch drops it; no `Instant` and no id match; tests drive it with the manual clock. |
| 08-039 | `WindowWanted::{Open, ClosedByUser(id), OpenFailed(id)}` replaces the two `Option<i32>` slots and resets when no call shows. |
| 08-040 | `CallPanel::devices_menu` is `Option<(call id, target)>` and is shown only for the call it was opened in, in its active phase; the render no longer resets it. |
| 08-050 | `raised_for` moves from `CallUi` (app-wide) to `CallScreen` (per account); `calls.raised_for` is dropped from `scripts/app-wide-fields.txt`. |
| 08-054 | The sound sync no longer clears `card_window`; `call_card_window` ignores it while the shown call is incoming. |
| 08-055 | `CallSounds` holds one `Voice::{Idle { cue }, Looping { sound, stage }}`; a muted cue returns before cutting a ring; `play` stops the old voice itself. |
| 08-061 | The backdrop cache evicts under a short `try_borrow_mut` and retires the picture after the borrow. |
| new-01 | The `Rc<RefCell<Option<_>>>` one-shot body cells of the bot edit, bot verify and bots dialogs are `Cell`s (no borrow to panic). |
| new-02 | `CallTick::{Idle, Running { _task }}` holds the call clock task instead of detaching it. |

### Sweep fixes (r2-24)

Files: capture_block, chat_appearance_dialog, chat_bars, chat_header_menu_ui,
chat_list_keys, chat_list_state, chat_look_ui, chat_setting_dialogs,
chat_swipe_ui, chat_ui_state, chatlist_global, command_shortcuts.

| id | result |
| --- | --- |
| 08-062 | `apply_sharing` returns whether the platform took the setting; the main and chat windows record `capture_blocked` only then, so a window with no handle yet is retried on the next render. |
| 08-056 | While `Applying` the Appearance draft is locked (`edit_chat_appearance` refuses). A box closed during Apply leaves its answers to `drain_chat_appearance`, which now takes them all and shows a refusal as a note. |
| 08-057 | The level line has three arms read from the request table: the level, "checking" while `getChatBoostStatus` is out, and "unavailable" with Retry once it ended without an answer. |
| 08-081 | The contact bar is dismissed only after both the unarchive and the notification reset were accepted. |
| 08-083, 08-086 | The take-once `Rc<RefCell<Option<body>>>` slot is `shell::one_shot_dialog_content` (the one helper, shared with 13-066), a `Cell` that is taken, so no `RefCell` borrow; used by chat bars (block box, join requests), the look box and Appearance. |
| 08-087 | `HeaderFacts::is_saved_messages` is gone; the profile entry reads `is_self`, which is the same fact. Baseline line dropped. |
| 08-096 | `ChatSwipeFacts::is_saved` is allow-listed: it is a fact of the chat (the user's own chat cannot be muted), not a stage. Baseline lines dropped. |
| 08-071, 08-072 | `Pick<R>` (`chat_list_pick.rs`) is one value per list: none, the pointer's row, or the keys' row over the pointer's. A mouse move hands the selection back to the pointer's row. The pointer's row in the chat list counts only while inside the items the last frame laid out. |
| 08-073 | `search_order` and `search_bounds` are `Cell`s that are taken and put back: no `RefCell` borrow in render. |
| 04-040 | `PinStage::Sliding` replaces `PinReorder`'s inner stage: `PinReorder` has no stage, the owner's variant says held or sliding (`PinView`). `release` moves only a held row: one release of the button reaches the list more than once, and the later calls leave the slide (and a press) as they are. |
| 08-067 | `MyChats { Closed, Groups, Channels }` replaces `Option<bool>`. |
| 08-068 | `close_archive_hint` is the box's one closer; the downloads panel has `close_downloads_panel` and `toggle_downloads_panel`. `pending_close_secret_chat` carries the chat, so it is the box's value. |
| 08-069 | Skipped: the open holds only the intent. The fetch's stage (cache, in flight, failed with its retry clock) lives in `story_fetch`, read by the story's key, which is the fetch's own identity; every exit (ready, gone, failed, a newer open, the viewer closing, the account going) clears the open. |
| 08-070 | `ChatListMenu { Closed, Row, Archive }`: opening one replaces the other. |
| 08-077 | `Dimming::Slider` is the only dimming: no mirrored field and no subscription writing it; Apply and the preview read the slider. |
| 08-078 | The wallpaper picker's task is held by the dialog (`WallpaperPick::Choosing`), so closing the box drops it; a second press while it is open does nothing. |
| 08-079 | Every edit of theme, wallpaper or dimming calls `choices_changed`, which returns a `Blocked` stage to `Editing`. |
| 08-100 | `close_ttl_picker` is the one closer (box close, Esc, leaving the chat, a picked timer). |
| 08-088 | `enter_chat_ui` forgets the request of a `Sending` edit it does not carry to the next chat. |
| 08-089, 08-090 | `EditStage { Editing, ConfirmingCancel, Sending }` replaces `confirm_cancel`; a sent edit refuses Esc and the box (the answer decides); sending closes an open box. tdesktop cancels its request locally and the edit lands anyway, which is the case this refuses. |
| 08-091 | `ScheduledBox { Closed, Open { load, selected }, Rescheduling { selected } }`: the ticks and the list's load (08-115) live and die with the box, and the ticks wait through the picker. |
| 08-092 | `ReplyBar { Plain, PickingQuote }` carries the reply. |
| 08-101 | `IndexBar { Away, Over, Scrubbing }` with one reset per way off the bar. |
| 08-102 | `ScheduleTarget::ComposerThenSend { effect }` replaces `send_after` and `send_effect`. |

### Sweep fixes (r2-25)

Round 2, files `composer_sends`, `composer_state`, `composer_suggest`,
`connect_ui`, `contacts`, `contacts_import`, `context_menu_keys`,
`crash_notice`, `custom_emoji_card`, `deep_link_routes`, `deep_links`,
`direct_messages_admin`, `disk_state`, `downloads`, `draft_timer`, `drafts`,
`drop_zones`, `early_connect`.

| id | what changed |
| --- | --- |
| 09-001 | The emoji suggest rows and the highlight live in `SuggestStage::Live { query, built, rows }`; `SuggestUi::active()` borrows them. `query`, `active`, `selected`, `built_*` are no longer separate fields. |
| 09-002 | The secret-chat inline-bot confirmation is account-wide state (`OnScreen::inline_bot`, `InlineBotUi`), not a per-window `ComposerUi` flag, so a chat window does not ask again. |
| 09-003 | `TraySend::Held` names the renders it waits on and holds only while one still renders in the tray (`send_is_held`). `release_idle_hold` and the manual resets are gone; a failed render of a waited render drops it. |
| 09-004 | `updateMessageSendFailed` names its slot (`messages.failed_sends`); the UI drops that slot's `DraftClear` so a later ack cannot take it. |
| 09-006, 09-065, 09-068 | `Cell` take and put back (`with_vec`, `EditorBlocks`, `early_connect::FIRST`) replace the `RefCell` borrows. |
| 09-007 | Skipped as a second stage, and made one value with one writer: `MenuFocusStage` keeps the focus to give back (no menu holds it); whether a menu is open is read from the menus every frame and the stage changes only in `MenuFocus::settle`. |
| 09-008 | The frame end (`menu_frame_end`) closes a submenu whose parent row was not drawn; the key handler no longer repairs it. |
| 09-009 | `ScreenWorkers` replaces the `screen_preparing` counter: each worker owns an `Arc` claim for as long as it runs. |
| 09-010 | `ConnectStage::Connecting { _task }` owns the connect task; a restart, an adopted client or a wipe (`cancel_connecting`) replaces the stage and ends it. `ConnectStage::Idle` is the stage with nothing in flight. The generation compare in the task is gone. |
| 09-011 | `ImportStage { Showing, Choosing { picker task }, Reading { read task } }` replaces `state` + `pick` + the box id: closing or choosing again drops the tasks. |
| 09-013, 09-022, 09-023, 09-058 | `shell::one_shot_dialog_content` (the one helper, shared with 13-066) hands the dialog body to the kit's `Fn` closure through a `Cell`. |
| 09-018, 09-019 | `CustomEmojiCard::Asked { request }` is settled by the answer or refusal of that request (`CustomEmojiAnswers`, by `RequestId`); `Shown { preview, _timer }` owns its timer. A failed send asks nothing. |
| 09-020 | `NoticeGate::{Unseen { root, crash }, Shown}` hands the crash over once. |
| 09-021 | The deep-link origin window is `DeepLinkFlow::window` in the session; the UI keeps only `InviteBox`, the generation its invite box is up for. The pump takes its steps through `ChatsState::take_deep_link_step` (02-057). |
| 09-059 | `direct.kicked` is `Fetch<bool>`; a refused check shows "Couldn't check the member. Try again." and asks again when picked. |
| 09-060 | `DiskUi::seen` is one `Seen { free, unwritable }` replaced by each reading. |
| 09-061 | The disk poll `Task` is held in `DiskUi`. |
| 09-062 | `DownloadRowKind { Active, Failed, Recent }` replaces `(active, failed)`. |
| 09-063 | `DraftTimer::Armed { token, due, _task }` holds its task; the leaving client's held draft is sent in `take_live`. |
| 09-064 | `take_live` sends the held draft of the client leaving the screen, so the one timer only commits the client on screen. |
| 09-066 | `FileDrag::Probing { paths, _probe }` owns its probe task; a drag that ends or is replaced drops it. |
| 09-067 | `ArchiveJob::Building { slot, _task }`: a zip for another chat is replaced by the drop in the open chat. |

- **Left as they are, on purpose.** `DiskUi::screen_refused` and
  `start_refused` are two clients' refusals (the screen's, the background
  accounts'), each cleared by its own retry, not stages of one thing.
  `ComposerUi::mention_selected` is an index into a menu derived from the text
  on every frame. The `RefCell<Parsed>` of `rich_draft/preview.rs` is read in
  render only and belongs to that file's bundle.
- **Behaviour that changed.** Tapping a custom emoji again while its card shows
  restarts the card's timer without asking again. A custom emoji whose pack
  lookup is refused shows no card instead of waiting for good.

### Sweep fixes (r2-27)

Files: `frame_clips`, `frame_clock`, `frame_grid`, `frame_profile`,
`frame_trace`, `gifs_ui`, the gift craft, market, resale, send, transfer and
value boxes, `giveaway_boosts_ui`, `giveaway_info_ui`. Branched from
`f9fd2d57b` (the commit the register's lines refer to): the named
integration branch `claude/project-thread-mnl7ex` was not on origin.

| Id | Result |
|---|---|
| 09-127 | A sleeping clip frame request's timer is held by its `VideoAsk` (`sleep: Option<Task<()>>`): a replaced, lost or forgotten ask drops and cancels it. |
| 09-128, 09-129, 09-130, 10-001 | Every non-test `RefCell` borrow in the frame files is `try_borrow_mut`/`try_borrow` behind `with_asks`, `with_answered`, `with_cell`, or a let-else; a re-entrant call skips its bookkeeping. |
| 09-122 | "A Quill window is active" is one cell: `FrameUi::window_active` is an `Rc<Cell<bool>>` and `AppGate::active` holds that same cell for the readers that have only a window (clip frame requests), so there is no second copy to keep in step. `AppGate::held` is not a copy of `main_covered`: it is the list of windows hidden whole (the main window when covered, plus the covered chat windows), worked out in `follow_windows` and published when it changes. |
| 09-126 | `Profiler { Off, Armed, Running }` replaces `State { path, last_write, started }`; baseline line `State::started` dropped. |
| 10-002, 10-003 | `GifTab { Wants, Asked, Typing { wanted, _pause } }` replaces `gif_wanted`, `gif_asked` and `gif_keystroke`. The debounce timer lives in the stage: the next keystroke, a section choice or the panel closing (`gif_tab_upkeep`) drops it. A refused send leaves `Wants(shown)`. |
| 10-004 | Skipped: no flag is held. In flight is read from the session's request table, which drops the entry on answer or error, and the driver's own single-flight gate (`set_gif_saved`) reads the same table, so a per-file disable would let a second save be refused silently. |
| 10-011 | A failed Show more page of the on-sale list draws its error with Retry (`retry_row`); the listings stay. |
| 10-009 | The forge's end lets the craft go on every exit that opens no box (`InvalidGift`, an error, a vanished session, a gate with no notice). The forge animation (`CraftStage::Forging`, a minimum display time) and `CraftRun::Crafting` (the request) are different timelines: the answer can come before or after the animation. |
| 10-010 | `CraftSession::reset_lists()` owns the reset. |
| 10-007, 10-005 | `MarketFlow { Boxes { recipient, hide_name }, Opening { request, from_send_box, .. } }` replaces `recipient`, `hide_name` and `auction_open`. The last box leaving, by pop, clear or close_where, resets it (`drop_orphans`). `AuctionFetch::Ready` carries its request: only the answer to this ask opens its box. |
| 10-008 | `bid(gift_id)` / `bid_mut(gift_id)` find the box of that gift. |
| 10-006 | `requested_files` holds `FileAsk::{Sent, Refused}` per file: a refusal is retried only when the file set changes or the boxes reopen. |
| 10-014, 10-017, 10-020 | The take-once dialog bodies are `Cell<Option<_>>`, not `RefCell`. |
| 10-016 | `ResaleBuy::go_to` and `pay_in` own the step and the currency (`pay_in` clears `price_changed`). `gift_inputs.hide_name` stays: the resale purchase continues the send-gift flow it came from, so the choice is shared on purpose; the auction's choice is its own flow's (`MarketFlow`). |
| 10-015 | A refused `open_gift_resale` drops the search it replaced when no resale box shows it. |
| 10-013 | A failed gift list or Premium options list shows its line with Retry (`retry_send_gift_lists`, asks only what failed). A failed Premium list used to carry an empty line and vanish. |
| 10-018 | `TransferPicker::export_refused()` and `ExportError::NotSent` replace the hand-written empty `Other`. |
| 10-019 | A learn-more that cannot go out stores a failed lookup the box shows. |
| 10-028 | The prepaid giveaway list draws Loading (request in flight) or Retry (status missing), from the request table. |
| 10-029 | The giveaway info box opens only once its ask went out. No test: the box host table is private to `chat_window`. |

Tests: `closing_the_panel_drops_the_gif_typing_pause`,
`a_refused_craft_after_the_boxes_closed_drops_the_craft`,
`clearing_the_boxes_ends_a_pending_open_and_forgets_the_recipient`,
`another_asks_auction_answer_does_not_open_this_asks_box`,
`a_bid_box_reads_its_own_slider`,
`a_failed_premium_list_shows_a_line_and_retry_asks_only_it`,
`a_learn_more_that_cannot_go_out_says_so`.

### Sweep fixes (r2-29)

Round 2, files: `inline_members_state/view`, `inline_playback`,
`inline_players`, `inline_return`, `inline_video`, `invite_admin_ui`,
`jump_buttons`, `jump_date`, `language_ui`, `link_inbox`,
`link_info_boxes`, `marketplace`, `message_effect_badge/play`, `motion`,
`paid_messages_privacy/state`, `paragraph_cache`, `payments_state`.

| id | result |
| --- | --- |
| 10-098 | The members list's next-page stage is the session's `MoreMembers` (`Asking { request, offset }`, `Settled { offset }`, `Failed { offset }`); the UI `Paging` copy and `take_member_more_failures` are gone. A scroll re-arms a failed page (`rearm_failed_member_more`), an open or a stale mark forgets answered ones (`forget_member_more`). |
| 10-101 | `InlineMembersUi::chat` deleted (never read). |
| 10-102 | The last good page lives in the session fetch: `SupergroupMembersFetch::Loading { request, last }` keeps the page a membership change dropped. The `RefCell` snapshot and the render-time write are gone; the page is per list key, so it cannot show in another group. |
| (new) | The administrators fallback is asked while the cache has no entry for it (a failed answer, or a failed send, stays an entry that a reopen or Retry drops): `AdminsFallback` is gone. |
| 10-106 | `GifPlayback::{Waiting, Playing}` hold the decode (`GifRun`: the `Task` and its cancel token). Leaving the stage (stop, another playback, the screen going away) cancels the decoder and drops the landing; `gif_decodes` and the "taken back" path are gone. A GIF stopped and started again decodes again. |
| 10-107 | `release_timer` carries its due time; a timer past it is spent even when its own clearing was skipped, so `arm_release` arms again. |
| 10-104 | `press_bot_start` reports whether the start is on its way; the round trip becomes `Started` only then, and ends on a refused start. |
| 10-105 | A round trip a chat window kept is taken only by its own account (`take_slot_inline_return` checks it). The in-place copy still ends through `drop_foreign_inline_return`; the main window's copy goes with the `OnScreen` swap. |
| 10-109 | `corner_mask` retires the evicted mask after the cache borrow ends, and uses `try_borrow_mut`. |
| 10-110, 10-111 | `GroupSettingsDialog::details_link` is gone: the "who joined" details are open when the session holds that link's members page. Offline (a demo) the page opens on the reason, closed by the same toggle. |
| 10-112 | `revoked_open` is gone: the revoked list is open when the session holds its fetch. A refused send is the fetch's `Failed`. The box drops the fetch when it closes or moves to another chat; the answer, the error and "Delete all" touch it only while it waits (`land_revoked`). |
| 10-116, 10-115, 10-122 | `try_borrow_mut` with an early return for the corner stack; `Cell<Option<_>>` for the one-shot dialog bodies. |
| 10-120 | Retry on the language list calls `request_language_packs`, which begins the stage itself; the UI no longer resets it. |
| 10-123 | `MainWindowState::LeavingFullScreen { _poll: Task }` holds the poll, so a raise drops it; its last step runs through the app (not the window handle), so a gone window still leaves the stage. |
| 10-124 | Skipped: the global's handle is the OS window the platform callbacks (no view at hand) act on, set by `attach_main_window`; `QuillApp::main_window.handle` is the window whose frame ran, set each frame. They are two facts that agree in normal use; reading the app's copy from the callbacks would read an entity that may be in the middle of its own update. |
| 10-125 | The link pump sleeps; `pump_deep_link` pokes it when its slot is free and a link waits in the OS inbox. |
| 10-121 | A giftcode lookup is refused while a code is being applied; a refused send puts the old box back. The same for the `setlanguage` lookup. |
| 11-010 | Each cause has its own line: no chat, not connected, request not sent. |
| 11-027 | The effect-badge registries use `try_borrow` / `try_borrow_mut`, take the map out while a callback runs, and skip a re-entrant paint. The pinned bar's presence is followed through `try_borrow_mut`; a frame that could not take it draws no bar. |
| 11-028 | The effect descriptions are asked once when an entry is added. |
| 11-037 | `MotionState::reveal` is a `Cell<Option<Reveal>>`. The marks of kit dialogs closed from outside (`MainWindow::dialogs_closed`) are written and read through `try_borrow_mut`. |
| 11-043 | Save price takes the staged price once the change is on its way; a refused save keeps it, and a lapsed Premium says so. |
| 11-041 | `DirectStage` is gone: saving, refused and done come from the session's `PaidSave` (`TargetSave::Saved` closes the box and is taken by it). |
| 11-042 | `PaidConfirmStage` holds the confirmation and its "Don't ask me again" tick in one value (`None`, `Composer { confirm, trust }`, `Held { trusted }`). |
| 11-050 | The paragraph cache uses `try_borrow_mut`; a re-entrant call lays out without the cache. |
| 11-049 | The four paged dialogs reset their `Asked` when they open. |

### Sweep fixes (r2-28)

Round 2, the call window, history helpers, giveaway pickers and the image
budget. Every file of the bundle was read again from the top.

| ids | what |
| --- | --- |
| 10-042, 10-045, 10-047, 10-054 | The box over the call window is one `CallBox { None, Rename, Settings, Invite, Share, Stream }`, held by the `CallView` of the call it acts on (`GroupCallScreen.view`), together with the pin, the in-call chat, the member levels and the stage volume. `settings_open`, `title_dialog`, `GroupCallBoxes.call_box`, `StreamWindow.overlay`, `boxes_call` and the pin field are gone. A call change drops the view whole, and so does an account switch (the screen is replaced). Opening a box needs a tracked call, so a share box can no longer be left open for a dropped one (`call: i32`). The screen-share privacy box stays the chooser's own stage: it is a shell dialog with its own enum, and `sync_group_call_boxes` still closes it with the window. |
| 10-048, 10-049 | `GroupCallScreen::start_join` keeps a wish under a join like `want_stream` keeps a join under a wish. A failed `getGroupCall` is recorded by call id (`CallsState::group_call_fetch_failures`) and taken by the wish that asked for that call, so it ends whichever call is tracked. |
| 10-050, 10-056 | The controls fade holds its wake-up task in `Reveal::Shown { until, timer }`: hiding, leaving full screen and `clear` drop it. The window's layout is `CallWindow::Open { layout }`; `window_is_stream` is gone. |
| 10-046 | The stage slider's release is `SliderStage::Sent { request }`: once the answer is in the slider is made again from the server's level, so a refused change snaps back. `set_group_call_participant_volume` returns the request. |
| 10-036 | The big button reads Connecting while the join request is pending (`JoinVideoChat` in the session's requests). |
| 10-034 | `PushToTalk<T>` carries the release timer in `Releasing { mute_at, wake }`; a new press, the deadline and a reset drop it. |
| 10-035 | The push-to-talk key capture is `Capture::PushToTalk`, the same stage as the Shortcuts rows: opening a row replaces it, and the lock, leaving the Calls page and an account change end it. `ptt_capture` is gone. |
| 10-031, 10-032, 10-033 | `glyph_sweep` uses `try_borrow_mut`; `Sweep::Left` replaces `IdleTrigger` and is spent by the same `begin` that decides; a `Marked` sweep whose frame never starts (hidden window) is given up after two trims. |
| 10-030, 10-039 | The group settings dialog hands its body over in a `Cell`; the invite-link page asks the list and the counts independently, and Retry sends the counts too. |
| 10-084, 10-086 | The reaction fly is taken out of its slot by the first frame after its run. The date pill's `settle` ends `Showing` when the hold runs out, with its timer and `covered_row`, which now lives in `Showing`. |
| 10-085 | Skipped as a stage, kept as a method: `ends`, `had_newer`, the chrome marks and `media_signature` describe the rows that are still in `rows`, which the next build is compared against (the splice keeps the reader's anchor); resetting them with `rows_key` would turn every append or prepend into a full replace. `rows_key` is cleared only by `HistoryUi::invalidate_rows`. |
| 10-087 | The deferred focus-in checks the history still has focus. |
| 10-089, 10-090 | `Entry.stage` is `EntryStage { Loading, Ready { bytes }, Failed { at } }`. The thread-local maps and `SizedLoad` use `try_borrow(_mut)` through `with_cell` / `read_cell`, with the safe answer when a cell is busy. |
| 10-026, 10-027 | The giveaway form has its own search input (`GiveawayForm.search`); the gift pickers keep `pick_search`. The members list decides from its stage in one place (`members_page_due`) and a failed page draws a Retry. |
| 10-099, 10-100, 10-103 | `InlineMembersUi::refetch` is the one transition for stale, Retry and open. A refused administrators send records the failure the list draws with Retry; a refused next-page send of the inline results sets `more_failed`. |

### Sweep fixes (r2-05b)

The skips of 10-065, 12-B004 and 13-048 above did not hold (owner's file-complete order); they are fixed here.

| id | what changed |
| --- | --- |
| 01-010 | The `take_turn` tests no longer sleep: the drop/commit and flood tests book on `State` and read the map. |
| 01-022, 01-023 | Verified fixed by r2-05 (`BoostWork` carries the request; the note lives in the phase). |
| 01-034, 01-036 | Verified fixed by r2-05; `plan` no longer marks a request, the driver marks it with `sent(speaker, request)` after the send went out, so a refused send needs no undo. |
| 01-035 | `ShareLink::Fetching(RequestId)`: `answered` and `failed` take the request id and settle only the slot waiting for that id, so a revoked link's late answer cannot land on the next click's fetch. |
| 02-003, 02-004 | Not a stored mode: the mode is derived by one function (`sends_as_files`) read by the checkbox, the next drop and the send options; the attachments' kinds are the only stored truth, and a second stored field would be two fields kept in step. |
| 03-B003, 03-B004 | Verified fixed by r2-05. |
| 10-059, 10-066, 10-069 | 10-059 and 10-066 verified fixed by r2-05. `LinkCheck::Typed { _timer: Task }` owns the pause before the server check; a keystroke, closing the box or the box going away drops it, so the version counter and its `ui-statics` line are gone. |
| 10-065 | `MemberChange { request, kind }` list on `AdminUi`: an unban and the Add-special "Remove user" ban report "unbanned" / "banned" (and close the picker) when their request is answered, and the error line when it is refused (`settle_member_changes`). |
| 10-068 | `chat_invite_link_count_unknown`: a count that is loading or failed makes Save ask again and stay open ("Invite links are still loading" / "Couldn't refresh invite links"), not read as zero. A chat whose count is never asked for (not the owner) still reads zero. |
| 12-B001, 12-B002 | Verified fixed by r2-05. |
| 12-B004 | `ShortcutPane { stage: PaneStage, error }`: `Editing`, `Saving { request }`, `Accepted`, `ConfirmDelete`, `Deleting { request }`. Any other action replaces `ConfirmDelete`; the shortcut's delete is `Deleting` until its own answer (list on success, the error line and the field on a refusal). A new text or edit is not taken while one is on its way. |
| 13-047, 13-048 | 13-047 verified fixed by r2-05. `StoryOpen { Shown(viewer), Waiting { open, shown } }` replaces `StoryUi::viewer` and `ChatListUi::pending_story_open`; the viewer reads through `Deref`, and a newer press, the story arriving, a step and a close are transitions of that one value. |

### Sweep fixes (r2-08b)

Corrects the r2-08 skips in `call_audio_ui`, `device_qr`, `inline_mode` and `shell`.

| id | result |
|---|---|
| 08-041, 08-042 | `MicTestRun::Opening(Task<()>)`: the stage owns the opening task, so Stop or closing Settings drops the task and the device it opens; `mic_test_starts` and the stale-answer branch are gone. The watchdog `Task` lives in `MicTest`; a failed device draws no 30 fps tick (already fixed by r2-08, checked). |
| 09-024 | Already fixed by r2-08: `DeviceLink::Linking(RequestId)`; `clear_device_qr` no longer touches the session while the confirm is out. |
| 09-025 | `DeviceLogin::InFlight` is gone: the request in flight is `sessions_op`'s (`begin(id)`), which already locks the sessions page. What stays is the answer slot `Done { request, result }`, taken only by the request that asked, and `clear_device_qr` discards it when the owner goes away (`DeviceLogin::discard`). |
| 10-092, 10-095, 10-096 | Already fixed by r2-08 (failed lookup asked again, failed query shows its error, a pick without a connection keeps the query); re-checked against the new per-username entries. |
| 10-093 | `BotsState::inline_bots`: one entry per lowercase username (bounded, settled entries leave first). `Resolving { request }` is answered only by that `RequestId` (purpose `ResolveInlineBot` is unit; `generation` and `inline_bot_resolve_seq` are gone). `ChatUi::inline_bot`, `capture_inline_bot` and the re-ask workaround are gone: another window's lookup cannot replace this window's. |
| 10-094 | `InlineQueryFetch::Loading(RequestId)`: a first page fills the slot, and a refusal fails it, only for the request the slot waits for. `InlineWait::Session` re-reads session state on every update and answers nothing, so it needs no id. |
| 10-097 | The confirmed-once latch moved from `ComposerUi` (per window) to `BotsState::inline_warning_confirmed` (the account's session: every window of it shares it, another account starts unconfirmed). The box being asked stays `ChatUi::bot_alert`; the latch is an independent per-account fact, not a stage of the ask. |
| 13-009 | Already fixed by r2-08 (`Host::Asked { waits }` ends with its own requests). |
| 13-011 | `ClosedKitDialogs` wraps the mailbox: `mark` and `take` use `try_borrow_mut` and a busy cell reads as no mark; no raw `borrow_mut` is left. |
| 13-010 | Not collapsed. The kit gives no way to name the dialog it shows, so the shell's `open_dialog` (what this window's kit shows), the mailbox (closers hold only `&QuillApp`, often inside the shell's own update, so they cannot write the shell) and the host map (which window owns a session-wide kind) stay three records; the mailbox is now a typed one-shot. |

### Sweep fixes (r2-03b)

Round 2, the entries left in the files r2-03 touched
(`connect/{call_screen, gift_manage, premium_hub, ton_wallet}.rs`,
`connect/core/mod.rs`, `connect/group_calls/live_story.rs`).

| id | result |
|---|---|
| 02-014 | What Telegram was told about a group share's pause is `GroupScreenShare::Sharing { paused }`; `ActiveGroupCall::screen_share_paused` is gone. `ActiveScreen::paused` stays: it is the capture's own state (the window is hidden), also used by 1:1 calls that have no pause on the server; the told mark is written only from the edge that sends and its revert. |
| 02-013, 02-015, 02-016, 02-018, 02-029, 02-033, 02-034, 02-035, 02-045, 05-030, 02-046, 02-067, 02-068, 02-074, 02-087, 02-108, 07-013 | Already fixed by r2-03 (see its entry); each re-read against the current code. |
| 02-017 | The 10 s engine wait was already in `next_deadline`. `CallAudio::until_reopen` now reports the level tap's 5 s reopen too; it is `None` once the wait is over, so a failed tap that no call wants cannot spin the loop. |
| 02-048 | `ListOp<K>` carries the kind of the change in flight (`Mutating(request, kind)`); the hub's is `ListOp<GiftMutation>` and `Delete(id)` rides in it. `GiftManageState::deleting` is gone; `changed(request)` gives the kind only to the change that is in flight, so a late answer of another request deletes nothing. |
| 02-049 | `Session::restart_gifts_list(collection)` resets the hub's list and the per-list management state (`show_collection`) together; opening an owner and switching tabs both call it. |
| 02-050 | A `sendGift` out keeps its selected gift when the box closes, so reopening for the same recipient shows that send (the box's `open`, the recipient and the `SendStage` are separate facts by design: the answer of a closed box becomes a notice). |
| 02-096 | `gifts_open` is gone (the dialog is open while `gifts_owner` is set); `stars_open` and `tx_selected` are one `StarsBox { Closed, List, Details(id) }` whose selection cannot outlive the box. |

### Sweep fixes (r2-35)

Round 2, bundle 35: the chat list (rows, drop on row, drop topic, sidebar,
view), join requests, chat look, forum row topics, the chat window slot,
saved windows and dialog hosts, communities, the composer's thread and
mention helpers, and the bar slide.

| Id | Fix |
| --- | --- |
| 08-082 | `ConnectDriver::retry_join_requests` drops the failed value and asks again; the box no longer edits the session from the UI. |
| 08-084, 08-085 | The take-once dialog bodies are `Rc<Cell<Option<AnyElement>>>` (`take()`), no `RefCell` borrow. |
| 08-063, 08-064, 08-065 | `DropOnRow` is `seen: Dragged` and `freeze: Freeze`. The lit row lives inside each drag (`Over::Row` for files, `hover` for a forward drag); a file drag is `Unsendable { paths }` or `Sendable { paths, kind, over }` with the pointer position and the one-second wait inside the row stage. The screenshot demos' fixed drag is `Dragged::Staged` (demo builds only). `Freeze::Released` names the released-with-timer stage. |
| 08-080 | The drop chooser's "topics could not load" line has Retry, which sends `getForumTopics` again. |
| 08-066 | `ChatListView { filter, folder }` is one value owned by `ChatListUi` (`view`); `FolderUi::tab` is gone. Only `ChatListUi::show_view` writes it and ends the selection and pinned drag. The folder-removed path in `notifications` runs the same transition (it used to clear the selection only). |
| 08-075 | `ChatListUi::show_tab` / `leave_rows` end the pinned drag and the laid-out rows when Contacts, Calls or the search take the column; `begin_pin_drag` owns the peek reset; `PinStage::advance` ends a drag whose window lost focus. Render writes none of them. |
| 08-074, 08-098 | `row_fx` is read through `begin_row_pass` / `observe_row` with `try_borrow_mut`; a map in use shows the row settled. |
| 08-076 | `createPrivateChat` is keyed by the chat it creates (a private chat's id is its user's id): another user's creation in flight no longer swallows Saved Messages. |
| 08-094 | `both` is the argument of `apply_chat_look`, not a dialog field. |
| 08-093 | `LookStage::Blocked { why, done }` keeps the acknowledged changes. |
| 08-095 | `RowForum::list` is `RowTopics::{Loading, Failed, Loaded(Vec<RowTopic>)}`; a failed list draws "topics could not load" with a Retry chip on the row. `RowForum::loaded` left the implicit-state baseline. |
| 08-097, 08-106 | Every non-test slot borrow in `cover.rs`, `slot.rs`, `view.rs` and `chat_window/mod.rs` is a `try_borrow`/`try_borrow_mut`; `swap_slot` reports whether it swapped and `set_window_scope` keeps the scope (or returns the session to the main view) when it did not. `WindowSlot::stand: Standing` records active, place and bounds in one `report`. |
| 08-099 | `Host::Asked { window, since }` ends when no request sent since the ask (`RequestRegistry::any_since`) is in flight, not when no request at all is. |
| 08-103 | `Restore::Opening { waiting, refused }` and `Restore::Done(kept)`: a window whose open was refused stays in the file; one whose chat can never have a window is dropped. |
| 08-104 | Closing the offer without an answer (`on_close`) is `Restore::dismissed`: "not now", nothing asked again this launch, no answer stored. |
| 08-105 | The reopen place and the "moved" test read the slot's `Standing`, which `placed()` also updates under the lock; a move while locked schedules the save. `bounds_seen` is gone. |
| 08-110 | The community create box's busy state is keyed by its chat (`has_purpose_for_chat`). |
| 08-112 | The chooser reads Loading while either list loads. |
| 08-111 | The photo picker's answer is applied only to the Create box that asked (its name field's entity id). |
| 08-109 | `ban_waits` is a queue: a newer ban waits behind the first. |
| 08-117 | `start_bot_thread` no longer dedups by purpose and chat; each window's own `BotThreadStart` stage guards its own send. |
| 08-118 | `end_mention_search(chat)` ends only that chat's `@` search; closing or completing one window's menu leaves another window's results. |
| 09-005 | `follow_bar` uses `try_borrow_mut` and draws no bar for one frame on contention. |

Touched outside the bundle's files, as small as possible: `src/connect/{invite_admin,chat_list/mod,forum_saved,composer}.rs`,
`src/state/{requests,session_history_window,session_subsection_tabs}.rs`, `src/ui/chat_list_state.rs`,
`src/ui/folders_state.rs`, `src/ui/shell.rs` (the ask expiry), `src/ui/deep_links.rs`, `src/ui/search_ui/mod.rs`
and the readers of the chat list view (`folder_tabs`, `folder_extras`, `chat_list_keys`, `shortcut_pack`, `pull_next_ui`,
`stories_pull_ui`, `folders/save_folder_editor`, `empty_state_demo`).

### Sweep fixes (r2-42)

| id | what | fix |
| --- | --- | --- |
| 14-021 | widget canvas read/put_image added to an unchecked position | saturating i64 positions, checked `w*h*4`, and `put_image` skips a size its pixel buffer cannot hold |
| 14-022 | `wait_for_cancelable_child` left a child unreaped on a `try_wait` error or a poisoned slot | on the integration branch's `Extraction` stage: every lock site (`cancel`, `is_cancelled`, publish, poll) recovers a poisoned lock through one `stage()` helper, and a `try_wait` error kills and reaps the child before returning |
| 14-072 | `BgraPool` `released` bool beside `buffers` | one enum, `Recycling(buffers)` or `Released`; released holds none by construction |
| 14-073 | NV12 rotation-0 arm split by zero-byte chunks for width 0 | an empty plane returns before the split |
| 14-029 | `State::tap` plus `AudioShared::detached` kept in step; `AudioTap::sounding` mirrored `AudioShared::starved` | the sound's life is the queue value (`Open` or `Detached`); the output's state is `AudioShared::starved` alone, which `AudioTap::sounding()` reads (the tap field is gone) |
| 14-051 | an unreadable trust file read as empty and written over | `Trust::Read` or `Trust::Unreadable`; unreadable is never saved over, and the next change reads it again and merges |
| 14-035 | a PIN sent while none was asked answered the next ask | `ask_pin` drops stale PINs before it sends `NeedPin` |
| 14-065 | unchecked `Instant + timeout` | `checked_add` with a fallback (a year, in `hid_macos` and `hybrid/flow` alike) |
| 14-038 | `Initiator::ephemeral` as a "first message sent" flag | typestate: `initial_message` consumes the `Initiator` and returns `AwaitingResponse`, the only holder of `process_response` |
| 14-023 | passkey flow tests waited on the wall clock | `Ceremony::wait` blocks on the worker's channel, the flow reads `Environment::now` (a manual clock in tests), the probe test uses a zero probe |
| file-complete | `video_decode/tests.rs`, `ceremony_tests.rs`, `fake.rs` slept and read the wall clock | `wait_until` and `pull_sound` count turns (`yield_now`), no clock; ceremony tests use `Ceremony::wait`, the key's `held` flag, and a `ManualPace` behind `Link::pace` (the device wait reads `Pace`, `SystemPace` in production) |

- **Tests.** `a_pin_sent_while_none_was_asked_does_not_answer_the_next_ask`,
  `an_unreadable_file_is_never_saved_over`,
  `load_tells_a_missing_file_from_a_corrupt_one`,
  `a_zero_width_picture_turns_to_nothing`, the canvas edge tests, and
  `the_code_expires_with_the_request` on the manual clock.

- **Tests deleted.** `a_paused_player_decodes_only_its_queue` and
  `an_idle_player_sleeps_long_but_wakes_when_a_picture_is_taken` only proved
  "nothing happens" with a sleep and the wake with an elapsed time; the
  queue bounds are `decoding_stops_at_the_queue_bounds`, and the wake on
  `take_frame` has no deterministic observer. `the_sound_buffer_is_bounded`
  now waits for the queue to reach its cap and bounds the decode calls at
  that moment.
- **Tests added.** `a_forgotten_account_stays_forgotten_when_the_file_reads_again`;
  `no_key_until_the_timeout` runs `run_register` on a `ManualPace`
  (exactly four looks).
- **Fresh review.** Every touched file re-read: 0 findings left.

### Sweep fixes (r2-20)

Round 2, bundle 20: the state of messages, payments, settings, stickers,
stories, threads and users. Every fetch answer and error lands only in the
stage whose request it is. The branch was merged with the integration branch
(`claude/sweep-r2-followup`) and every duplicate was settled in favour of its
shape: the checkout (`CheckoutForm`), gift manage and giveaway launches, the
chatbot lookup legs (`Fetch` per search), `ForumTopicsMap::answered`,
`DeviceLogin`, the sessions list, `data_storage_dirty`, the auto-download
answers (`AutoDownloadAnswers`). What follows is what this bundle adds on top.

| id | what changed |
| --- | --- |
| 05-039 | `SponsoredReportFlow` has `Choosing(target)` (before the send) and `Asking { target, request }`; the request is never an `Option` in a stage, `Picking` holds the flight only |
| 03-A007, 05-036 | `EarnPage::ad_account_url` is a `Load<String>` with the `RequestId` inside `Loading` (ton and stars already were on the integration branch); an answer or error settles only its own request. A refused link is `Failed`, drawn next to the withdraw buttons with a Retry (`retry_earn_ad_account_url`), and a send that never left is `Failed` too, not `Idle`; an empty link is `Unavailable` and leaves the row out. tdesktop asks the link once per page and shows nothing when it fails; the Retry is the smallest safe addition, since the stage was stored but never drawn |
| 05-031 | gift collections: the integration branch's `Fetch` shape stands |
| 05-034, 05-035 | giveaway options and launches: the integration shape stands. `apply_gift_market_purchase` and `apply_gift_market_error` take the `Option<RequestId>` from `payments/apply.rs` and `error.rs` (no `map_or(0, ..)` sentinel at the call sites); a request nobody tracked fails nothing. Covered by `a_gift_market_error_with_no_request_fails_nothing_it_does_not_wait_for`. The u64 request fields inside `gift_market/state.rs` stay with the gift market's own state (they hold real ids, compared by `waits_for`) |
| 05-032 | the checkout is the integration branch's `CheckoutForm` (invoice inside `Loading`/`Ready`/`Verifying`/`Paid`); this bundle's own `Checkout.request` is gone |
| 05-033 | "Media Unlocked", receipts and verification pages: the integration branch's shapes stand (`unlocked_notice`, `verification_url`) |
| 05-042 | sessions, websites, storage statistics, account TTL, auto-delete and saved sounds are matched by request id at every answer and error site; every `begin()` is followed by `sent(extra)` |
| 05-043, 08-033 | Business saves are `(RequestId, kind)` removed by id; the refusal carries its `BusinessArea` and a screen draws only its own; chat links, the connected bot and the time zones answer through `Fetch::answered`/`failed` |
| 05-040 | the chatbot lookup: the integration branch's `Fetch` per leg stands (a refused search is `Failed`, not "not found") |
| 08-031 | a bot save does not rewrite the `connected_bot` fetch; `ConnectedBotSave::Saving(request)` holds the newest save, and only its answer or error forgets the bot it wrote ahead (an older save's answer finds another request there). No scan of the request registry |
| 05-045 | `SetOption { generation }` and `Synchronize { generation }` belong to their switch; their error ends it with `SwitchFailed` |
| 05-044 | quick reply `sending`/`changing` mirrors are deleted; the request registry carries what each request is for (`subject_id`) |
| 05-041 | `ScopeSettings`: one `Fetch` per scope (no loading set beside the data); a refused scope shows Retry and is asked again when the page opens, not on every ingest |
| 05-046 | `pending_attention: bool` stays, as on the integration branch (r2-18, 05-080): it is a one-shot dock-bounce edge that `take` consumes, nothing reads which chat asked, and its line is in `scripts/implicit-state-allow.txt`. The `attention_chats: Vec<ChatId>` this bundle had was the same flag in a new type |
| 05-047 | the integration branch's `data_storage_dirty` stands, with one change: the driver makes one write attempt per mark. A failed write is said on the Data & Storage page (`StorageSaveKind::Persist`) and tried again when the settings change next, not on every ingest |
| 05-057 | `AttachedSets { Idle, Asked(request), Answered(first) }` replaces `attached_request` + `attached_answer` |
| 05-063 | `ManagerCommit::stage: CommitStage { Idle, Sent(request) }`: `Sent` leaves on the step's own answer (`StickersManager::step_answered`, called after every `apply`) or failure, and "a step is out" is that stage, no longer a scan of the request registry (`manager_change_pending` is gone). The failure the manager records is read by the poll and shown once |
| 05-058 | `MasksPanel::archived` is `Fetch<ArchivedMasks>` with the next offset inside |
| 05-054 | one `active_reactions` list |
| 05-055 | the stale marks are `Fetch::Outdated` on their lists (`recent`, `favorites`, `trending`, saved GIFs, both installed sets) and `MessageReactionsState::Stale` for the open picker; a refused refetch keeps the mark. After a set edit each installed list (sticker and emoji) is refetched on its own mark: a refused emoji send marks its list `Outdated` again instead of waiting for an unrelated refresh |
| 05-051 | `StoriesState::available_reactions` is a `Fetch` answered only by `GetStoryAvailableReactions` under its request; a failure is stored and the next open asks again |
| 05-059 | the integration branch's `ForumTopicsMap::begin`/`sent`/`answered`/`failed_request`: an older ask's answer or error finds another request waiting and lands nowhere. After the cached list is dropped (`remove`, a created or edited topic) nothing waits, so a late older answer is taken as an untracked one; the integration tests rely on untracked answers being taken |
| 05-052 | `CloseFriendsList::answered(request, ids)` |
| 05-053 | contact changes `outdate()` the list in place (it no longer blanks under a spinner); `fetch_contacts` reads the list's own stage instead of asking the request registry, so a fetch out from before the change is ignored when it lands |
| 05-060 | `SaveWatch::take_outcome` reads the request registry itself instead of a caller-supplied `in_flight` |

Files read again from the top with the re-sweep criteria and left with no
finding: every file of the bundle. Kept as they are, with the reason: the
one-shot UI notes in `MessagesState` (`Notes`, `message_link`,
`instant_view`, AI results) are each drained by the one surface that asked
and their register entries were all closed in round 1;
`StickersManager::open` and `sponsored_hidden` are a box's visibility and a
fact the server confirmed.

### Sweep fixes (r2-38)

Files: image budget, interface zoom, invite-link custom boxes, keybindings, location picker, media panel status box, media viewer, message actions (peek, chat menu), message menu UI, message text, new chat, notifications inline reply.

| id | result |
| --- | --- |
| 10-091 | `retired.rs`: every thread-local cell goes through `with_cell` (`try_borrow_mut`); a sweep that finds the frame books busy drops nothing (`close_frame`/`LatestFrames::now`/`see` return `Option`). The sweeper task is held by its `Sweeper` global. |
| 10-108 | `interface_zoom`: `try_borrow_mut` everywhere; the run state moved to `window_zoom.rs` (file size). |
| 10-113 | `LimitFocus` deleted: the field takes focus from `window.defer` at open (a test checks it holds focus). |
| 10-114, 11-017 | dialog bodies are taken from a `Cell`, no `RefCell` (custom box, status time box, block-from-replies box). |
| 10-119 | `apply_custom_bindings` reads the keymap with `try_borrow` and leaves it alone when it is held. |
| 11-011 | `location_search_changed(SearchEdit)` is the one transition that opens, edits and closes the search field and the picker's query together (`SearchEdit { Open(input), Text, Close }`); an edit with no field open reaches neither. |
| 11-012 | `locate_started` puts the picker back to `Locating` when "My location" starts a new lookup. |
| 11-013 | macOS locate: the replaced `Running` leaves the cell before the native stop calls; other cells use `try_borrow`. |
| 11-007 | `BoostState<T>`: the hold timer lives in `Armed`, so tick, release, cancel and a stale press drop it. |
| 11-005 | the window's pause of a looping clip lives in `ViewerClip::Playing { pause }` (`hold_for_window`, `release_window_hold`), so a stopped or replaced clip loses it; `ViewerExtra::loop_pause` is gone. |
| 11-008 | merged onto the integration shape: `SwipeTimer` on the viewer owns the end-of-swipe task and `closed` drops it. |
| 11-014 | on the integration shape: `ViewerUi::new_item` and `closed` replace the two hand lists, and both name every field of `ViewerUi`, so a field added later does not compile until it is decided. |
| 11-016 | the viewer render copies only the item's files and download intents (`FileMap::only`, `DownloadIntents::only`). |
| 11-006 | merged onto the integration shape: one `SliderDrag` on `VideoControlsState`. |
| 11-015 | merged onto the integration shape: `ForwardStage::Picking.share_at`. |
| 11-003, 11-020 | `ChatOverlay::Pressing` (r2-31's one overlay value) owns its timer task; `is_press` and the `Instant` token are gone; a release, a newer press, a menu or `close_peek` drops it. |
| 11-023 | on the integration shape: `ChatOverlay::RowMenu` is the only chat-row menu owner; `ChatMenuState::kind` (`ChatMenuKind {Row, TopPerson}`) replaces the `top_person` bool. |
| 11-024 | one `RightPress` stage holds link and item and the task that ends it. |
| 11-026 | one message menu is open across windows (`close_message_menus_of_other_windows`), so the session's audience and reaction slots belong to it. `wanted_reactor_tab` is taken synchronously by the fetch that follows, so no other window can touch it. |
| 11-021 | `SetBoxHost {Closed, In(window)}` on the box's own state replaces the per-window `StickerBox` flag. |
| 11-029 | Save As says "Couldn't start the download" when the send is refused. |
| 11-025 | a new Block target is not written over a `Submitting` one. |
| 11-022 | `Delivery {Incoming, Sending, Sent, Read}` replaces `FooterMeta::pending` (baseline line dropped). It is a draw-time mark derived from the message, so its `Sending` and `Sent` are in `stage-enums-allow.txt` with the reason. |
| 11-036 | closing the dialog or leaving the Saving step forgets the type-save requests. |
| 11-032 | an account's reply targets, parked replies and photo waits go with it. Parked text is kept across a lock on purpose (existing test). |
| new-01 | the viewer's saved toast owns its hide timer (`SavedToastStage`, in `saved_toast.rs`), no generation counter. |

Not changed: `NewChatDialog.photo`/`photo_note` (the kept photo and the note about the latest failed pick are independent). The locate poll loop is bounded by its timeout and owned by the `Locate` stage.

### Sweep fixes (r2-14)

Second-round re-sweep of `src/connect` files (peer QR, polls, proxy, quick
replies, stickers and their creator, story insights, suggested posts, calls,
group calls, boosts). Every answer is matched to the request its stage holds.

| Id | What changed |
| --- | --- |
| 02-094 | `UserLinkFetch::Loading(RequestId)`; `accept_link`/`fail_link` take the request and act only on their own. |
| 02-093 | Same shape as r2-18's 05-073: `PollVotersFetch::Loading(RequestId)` and `VotersMore::{Ready, Asking(RequestId)}` inside `Loaded` (this branch's `PollVotersMore` is dropped in the merge). |
| 02-091 | `PingStatus::Checking(RequestId)`, set once the send went out (r2-10's shape kept in the merge, with its `PROBE_TIMEOUT_MS` give-up); `proxy_ping_requests` deleted. |
| 02-095 | A change is answered by its own request through `session.acks` (r2-05's 12-B002; this branch's `in_flight` list is dropped in the merge). Message loads carry their `RequestId` and an `ok` with no update ends the wait (`messages_settled`). |
| 02-099 | `StickerBatch::{Idle, Running, Finished}` with `begin(total, sent)` and `finish_item(request, success)` (r2-18's shape kept in the merge). |
| 02-100, 05-056, 06-029 | Stale marks are `Fetch::Outdated` on `lists.recent/favorites/trending` and `GifPanel::saved`, and `MessageReactionsState::Outdated`; a refused refetch keeps the mark (lists, GIFs) or fails the picker (reactions). The in-flight request asked before the change is dropped, not trusted. |
| 02-101 | `StickerSelection` and `StickerSearchResults` are replaced whole at every mode change. |
| 02-105 | `StickerSetViewStage::{LookingUp(RequestId), AttachedFound(set)}`; the answer slots and the set-0 sentinel are gone. |
| 13-027 | The refetch after a set edit drops a list request asked before the edit and sends a new one. |
| 13-029 | A refused `getStickerSet` refresh ends the ask with a note instead of being rebuilt every poll. |
| 13-036 | A first forwards page that cannot be sent is `ForwardsPage::Failed`; a scroll asks again. |
| 02-107 | One `addOffer` per message at a time (`suggested_offer_busy`); the box keeps its "Sending" note. |
| 03-A009 | Same fix as r2-03's 02-056: `OwnerClaim::{Unclaimed, Saved, Unsaved}` and `account_data::write_owner` (this branch's `Claimed` and `save_owner` are dropped in the merge). |
| 02-031 | A refused `discardCall` keeps its entry and the rest of both queues. |
| 02-037 | The call-privacy ask stage, its mirror fields (`privacy_allow_calls`, `privacy_p2p`), `CallPrivacySetting` and the two driver methods are deleted: nothing in production used them. |
| 02-032 | `CallProgress::Answered { call_id, accept }`: a second Accept returns the first request; only its own error clears the answer. |
| 02-040 | `createPrivateChat` is guarded per user, keyed by the chat it creates (the integration branch's shape). |
| 02-039 | A second Log out confirm returns the `logOut` request that is out. |
| 02-059, 02-060 | `ParticipantPaging::{Idle, Loading(RequestId), Failed}` on the call; `loaded_all_participants` stays as TDLib's own report (allow list), its baseline lines are dropped. |
| 02-061 | `ConferenceKey::Joining(RequestId)`: the join's answer or error is taken only by the stage waiting for it. |
| 02-058 | `GroupScreenShare::{Off, Failed(String), Starting { request, answer: Option }, Sharing { paused }}`; `screen_share_paused` and `screen_share_error` deleted. |
| 02-072 | Replacing the engine fails each waiting offer (`EngineError::Unavailable`) so the rejoin, share or conference join is put back. |
| 02-069 | A start after a failed first fetch asks again. |
| 02-071 | `BoostWork::{Slots { request }, SlotsReady, Boosting { request, reassign }}` as r2-05's 01-022: a press whose slots read is still out joins that request. |
| 02-075 | A boost status asked before the level moved is dropped and asked again. |

- **Behaviour that changed.** Opening the "Show more" of a call with
  100+ people twice sends one page. A rejoin's fresh native context and a
  new share clear the last share's failure (`GroupScreenShare::Failed` is
  left by `start`).
- **Skipped.** None.
### Sweep fixes (r2-02)

| Id | Fix |
| --- | --- |
| 05-001 | The refetch mark is gone. `AuthState::password_state` is a `PasswordCache` (`Unknown`, `Fresh`, `Outdated`, `Refetching { request }`): a reset, a cancelled reset or a changed login email mark it `Outdated`; the ingest sends the refetch and moves it to `Refetching` only once the send went out (a refused send stays `Outdated` for the next ingest); the answer of any password request makes it `Fresh`; a refused refetch leaves what is shown as `Fresh` and does not ask again by itself. |
| 05-002 | `ReviewStage::Reviewing { items: Vec<ReviewItem> }`: each session holds the `RequestId` of its own `confirmSession`/`terminateSession` (`None` until sent). An answer or error settles only its own item; a lost request settles through the error arm (`settle_lost_request`); an answer of another request, or a repeat, changes nothing. The `waiting` counter is gone. |
| 05-003 | `AccountNotices::unconfirmed` is one `Option<UnconfirmedAlert { first, count, entries }>` replaced whole; `unconfirmed_count()` and `unconfirmed_entries()` read it. A count of 0 drops the alert; a `getActiveSessions` answer with no alert announced resolves nothing (the update that announces one asks again). |
| 05-004 | `TwofaFlow` is one enum (`Idle`, `RecoveryCode`, `LoginEmailCode`, `Done(TwofaNotice)`): one step at a time, a step that starts or finishes replaces what the last left, and the dialog going back to its status view clears it (`clear`, `clear_notice`). `CodeWait` and the notice slot are gone. |
| 05-013 | `InlineQueryFetch::Loading(RequestId)`: only the first-page answer or error of the request the slot holds leaves it. The request is registered before the slot is written. |
| 05-014 | `BotsState::login_url: LoginUrlStage { Idle, Asking { request, press }, Answered { info, press } }` replaces `last_login_url_info` + `login_url_request`. Only the answer or error of the request the stage asks moves it (`httpUrl` too); a second press waits while a press is out or its answer is not taken (`is_busy`); a refused send returns it to `Idle`; the return to the screen drops an answer nobody took and keeps a press in flight. A press refused by TDLib becomes `Answered(Failed { fallback_url })`. |
| 05-020 | `ChatListState::folder_paging: HashMap<folder id, ListPaging>` replaces `folder_chats_exhausted`. A failed folder page lands in `Retrying`; `chat_list_tick` asks again when the wait is over; 404 ends it; a page that goes through clears the wait. Leaving the folder tab (`set_chat_list_view`) calls `folder_left`: a folder nobody views stops retrying, and opening it asks at once. |
| 05-048 | The tray refresh is asked by the ingest that queued it (`ConnectDriver::ingest` calls `refresh_story_trays`), not by frame upkeep. A poster stays in `tray_refresh` until its `getChatActiveStories` is sent; a refused send waits for the next ingest, never for every frame. |
| 05-049, 05-050 | `LiveBursts::{Unasked, Requested(RequestId), Failed, Loaded}`. A failure is `Failed`, not asked again by the call updates of the same story; the next live story opened (`begin_live_story`) asks once more. The answer is taken only from the request the stage holds. |
| 05-067, 05-068 | `state::saves::{Saves, Optimistic}` replace `*_saved_at` and the `previous` carried in the request. The confirmed value and the saves in flight (each with its `RequestId`) are one value; the row shows the newest save and falls back to the next save still out, or to the confirmed value, whichever order the saves fail or land in (tests for both orders). A save that lands confirms its value and supersedes the older ones; a fetch asked after every save replaces them. `NewChatPrivacyState::Saving(Saves)` and `PrivacyData::inactive_session_ttl: Optimistic<i32>` hold them. The timeout is not saved before the server's value is known, so a refusal never restores "unknown" (loading for good). |
| 05-084 | `PrivacyData::check_password_suggested` is gone: `Session::check_password_suggested()` reads the chat-list suggestion. A refused `HideCheckPasswordSuggestion` puts the suggestion back like `HideSuggestedAction`. |
| 06-001 | `ThreadStatus::takes_page_of`: while `LoadingHistory(r)` only request `r`'s page or error moves the thread; a `getMessageThread` error only fails a thread still `Resolving`; a newer-page error only touches a loaded thread. |
| 06-015 | `Session::thread_upsert` resets the window through `reset_thread_window` (which stales both ends' pages in flight) before `note_live` can replace it. |
| 06-002 | `begin_thread` marks the pages the replaced view asked for as stale in the new view. |
| 06-009 | A chat trimmed to its tail drops the finished (and failed) translations of the messages it no longer holds (`prune_message_translations`); a translation still out keeps its entry for its answer. A chat that is gone already dropped all of them. |
| 06-017 | `StatsState::posts` holds `PostSlot::{Ready, Missing, Failed}`. Only a not-found error is `Missing`; any other failure is `Failed` (drawn as Missing in `PostLookup`, a post that cannot be shown) and `StatsState::reset` (the panel opens) clears it so it is asked again. |
| 06-019 | `MessageStatsFetch::Loading(Option<RequestId>)` and `StoryStatsFetch::Loading(Option<RequestId>)` (`None` until the send went out): the answer or error is applied only when it is the request the fetch waits for (`waits_for`), so a retry that races an older answer cannot fill or fail the page. A reopened story whose statistics are still out waits for that request. |
| 06-020 | A failed first page of Stories or Gifts no longer writes a zero count; the count stays unknown and `visible_tabs` keeps a tab whose page failed, with its Retry. |
| 06-021 | `fetch_chat_media_counts` and the gallery's extra counts ask again on every open (a count still out is not asked twice). Counts shown stay until each new answer replaces its own; a failed count stays unknown. |
| 06-025 | A Stories page without the tab's stamp (the stories page's own request) never fills or fails the gallery tab: `accept_profile`/`fail_profile` take the stamp as a `u64`. Only the gallery's own request, which carries it, answers its tab. |
| 06-037 | `WebApps::found` holds the `RequestId` and `LaunchFlow::Searching { request, awaited }` takes only that request's answer. The attach-bot fetch and the add-to-menu toggle are matched the same way (`FetchingAttachBot { request }`, `Adding { request }`, `AttachBotAnswer`, `AttachmentToggle::request`), and so is the emoji-status grant (`EmojiAccessAnswer`). |
| 06-038 | `WebApps::pending: Vec` is `opening: Option<WebAppPending>`: one open call in flight, replaced by a newer launch. |
| 06-039 | The custom-method answer carries only its `RequestId` (`CustomRequestReply::request`); the `custom_requests` map is gone from the state. The window that sent it keeps `custom_calls: request -> req_id` and emits only its own answers; a closed or replaced window drops them. |
| 06-040 | `WebApps::data_sent: Vec<DataSent { request, result }>`. The flow that sent the data records its request in `MiniApps::data_sends` (per account, with the `Farewell`); only an outcome of a request it holds is reported; the return to the screen clears the outcomes that landed while away. |
| 13-057, 06-008 | `Translation::Failed { reason, attempts }`; the pump (`needs_translation`) asks a failed message again until `TRANSLATE_PUMP_ATTEMPTS` failures. The count rides `Pending { attempts }` across the retry. |
| 03-A008 | `forget_screen_outcomes` also drops the mini-app one-shot answers that landed while away. |

Tests: `state::saves` (both failure orders), `connect/tests/privacy_data.rs`
(newest-fails-first for the timeout and the new-chat privacy, a save before
the server's value is known), `connect/tests/account_hygiene.rs` (refetch
kept until sent, a refused refetch does not repeat), `state/account_notices`
(answers matched by request, a lost request, a count of 0),
`state/tests/bots.rs` (login press matched by request),
`connect/tests/web_apps.rs` (data sends and custom errors per request),
`state/tests/story_insights.rs`, `state/tests/shared_media_profile.rs`,
`connect/tests/stories.rs` (tray refresh on ingest),
`connect/tests/chat_list/folder_removals.rs` (leaving a folder stops the
retry; no wall clock), `state/tests/history_prune.rs` (translations pruned).

### Sweep fixes (r3-07)

Round 3 bundle for the audio engine, the account and appearance upkeep, the
call sounds, the checklist and community boxes.

| Entry | What changed |
| --- | --- |
| 12-001 | `ExportView` (the Shown/Hidden bit on `ChatExport` / `AccountExport`) is gone. `ExportBox` is `Closed` / `Progress { seen }` / `Options(draft)`, one value per account on screen: Hide, Cancel, close and Done go to `Closed`; opening over a running export or starting one goes to `Progress`. The dialog predicates read that value (and that an export exists); the poll treats a settled export as hidden when the box is not `Progress`. Switching accounts replaces the box, so it comes back closed and the export's end shows as a note. |
| 12-006 | `agent_driver_surface::window_closed` drops what the queue holds for a chat window when `chat_window_closed` runs, so a later window for the same chat never takes the entry. |
| 12-003 | The minute loop in `app_demo.rs` is gone. Scheduled auto-night holds one timer to the next boundary in `PreferencesUi::night_clock` (`NightClock::Waiting { _task }`, armed by every appearance pass, dropped when the schedule is off; `secs_until_night_change`). The background executor's clock stops while the machine sleeps and a long wait is an hour off across a DST change, so the appearance pass (which re-arms the clock) also runs from `cx.on_system_wake` and from window activation while the schedule is on. The OS light/dark setting is an observer (`observe_window_appearance`). `check-owned-timers.py` no longer skips `app_demo.rs` (`shape_guard.is_seeder`); the demo-only redraw loop is its own function, `QuillApp::spawn_stress_redraw`, and only that name is allowed. The wake is a platform signal with no fake, so it is not driven by a test. |
| 12-004 | The theme cache holds `CachedPalette::{Ready, Failed}`. A failed read stands for the pass that read it and is forgotten right after (`forget_failed`), so the next appearance pass reads the file again; the startup read does not cache a failure. |
| 12-005 | `AudioEngine`'s `track`, `building`, `backend` and `standing` are one `Playback::{Settled(Sound), Building { build, sound }, Simulated(SimRun)}` with `Sound::{Empty, Current, Stale, Failed}` holding the track. `fault` stays: it is the last-error log the row reads and is independent of the stage (a failed seek leaves the old sound playing). The build pieces moved to `engine_build.rs` (the file was at the size limit). |
| 13-001 | `finish_background_prepare` settles the close waiters after every end of a preparation, so a deletion waiting on it is not left to the 60 s cap. |
| 13-002 | `EditStage::Loading { asked }` holds the two requests of its load; `begin_bot_edit` returns them, `BotEditTexts::asked` carries them, and the fields fill only from that load. Baseline line `bare:EditStage::Loading` dropped. |
| 13-003 | `sync_call_sounds` without a session keeps only a ring of another account, and the call screen's `Farewell` stops the loop, so a lost session never leaves the ring and its output hold. |
| 13-004 | A device-access press dropped while another recording starts says "Finish the recording first" (the note `toggle_record_mode` already uses). Not driven by a test: the device-access answer (`capture_access::access`) is the platform's, and has no fake. |
| 14-002 | `EditMode::replace_as_file` is gone: `ComposerEdit::replacement_as_file` reads the choice from the staged replacement (a document of a photo or video file), so dropping the replacement drops the choice. |
| 14-001 | `ChecklistDialog::send` and `PollAddOption::send` are `DialogSend::{Editing, Sending { request }}`. Create / Add / Save keeps the box open with what was typed (the button shows loading); the request's answer (`session.acks`) closes it, a refusal returns it to editing with a note; closing the box or leaving the chat forgets the wait. `Session::settle_lost_request` (shared file `src/state/requests.rs`) now settles the request's ack as a failure, so a swept or abandoned request returns the box to editing instead of leaving the button spinning (`lost_requests.rs`). tdesktop closes the box when it sends; this keeps the typed text on a refusal, the smallest safe choice (the tdesktop source was not at hand). |
| 14-003 | `CommunityBox::{Create, CreateVisibility}` hold `CreateStep::{Choosing, Creating { request }}`; the busy state and the answers read that request. `CommunityOutcome::{Created, CreateFailed}` carry the request. `check-request-match.py` also counts `has_purpose_for_chat` / `take_purpose_for_chat` in `src/ui`; the three hits already there are in the baseline. |
| 14-005 | A waiting community ban whose answer lands while a box other than the one it was pressed from is open ends there (no warning pops up for a forgotten press); it is not sent. |

Tests: `ui/export_box_tests.rs` and `ui/account_export_box_tests.rs` (the box value says whether the progress shows), `state/tests/lost_requests.rs` (a swept request settles its ack), `ui/communities/flow_tests.rs` (bans settle by request, in both orders; a ban ends with its box), `ui/audio/engine.rs` (the existing transition tests, on the new
stage), `ui/communities/flow_tests.rs` (create waits for its own request;
ban dropped), `ui/background_accounts/closing_tests.rs` (a preparation that
ends settles its waiters), `ui/background_accounts/tests.rs` (ring ends with
the session), `ui/agent_driver_hard_tests.rs` (blocked entry ends with its
window), `ui/app_theme/ui_tests.rs` (failed theme read; night clock),
`settings/appearance.rs` (`secs_until_night_change`), `composer/edit.rs`
(as-file choice), `ui/dialogs/checklist.rs` (`DialogSend`),
`connect/tests/owned_bots.rs` (asked requests).

### Sweep fixes (r3-09)

| Id | Fix |
| --- | --- |
| 16-012 | `CraftStage::Forging` holds the `RequestId` of the `craftGift` it was started for (`start_forge(since, request)`). `craft_frame_upkeep` ends it when the session's `CraftRun` is no longer `Crafting { request }` for that id (answered, refused, or the craft gone), so a forge cannot end on another run's state. `after_craft_pop` reads only the forge stage; it no longer also asks `craft.crafting()`. |
| 16-009 | `GiftMarketUi::refused_files` (`RefusedFiles`) holds the sticker files whose `download_file` send the driver refused, for the files shown when they were refused. `gift_market_frame_upkeep` settles it against the shown set each upkeep (a different set forgets every refusal; the boxes closing resets it) and asks only files not refused, so a refused send is not re-sent every frame. A file that is downloading or downloaded is skipped by the session's own file state. |
| 16-010 | `SliderStage::Refused(level)`: a release whose send was refused serves nothing, so the stage slider is made again from the server's level (and the view redraws). |
| 16-011 | Skipped. `window_closed_by_user` is the user's decision about a call and is owned by the account's `GroupCallScreen`; `CallWindow` describes the one app-wide OS window (`GroupCall::window`). They are two owners: a dismissed call can have its window `Open` for the sync pass that removes it, and folding the call id into `CallWindow` would move per-account state into the app-wide window value (and lose the handle the sync needs). |
| 16-008 | `StreamFacts::transport_failed` is gone (baseline line dropped). `settle_stream_join`, which runs with the window sync outside the draw, reads the call's `transport.error()` from the session. |
| 17-001 | The Edit box holds `PackChanges { sticker, emoji }` (`PackAsk::Out(RequestId)`): the pack buttons are busy while one of this box's own requests is in the session's pending set, and the click handler refuses while busy. `settle_group_edit_box` returns an `Out` ask to `Idle` once its request is no longer pending. |
| 17-004 | `save_group_edit_box` closes through `close_group_edit_box` on all three exits (nothing changed, demo mode, empty send list), so the emoji panel and the boost ask go with it. |
| 17-002 | The HDR probe cache's `Entry::Probing { _job: Task<()> }` owns its probe: one foreground task awaits the background read, notifies, and replaces the entry with `Known` or `Unreadable`; dropping the entry cancels it. A path has at most one probe (no time-based re-probe, so a stalled drive piles up nothing), and a full cache keeps its running probes. The bare `Entry::Probing` baseline line stays removed because the variant now holds the task. |
| 17-003 | `InlineBotUi` holds the user's acknowledgement as a plain value (the two-variant enum is gone): it is one fact, not a stage. |
| 17-005 | The member list's `MembersFetchView` (renamed from `MembersStage`) is a view derived each frame from the session's `SupergroupMembersFetch`, which holds the request and is settled by its `RequestId`; the unused request field is gone. |
| 17-006 | `State::unopenable` holds `Unopenable { path, windows }` for a clip whose file did not open, so a tile does not run `VideoPlayer::open` every frame. It follows the slot lifecycle: `retire` (used by `leave` and every pass) drops it with the same window rules as a player, and a row that keeps rendering refreshes its pass; a new path is tried again. A failed open has no player, so it cannot be a `Slot` state; it is a sibling record with the slot's windows and lifetime. |
| 18-001 | `apply_custom_bindings` returns whether it rebuilt the keymap. `apply_pending_keybindings` records `Keymap::Applied` only then; the Shortcuts page marks the keymap `Stale` (`keymap_missed`) when its rebuild was skipped, so the next poll applies it. |
| 18-002 | `install_language` drops a re-picked pack from `LanguageBox::removed`, so Restore is not offered for a row that is in the list again. |

Tests: `language_box_tests` (a re-picked removed pack is not restored
twice), `group_call_stage_volume` (a refused send serves nothing),
`group_edit_box::extras` (pack buttons wait for their own request),
`hdr_image::probe_cache` (a running probe is waited for, never restarted), `gift_market_state::refused_files_tests`, `inline_players::windows` (an unopenable clip leaves with its row).

### Sweep fixes (r3-02)

Bundle `r3-02` of the third full read. Each stage below is one value that
holds its request, and a request the session no longer holds ends the stage
that waited for it.

| Ids | Fix |
|---|---|
| 05-001 | `GiftManageState::flow_sent(FlowForm, request)`: a collection form is marked sent by its own submit (`create`, `rename` or `delete_gift_collection`), not by matching the request's `PaymentsPurpose`, so a `ChangeGiftCollection` from the gifts list no longer takes over an open form. The Upgrade and resale forms of a gift panel are matched by the gift's id too (`GiftView::form_sent(gift, ..)`). |
| 05-002 | `GiftMarketState::events: Vec<MarketEvent>` (`Note`, `Toast`, `BalanceLow`) replaces `notes`, `toasts` and `balance_low`; the UI drains it once, in the order the events happened. The market's stage fields that held a bare `u64` for a request (`AuctionFetch`, `AuctionFollow`, `CraftRun`, `PageLoad`, `ResaleBuy::waits_for`) now hold the `RequestId`. |
| 05-004 | `BadgeStage::Failed { count, at }`: the same count that failed is pushed again after `FAILED_RETRY_AFTER`, a different count at once. A missing session bus no longer wakes the launcher worker on every pass. Review fix: the worker-end test retries relative to the real clock `WorkerEnd` stamps, not the process-wide `t0()`. |
| 05-005 | The picker's stages hold their `RequestId` (`VenueBot::Resolving { request }`, `AskStage::InFlight { request }`, `TileState::Requested`, `IconState::Requested`), nothing is marked asked before the send went out, and `LocationSharing::forget_dropped` (run first in `pump_location_picker`) fails what the session no longer holds: the bot lookup and the venue query (the list shows its Retry), a tile (one more failed try), an icon (asked again). The venue `generation` counter is gone: an answer is matched by its request. |
| 05-006 | `MediaViewer::origin: Origin` (`Closed`, `Chat`, `Shared { total }`, `Profile { owner, total }`, `Poll`) replaces `source` and `total`; the viewer is open exactly when it has an origin, and `retain` that empties the list closes it. |
| 05-003 | `NewChatStage::{Creating { request, back }, Created, CreateRefused, CreateLost}`: `Session::settled_new_chat` folds the answer in and the request is not held past it. `CreateProgress` is gone. |
| 06-002 | The QR box's `ThemeStage::Saving { request, choice }`, `FreeUi::Requesting { what, request }` and `GiftsAsk { request, at }` hold the `RequestId`; the counters (`save_serial`, `FreeState::last_serial`, `ThemeGifts::last_serial`) and the `SaveTicket` / `FreeTicket` are gone. The UI sends, then tells the box (`SaveSent` / `FreeSent`, or `SaveUnsent` / `FreeUnsent`); `PeerQrData::saves` is keyed by request; `qr_settle_dropped_requests` finds a waiting save or free code gone from the request table and settles it as a failure with no reason, and `PeerQrData::forget_dropped` lets go of the saves, the gifts page and the contact link; it returns whether it dropped anything, and `qr_settle_dropped_requests` reports progress only then, not on every poll while a request waits. |
| 06-001 | `ProxyFetch::{NotSent { at }, NotSentStale { at }}` replace `Failed` / `FailedStale` for a `getProxies` that could not be sent: `refresh_proxies_if_stale` (every ingest) sends it again once `UNSENT_RETRY_AFTER` has passed, so a background account's list settles. An error answer is still `Failed` (the box's Refresh). |
| 07-001 | A corrupt `accounts.json` that cannot be moved aside is marked unreadable and its default is cached under the file's stamp: a save refuses, and the file is not read, parsed and renamed again on every frame. Review fix: `load_registry_via` takes the cache slot and the write count, so the test owns both and no parallel test moves them. |
| 07-002 | `Saver::writer: Writer { Idle, Running { thread } }`: one background writer takes the staged geometry until the slot is empty (checked under the writer lock), so a drag stages and does not spawn a thread per report; a failed write ends the writer with the state still staged. Review fix: `start_writer` treats a `Running` thread that `is_finished()` (it panicked) as `Idle` and starts a new one, and a poisoned `writer` lock is recovered instead of ending the writer. |
| 07-006 | `ReviewItem::stage: ReviewItemStage { ToSend, InFlight(RequestId) }` replaces the `Option<RequestId>`. |
| 07-003 | `ChatSummary::my_member: OwnMember` holds the restriction (`Unknown`, `NoRecord`, `Other`, `Restricted(Option<MemberRestriction>)`, `Administrator`); `my_restriction` is gone. A status change (`updateSupergroup`, leaving, rejoining) replaces the restriction with it, and `record_own_member` lands the status, rights and restriction together. Review fix: a status known only from `updateSupergroup` is `AdminRightsUnread` for an administrator and `Restricted(None)` for a restricted member (`adopt_status_only`; any other status is `Other`), which `rights_fetched` counts as not fetched, so the own-member probe is still sent and an administrator gets rights from its answer; `note_no_member_record` turns `Unknown`, `AdminRightsUnread` or `Restricted(None)` into `NoRecord(status)`, so a refused probe settles it and keeps the status. |
| 07-004 | `HangUp::stage: HangUpStage { Discarding, EngineOwed, EngineTold }` replaces `engine_hang_up_due`; the driver pump settles `EngineOwed` with `take_engine_hang_up`. |

Guard: `check-stage-enums.py` gained the `rawid` rule: a stage variant (not a
`..Purpose`) with a `request`, `extra`, `request_id`, `generation` or `serial`
field typed `u64` is rejected; the hits in the tree are in the baseline.

Tests: `peer_qr/theme_tests` and `peer_qr/tests` (answers matched by request,
a save or code that never left, a dropped request), `connect/tests/location_picker`
and `location_picker/{picker,sharing}_tests` (dropped requests),
`connect/tests/gift_manage` (a list change is not the form's request),
`proxy_ops` and `connect/tests/sweep_s3_payments_1` (retry after the wait, on a
fixed clock), `icon_badge` (backoff), `settings/{accounts,window_save}`,
`state/tests/{calls,groups_more}`, `media_viewer`, `new_chat`.

### Sweep fixes (r3-06)

Bundle `r3-06` of the third full sweep (state: requests, search, sponsored,
translate, web apps, shared media, statistics, story insights, threads; envelope:
chats, giveaway, sessions). Every stage that waits for an answer holds the
`RequestId` of its request, written in the same call that registers it; the
answer is applied only when it is that request's.

| ids | fix |
| --- | --- |
| 09-008 | `PendingRequest`'s eighteen optional correlation fields (`user_id`, `story_ids`, `around_message_id`, `window_epoch`, `scope`, `folder_id`, `file_id`, `search_generation`, ...) are one `Correlation` enum (`state/request_correlation.rs`), set once by the `request_for_*` builders through `RequestRegistry::correlate`. A history page is `HistoryPage { window_epoch, from }`; a request that is about a user is `User(id)` and no other reader sees a field it did not set. Readers use the accessors (`pending.user_id()`), which answer `None` for another purpose's data. `Correlation::GalleryTab` marks the shared-media gallery's Stories page, which shares its purpose with the stories page. |
| 09-009 | `ChatSearchJump::Loading { request, message_id, then }`. `begin_jump` resets the window, registers the around page and writes the stage in one step and returns `ChatSearchJumpNeed::LoadAround { request, chat_id }` for the driver to send. `finish_history_around` and the error arm settle the jump only for the stage's own request, so jump A, jump B, jump A while the first page is out cannot be settled by the first page. `jump_serial` stays: it is the UI's highlight key for a repeated jump to the same row, never matched against an answer. |
| 09-010 | `ChatSponsoredMessages::views: HashMap<ad, AdView>` with `AdView::{Counting { request, attempts }, Counted, Failed { attempts }}` replaces `viewed`. `begin_sponsored_views` registers `viewMessages` and marks the ads `Counting` together; the `ok` counts them, an error answer (or a send that went nowhere) makes them `Failed` and the next frame asks again until `AD_VIEW_ATTEMPTS`. A refetched list keeps `Counted` and `Counting`, and resets `Failed`. |
| 09-012 | `Translation::Pending { request, attempts }`; `TranslateState::next_job` and the job numbers are gone (`TranslateJob` is a unit purpose, a selection translation is keyed by its `RequestId`). `finish_translation(request, ..)` finds the entry by the request; an older request of a replaced entry settles nothing. `fail_orphaned_translations` looks the request up in the registry instead of scanning by purpose. |
| 09-013 | `WebApps::write_access` and `phone_share` are `Vec`s like the other one-shot queues; the send no longer clears them, the poll drains `write_access`, and `WebApps::take_phone_share(waiting)` drains `phone_share` every poll: the window's `Sending` request takes its answer, any other (timed-out send, window closed) is dropped. |
| 09-014 | `SharedMediaTabStatus::Loading { request }` and `NextPage::Loading { request }`; the `generation` counters and `restamp_all_tabs` are gone (a cleared tab waits for nothing). `begin_fetch`, `begin_fetch_at_day`, `begin_fetch_more` and `begin_profile_*` take the request registered just before. The first-page and older-page answers are matched separately (`accepts_first`, `accepts_more`). `SharedMediaTabState::list_epoch` stays for the UI's scroll memory only (`PageKey`). The Stories tab's own request is `Correlation::GalleryTab`; a stale gallery page is dropped whole by asking the tab. |
| 09-015 | `GraphFetch::Loading { request }`; `graph_requests` is gone (`accept_graph` / `fail_graph` find the graph waiting for the request). |
| 09-016 | `fetch_message_statistics` adopts the request in flight for that post (`id_for_purpose_for_chat`) into `MessageStatsFetch::Loading { request }` instead of returning with the state left unwaiting; the baseline line for the purpose scan is gone. |
| 09-017 | `MessageStatsFetch`, `StoryStatsFetch` and `PageFetch` are `Idle` until the request exists and `Loading { request }` after; no `Loading(None)`. `story_search_page_requested` is gone. |
| 09-018 | `ThreadStatus::Resolving { request }`. `begin_thread_for` registers `getMessageThread` and returns it; `apply_message_thread_info` and `fail_thread` settle the thread only for that request. A replaced or closed view takes its resolve out of the registry. `SwitchingChat`, `Starting` and `ReloadNeeded` stay: they wait for a local action of the driver, not an answer, and leave by it, by `Failed` or by closing the thread. |
| 10-001 | The deep-link stages hold the `RequestId` (`ResolvingInfo { request, original }`, `ResolvingChat { action, request }`, `Info { .. request }`, `InvitePreview { .. request }`); the `ChatsPurpose::DeepLink*` purposes carry nothing and `ChatsState::deep_link_seq` is gone. Confirming an invite starts the join under its own request; the invite box (`InviteBox::Open { request }`) and the Premium bot's `/start` (`PremiumAutoStart::request`) are tied to the request too. |
| 10-002 | `GiveawaySummary::card: GiveawayCard::{Start, Results { info_message_id, unclaimed_count, was_refunded }}`; `finished` is gone. |
| 11-001 | `ParsedSession::kind: SessionKind::{Current, PasswordPending, Other}` replaces `is_current` + `is_password_pending` (the running device wins). |

Guards: `check-stage-enums.py` has a new `optional` rule (an in-flight
variant whose only request is an `Option<RequestId>`), `check-request-match.py`
a new `generation` rule (a purpose variant that carries `generation: u64`, on its own line or inline as `Variant { generation: u64 }`; the script self-tests both shapes on every run);
the existing hits are in their baselines and only shrink. The purpose-scan
baselines of `fetch_message_statistics` and `fail_orphaned_translations` and
the stage baseline of `Translation::Pending` are removed.

Tests: `state/tests/search.rs` (jump A, B, A), `connect/tests/sponsored.rs`
(failed view asked again a bounded number of times, ok counts for good),
`state/tests/history_prune.rs` (a replaced translation ignores the older
request), `connect/tests/web_apps.rs` (two write-access answers survive),
`state/tests/shared_media.rs` (duplicate older page), `connect/tests/stats.rs`
(back on a post whose statistics are out), `state/tests/threads.rs` (reopened
thread), `state/tests/deep_link_flow.rs`, `state/tests/premium_limit.rs`,
`connect/tests/deep_links.rs`, `envelope/giveaway.rs`, `envelope/sessions_tests.rs`.

### Sweep fixes (r3-10)

| Id | Fix |
| --- | --- |
| 18-003 | `Locating` holds a `oneshot::Sender<()>`; the Linux lookup selects on the receiver beside its timer and fix wait, so closing the picker (or switching account) ends the wait at once and the GeoClue client is stopped and deleted instead of staying started for up to 20 s. The Windows lookup checks the receiver between its calls and waits on `GetGeopositionAsync` beside it (`position_unless_dropped`, a std-only park loop over the operation's future and the receiver); the drop calls `Cancel` on the operation in flight. `IAsyncOperation` comes from `windows-future` 0.3 (added under the Windows dependencies; `windows` 0.62 does not re-export it). The file type-checks for `x86_64-pc-windows-msvc` against windows 0.62.2 and windows-future 0.3.2 in a scratch crate (a full `cargo check` stops at the native `lib.exe` build step on Linux). macOS keeps its own `cancel`. |
| 18-005 | `chat_preview_overlay` draws its skeleton from the `chat_preview_fetch` `Fetch` it already reads (`is_loading`), not from `has_purpose_for_chat`. |
| 18-006 | `send_message_report_text` clears the details field only when the send was accepted; a refused send keeps the typed text and shows the note. |
| 18-004 | `MessageUi`'s `Farewell` closes the Instant View reader (`messages.instant_view`) like its sibling session-held boxes, so it does not come back after switching accounts and back. |
| 18-007 | `ParagraphText::targets: Vec<ClickTarget { range, action }>` replaces the three parallel vectors; `ClickTarget::is_link` is derived from the action, so there is no flag to forget. The consumer builds the click ranges, actions and underline flags from it in one place. |
| 19-001 | Already fixed by 2ca5a1835: `show_scope_save` / `show_reaction_save` write the sent object into the snapshot, so a second toggle merges into the first (`state/tests/notification_saves.rs`, `two_quick_edits_do_not_revert_each_other`). |
| 19-003 | `NotifyDelivery::avatar_decodes: HashMap<path, AvatarDecode { _job: Task, waiting }>` replaces the `DECODING` thread-local and its detached task. Notifications queue behind the entry; the task takes the entry when the decode lands; dropping the entry (or `forget_account`) ends the wait. |
| 19-010 | `Shown::sending` is keyed by a per-call `Ticket`; `withdraw_sending(key)` marks every call of the chat, and each `land` consumes its own ticket, so two `Notify` calls for one chat both close when it was read. |
| 19-011 | The capability probe is `Probe { Idle, Asking { thread, last }, Answered { service, at } }` (an `Asking` whose thread has finished without answering, because it panicked, is due again), started by the question (the Notifications page or a notification) and asked again by a question after `REPROBE` while the daemon is missing; no thread loops on a clock. `check-owned-timers.py` now counts a `thread::sleep` loop in `src/ui` as a poll (two existing sites baselined). |
| 19-004 | `ConnectGeneration::poll: PollLoop { Stopped, Running { _task } }` owns the poll loop; `replace_client` (the one place the generation is bumped) drops it, so replacing the client ends the loop at once. The generation compare and the `detached:spawn_poll_loop` baseline line are gone. |
| 19-002 | Pay passes the agreed `(chat, price)` as an argument: `confirm_paid_send` calls `submit_composer_agreed(.., Some(agreed))` (`submit_composer` is the `None` wrapper) down to `paid_send_waits`, which lets a send at that price through. No stage variant or slot holds it, so nothing is left to clear (the review's one-shot `Agreed` is gone). Pay without the "Don't ask me again" tick no longer reopens the same box. |
| 19-007 | `CardInput::check` and `CardForm::country_check` are `FieldCheck { Fine, Rejected { judged } }`; the field shows the error only while it still holds the judged text, so the mark goes with the next edit without a reset. |
| 19-006 | `DeleteEdit::Snapshot { before, press }`: the press number (`CardInput::presses`) names the press that took the snapshot. The clear runs inside the second nested defer (a defer queued from a defer runs after the input's Change in the same effect cycle), so it lands before any queued key; it clears only if the field still holds that press, so a newer press or the Change that took the snapshot is left alone. No task or oneshot outlives the press. Test: `an_older_presses_clear_leaves_a_newer_snapshot`. |

Tests: `demos/code_highlight` (the farewell closes the reader),
`paid_messages_tests` (the agreed price covers only its own resubmit; Pay on a priced chat sends and asks again next time),
`payments/card_form_tests` (a rejected number is unmarked by its own edit),
`notifications/reply_after_switch_tests` (a decode is held and queues),
`notifications/linux_dbus` (two calls for one chat, probe schedule).

### Sweep fixes (r3-12)

| Ids | Fix |
|---|---|
| 21-001 | `ParsedSession::kind: SessionKind` replaces `is_current` and `is_password_pending` (merged with r3-06's 11-001: one enum `{ Current, PasswordPending, Other }`). It is parsed once (the current session wins when TDLib sets both); the Devices page, the session box and the terminate confirm match that one value. |
| 12-002 | `AgentUi::shown: AgentBox { None, Consent(AllChatsConsent), ResetToken }` replaces `AgentUi::consent` and `AgentPageUi::reset_confirm`. Access off, a close or the next question replaces it whole; the Esc/cancel closers end only their own box. |
| 21-002 | `QuillShell::open_dialog` is a `ShownDialog` cell the shell registers in `MainWindow::shown_dialogs` (by window id). Every closer (`on_close_kind`, `close_kit_dialog_if_done`, the account switch, the agent request's answer) moves that same cell, so the `dialogs_closed` mailbox is gone. `ChatWindows::dialogs_shown_by` is not the same fact (which window hosts a session-wide box, with its pending ask), and stays. |
| 22-001 | `TileState::Building { _build: Task<()> }`: the spoiler tile's build is owned by the stage and started from the one frame callback that finds it idle. |
| 22-002 | `StickerSetBoxUi::editing: SetEditing { Idle, Reordering, Picking, Waiting(PickWait), Creating { create, encoding } }` replaces `pick`, `pending`, `create`, `encoding` and `reordering`. `enter` ends the flow before it (a create box's files go, its encode stops); the closers end only their own flow. `PickWait::Decoding` holds its decode job and `Encoding::Running` holds its cancel flag (`CancelOnDrop`) and the continuation task, so leaving the stage stops them and the answer-by-token match is gone. "Edit Stickers" and the add cell are drawn disabled unless `editing.is_idle()`, so a press never ends a wait (opening the chooser would drop a waiting pick). The sticker video encode wraps its output in an `OwnedFile` inside the worker, so a step still running when the box closes removes its file. |
| 22-005 | `commit_sticker_manager` fails when nothing was queued, and when the change is at the front of the queue and its first send is refused: it is then taken back off the queue and the failure it noted is cleared, so `finish_manager` keeps the box's removals and order and shows its own toast. A change behind another waits its turn (a step lost later is reported once through `manager.failure`). Opening the box no longer clears the edits. Round 2: an earlier commit's failure the UI has not shown yet is set aside while the first send is tried and put back after a take-back, so the take-back clears only the failure this call set. The ⋯ menu's "Reorder" is disabled unless `editing.is_idle()` (as the footer's "Edit Stickers" is), so the press is not dropped while a pick waits. |
| 22-004 | `StoryViewerUi::menu: StoryMenu { Closed, Open { story } }` replaces `menu_open`. The pause reads `holds(current story)`, so a menu whose close the kit never reported cannot freeze another story, and another story's close does not close this menu. |
| 22-003 | `take_stealth_refusal` also leaves `Activating(request)` for `Failed(generic)` when the request is gone with no refusal (the button asks again). |
| 23-003 | `UsersState::userpic_full_info: HashMap<user, UserpicInfoAsk { photo_id, stage: Asked { request } \| Answered \| Failed { until } }>` replaces the `(user, photo)` set. It is recorded after the send went out, ended by the answer or error of its own request, retried after `USERPIC_INFO_RETRY` when refused (the next draw after the wait, no timer) or at once when its request vanished, and holds one entry per user. |
| 23-002 | `State::sweep_due` is gone: every `begin_render` defers its sweep, the first one takes every pass mark and the rest find nothing, so no flag can stay set when a borrow fails. `timer` is `Option<(due, Task<()>)>`: an armed sweep a whole `RELEASE_AFTER` past its due time counts as spent (one due now may be running), so a clear lost to a failed borrow cannot stop a new one being armed, `history_listed` is a per-frame observation set by the pass and read and reset by `frame_start`, not a stage. |
| 23-001 | `StillCut::Cutting { next, _cut: Task<()> }` and `EditorStage::Encoding { .., _job: Task<()> }`: the stage owns its worker's continuation, so leaving it (closing the editor, the screen replaced) cancels it; the token and the "serial" match for the cut are gone. `begin_encode` is `encode_serial` plus `start_encode(serial, info, job)` (the avatar encoder uses it too). Cut frames and encoded files are `OwnedFile`s from the worker on, so an answer nobody takes removes its file. |

Skipped as not real: none. Also: `a_failed_clip_is_tried_again_on_a_different_file` asserted a synchronous refusal that only AVPlayer gives; off macOS the FFmpeg backend opens on its decode thread, so that half runs on macOS only.

Guards: `check-stage-enums.py` now also reads `Building`, `Cutting`, `Encoding` and `Activating` as in-flight variant names (no new baseline lines; `PickWait::Decoding` and `Encoding::Running` left the baseline).

Tests: `state/tests/userpic_video.rs` (refused ask waits and retries, error answer, vanished request, one entry per user), `connect/tests/sticker_manager.rs` (commit fails only when nothing queued or its first send is refused, and goes out once the link is back), `sticker_set_box/state.rs` (leaving the create stage cancels the encode, entering a flow ends the one before, a press of Edit Stickers keeps a waiting pick), `chat_window/dialog_host_tests.rs` (an app-side close moves the shell's value), `story_viewer/item_tests.rs` (menu for another story), `story_stealth_ui_tests.rs` (lost request), `spoiler_fx_window_tests.rs` (build ends Ready), `profile_photo_sources/tests.rs` (closing the editor stops the encode).

### Sweep fixes (r3-04)

Files: messages state, payments state and error, settings apply and language, stickers apply, stories state, bot threads, direct messages topics, forum topics map.

| id | result |
| --- | --- |
| 08-011 | Kept, with the reason: `message_link` is a clipboard hand-off whose last answer wins like a second click (and is cleared when the account is away, `forget_screen_outcomes`); `instant_view` is set only in the view of the window that asked (`with_view`), and an answer needs its `InstantViewAsk` entry; `unread_jump` is set and taken inside one `ingest` call and carries the asking window (`ViewJump::for_request`), so it cannot outlive an answer. |
| 08-012 | Kept, with the reason: `draft_clears` and `failed_sends` are event hand-offs drained whole by `finish_successful_sends` on every poll pass, locked or not (it runs outside the `lock_waits` gate); the state that waits for an ack is `ComposerSends::draft_clears`, keyed by the composer slot. Nothing accumulates, and a clear is applied only when `DraftClear::matches` the stored draft. |
| 09-011 | `Messages::sponsored_hidden` is gone. `Session::sponsored_hidden()` is derived: a Premium account whose own cached `userFullInfo` has `has_sponsored_messages_enabled` false (no own full info: the server decides, ads are asked for). The `toggleHasSponsoredMessagesEnabled(false)` ok and `ReportSponsoredResult::AdsHidden` write `extras.sponsored_enabled = false` into that own full info. With none cached, no placeholder is stored (it would stop the real one being fetched, and the Business screens would read it as `Ready`): the own user is *wanted* in `UserFullInfos` (an idle entry), `sponsored_hidden()` holds while it is wanted, and `ConnectDriver::pump_wanted_full_infos` (poll loop) sends `getUserFullInfo(me)` once; its answer replaces the entry and carries the setting, a failed send or answer makes it `Failed`. An `updateUserFullInfo` that turns ads on, a `getUserFullInfo` answer, or an `updateUser` that ends Premium therefore show ads again with nothing to reset. Tests: `ads_turned_back_on_by_an_update_show_again`, `a_premium_lapse_shows_ads_again`, `hiding_ads_without_the_own_full_info_still_fetches_it`. |
| review: PsaLabels epoch | `LanguagePurpose::PsaLabels` is a unit variant and `PsaPlan`/`PsaLabels` lost their `epoch`: `PsaLabels::store` writes the badge texts only when some `PsaAsk::Asking { request }` waits for the answering request, and a new pack clears the waiters, so an older pack's answer finds none. The `counter:LanguagePurpose` baseline line is removed, and the guard's `COUNTER` pattern now also matches `Option<u32>` (the old `SetOption { generation: Option<u32> }` shape). The unit tests own their request counter (`ask` takes `next: &mut u64`), no process-wide static. |
| 08-015 | `GetGiftTextLimit` failing no longer clears a limit an `updateOption` set meanwhile. `GetStarPackOptions` and `GetWearableGifts` are sent with `Fetch::sent` and settled with `answered`/`failed` by their `RequestId` (an older answer or error lands nowhere). A failed `ToggleGiftSaved`, or another gift's sale, leaves the open sale confirmation alone; `GiftView::abandon_confirmation(gift)` closes only the confirmation of the gift the failed `SellGift` named (`subject_id`). |
| 08-013 | Kept, with the reason: `PaymentsState::paying` holds one entry per `sendPaymentForm` with its own `RequestId`, invoice and form, and it outlives the checkout by design (tdesktop keeps one checkout process per invoice, so a closed checkout still pays and unlocks). A payment is never swept as lost (`is_sweepable` excludes mutations): a timer that freed Pay could charge twice. It leaves on the answer or error of its request. |
| 08-019 | `StorageClear::Clearing { request }`: the `optimizeStorage` answer sets `Freed` and drops the statistics only for the clear that waits for it (`answered`), and its error fails only that clear (`failed_by`). The baseline line `bare:StorageClear::Clearing` is gone. |
| 08-018 | `DeviceLogin::Waiting { request }` is set at send; `Done` comes only from `Waiting` with the same id (`settle`), and `discard` (the account went away) leaves `Idle`, so a late answer lands nowhere. The sessions page lock (`sessions_op`) still ends on every answer. A send that fails discards the stage. |
| 08-016 | `LanguageSwitch` holds `pack: Fetch<Strings>` and `base: Fetch<Strings>`, each under the `RequestId` that asked, and `SwitchGuards` (the `language_pack_id` option and the synchronize requests, which refuse the switch). The generation counter, `SwitchStrings`, and the `generation`/`base` fields of `LanguagePurpose` are gone; an answer or refusal of a request an older switch sent changes nothing. `getLocalizationTargetInfo` is sent with `sent` and settled with `answered`/`failed`. `stage-enums-baseline` line `unkeyed:LanguageStage::Fetching` and `request-match-baseline` line `unmatched:Session::apply_language_payload` removed. |
| 08-017 | The sticker lists (installed, archived, recent, favorites, trending, search sets, search stickers) write their data only when `lists.for_purpose(..).answered(Some(pending.id))` says the answer is the request the list waits for. The direct `getRecentStickers` in the media library goes through `sticker_request` so it is tracked. The mask lists (`installed`, `archived`, each mask set) are sent with `sent` and settled by id. Four `unmatched:` baseline lines and one `purpose:` line removed. | The group sticker-pack picker (`load_group_sticker_choices`) records the request it sent on both lists (`begin` then `sent`), so its answers are accepted and the picker leaves "Loading packs" (test in `driver_group_sticker_set_gates_and_shape`). |
| 08-020 | `ActiveStoriesPaging::Loading { request, wait }`: the page in flight is in the stage, not found by `has_purpose`. Only its answer or error leaves it; a reset (`reset_active_stories_paging`) drops it so a late answer lands nowhere. A lost page settles as a failure the list waits out (`LoadActiveStories` and `LoadHiddenStories` are now sweepable; `requests.rs` is outside the bundle, one list entry). |
| 08-023 | `AnswerBox<T>` replaces the two eight-entry queues: answers are kept per asking view (`AskingView`), at most four per window, dropped with a closed window, so another window's answers never push out the live one. |
| 08-021 | `TopicsLoad::Loading { request }` / `Loaded` replace `Asked`: only the page in flight answers (`page_answered`, on `ok`) or fails (`page_failed`); an older page's 404 no longer ends the list, and a lost page settles as a failure (retried on open). The sender asks from the stage (`loading`), not by purpose. |
| 08-022 | `ForumTopicsMap::answered` and `failed_request` settle only `Loading { request: Some(id) }` with the same id: a loaded list is never failed by an older error, and an answer after the list was forgotten lands nowhere. `Session::ask_forum_topics` registers an ask as the driver does, for fixtures and demos. |

Tests: `state/tests/sweep_r3_04.rs` (each fix above that has a drivable exit), `state/tests/premium_hub.rs`, `tests/data_storage.rs`, `connect/tests/settings.rs` (device link waiting), the sticker, mask, story-paging and forum-topics tests that now register their asks.

Guard, at the round 3 merge: r3-04's `counter` rule and r3-06's
`generation` rule in `check-request-match.py` flagged the same purpose
fields, so they are one rule, `counter`, keyed by enum and variant
(`counter:BotsPurpose::CreateBot`); it keeps r3-04's reach (`generation`,
`epoch` or `token`, any `uN` or `Option<uN>`, any `*Purpose` enum) and the
self-tests of both.

### Sweep fixes (r3-11)

| Id | Fix |
| --- | --- |
| 19-005 | The Share render's worker returns the card file as a `scratch::TempFile`, a guard that removes the file when it is dropped unclaimed. A render whose task was dropped with the box therefore leaves no PNG; `done` claims it (`claim`) when the box still wants it, and removes it off the UI thread when it does not. |
| 19-009 | The profile crop's worker returns its PNG as a `TempFile` too. Every early return of the callback (the target closed, the render failed, the screen went away) drops it, which removes the file; a `take_chosen_photo` refusal claims it and removes it in the background. |
| 19-008 | The edit renders are owned by what waits for them. `task_hold::TaskHold` (an opaque, shared handle the lib can hold; the UI hands it its task) sits in `PhotoPrep::Rendering { render, hold, before }` and `EditRender::Rendering { number, hold }`: a newer Done, the card going or the tray going drops the older render's task, which cancels it. The worker returns a `TempFile`, so a render cancelled after it wrote its PNG removes it. The number stays as the key the landing is matched by (`waits_for`, `photo_render`). |
| 20-002 | `load_created_public_chats` records a refused ask (chats path inactive) as `PublicChatsFetch::Failed`, as the failed send already did, so the list draws its Retry row and the render asks once, not on every frame. |
| 20-001 | The packs section is part of the open view: `StarsView::{Closed, List { packs }, Detail { id, packs }}` with `PacksView::{Folded, Expanded}`. `StarPacksState::open` is gone; every way of closing the Stars box (`close`, an account switch, a Farewell) replaces the view, so the section folds with it. `toggle_packs` / `expand_packs` / `packs_expanded` are the only writers and readers; the story viewer's "needs more Stars" path expands through `expand_packs`. |
| 20-003 | `PrivacyKeyState::editable_detail()` returns the rule only for `Ready`, `Saving` and `SaveFailed`; `mutate_privacy_detail` uses it, so an edit made while the rule is `Loading` or `Failed` is refused instead of being composed on a stale copy and overwriting another device's newer exceptions. The picker stays open on a refused edit. |
| 20-004 | `GiftSave::{Idle, Saving(Saves<GiftSettings>)}` (in `PrivacyData::gift_save`) holds what the server is known to have and the saves in flight, each under its `RequestId`. The switch still shows the newest value at once; a refusal restores the next save still out, or the confirmed value, whichever order two saves fail in (`apply_privacy_data_error`), and an `ok` confirms. |
| 20-005 | Already handled: `check_remembered_password` puts the card in `PasswordCheck::Failed` on an inactive path and on a refused send, and the card draws "Couldn't check the password". Now covered by a test. |
| 20-006 | `fetch_suitable_personal_chats` records a refused ask (chats path inactive) as `ProfileChatsFetch::Failed`, so the picker draws its Retry row instead of Loading until the box is reopened. |
| 20-007 | `PullUi` no longer keeps `Route` beside the machine. The machine's pull stage holds `found: Option<Found { next, via }>` (where the pull leads and the window of the gesture that found it) and `Stage::Jumping { pull, until, job: TaskHold }` holds the jump's task, so has-next, the hint and the jump cannot disagree and every way a pull ends (`reset_all`, the hold running out) takes all three along. `PullMachine<W>` is generic over the gesture's handle. |
| 20-008 | `QuickRepliesDialog::watch` is gone. The poll is a `Poll` (resting on `quill::wake::APPLIED`, poked at the end of every `poll_live` pass after the drain (`UPDATES` is poked by the receive thread before the drain, so a pump woken by it found nothing and slept the full fallback), with a 2 s fallback via `sleep_or_wake`, not a bare timer; no allow-list line) inside `PaneStage::{Saving, Accepted, Deleting}`; `Saving` hands it on to `Accepted`, and leaving those stages (settled, another screen, the box closing) ends it, so the end-of-poll "a save went out as it ended" restart is not needed. While locked a request still out keeps being settled; a wait for an id is let go. |
| 20-009 | `Chooser::Listing { target, epoch, _list: Task<()> }` owns the listing task and `OpenChooser::_thumbnails: Task<()>` the thumbnail task; the generation counters (`ScreenShareUi::generation`, `OpenChooser::generation`, the `Listing` copy) are gone. A closed or replaced chooser drops both tasks, so no old answer or picture can land in a later chooser. `epoch` stays: it names the screen a listing started on. |
| 21-003 | The countdown badges have no task: `ensure_self_destruct_tick` (from render) asks the frame clock for a 1 fps tick, aimed at the conversation slice (`conversation_view_id`, not the whole app), while the open chat has a live timer and Quill is unlocked. The clock is app-gated, so a hidden, minimized or inactive window and a locked Quill get no tick; the next render asks again. `ChatUi::self_destruct_tick` and its owned-timers allow/baseline lines are gone. Tests: locked asks for no tick, a hidden window asks for none and the tick never targets the whole app; the quick-replies poll ignores an `UPDATES` poke and reads the answer on the `APPLIED` poke. Not checked against tdesktop: the badge text is the same either way. |

New guard rules: `check-stage-enums.py` now treats `Rendering`, `Listing`, `Jumping`, `Encoding` and `Capturing` as in-flight variants and accepts a `TaskHold` as what ends one (two existing hits, `LookStage::Rendering` and `EditorStage::Encoding`, are baselined). `Step::started` left the implicit-state baseline (it is allowed with its reason). `check-owned-timers.py`: `quick_replies_watch` left the baseline and `quick_replies_poll` is allowed, owned by the stage.

Tests: `connect/tests/group_type.rs`, `connect/tests/profile_panels.rs`, `connect/tests/privacy_data.rs` (refused asks are Failed; gift settings roll back in both failure orders; the password card), `privacy.rs` (stale rules are not editable), `premium_hub/tests.rs` (packs fold with the box), `pull_next/tests.rs` (found and jump end with the stage), `task_hold.rs`, `peer_qr/export.rs` (`TempFile`), `ui/quick_replies/tests.rs` (the poll lives in its stage), `ui/passcode/lock_gate_tests.rs` (no self-destruct tick while locked), `ui/screen_chooser_tests.rs`.

At the round 3 merge, `unkeyed:EditorStage::Encoding` left the stage
baseline (r3-12's 23-001 gave the stage its `_job: Task<()>`), and so did
`rawid:Chooser::Listing` (20-009 dropped its generation).

### Sweep fixes (r3-01)

| ids | fix |
| --- | --- |
| 01-001 | `StyleList::recent` holds `Recent::Unlisted(name)` / `Recent::Listed(name)`. A name becomes `Listed` when a server list names it; `set_server` prunes only a `Listed` name a new list lacks (or lists as built-in). A created, edited or added style (`promote_front`, `promote_name`) stays `Unlisted`, so it keeps its front place across lists that do not carry it yet, however it was promoted. |
| 02-001, 02-002 | `quill::composer::RenderJob { token, task }` (an opaque, droppable owner of the render's task) replaces the `NEXT_RENDER` counter. `EditRender::Rendering { job }` and `PhotoPrep::Rendering { job, before }` hold it, so a cancelled edit, a removed photo or a later Done drops the job and cancels the render. The identity others keep (the held Send's `waiting`, the deferred editor open) is the `RenderToken`, matched at landing (`EditRender::waits_for`, `end_render`, `take_deferred_for`). The edit bar follows `ComposerEdit::for_display` (the edit without its render job), so the stage is the only holder and a cancelled edit cancels the render at once, not when the bar's exit slide ends. Dropping the job marks its token cancelled (`RenderToken::is_cancelled`): a render already running off-thread checks it before and after writing and deletes its PNG (`a_photo_gone_from_the_tray_leaves_no_png`). |
| 03-001 | `OwnerClaim::Unsaved { user, retry_at }`: a failed owner-file write is retried by the first ingest at or after `retry_at` (`OWNER_RETRY`, 30 s), not on every update; `claim_account_owner` takes `now`, so the test drives it without a clock. |
| 03-002 | The Share Folder `links` list records its request (`sent`) and takes its answer and error by it (`answered`/`failed`), like `chats`. A link created, edited or deleted while the list loads outdates it (`ChatListState::outdate_loading_links`), so the in-flight answer, which predates the edit, is dropped; the list is fetched again once (`fetch_folder_share` dedupes on the list's own stage, not on the old request still in `requests`): every refetch goes out from the post-ingest hook `refetch_open_folder_links`, and `refetch_outdated_folder_links` holds it back while a `deleteChatFolderInviteLink` for the folder is in flight (MTProto does not order parallel queries, so a list read before the delete lands could bring the link back); a refused delete ends the list in the error with a Retry. |
| 03-003 | The welcome pack and the group picker's installed sticker and emoji lists record their request after `begin()`; the pack's error and the emoji answer are matched by it. The sticker list's data is taken only when the load that waits for it answers (`installed_wanted` in `apply_stickers_payload`; every sender already goes through `sticker_request` or `begin()`+`sent()`, the demo seed now records its request too), and a sticker-set change outdates the list so the load it dropped is asked again whether or not the panel is open. `accept_installed_sticker_sets` left the baseline. |
| new-01 | `PaidReactions::senders_asked(chat, request)` records the request; `set_senders` and `senders_failed` take the answer and error by it. Both lines left the request-match baseline. |
| 03-004 | `StyleReply::Running { request }`; the style lookup and example answers and errors are taken only from the request the stage holds, and `SearchStyle`/`GetExample` are sweepable (`is_style_preview_lookup`), so a lost request fails the stage. The UI token stays: it names which preview asked, not which request answers. |
| 03-005, 03-006 | The Star packs list, the sign-in app config and the wearable gifts list record their request and take answer and error by it. |
| 04-001 | `send_web_app_custom_request` records the `custom_requests` entry after the send went out; a refused send leaves none. |
| 04-002 | A refused `getMessagePublicForwards` send ends the list in `Failed` (`forwards_send_failed`); the page draws Retry when the first page failed (nothing to page from). |
| 04-003 | A release that times out in the section path is a leftover (`Err(Tdlib)` is ignored), as in the chat path; only `Cancelled` and `Io` end the export. |
| 04-004 | Skipped. `ExportView` is the user's show/hide choice and the worker's progress is the export's stage: they are written by different parties, every combination is valid, and nothing resets one when the other moves (the poll reads both once). It is not a stage kept in step by hand. |

Guard: `check-request-match.py` has a third rule, `unsent`: a `Fetch::begin()` with no `.sent(..)` after it in the same function. The 21 hits in the tree are in its baseline; the senders of this bundle are off it.

Tests: `composer::render_job`, `connect/tests/sweep_r3_01.rs` (late links answer, requests held, style lookup by request and lost, refused forwards, refused custom request), `connect/tests/sweep_r2_03.rs` (owner retry deadline), `ai_compose::style`, `export/account/tests.rs` (a release that never answers), `ui/composer_sends_tests.rs` (the tray photo owns its job).

At the round 3 merge, r3-11's 19-008 and r3-01's 02-001/02-002 owned the
same renders. The stages hold r3-01's `RenderJob` (its token replaces the
render number, and the edit bar keeps `for_display`), and the job keeps its
task in r3-11's `TaskHold`; the worker still returns r3-11's `TempFile`, so a
PNG whose landing task was dropped after it wrote is removed as well.
`check-stage-enums.py` accepts a `RenderJob` as what ends a `Rendering`
variant. r3-04's 08-017 already gated every sticker list on
`answered(Some(pending.id))`, so 03-003's `installed_wanted` is that same
`current` check and was folded into it; the wearable gifts and Star pack
options fixes (08-015 and 03-005/03-006) were the same change.

### Sweep fixes (r3-03)

| Id | Fix |
| --- | --- |
| 08-001, 08-003 | One `MemberList { query, fetch, more }` per (chat, filter) replaces `supergroup_members` + `member_asks.first/more`. The first-page request lives in `SupergroupMembersFetch::Loading { request }` (not an `Option`), the next page in `MoreMembers::Asking { request }`, and a new first page replaces the whole value. A first page answers or fails only a list that is `Loading` as that very request (`answer_member_first` / `fail_member_first`), so a search refused at send (`NotSent` under the new query, no request) is never filled by an older search's answer. A send that is refused puts back the list as it was (`restore_member_list`), except a stale one, which becomes `NotSent`. |
| 08-002 | `members_added` and the one-shot `member_list_stale` queue are gone. `GroupsState::members_changed(chat)` makes every member list `SupergroupMembersFetch::Stale { last: Option<MembersPage> }` (a page stays drawn; a first load still out has no page yet; no request is out, so no answer can land on it; `Failed`/`NotSent` keep their Retry; nothing is deleted, so the poll and the readers always have a list to ask again). The poll refreshes exactly the key the dialog shows (`MemberListFilter::shown_for`: without the restrict right, Restricted and Banned show Recent) with the query it was asked with (`refresh_shown_member_dialog` uses `dialog.searched`; only the Search button and tab switches read the text box, so unsent text never makes the poll search) and an asked administrators list `AdminListFetch::Stale`. Whoever shows the list asks: the fetch functions no longer treat an older request of the same purpose as the list being asked, the poll refetches only the dialog's and the panel's stale list and the stale administrators, and a stale list that cannot be asked goes (`drop_stale_*`) or becomes `NotSent` so the poll does not repeat it. |
| 08-004 | `DeepLinkState::ResolvingChat { action, request }` and `ResolvingInfo { request, original }` hold the request they wait for; `DeepLinkState::waits_for` / `resolving_action` compare it with `pending.id` (join, check, resolve, lookups, the sticker/user/chat answers and the error arm). The purposes lost their `generation`. The invite box is identified by a generation minted when the preview appears, and the Premium `/start` is tied to the follow-up's request (`PremiumAutoStart::request`). |
| 08-005 | A missing `admin_lists` / `invite_links` entry no longer counts as waiting: an answer or error lands only on a list that is `Loading` as that request (every sender writes `Loading` first). |
| 08-006 | `ToggleBroadcastGroup` rolls back through `RequestRollback::BroadcastGroup { previous }`, restoring the known flag instead of removing the entry. |
| 08-007, 08-009 | `AiComposeState::style_edits` is a `Replies<_>` filed by request (a few kept); the editor takes the answer of its own request, so two answers in one drain overwrite nothing. Style notices are a bounded queue (`take_style_notices`), and the op failures a bounded list. |
| 08-008 | The AI box, the apply-in-place shortcut, the style lookup and the example hold the `RequestId`: `CardState::Loading { request }` (set by `sent` once the driver returned the id, failed by `send_failed`), `ApplyInPlace::request`, `PreviewStage::LookingUp { request }`, `ExampleFetch::Asked { request }`, `StyleReply::Running { request }`. Answers are filed in `Replies` under the request (`box_answers`, `apply_answers`); a box that is gone drops them. The `u64` tokens and `fresh_token` are gone. |
| 08-010 | `PrivateSharingState::enable` is one `EnableSharing { Idle, Asking { chat, request }, TurnOff, AskToRequest }`: set when the lookup went out, moved only by the answer or error of that request (`answered`, `lookup_failed`), left by the box closing; a second tap while it is out does nothing (no purpose check). |
| 08-030 | The chat-wide list and a link's list are different scopes (search and paging vs. one link's page); both are edited only by `forget_join_request`, which now calls one `drop_request` helper for the row and the total. |
| 07-005 | `ChatsState::background_error` is cleared by a wallpaper change that succeeds, a retry of the list, a preview opening and the gallery being left. |

Guard: `check-request-match.py` has a third rule, `ticket`: a purpose
variant carrying a `u64` `token`/`generation`/`seq` stands in for the
`RequestId` (the existing ones are baselined). It scans the body of every
`enum *Purpose` under `src/`, not only files named `purpose` (the location
picker's `SearchVenues { generation }` is now baselined).

Tests: `state/tests/sweep_r3_03.rs`, `connect/tests/sweep_r3_03.rs`
(stale lists asked again while an older request is out, the poll not searching for unsent text, a refused search
not filled by an older answer, the sharing lookup, the wallpaper line),
`connect/tests/ai_compose.rs` (answers per request, older lookup ignored,
notices kept together and bounded), `state/tests/premium_limit.rs`
(join errors of an older request dropped), `state/tests/groups.rs` (the
broadcast flag comes back as it was).

At the round 3 merge, 08-004 and r3-06's 10-001 were the same deep-link
fix. r3-06's shape stays: every stage that answers holds its request
(`InvitePreview { request }` and `Info { request }` too, read by
`DeepLinkState::request`), so the invite box is tied to the check that
answered instead of a minted generation, and `PremiumAutoStart::request` is
a plain `RequestId` (0, never allocated, in demos). r3-03's `waits_for`,
`resolving_action`, `next_deep_link_generation` and `deep_link_request`
are not kept; its test of an older Join's failure sits beside r3-06's in
`state/tests/premium_limit.rs`. For 08-008, r3-03's `StyleReply::{Running,
Answered}` (both carrying the request, settled by `settle`) replaces
r3-01's, and r3-01's `is_style_preview_lookup` still makes a lost lookup or
example sweepable, which settles it through the same error arm. The
inline members match draws a `Stale` list like a refetch: its last page
stays up, an empty one reads Loading, and the poll asks again. The `ticket`
rule is folded into `counter` (its `ticket` and `seq` names joined
`counter`'s; every `ticket:` hit was already a `counter:` key or gone).

### Sweep fixes (r3-05)

| Id | Fix |
| --- | --- |
| 08-027 | `SaveWatch` is a map keyed by `RequestId` (`entries`), not one slot: the profile box in a chat window, the main window's box and the proxy editor each watch their own request, so a second Save no longer makes the first box's outcome `Unknown`. Each box still ends its watch when it closes or is replaced (`forget`). |
| 08-028 | `HistoryState::unread_open: UnreadOpen { Closed, Bar(anchor), Opened { bar } }` replaces `unread_anchor` + `opened_with_unread`. `unread_anchor()` and `opened_with_unread()` read it; deleting the bar's message (`drop_bar`) keeps reads held in `Opened`, jumping to the latest or sending (`end_unread_open`) closes both, and the jump-down button (`show_bar`) brings the bar back without holding reads. |
| 08-029 | `EventLog::wait` and `EventLogWait` are gone. `ChatEventLogFetch::Loading { request }` holds the first page's request; the loaded page holds the older page in `OlderEvents::Loading { request }` and the poll in `LogPoll::Polling { request }`. A page or poll lands only when the stage still holds its id, so a refresh (`reset_event_log_fetch`) drops everything in flight with the stage. A failed older page keeps the list with Retry; a failed poll changes nothing shown. |
| 08-024 | `MessageReactionBook` (`StickersState::message_reaction_options`) keeps one `MessageReactionsState` per message, so the main window's menu and a chat window's menu no longer replace each other's strip. `Loading { request }` is settled only by that request's answer or error; an older ask for the same message (a lost reply asked again) finds another id. A lost request is asked again (`fetch_message_reactions` skips only a request still in flight). Every exit of a menu forgets its message's entry through `QuillApp::close_message_menu` (`reset_message_reactions(chat, message)`): the close, a menu opened in another window (`close_message_menus_of_other_windows`) and the window closing (`forget_chat_window`). The book has no size cap, so a missing exit shows as a leftover entry in the test. |
| 08-026 | `RequestRollback::ChatPinOrder` and `ArchiveChatListSettings` carry `shown` like the other rollbacks: a refusal puts back a chat's order, or the archive settings, only while the cached value is still the one the request showed. |
| 08-025 | The fifteen correlation `Option`s of `PendingRequest` (file, message, topic, user, supergroup, community, story, story ids, user ids, album, folder, scope, secret chat, invite link, subject) are one value, set by exactly one `Session::request_for_*` constructor and read through typed accessors (`pending.user_id()`...). A request cannot carry two or half-stamp one, and `check-request-match.py` (rule `stamped`) fails any `Correlation::` or `.correlate(` outside the constructors (counted once per line). r3-05 named the enum `RequestKey`; at the merge it became r3-06's `Correlation` (`state/request_correlation.rs`), which also holds the history page's window epoch and `from` (`HistoryPage`), the search generation of the in-chat search and the shared-media tabs (`Search`) and the gallery's Stories page (`GalleryTab`, now asked through `request_for_gallery_tab`). `chat_id` and the view stamp stay fields. |
| 09-001 | `SavedTopicsLoad::Loading { request }` (and `Loaded` between pages) replaces `Started`. `page_sent` / `page_answered` / `page_failed` take the request: the `ok` settles in `apply_ok`, the 404 and the error in the threads error arm, each only for the request held. The dedupe and the pane's spinner read the stage, not `has_purpose`. |
| 09-002 | `SearchPaging::Loading { request }`; `begin_more` only reads the offset and the sender records the page with `page_sent` once it went out. A page's answer or error acts only when the stage holds its id (`SearchState::awaits`), so a page of a list that started over is dropped. |
| 09-003 | `Leg::Pending { request }`; `Session::request_search(purpose)` registers the request and records it in the leg it belongs to (`leg_sent`). `matches_generation` is gone from the global search: answers of the legs, the paging, the server supplement (`server_chats: Fetch`) and the quota (`tabs.limits: Fetch`) are matched by id. `SearchState::generation` is now `epoch`, only the UI's scroll reset. A typed query waiting out its quiet window is `typed.armed()`; the panel reads `Searching` from it, so no stage of a leg waits for the debounce (and a cancelled debounce leaves none behind). `begin_query`, `begin_recents` and `begin_browse` return nothing. |
| 09-004 | `GroupsState::supergroups: HashMap<id, SupergroupFacts>` holds the seven rights and four flags (`join_by_request`, `is_broadcast_group`, `sign_messages`, `show_message_sender`) of an `updateSupergroup` / `getSupergroup`, replaced whole by one `SupergroupFacts::from_update` in both appliers; `anti_spam: HashMap<id, SupergroupAntiSpam>` is the `supergroupFullInfo` pair, replaced whole in both appliers. The thirteen per-flag maps are gone. Quill never removes a chat summary before the account goes, so there is no earlier owner exit; the next update resets every right at once (test: a demotion). |
| 09-005 | `Audience<T>::Loading { request }` and `Paging { page, request }`. The viewers, read date, author and each reactions tab take an answer or an error only from the request they hold (`fill`, `accept_added_reactions`, `fail_audience(&PendingRequest)`), and a failed next page returns to `Ready`. |
| 09-006 | `SetChangeStage::Working { change, request }`; `finish_set_change(request, set_id, success)` settles only that request, so the first answer no longer ends the second change of the same kind. |
| 09-007 | `accept_replied_message` and `reject_replied_message` take the request and act only on `ReplyTarget::Loading { request }` of that id: a late error of an older ask neither bumps the attempts nor makes the row Missing, a late answer neither fills it, and a row pruned meanwhile is not brought back. |

Second round (08-024): the reaction selector is the other exit that keeps a
message's options. `set_media_panel_mode` forgets them (`forget_message_reactions`,
shared with `close_message_menu`) when it leaves `PanelMode::Reaction(target)` for
anything but the same target, so a pick, a close and a mode change all end the
entry; `forget_chat_window` closes a selector open in that window. Test:
`window_menu_tests.rs` (open the selector from a menu, close it, the book holds
no entry).

Third round (08-024): `react_from_panel` reacts first and closes the selector
after, because `toggle_reaction` reads the message's `premium_locked` options for the
Premium gate and closing forgets them. A free account picking a locked reaction in the
selector opens the Premium preview and sends nothing. Test:
`reaction_rows_tests.rs` (`a_locked_reaction_in_the_selector_opens_the_premium_preview`,
which also finds no book entry afterwards).

Merged after r3-06 and r3-04: `RequestRollback::BroadcastGroup` (r3-06, 08-006)
reads and restores `SupergroupFacts::is_broadcast_group` (09-004) instead of the
removed `supergroup_is_broadcast` map, so the error arm for
`ToggleBroadcastGroup` stays gone. The around-page jump keeps r3-06's
`ChatSearchJump::Loading { request }` matching; the global search drops
`matches_generation` (09-003) while the in-chat search keeps its baselined calls.

Shared file touched: `src/state/session_apply_error.rs` reads `file_id()` where it read the
field (08-025); nothing else of it changed.

Guards: `check-request-match.py` gains `generation` (a call to
`matches_generation`: an answer taken because it carries the query's counter,
not the `RequestId` its stage holds; the in-chat search's four calls are
baselined) and `stamped` (a `Correlation::` or `.correlate(` outside the request
constructors).
`check-stage-enums.py`'s baseline lost the six stages that now hold a request
(`ChatEventLogFetch::Loading`, `MessageReactionsState::Loading`,
`SetChangeStage::Working`, `SavedTopicsLoad::Started`, `SearchPaging::Loading`,
`Leg::Pending`).

Tests: `state/tests/event_log_poll.rs` (a refresh drops the page and poll
asked before it; a poll and an older page settle on their own request),
`connect/tests/media_library.rs` (two menus on two messages; an older request
for the same message cannot settle the newer ask), `state/tests/rollback_shown.rs`
(a newer position update and a newer fetch survive a refused rollback),
`state/tests/forum_saved.rs` (an older page cannot settle the page now out),
`state/tests/search_upgrades.rs` (a leg settles only on its request; a page of
a closed list is dropped), `state/tests/bubble_headers.rs` (an older reply
ask cannot settle the newer one), `state/tests/messages_more.rs` (the reactions
tab pages with the older ask's failure ignored), `connect/tests/set_box_change.rs`
(another request naming the same change), `state/tests/groups_more.rs` (a
demotion resets rights and flags together),
`state/domains/users/save_watch.rs` (two boxes save at once).
### Sweep fixes (r3-08)

| Ids | What changed |
| --- | --- |
| 15-002, 16-001 | `FolderLoad::Merging(RequestId)`. `start_folder_editor_merge` asks through `ask_chat_folder` (the same send as Loading, which also finds a fetch already out) and keeps the request in the stage. `settle_folder_editor_merge` waits on that request only: the spec arriving merges and saves, the folder's error or its own request ending with nothing puts the form back with the line (Retry is Save again). It no longer sends from upkeep and no longer asks `has_purpose_for_folder`, so another box's `GetChatFolder` is not its answer. `Merging` joins the in-flight names of `check-stage-enums.py`, so a bare one is rejected. |
| 15-003 | The two refusal bools are gone, and so is the `Refused` enum that held them. A refused on-screen client is a stage of the connection that asked: `ConnectStage::DiskRefused`, set by `open_client` and left by `reconnect_after_disk` (which starts a connection, or clears it to `Idle` while the app is locked). A refused background start is a stage of that account in `PrepareSet` (`Prepare::DiskRefused`: no worker runs, so it blocks no deletion), set by `finish_background_start`, dropped by a stop, and released all at once by a reading with room (`release_refused_starts`) before the pass that asks again. `DiskUi` keeps only the readings. `disk.refused` leaves `scripts/app-wide-fields.txt`; the poll function is `watch_disk_space`, and the allow line in `owned-timers-allow.txt` names that, not a bare `watch`. |
| 15-004 | `EarnLayer` is no longer `Closed`/`Open`: whether an earn page is open is the session's info column (`earn_layer_open`), and the layer keeps only what that column does not (back stack, covered panel, fade, refused page). Leaving, or opening over another panel, replaces the whole value (`EarnLayer::idle`, `covering`), so there is no second copy of "open" to drift. |
| 16-007 | `DestRow::saved` leaves `scripts/implicit-state-baseline.txt`: the allow line already says it is a fact about the chat, read when the rows are built. |
| 16-006 | `PasswordField` keeps no copy of the caller's view. The code that returns the box to idle queues the focus: `GiftManageState::password_ready` (a `GiftEvent::PasswordReady`, after a probe or a try that came back to the idle box, and a refused try) and `GramsHub::enter_withdraw` (every move into the idle box, which queues one `take_ready`). The frame upkeep gives the field the keyboard on that event and still empties it whenever the box is off screen. |
| 16-005 | `GifTab::Asking { wanted, request }` (a request went out) settles to `GifTab::Asked(wanted)` when that request is gone from the session (`settle_gif_ask`, run by `gif_tab_upkeep`), whatever ended it, so another window's search cancelling it does not leave the tab waiting. A send with nothing to wait for is `Asked` at once. Asked only after the send went out; a send that failed is `Wants` again, as before. |
| 14-004 | `take_draft_clear` takes the acked slot's own clear and nothing else. The chat-id fallback (HashMap order) is gone, so a send that set no clear cannot take another topic's. The ack names its slot with the same `draft_slot_of_message` the composer sends from. A TDLib ack carries no `RequestId` of the send (`updateMessageSendSucceeded` names the temporary message id), so the key stays the slot; keying by request would need the message id of each send recorded when its answer lands, which no path does yet. |
| 15-001 | Not a bug as described: `PlaybackUi` lives in `OnScreen`, which an account switch replaces whole, so another account never reads or drops this account's seek (test `a_seek_does_not_follow_an_account_switch`). |
| 15-005 | `DialogKind::DeepLinkInfo` is a per-window kind and its text is `MessageUi::deep_link_info`, set in the window the link was clicked in (`show_deep_link_info`; the main window when that one has closed). Two links resolved for two windows each keep their own text. The host-machinery tests that used it as their session-wide example use `BotAlert`. |
| 16-002, 16-003 | Closing the Add folder box while the add is out, or the Share folder box while a link save or delete is out, hands the request to `FolderUi::closed_asks` (`ClosedAsk::{FolderAdded, LinkChange}`). `settle_closed_folder_asks` (run from `settle_folder_share`) takes the answer and ends in the same toast as the open box: failure line, "added" text with the chats joined, nothing for a limit hit (its box says it). The map is in the account's `OnScreen`. An entry whose request is gone from the session with no answer kept (the ring of 16 dropped it) ends without a toast (`a_closed_ask_ends_when_its_answer_was_evicted`). |
| 16-004 | `TopicAction::Waiting(Vec<TopicWait>)`: each topic menu action keeps its own request, and the last answer leaves the stage (`Idle`). A second action no longer replaces the first one's wait. |
| 14-004 (round 2, albums) | `SendMessageAlbum` answers now record one `SendAnswered { request, chat_id, message_id }` per returned message (`Session::record_send_answer`, the same 64 cap as text), so a captioned album sent as a reply from a main composer finds its draft clear by `old_message_id`. Test `an_album_answer_names_each_temporary_message_of_its_request`. |
| 14-004 (round 3, album acks) | `DraftClear` records every message of a send as `SentMessage::Made { request, message }`: the first answer replaces the `Asked(request)`, later answers of the same request (the rest of an album) append. A `ClearStage` (`Armed` / `Spent`) keeps the entry after the first ack or failure until the last of its messages has acked or failed (`ack`, `fail`, `is_settled`), so a later ack finds it by id and clears nothing. The slot fallback now takes only a send that named no request (`is_unasked`), never a text send pending in the same slot; `fail_draft_clear` is the failure twin. Tests `a_later_album_ack_never_takes_a_neighbours_clear`, `a_later_album_failure_never_drops_a_neighbours_clear`. |
| 14-004 (round 4, one stage value) | The message list moved into the variants: `ClearStage` is `Unasked` / `Armed(Vec<SentMessage>)` / `Spent(Vec<SentMessage>)`, and `DraftClear` has no separate `messages` field. `Unasked` is the only stage a slot match can take (`is_unasked`); `ack` and `fail` move `Armed(rest)` to `Spent(rest minus message)`, and `Spent` with an empty list is settled. The earlier `Waiting`/`Armed` rename is gone with the bare variant. Tests `album_acks_from_one_slot_never_take_the_texts_clear` and `album_failures_from_one_slot_never_drop_the_texts_clear` send an album then a text from one slot (the text's clear replaces the album's); the album's acks or failures leave the text's clear for its own ack. The two-slot helper is renamed `album_and_text_in_two_slots` to match what it builds. |
| 15-003 (round 2) | `Prepare::DiskRefused { workers }` carries the workers still running. `refuse` turns `Cancelled { workers }` into it instead of keeping `Cancelled`, `finished` counts it down and keeps the refusal at 0, `running` and `keys` stay true while workers > 0, and `take_refused` / `cancel` hand the workers back as `Cancelled`. A refusal can no longer be lost when two workers overlap (test case in `a_refused_start_waits_without_blocking_deletion_and_leaves_on_a_retry`). |
| 16-006 (round 2) | `GramsHub::ready: Vec<WithdrawReady>` is now `events: Vec<GramsEvent>` with a real `GramsEvent::PasswordReady`, drained by `take_events` (the way `GiftEvent` is); `take_ready() -> bool` is gone. |

Tests: `folders::box_stage_tests` (merge waits for its own request; closed
add and closed link save), `composer_sends_tests` (an ack never takes a
neighbour's clear), `chat_window::asker_tests` (gif ask settles; two
windows keep their own link text), `forum_extras::tests` (both answer
orders), `deep_link_routes_tests` (seek stays with its account).

### Sweep fixes (r4-12)

| Ids | What changed |
| --- | --- |
| 23-008 | `StoryOpen::Waiting` holds `StoryAsk`: `Out(RequestId)` (the `getStory` the press is waiting on, found with `request_for_story_in_flight` after the send; the demo records the ask it would have sent) or `NoneOut` (nothing went out). `resolve_pending_story_open` ends the wait with the failure note when that request left flight with no story and no error recorded (a dropped request), and when none went out. Test `an_open_whose_request_was_dropped_ends_with_the_failure_note`. |
| 23-001 | `ToastQueue` keeps `Footer::{Plain(note), Toasted(note)}` instead of a copy of the note reset by `advance`. `note()` reads a toasted note only while its toast is on screen or waiting (`holds`), so the note ends with its toast; `drop_toasts` turns it into a plain note (the footer keeps it without a session). |
| 23-004, 23-005 | `EditorStage::Reading` and `Encoding` carry no `serial`: the stage holds the task that would answer, so a replaced stage cannot be answered (`is_reading()`, `is_encoding()`, `can_encode()`); `next_editor_serial` and its static are gone. Opening an editor over another goes through `release_video_editor`, which deletes the old cover file (test `opening_an_editor_over_another_deletes_the_first_ones_cover_file`). The avatar encode names its file by time. |
| 23-006, g-431..g-433 | The release recheck is one timer task held in `preferences.update_recheck` (each fire arms the next; replacing or dropping ends it). The download's 250 ms redraw loop is gone: the title-bar pill asks for 4 frames a second on the frame clock while it draws a download, so nothing ticks once it is not drawn. `UpdateControl::Downloading` is `Progress` (a derived view of `UpdateState::Downloading`, which owns the task). |
| 23-007 | `MediaViewer` carries `ViewerOpen`, minted by every `MediaViewer::open*` and kept by `close`; `ViewerUi::open_gen` and its six hand bumps are gone. `PendingGallery` holds the `ViewerOpen` it was asked over. Test `every_open_is_a_new_one_and_a_closed_viewer_remembers_the_last`. |
| 23-009, 24-009, g-439..g-446 | `Cell`/`try_borrow_mut` in place of `RefCell` borrows: the dialog bodies (`Rc<Cell<Option<_>>>` and `take()`), `text_reveal::CURRENT`, `window_frame::APPLIED`. |
| 23-010 | A `StripFrame::Failed` slot paints its nearest cut neighbour (the one before, else the one after), so one bad timestamp leaves no hole (tests in `frames.rs`). |
| 24-001, 24-003, 24-004 | Popups have a `PopupId`; `show_popup` returns it, `popups` is keyed by it, and `AccessStage::Asking`, `WriteAccess::Asking`, `PhoneFlow::Asking` and `SetStage::Confirming` hold the id of the popup they opened. An answer from any other popup leaves the stage (test `an_access_answer_from_another_popup_leaves_the_stage`). `check-stage-enums.py` counts `PopupId` as what ends such a stage. |
| 24-002 | `SetStage::Resolving { request: Option }` is `Deferred` (the shared lookup was busy; asked again each tick until the timeout) and `Resolving { request }`. |
| 24-005 | `LaunchFlow::take_lost`: a wait whose request left flight with no answer ends with the "couldn't start" note, so the mini-app poll stops waiting (test `a_wait_whose_request_left_flight_is_given_up`). |
| 24-006 | `MiniApps::generation` is gone; the poll task is held in `MiniApps::poll` and ends itself. |
| 24-007 | `Poll::Asking { request, asked }`: `schedule_widget_polls` makes a poll whose request left flight `Due` after a period, and looks again when a period has passed without an answer (test `a_poll_whose_request_was_dropped_is_due_again`). |
| 24-011 | `VoiceCapture::drop` removes the file of a capture nobody ended (after the encoder ends, off the UI thread); a fixture's and a handed-on file are kept (tests in `voice.rs`). |
| 24-008 | Not a leak: `Answers::take_for` drops every answer it is not asked for, and the preview's drain runs on every poll, so a link arriving after Cancel is dropped at once (test `a_share_link_arriving_after_cancel_is_dropped`). |
| 23-002 | Skipped (plausible): the dropped order is held only until the model shows it (`Echo::Caught` ends it), and the driver's optimistic reorder puts the dropped order into the model before the send returns, so a later server refusal rolls back a hold that is already over. Only a refusal before any sync saw the dropped order would hold it, and `reorder_refused` covers the synchronous refusals. No Touch Bar on Linux to drive it. |
| 23-003 | Skipped: tdesktop seeks the viewer's video while the slider moves (`playbackControlsSeekProgress`), as the comment at `Command::ViewerSeek` says. |
| 23-011 | Fixed in fix round 2 (below). Skipped before: the retry on reopen is the documented choice (a pass that retried would hammer a connection that is down); a backoff retry needs a clock the pump does not have, and tdesktop's source was not at hand to copy its rule. |
| g-403..g-430, g-434..g-438 | Names that said work was in flight on values that hold none: `Decoding::Active`, `Stage::Claimed`, `LookStage::Coming`, `Phase::Intact`/`Percent`/`NoPicture`, `PlayState::NotOpen`, `PictureState::{Spinner, Progress}`, `LiveOverlay::Placeholder`, `WaitingCheck::ShowAfter`, `Echo::OldOrder`, `PickerFill::Held`, `Unavailable::Undecided`, `Kind::HelperHolds`, `ToldStage::Untold`, `SendResult`, `ExtractionStage::{NoChild, Spawned}`, `VcardImport::Picked`, `Assembly::Partial`, `Cancellation::Active`. The open poll of `VideoPlayer` is gone: `Player::on_open` runs a wake from the decode thread when the file opens, and a capture's still waits on a condvar (`wait_open`, `wait_frame`). The caBLE scan waits on a stop signal instead of a 200 ms flag check. `run_poll_loop`'s gap is `throttle`. |

Tests: `story_viewer::open_tests`, `profile_photo_sources::tests`,
`video_editor_timeline::frames`, `web_app_ui::{emoji_status_tests,
launch_flow}`, `widgets_lock_tests`, `wallpaper_preview_tests`, `voice`,
`media_viewer`, `video_decode::tests`.

#### Fix round 2 (r4-12)

| Ids | What changed |
| --- | --- |
| toast footer | `Footer::Kept(note)`: `drop_toasts` (before a session exists) keeps a toast-kind note for the footer line, and `advance` (called only with a session) turns it into the blank default, so a failed sign-in's error no longer swallows the progress notes after sign-in. |
| AVPlayer open | `avplayer_open::OpenWatch`: a key-value observer on `AVPlayerItem.status` signals `Backend::on_open`, so the open wake exists on macOS too. `awaits_picture` lost its `NotOpen` branch; a backend that signals gets no vsync frames while it opens (`await_open` clears the wait, the wake asks for the first picture). |
| 23-011 | A reconnect (`updateConnectionState` to Ready) gives every failed translation its attempts back (`Session::translations_back_online`), and the pump's scan keeps unsettled messages, failures that ran out of attempts included, so a chat that stays open asks again. |
| recheck timer | `recheck_for_updates` starts a check only from Idle, UpToDate, NoRelease or Failed: it never turns a known update (Available, Staged, Downloading, Installing, Installed) into `Checking`. Superseded by the round 3 and 5 tables: `DownloadFailed` is in the timer's start set too, and its check keeps the kept release. |
| updates.rs doc | `RECHECK_EVERY` has its own doc again. |
| self-update probe | `finish_previous_update` answers `AVAILABILITY` when `current_exe` fails and when the cleanup thread cannot start. `wait_with_timeout` is `child_timeout::wait_within` plus the kill. |
| widgets drain | `schedule_widget_polls` does not call a poll dropped while its answer waits in `bots.widget_answers` (held back while locked). |
| story doc | `StoryAsk::NoneOut` says what gives it. |
| vanish | `GhostStage::Waiting { deletion: Deletion, requested }`: `Deletion::Sent(RequestId)` is ended by the answer (a refused delete drops the ghost: `settle_ghosts`, run by `poll_drains`), `Local` is the demo's. A ghost turns to dust only when the session knows the deletion (`message_deleted`), in every merge case. `LATEST` and the painted rows of a closed window are dropped (`painted::window_closed`). Split `vanish/painted.rs`. |
| popups | The popups map holds only `App`, `Clipboard` and `CloseConfirm`; a stage that opened its popup holds the `PopupId` and `popup_owner` asks each stage whether it owns an answer's id. `show_popup` returns the ids it replaced and `show_mini_app_popup` answers them, so no later helper event is needed (`answer_replaced_popups` is gone). |
| mini-app window | `MiniAppWindow::launch_window` replaces `MiniApps::from_window`: the poll and the helper's events run as the window the open app's flow started in. `mini_app_watch_due` ticks only while the phone flow waits on a request. |
| emoji status | `SetStage::Deferred` is gone: the suggested emoji's own lookup (`ConnectDriver::resolve_suggested_emoji`) does not wait behind the shared slot, so the stage goes straight to `Resolving { request }`. |
| helper pipe tests | The broken sink holds a sender dropped when the writer thread ends; the tests block on `recv()`. |
| translate Hide | A Hide that TDLib refuses (the flag is back) ends its Undo toast with an error note (`end_refused_hide_toast`): superseded, that function is gone; `HideStage` and `end_refused_hide` settle it (r4-12 round 3 table). |
| suggested posts | The Decline box closes when its post is decided elsewhere (`close_decided_decline`) and `confirm_suggest_decline` sends nothing for a decided post; the dead `.loading/.disabled` and the dead busy branch of Publish are gone. |
| touch bar | `PickerFill::{Unfilled, Shown(contents), Held { shown, waiting }}` compares against what is shown first; `Anim::{Idle, Animating(timer)}` replaces `Option<NSTimer>`. |
| update pill | `UpdatePill` is one enum (`Other`, `Percent`, `Ready`) carrying the glide and the swap start. |
| video editor | A pointer move with the button up ends a held strip, a window losing focus ends it, and `start_encode` ends every drag. |
| video player | `Stage::Landing` carries the instant of the loop seek; one not left after `LANDING_WAIT` is sent again. The FFmpeg volume reopen comment says what a failed open does. |
| video decode | `Decoding::Gone(reason)`: a dead decode thread's failure stays, `seek` and `fail` leave it alone, and the decode loop notifies on a failed seek and on the end. |
| video/mod.rs | The extraction worker blocks in `waitid` (unix) or `Child::wait` (Windows) instead of polling `try_wait`; cancel kills by pid or handle while the stage says `Spawned`, and the worker reaps. |
| vcard import | `VcardImport::Picked` is gone; `BoxView::{Reading, Shown}` is the box's view of an `ImportStage`. |
| voice | `Drop for VoiceFinish` removes the file of a finish nobody completed (a fixture's is kept); `VoiceCapture::drop` closes the microphone on the discard thread. |
| BLE scan (Linux) | The stop branch is polled first, and a stream that ends sends `Available(false)`. |
| CTAPHID | `Phase::{Waiting { deadline }, Cancelling { reason, until }}` replaces `Option<(HidError, Instant)>`. |
| baselines | `poll:scan` is gone from the owned-timers baseline; stale allow lines for `Phase::Opening`, `LiveOverlay::Opening` and `poll:run_poll_loop` are gone. `app-wide-fields.txt` lists the new `UpdatePill` paths in place of the old ones. |

#### Fix round 3 (r4-12)

| Ids | What changed |
| --- | --- |
| 23-011 | `translation_unsettled` is true for everything but `Done`, so a `Pending` message stays in the pump's scan; `needs_translation` still keeps the ask from going twice. Ask, fail out of attempts, reconnect, asked again is tested through `finish_translation`. |
| open wake | `PlayerCell.drawn_in` remembers the view the element last painted in; the open wake redraws it with the watchers, so an unwatched autoplaying inline clip leaves its still. |
| mini-app window | `WebAppPending.window` is set from `asking_window()` when the open request goes out (`note_open_window`) and rides into `WebAppOpenResult::Opened.window`; `spawn_mini_app_window` takes it from the matched answer. The helper's queued events are drained before the session poll, so a popup shown from the poll side does not treat a queued `PopupClosed` as replaced. A lost `sendWebAppData` ends with an error (`end_lost_data_sends`) and keeps the poll alive until it does. Superseded by the round 4 table: the window is a parameter of every driver launch call; `note_open_window` is gone. |
| vanish | `begin_vanish` returns whether a ghost holds the request; the callers `expect` it only then. `Crumbling` keeps the `Deletion` until its answer is taken; `prune_vanishing` leaves ghosts holding a request to `settle_ghosts`, which forgets it. `settle_vanishing` runs in every window; a waiting ghost whose message is gone in a chat not shown is dropped. Split `vanish/settle.rs`. Superseded by the round 4 table: a waiting ghost lets go of its `Deletion` too (`settle_ghosts`). |
| video editor | `Drag::release` names the release for any held drag; the blur and button-up-move hooks run it (`release_lost_video_drag`). |
| translate Hide | `HideStage::Sent(RequestId)` in `TranslateScreen` is settled by the `toggleChatIsTranslatable` answer through `RequestAcks`; it outlives the toast, and Undo forgets it. `toggle_chat_is_translatable` returns its request. |
| suggested posts | Decline is not opened or confirmed, and Accept not confirmed, while a Publish or Decline of the post is out. |
| update pill | A pill already `Ready` keeps its glide for the next `Restart` pass. |
| recheck timer | From `DownloadFailed` the timer retries the download from the kept release without going through `Checking`. Superseded by the round 4 and 5 tables: the timer runs a check from `DownloadFailed`, keeping the release. |
| emoji status | `Ok(None)` from `change_emoji_status` answers `UNKNOWN_ERROR`. A `getCustomEmojiStickers` answer or error pokes `MINI_APPS`; `mini_app_watch_due` carries the lookup's 15 s deadline instead of a 250 ms tick. |
| tests | `emoji_status_tests.rs` and `mini_app_flow_tests.rs` block on `recv()` instead of `recv_timeout`. |
| hybrid sign-in | After the code is shown, `Available(false)` or a stopped scanner ends the ceremony with the phone error. |
| AVPlayer | `OpenWatch` holds its item and removes the observer in `Drop`; the observer is released on the main queue. `buffer` is `Option<(Instant, BufferState)>`. |
| FFmpeg reopen | The open wake lives in the backend and every player it opens forwards to it; a clip that failed is not reopened. Superseded by the round 4 and 5 tables: each reopen gets a fresh wake slot, and a refused one registers again. |
| video decode | `wait_frame` also stops on `at_end()` and `is_stopped()`. |
| waiting gate | `WaitingGate::Delayed { since, _timer }` is built whole by `observe`; `shown_in` spawns the redraw timer. |
| voice | `Recording::{Ours, HandedOn}` replaces the empty `PathBuf` sentinel in `VoiceCapture` and `VoiceFinish`. |

#### Fix round 4 (r4-12)

| Ids | What changed |
| --- | --- |
| vanish | `settle_ghosts` turns every answered ghost's `Deletion` into `Local`, waiting or crumbling, so none keeps a request nothing will answer. `vanish_overlay` does not count a ghost at `Phase::Done` as running (`animating`). Test: a delete answered while waiting stops asking for frames after the crumble and is pruned with no later poll. |
| FFmpeg reopen | The reopen moves the open wake into a fresh slot before the new player forwards to it, so a dropped player still opening fires an empty slot. Test: a gated demuxer is mid-open at `set_volume`; its open does not fire the wake, the new player's does, once. |
| mini-app window | The window goes into `open_web_app`, `get_web_app_url`, `get_main_web_app` and `get_web_app_link_url`, and `mark_web_app_pending` builds `WebAppPending` whole; `note_open_window` is gone. The connect test passes the window to the driver call; a UI test reads `launch_window()` inside `as_asked_by(Some(12))`. A UI test cannot send through a live driver, so the capture is covered by those two halves. |
| tests | No wall-clock deadline in the new tests: `Player::wait_open_settled` and `wait_frame_settled` wait on the decode thread's condvar with no deadline; the update test advances the manual clock. |
| video editor | `release_tests.rs` drives `release_lost_video_drag` (the blur hook) and `video_timeline_moved` (the button-up move hook) with a held quality line and a held picture: the drag is idle and the crop grid goes. |
| translate Hide | The `TranslateScreen` farewell calls `forget_hide`, so an account switch with a Hide out leaves nothing in the old client's acks. Test: farewell, settle the request, nothing to take. |
| recheck timer | Supersedes round 3's row: from `DownloadFailed` the timer runs a fresh check (`Checking`), not only a retry of the kept release. A newer release that fixes a bad download is found; the kept one, when still the latest, downloads again from the check's answer. A check that fails offline leaves `Failed`, and the next timer checks again. `updater::force_latest_release` is the test seam; test on the manual clock. |
| docs | The 14-002 and translate Hide rows of earlier rounds say they are superseded. |
| suggested posts | The old test's doc comment is back above its test. |

#### Fix round 5 (r4-12)

| Ids | What changed |
| --- | --- |
| recheck timer | Supersedes round 4's row: from `DownloadFailed(release, _)` the timer's check keeps the release. `check_for_updates_keeping` maps a `Failed(message)` answer to `DownloadFailed(release, message)`, so an offline recheck leaves the release notes, the Update button and the pill. `Available`, `UpToDate` and `NoRelease` apply as before. Tests on the manual clock force the check failure (`updater::force_check_failure`) and the self-update answer, so none depends on the real process. |
| FFmpeg reopen | Supersedes round 4's row for the refused branch: after the wake is put back, `set_volume` calls `on_open` again, which runs at once for a player that settled while the slot was empty, else queues a forwarder that finds the slot empty after the first. The test seam `reopen_with` is a closure that may refuse; the test refuses after the first open settled and the wake fires once. |
| decisions | The five round 2 to 4 rows that stated the old behaviour say they are superseded. |
