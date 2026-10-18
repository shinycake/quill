# launch-time-early-connect: open the first TDLib client before the window

## Problem
After the font fix, a warm launch took a median of 407 ms from `open` to the first frame that
draws the chat list. The first frame itself was drawn at about 320 ms, but it had no chats:
the TDLib chain only started inside `QuillApp::new`, once the window existed.

- `start_connection` prepared the client on a background task (startup sweep, database key
  from the keychain, account folders, loading tdjson), so the client opened about 50 ms after
  the first frame.
- Signing in from the database then took two trips through the UI thread: the poll loop read
  `authorizationStateWaitTdlibParameters` and sent `setTdlibParameters`, then read
  `authorizationStateReady` and sent `loadChats`. Each trip waited for the main thread, which
  was busy drawing.

## Quill
- `ui::early_connect` begins the run's first connection in `main`, before AppKit starts: a
  worker runs the same preparation (`prepare_startup`, the pending-delete sweep) and loads
  tdjson. `open_if_ready` opens the client on the main thread as soon as that is done (checked
  when the application starts running and when the window is built), and `start_connection`
  takes it. With a local passcode, or if the worker has not finished, `start_connection` does
  what it did before.
- `ConnectDriver::kickoff` sends `setTdlibParameters` with `getAuthorizationState`. A new
  client always starts waiting for its parameters, so there is nothing to wait for, and TDLib
  opens its database while the window is still being built.
- The UI thread polls the client when `QuillApp` takes it (which sends `loadChats`) and once
  more just before the window's first draw (`poll_before_first_frame`), so the first frame
  draws the chats TDLib has answered with.

The client, its call engine and the poll loop are the same objects on the same thread as
before; only the moments they start moved earlier.

## Measuring
`scripts/launch-bench.py`, seven alternating warm runs per side on an M1 Pro (load average 7
to 9 from other builds), against the font-fix build:

| milestone | before | after |
| --- | --- | --- |
| `client_open` | 372 | 311 |
| `auth_ready` | 390 | 311 |
| `first_render` | 319 | 328 |
| `chat_list` | 407 (375–424) | 329 (322–341) |

`chat_list` now lands on `first_render`: the first frame has the chats. What remains before it
is AppKit and GPUI startup (about 100 ms to `main`, about 85 ms creating the platform and the
application, about 60 ms creating the window) and building and drawing `QuillApp`.
