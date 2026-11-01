Stop an open chat growing without limit and let exports finish off screen

## What was wrong

A chat left open kept every live message for as long as it stayed open. The history window cap ran only when a page landed, and a chat that only receives messages never lands one. Forum topics and reply threads grew the same way. Each extra row costs about 4 KB here.

Exports had four problems:

- A running account export marked every poll as progress, so the poll loop stayed busy and asked for a redraw on every pass, for the whole export.
- An export's requests went out only for the account on screen. After switching accounts the worker waited 300 s and failed with "the request timed out".
- Quitting dropped a running export without asking.
- A media export left a second copy of every file in TDLib's cache, and Cancel left the current download running.

Smaller leaks: replied-to messages stayed in memory after their rows left the window, pages of a comment thread had no cap, each opened link or file left a zombie process until Quill quit, and notification icons were never cleaned up, even after logout or account removal.

## What changed

Live messages now trim. Past 1,500 rows the main history, a topic and a thread each shrink to 1,200. The view reports a `ReaderPosition` to the session, and it stays as it was while a hidden window stops drawing. At the tail the oldest rows go. Away from the tail, rows more than 200 above the reader go first, then the newest end, and live messages wait until the reader pages down to them. A topic has no newer pages, so it only loses rows above its reader.

Thread pages use the same cap, and a page on its way past a trim is dropped. Each chat keeps at most 256 replied-to messages outside its window, nearest first, so a row paged back still shows its strip.

Export progress asks for the coalesced redraw, a hidden box asks for none, and the end shows at once. Both workers wake the poll pass through one `publish`, so the spin that read a `finished` flag is gone. Requests are forwarded from `poll_ticks` for every account. An export that ends off screen reports through the status note. Quit, closing the main window, the tray's Quit and restart to update ask first with tdesktop's "Stop" box while an export runs.

A file only the export wanted is hard linked into the export folder where the volume allows, then deleted from the cache. A file TDLib already held or was fetching stays there. Cancel stops the current download, again only if the export started it.

`child_reap::wait_in_background` collects helpers without killing them, for every `open`, `xdg-open` and `explorer` start and two macOS call sites. Notification icons older than 30 days go at startup, off the main thread. All of them go on logout and account removal. The folder is app-wide, so other accounts' icons are rendered again on demand.

## Measurements

Same demo on both sides: `ready-chats` with 30 chats of 800 messages, a stream of live updates into the open chat, and the jump-down button pressed first so the view follows the tail. Fastrel builds of base `570687682` and branch `d73a430b8`, 85 s each, two runs per side in the order base, branch, branch, base, holding the build slots. Nothing was swapped in any run.

| | Base | Branch |
| --- | --- | --- |
| Rows in the chat at the last count | 4,137 and 3,803 | 1,395 and 1,428 |
| Footprint at 85 s | 125.1 and 121.0 MB | 114.2 and 112.4 MB |
| Footprint growth from 45 s to 85 s | 8 and 6 MB | 0 and 0 MB |
| Frames per second over the last stretch | 60.0 (one run) | 59.9 and 59.9 |
| Gaps over 34 ms in that stretch | 0 | 0 and 0 |
| CPU over the run | 42.3 and 38.4 s | 38.0 and 38.0 s |

The 10 MB gap at the end covers about 2,600 fewer rows, which is about 4 KB per row, close to the 3.5 KB the audit inferred. Base keeps climbing and the branch does not. By arithmetic, a group at one message a second would cost about 14 MB an hour on base.

The first pass left the view where the demo opens it, near the top of the chat, so it tested a reader away from the tail. The branch held 107.7 and 108.3 MB against 121.9 and 119.5 MB on base, with 59.8 to 60.1 frames per second on both sides and at most two gaps over 34 ms per run.

The stream reached 230 to 275 updates a second, short of the 800 asked for, which is about 40 net new rows a second. One base trace in the tail pass was lost when `xctrace` hung, so the frame row has one base run. Two runs per side are enough for the memory gap. They do not separate the CPU figures, so I claim no CPU change. Fastrel has no LTO, so these are relative numbers.

Captures of the real window under the stream: the view stays at the tail after two trims, a reader scrolled up a few screens stays on the same rows through 10 s of live messages, and Cmd-Q with an export running opens the Stop box.

## Tests

New behavior tests cover the live trim at the tail and away from it, sends in flight, topics, threads and paged thread windows, reply targets, the row list following the tail through a trim, the export request flow off screen, the end note from a background account, the quit box, file release and cancel against a fake TDLib, the child collector, and the icon sweep. The poll liveness simulation now reports the reader and passes with the live trim on.

Lane tests after the last merge: 348 core tests and 53 UI tests passed. `quick-gate.sh` printed QUICK OK, which covers clippy with `-D warnings`, hotspots, file size, window scope, app name and language checks. The full suites were left to CI.

## Not verified

Nothing here ran against a Telegram account or real TDLib, so `deleteFile`, `cancelDownloadFile` and a real media export are proven only against fakes. The account export's poll and redraw saving has no number, because the demo has no poll loop for a live account. Its redraw decision and the worker's wake are unit tested.

Windows and Linux were not run. The child collector was tested with `true` and `sleep` on macOS only.

macOS's Dock menu and an OS logout still end a running export with no prompt, because the app gets no way to refuse them. A fix belongs in the vendored GPUI patches. With the export box itself open, Cmd-Q does nothing in the demo, on base and on this branch.

Pages of a forum topic still have no cap. A topic has no newer-page path, so its newest end cannot be dropped and paged back in yet. Live messages in a topic are capped, and a long read of old pages is returned when the view closes. growth-2, a default TDLib cache limit, is the owner's call and stays out.
