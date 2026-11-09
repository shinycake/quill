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
