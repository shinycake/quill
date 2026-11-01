Filter hostile text on every surface, keep failed settings saves, and send files as albums

Branch: claude/sweep-b14. Bundle B14 (text-and-tray), one commit per item, every item fixed.

## What was wrong

- robustness-8: message text dropped Zalgo marks, tag characters and bad controls before shaping, but chat titles, chat list previews and their sender names, the typing line, bubble sender names, reply strips, the composer reply bar, the chat header, the chat peek preview and notifications shaped the raw text.
- N55: a settings write that failed was dropped from the writer's queue, so the next read went back to the old file and the change was lost.
- N56: write_json_deferred and save_chat_windows returned a Result that every caller threw away. It only said whether the value serialized; disk failures reached the user only through the writer's status note.
- N57: no test drove removing tray cards past the tenth through to the send. Driving it found two faults in the demo send: the album JSON had a stray brace and never parsed, and a tray of more than 10 went as one album.
- N58: when the filter dropped characters before a paragraph's first letter, copying from the start of that paragraph left them out.
- N59: each painted paragraph built its full copy string on every paint, whether or not anything was copied.
- N60: the quit flush gave up after 5 s, so a slow disk lost whatever was still queued. Tests shared the process-wide writer and its gates, so a held write in one test could stall another test's flush.
- N61: the audio position test allowed a block either side of the played sound, so a position past the sound, which a resume would skip, still passed.
- N62: with a Send held on one photo's render, an editor opened on another photo of the same tray stayed open after the Send took the tray, so its Done landed nowhere.
- N63: nothing tested the Send cancelling a tap on a rendering card, or the fallback when the chat window closes before the render lands.
- N64: a picture pasted over one document replaced the document, and then the note said "album is full (1)".
- L06 (owner decision, default applied): a tray of files always went as one message per file. Telegram Desktop sends files in albums of 10 (attach_prepare.cpp, AlbumType::File) behind the Group option.

## What changed

- robustness-8: display_text::filtered applies the message filter to labels. It runs in bidi_line::one_line_plain and emoji_sprites::one_line (titles, names, reply text, header), on each preview run in chatlist_style (marks counted across runs, the way a paragraph counts them), on the row's sender prefix, third-line sender and typing line, on bubble sender names and reply strip names, in chat_preview_line, and in OsNotification::shown, which show_notification_as applies before any backend (native, notify-send, Quill's own windows).
- N55: a failed write stays queued and readable. The writer skips it until the next submit or the quit flush tries it again, and each failed try leaves one note.
- N56: both functions return nothing, and every failure, serialization included, goes to the writer's one note. The contacts prefs save, which writes synchronously and notes its own failure, no longer claims to report through the writer.
- N57: a UI test fills the tray with 13 photos, removes the first card and then the eleventh, and sends. The 11 left go in tray order as one album of 10 and one photo. The demo album now parses, splits like the real send, and numbers its messages after the newest one. Quill has no control that moves a tray card, so the reorder in this item is the shift after a removal, which the test covers.
- N58: SourceMap::source_of gives back what was dropped at offset 0 when the range starts there.
- N59: the copy callback builds the string when a copy asks for it. Each paint now moves the map into the callback and clones the ranges.
- N60: the flush waits until every queued write has been tried; each try ends in a write or an error. Each test thread has its own writer and gates, and the writer tests wait on channels instead of timeouts.
- N61: the test asserts the position never runs past the played sound and trails it by at most a block, at 1x and 1.5x. The engine already held this, so no code changed.
- N62: finishing a slot's send closes an editor open on that slot's tray.
- N63: two UI tests. A tap on a rendering card, then Send, with a second render still going: no editor opens when the first lands. A held Send in a chat window that closes first: the main window says the photo did not go, the tap opens nothing, and the edited photo waits in the chat's tray.
- N64: a pasted picture joins the tray the way a dropped file does: a photo next to media, a file next to files. The album-full note and its string are gone.
- L06: two or more files go in albums of up to 10 when grouping is on, with the caption on the last file and never in one album with media. The driver builds inputMessageDocument items for sendMessageAlbum.

## What ran

- cargo fmt --check, scripts/check-file-size.sh, scripts/check-hotspots.sh and scripts/check-window-scope.py: pass. No hotspot file changed.
- cargo clippy -D warnings with `--features demo-capture --all-targets` and with `--no-default-features --all-targets`: clean.
- cargo test --features demo-capture --lib: 6858 pass. One fails: the chmod 0555 test, which fails as root.
- cargo test --features demo-capture --bin quill: 2446 pass, 2 ignored. One fails: a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why, which also uses chmod 0555 and fails as root.
- I ran both N63 tests with their fix taken out, and both failed. For N62 and N64 I checked against the old code instead of rerunning: the old code leaves the editor open and clears the document, which their asserts reject.

## What did not run

- No macOS or Windows build. The x86_64-pc-windows-gnu target is not installed here. No change touches platform-specific code, and the notification filter runs before the backend branch.
- N59 gives no number, so nothing was measured. A test cannot see the allocation that went away; the existing copy and quote test covers the behavior.
- N56 is a signature change. The compiler enforces it, and no test was added.
- No screenshot of a Zalgo chat row. The filter tests cover each surface's string, not the drawn pixels.

## What stays open

- The quit flush no longer has a deadline. A filesystem that never returns from a write would hold quit until it does. Each write is one atomic temp-file write and rename, as before.
- OS window titles (set_window_title) still get only window_title::single_line. The OS shapes those, not Quill.
- Received file albums still show as separate messages; the album layout handles photos and videos only.
