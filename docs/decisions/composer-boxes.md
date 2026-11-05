# Composer boxes as dialogs

## What Telegram Desktop does

The poll creator (`CreatePollBox`), the checklist editor (`EditTodoListBox`),
Add Link (`EditLinkBox`) and the date box (chat_helpers/message_field.cpp)
are all boxes over the window: a title, the fields, and the buttons in a
footer. Adding an option to a poll is also a small box.

## What Quill did

Each of these was a panel pushed in above the composer, framed by an accent
border, with its own title row and left-aligned buttons. The history got
squeezed to make room, a long poll pushed its buttons below the window, and
every panel looked slightly different.

## What Quill does now

- Poll, New Checklist / Add Tasks, Add an Option, Add Link and Choose Date
  are kit dialogs (`polls/poll_box.rs`, `checklist_box.rs`,
  `composer_boxes.rs`). Their open flags are unchanged, so every place that
  opens or closes them works as before; the shell opens and closes the
  dialog from the flag.
- The primary action sits at the right of a footer pinned under the
  scrolling content, with Cancel beside it, as in other kit dialogs.
- A kit dialog takes focus when it opens, so the field to type in is kept
  until the first render and focused then (`shell::focus_after_mount`).
- The poll box keeps its own Escape: it steps out of the country and
  deadline pickers first, then asks before discarding typed input.

## Verified

- `ready-poll-settings`, `ready-item-menus` with `QUILL_DEMO_ITEMS` set to
  `checklist`, `add-tasks` or `add-option`, and `ready-composer-date` (with
  `QUILL_DEMO_COMPOSER_BOX=link` for the link box), light and dark, with
  typing right after opening.
- Poll, country box and date box UI tests mount the app through the shell.
- Not checked against a live account.
