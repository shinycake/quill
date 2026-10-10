# Refactor 3: QuillApp feature state

Part 3 of the structure refactor. Pure refactor, no behaviour change.

## Problem

`QuillApp` (`src/ui/app.rs`) had 487 fields, and every feature added a few
more: one in the struct, one in the 700-line initializer in
`ui/app_demo.rs`. Both files were in about 40 of the PRs merged over 30
hours, mostly for those two lines.

## Decision

Fields that belong to one feature move into a struct owned by that feature,
and `QuillApp` holds one field per feature:

| `QuillApp` field | Struct | Module | Was |
| --- | --- | --- | --- |
| `stories` | `StoryUi` | `ui/stories_state.rs` | `story_*`, `close_friends_*` (37 fields) |
| `twofa` | `TwoStepUi` | `ui/twofa_state.rs` | `twofa_*` (9) |
| `folders` | `FolderUi` | `ui/folders_state.rs` | `folder_*` (10) |
| `calls` | `CallUi` | `ui/calls_state.rs` | `call_*` (13) |
| `group_call` | `GroupCallUi` | `ui/group_call_state.rs` | `group_call_*`, `ptt_clock`, `ptt_capture`, `global_ptt*` (14) |

The field keeps its name without the feature prefix (`self.story_viewer`
is now `self.stories.viewer`, `self.twofa_notice` is `self.twofa.notice`).
Types, doc comments and initial values are unchanged. Each struct has a
`new` in its module that holds the initial values, and creates its own text
inputs where it owns them (the same inputs, created in the same order, from
the same spot in `new_with_demo`).

A script did the move: it takes the field list, moves the declarations and
initializers, and rewrites every `.old_name` field access under `src/`
(method calls with the same name are left alone). The compiler checks the
rest: a rewrite that hit a different struct's field would not build.

This part (3a) covers the features whose files no other refactor PR is
splitting. Later parts move the composer, notifications, settings, chat
list and dialog fields the same way, and the media viewer fields once the
media viewer split has landed.

## How to add feature state now

Put new fields in your feature's state struct and its `new`, and read and
write them as `self.<feature>.<field>`. That touches only the feature's
`ui/<feature>_state.rs`. A feature without a struct yet gets one: a
`pub(crate) struct <Feature>Ui` with `new` in `ui/<feature>_state.rs`, one
`mod` line in `ui/mod.rs`, one field in `QuillApp` and one line in
`new_with_demo`.

## Verification

Gate (fmt, core and UI clippy, core and UI tests). Test counts are unchanged.
