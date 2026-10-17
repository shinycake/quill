# Toast presentation: queue, hover pause, centred on the conversation

## What tdesktop does

`Ui::Toast` (lib_ui `ui/toast/`) keeps every toast in a manager with its own
hide time, wraps text to several lines (`defaultMultilineToast`), and fades
each one out when its time is up. Several toasts can be alive at once, so a
second message never erases the first. Toasts are anchored to the window
widget they were made for, which in the main window is the history column.
Hover-to-pause and click-to-dismiss are not part of `Ui::Toast` itself
(`acceptinput` is opt-in for action links); Quill adds both because a
failure that vanishes while being read is the complaint in round 2 finding 10.
Action links (Undo, Show in folder) stay out of scope.

## What changed

- `ui/toast_queue.rs`: the queue and timers as pure state with the clock
  passed in. An error is never dropped by a later info note (it waits, and
  jumps ahead of waiting confirmations), an identical consecutive note shows
  once and restarts the visible timer, at most eight notes wait (old
  confirmations go first, then the oldest failure), hovering stores the remaining time
  and resumes from it, the hover flag carries to the next toast so a click
  while hovered keeps it paused, clicking dismisses and shows the next. Unit tested.
- `ConnectionUi::set_note` pushes toast-kind notes into the queue; the
  current-note slot keeps its meaning for the code that reads it, and is
  freed when its toast ends (`advance_toasts`).
- `status_toast.rs`: render reads the queue and schedules one wake-up for
  the next deadline (none while paused). Before a session exists the
  footer status line carries the note (a blocker stays visible until it is
  resolved), so nothing toasts then and the queue is dropped; no duplicate.
  A finished toast never leaves its deadline set (no render loop). The demo flag
  `QUILL_DEMO_SHOW_TOAST` still freezes timers for captures.
- Layout: the toast is centred over the conversation pane using
  `ShellLayout::chat_insets` (list, rail and topics on the left, side panels
  on the right, nothing in one-column mode), up to three lines at 440 px,
  `rounded_xl` instead of a one-line pill. The insets are zero on the
  sign-in, passcode and onboarding screens (no columns). The bottom offset
  is the composer's measured height plus a 16 px gap, so a tall composer lifts the toast in every layout.
- Accessibility: failures are `Role::Alert`, other toasts `Role::Status`.
  GPUI exposes no separate live-region property; the role is what screen
  readers announce from.

## Kit component

gpui-component's `Notification` stacks at a window edge through the
window `Root`, owns its own timers outside Quill's state and cannot centre
on a pane or be paused by Quill's queue. The toast stays a small Quill
element built from kit theme tokens.

## How verified

Unit tests (queue rules, hover pause, `chat_insets`, note release), plus
`/verify` captures of the long error and the queued pair in light and dark
at 1200x800 and 500x700. See the PR description.
