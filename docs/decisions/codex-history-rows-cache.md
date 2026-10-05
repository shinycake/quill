# codex/history-rows-cache — rebuild history rows only when their inputs change

## Measured
Stress fixture (3000 chats, 2000 loaded messages, forced 60 Hz re-render, release, M1 Pro):
process CPU went from 36.7% to 26.3%. Main-thread time in `Window::draw` went from 47.8% to
28.4%, and samples in `history_message_list` from 290 to 43. Every app re-render (typing a
key in the composer, a hover, a poll tick) used to clone and rebuild every loaded message.

## Design
- `HistoryState::messages` is a `HistoryMessages` newtype: reads go through `Deref` to the
  `BTreeMap`, and every mutating method (`insert`, `remove`, `get_mut`, `values_mut`,
  `iter_mut`, `retain`, `clear`) bumps a revision. Message changes are caught wherever they
  happen (reducer, driver, UI) with no call-site changes.
- `Session::revision` is bumped for every applied envelope, which covers chats, users, quoted
  messages and files.
- `HistoryRowsKey` has the list, chat/topic, messages revision, session revision, window
  epoch, unread anchor, highlight, local day, and a hash of the app-side inputs (forward
  selection, paused playback positions, playback speed, volume and error). When the key
  matches, the caller skips snapshotting the messages, and `history_message_list` skips the
  rebuild and the scroller sync.
- It is not cacheable (`None`) while audio, a GIF or a video plays (rows carry per-frame
  state), for topic histories (a different type), and for demo fixtures, which mutate chats
  and users without `apply`. The stress fixture is the exception, for profiling.
- Also fixed (regression from #351): a replaced window is empty while its page loads.
  `Session::history_loading` now covers "empty with a window page in flight", so the skeleton
  shows instead of "No messages yet".
