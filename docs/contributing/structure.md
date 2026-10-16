# Code structure: where new code goes

Several agents and people work on Quill at once. When every feature edited
the same few files (`main.rs`, `ui/app.rs`, `ui/app_demo.rs`, `ui/shell.rs`,
the module lists and a handful of central enums), nearly every pair of PRs
conflicted. The refactor series moved those registrations into the feature
modules. This page says where each kind of change goes now, and what CI
checks.

## Adding things without touching shared files

### A feature

Put it in its own module: `src/ui/<feature>.rs` (or a `<feature>/`
directory) for UI, `src/<feature>.rs` or a domain directory for core code.
The only shared line is its `mod` line in `src/ui/mod.rs` or `src/lib.rs`.
Keep each file under the size limit below; split by concern (render,
actions, fixtures) before a file gets there, the way
`codex-refactor-5a.md` split the long files.

### App state

State that belongs to one feature lives in that feature's struct in
`src/ui/<feature>_state.rs`, read as `self.<feature>.<field>`. Adding a field
touches only that file: the declaration and its initial value in `new`.

A feature with no struct yet adds `ui/<feature>_state.rs` with a
`pub(crate) struct <Feature>Ui` and a `new`, plus one `mod` line in
`ui/mod.rs`, one field in `QuillApp` (`ui/app.rs`) and one line in
`new_with_demo` (`ui/app_demo.rs`). `new` creates the feature's own text
fields. Details: `docs/decisions/codex-refactor-3.md`, `-3b.md`, `-3c.md`.

### A screenshot demo

Register it next to its setup code with `register_demos!`. No change to
`main.rs`, `screenshot_demo.rs` or `app_demo.rs`. Details:
`docs/decisions/codex-refactor-1.md`. Read the screenshot policy below
before adding one.

### A dialog

Register it next to its builder with `register_dialogs!`, with a free
priority number. No change to `ui/shell.rs`. Details:
`docs/decisions/codex-refactor-2.md`.

### A request or an update handler

- New request: add the variant to `src/state/domains/<domain>/purpose.rs`
  (unit variants also go in its `flat_purposes!` list), handle the answer
  in the domain's `apply.rs` and the failure in its `error.rs`.
- New update or answer type: add the variant to the domain's
  `<Domain>Payload` in `src/telegram/envelope/domains/<domain>/mod.rs`,
  parse it in that domain's `parse.rs`, and apply it in
  `src/state/domains/<domain>/apply.rs`.

Only a new domain touches the hubs (`request_purpose.rs`,
`envelope_types.rs`, `payload.rs`, `session_apply.rs`,
`session_apply_error.rs` and both `domains/mod.rs`). Details:
`docs/decisions/codex-refactor-4.md`.

## File size limit

`scripts/check-file-size.sh` fails when a `.rs` file under `src/` or
`crates/` has more than 1,000 lines. Test-only files are not counted (files
under a `tests/` directory and files named `tests.rs`, `*_tests.rs` or
`*_tests_*.rs`), and neither are the `#[cfg(test)] mod` blocks at the end of
a file.

Files that were over the limit when the check started are listed in
`scripts/file-size-baseline.txt` with their line count. A baselined file may
shrink but never grow. Once it is under the limit, remove its line from the
baseline in the same PR (the check fails until you do). After shrinking a
baselined file, `bash scripts/check-file-size.sh --update` lowers its entry;
it never adds or raises one. A new file that needs more than 1,000 lines
should be split instead of baselined.

## Hotspots

`scripts/hotspots.txt` lists the shared files that are left and, for each,
the change a normal PR may make, for example one `mod` line in a module
list, one field in `QuillApp`, or nothing at all in `main.rs`, `ui/shell.rs`
and the central request and payload enums. `scripts/check-hotspots.sh`
compares the PR with `origin/main` and fails when a change goes beyond
that.

When a shared file really has to change (a new domain, a CLI flag in
`main.rs`), either add the `hotspot-ok` label to the PR and re-run the CI
job, or put a trailer on one of the commits:

```
Hotspot-change: new request domain for <feature>
```

The check also warns, without failing, when a changed file gains a `match`
with more than 30 arms or a `pub enum` with more than 40 variants. That is
how the old hubs grew; give the new cases their own module and dispatch to
it instead.

Both checks run in the `linux-fmt-clippy-test` CI job. To run them locally:

```
bash scripts/check-file-size.sh
bash scripts/check-hotspots.sh
```

## Window scope

A chat window's state (composer, history, viewer, pickers, message menu,
share box, and the session's view of its chat) sits in the shared
`QuillApp` fields only while that window is drawn or handles input
(`src/ui/chat_window/slot.rs`). Code that runs later finds whatever window's
state is in place then, so it has to say which window it belongs to:

- A timer, file picker or background job started from a window:
  `spawn_scoped` / `spawn_scoped_in`. It finishes in that window, or not
  at all once the window has closed.
- Work that belongs to no chat window (the poll loop, background accounts,
  the passcode): `QuillApp::spawn_main` / `spawn_main_in`.
- A subscription or observer made with a window (`subscribe_in`,
  `on_focus_out`, `observe_window_activation`, `window.defer`): run its body
  in `in_window_of(window, cx, ...)`. GPUI calls it with the window it was
  made in, but an edit made inside another window's scope is delivered
  after that scope ends.
- A subscription without a window (`cx.subscribe` on a viewer slider): read
  `self.windows.scope` when you make it and run the body in
  `in_window_scope(scope, ...)`.
- State that outlives one window's draw but belongs to one window (the
  recording) carries that window inside the value itself
  (`recording_state::Capture::origin`), so taking or dropping the value
  takes the window with it, and goes through `in_window_scope` with it. A
  separate "which window" field next to it goes stale.

`scripts/check-window-scope.py` (CI step "window scope") flags async calls
in `src/ui` whose closure touches window state without one of these. Work
that really needs no window says why in a comment on one of the three lines
above the call:

```
// window-scope: the pending draft names its own chat and topic
cx.spawn(async move |this, cx| { ... })
```

The check matches methods by name and can't see a window chosen wrongly:
a scoped task started from the main window's render captures the main
window even when the work belongs to another one. The limits are listed in
`docs/decisions/sweep-window-slot-scope.md`.

## Screenshots

- Visual checks while developing go to a temporary directory (`mktemp -d`,
  or the agent's scratchpad), never into the repository. Do not commit
  captures, and do not add them to `docs/screenshots/`; the README and site
  images come from `ready-showcase` only.
- Add a `--screenshot-demo` kind only when something will reuse it: a
  script, a smoke test, the showcase, or a capture people will repeat.
  A one-off check for a PR does not need a registered kind.
- Prefer a GPUI UI test (`#[gpui::test]` with the test window helpers) over
  a demo for checking that a screen opens, shows the right state or reacts
  to input. It runs in CI and keeps working; a screenshot only shows one
  moment and needs a person to look at it.
