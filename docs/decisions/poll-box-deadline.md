# Poll box settings, Limit Duration and the request-poll button

## What tdesktop does

`boxes/create_poll_box.cpp` ends the box with a "Settings" section of switch
rows (title, about line, switch), in this order: Show Who Voted (not in
channels), Allow Multiple Answers, Allow Adding Options (not in channels;
locked off for quizzes and anonymous polls), Allow Revoting, Shuffle Options,
Set Correct Answer (the quiz; several right answers when multiple answers is
on), then in broadcast channels Restrict to Subscribers and Limit by Country
(with "Allowed Countries" under it), then Limit Duration. Limit Duration opens
a "Poll ends" row (a menu: 1, 3, 8, 24, 72 hours, or Custom, which opens
`ChooseDateTimeBox` titled "Deadline" with "Set Deadline") and a "Hide results"
switch. The box starts with `kDefaultPollCreateFlags`: who voted shown, multiple
answers, adding options and shuffle on. A deadline that has passed at Create
shows "The poll deadline has already passed".

`api/api_bot.cpp` (`ButtonType::RequestPoll`) opens the same box. A button that
forces a quiz or a regular poll fixes the Set Correct Answer switch; a one-time
keyboard hides first.

## What changed

- The typed "Close after, hours" field and the "Close at a set date" checkbox
  are gone. The inline checkboxes became `src/ui/polls/poll_settings.rs`: switch
  rows in tdesktop's order and with its about lines, hidden where tdesktop hides
  them. Limit by Country keeps its place from the country box.
- "Poll ends" is a button with a kit dropdown menu (presets that fit the
  duration cap, then Custom). Custom opens `poll_deadline.rs`: a panel with the
  kit date and time picker, Cancel and Set Deadline, replacing the poll panel
  like the country box does; Esc closes it first.
- Presets are sent as `open_period`, a custom date as `close_date`
  (`PollDraft`; TDLib takes one of them). The cap is `poll_open_period_max`
  from `updateOption` (`payments.poll_open_period_max`), falling back to a year
  until TDLib reports it; presets above it are not offered and a picked date is
  held to it.
- Quizzes may have several right answers when multiple answers is on
  (`PollDraft.quiz_correct` is a list, sent as `correct_option_ids`).
- The panel body scrolls when the window is short; the title and Create/Cancel
  stay in view.
- `KeyboardButtonType::RequestPoll` now carries `force_regular` and `force_quiz`
  and its keyboard button opens the box with the type fixed.

## Deviations

- tdesktop's colored icons on the switch rows are not drawn (Quill has no
  such icon set; the rows follow the kit's switch layout).
- tdesktop's menu shows each preset with a tiny-time label ("1h"); Quill shows
  the spoken form only.
- The poll sent from a keyboard button does not carry the keyboard message as
  its reply target (the box sends with the composer's current reply).

## How verified

GPUI UI tests (`src/ui/polls/settings_tests.rs`): defaults, locks, multi-answer
marks, Limit Duration draft, the deadline box (set, cancel), the TDLib cap, the
bot button for free, regular and quiz types. Driver tests for `open_period`,
`close_date` and multi-answer quizzes; a state test for the option. Captures
of the `ready-poll-settings` demo in light and dark.
