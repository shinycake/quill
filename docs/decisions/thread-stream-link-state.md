# Thread root jump, stream join and pending link open as explicit state

Date: 2026-11-07

Three small bugs had the same cause: two pieces of state that had to agree,
or a value that meant different things to different writers.

## Jump to root in a thread

"Jump to root" while older replies were still unloaded set a flag. On the
frame the last older page landed, `advance_thread` scrolled to row 0 and
cleared the flag. The same render then built the rows. It saw a prepend in
the same window, so it captured a view hold of the message at the top before
the jump and restored it. The jump was undone.

Now `HistoryUi::scroll_goal` holds `Option<ScrollGoal>`, and the history list
is the only code that acts on it. `jump_to_thread_root` only sets the goal.
`advance_thread` only pages older replies while the goal waits. When the
thread is loaded down to its root, the list skips the view hold for that
render, builds the rows, and then scrolls to row 0 and clears the goal. A goal
for a thread that is no longer open is dropped. Other scroll owners (the
landing, the open anchor) run before the goal in that render, so the goal
wins.

## Stream join and the screen share error

The stream window treated `ActiveGroupCall::transport_error` as "the join
failed". A failed screen share wrote the same field (start, stop, connect, or
its offer), and nothing cleared it on the next join. So after a failed share,
pressing Join on a stream failed at once. The check also ran inside the
window's draw and changed `joining` and `join_timeout` there.

Now:

- Screen share failures go to `screen_share_error`. A failed share leaves the
  call itself working.
- `join_video_chat` (and the demo join) clears `transport_error`. While a join
  waits, a transport error belongs to that join.
- `settle_stream_join` does the join bookkeeping. It runs from
  `sync_group_call_window`, outside the draw. The draw only reads `joining`
  and `group_call_error`.

## Pending deep link open

`pending_deep_link_open` and `pending_deep_link_open_from_agent` were two
fields. `choose_share_chat` and the group call share set the first one and
left the second alone. So a share the user picked could inherit an earlier
agent link's origin and get blocked by the agent's chat grants. Now it is one
`Option<PendingOpen { chat_id, action, from_agent }>`. `PendingOpen::user`
builds an open the user asked for.

## Tests

- `thread_root_jump_tests`: the page that reaches the root lands the view on
  row 0, after the reader scrolled away from the latest.
- `a_failed_share_does_not_fail_the_join` (UI) and
  `a_new_join_drops_the_last_transport_error`, plus the updated
  `a_failed_share_payload_clears_the_pending_share` (driver).
- `a_share_the_user_picks_replaces_an_agents_waiting_open`.
