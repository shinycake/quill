# Confirm boxes

## What Telegram Desktop does

Destructive steps ask in a box over the window (`Ui::MakeConfirmBox`,
`DeleteMessagesBox`): a title, a line of text, Cancel and a red button.
Enter confirms and Escape cancels.

## What Quill did

Stop Poll and Close Secret Chat were red banners pushed in above the
composer, with gray buttons. The status line also said "Confirm stop poll"
while the banner showed and "Stop cancelled" after Cancel.

Deleting one message already opened a kit alert, but the "Stars will be
lost" warning for a paid suggested post younger than
`suggested_post_lifetime_min` lived only in a delete banner that never
rendered: `pending_delete` was set and taken in the same update. So the
warning never showed.

## What Quill does now

- Stop Poll / Stop Quiz and Close Secret Chat are kit dialogs
  (`confirm_boxes.rs`) with the TGX copy, Cancel and a red confirm button.
  Enter confirms, Escape and ✕ cancel.
- The delete box shows the paid-post warning as its title and text, with
  "Delete anyway", the way the dead banner meant to.
- The dead delete banner, its Escape layer and the status-line notes
  around these boxes are gone; the box is the feedback.
- Stop Poll stays out of reach for agents: `StopPollConfirm` replaces the
  `pending-stop-poll` hard layer in `HARD_DIALOGS`.

## Verified

- `ready-item-menus` with `QUILL_DEMO_ITEMS=stop-poll` and
  `ready-secret-chat-menu` with `QUILL_DEMO_SECRET_CLOSE=1`, light and dark.
- `selection_mode_delete_tests`: a young paid post shows the warning, an
  old one does not.
- Not checked against a live account.
