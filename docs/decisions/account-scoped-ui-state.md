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
| `GroupCallUi` (31) | dialogs, `composer`, `window*`, `chat_shown`, `pin`, levels, scrolls, `stream`, `rtmp_start`, `live_pip` | `ptt`, `ptt_clock`, `ptt_capture`, `ptt_delay_slider`, `global_ptt*` |
| `AgentUi` (5) | `picked`, `consent`, `page` | `activity_shown`, `ui_send` |
| `ConnectionUi` (9) | `status`, `generation`, `closed_noted_generation`, `note`, `note_window`, `status_traced`, `presence` | `toasts`, `toast_wake` |
| `ChatWindows` (12) | `open`, `scope`, `last_input`, `dialog_host`, `closed`, `restore`, `save_pending` | `main`, `main_place`, `cascade`, `main_dialogs_closed`, `_key_scope` |
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
