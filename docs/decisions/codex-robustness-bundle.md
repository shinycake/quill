# Robustness bundle (Q2, R8, R6, Q11)

## Q2 - TDLib payload parsing hardening

Sweep: production code only (everything before `#[cfg(test)]`, test files
excluded) of `src/telegram/envelope/*.rs` and `src/telegram/requests/*.rs`
for `unwrap()`, `expect()`, `panic!`-family macros, direct indexing, `as`
numeric casts and arithmetic on values that come from TDLib JSON.

Findings and changes:

- `expect()` on server-shaped data, 3 sites in
  `envelope/message_audio_video.rs` (`parse_message_video`,
  `parse_message_video_note`, `parse_message_audio`): the type check and the
  `expect` were two steps over the same `Option`. They are now one
  `let Some(..) = ..filter(..) else { return Unsupported }`.
- `expect()` in `requests/groups.rs` (`set_forum_topic_notification_settings`)
  re-parsed our own builder output; it now falls back to an empty object
  instead of panicking.
- Narrowing `as i32` of TDLib `int53`/`i64` values (about 210 sites; wrapped
  silently to garbage for out-of-range input, e.g. a negative date or a count
  past `i32::MAX`). Replaced by `SatI32::sat_i32` (saturating, in
  `envelope/json_helpers.rs`). Per file: `payload.rs` 62, `stories.rs` 15,
  `statistics.rs` 12, `chat_invite.rs` 12, `calls.rs` 11, `forum.rs` 9,
  `message.rs` 8, `message_sticker.rs` 7, `json_helpers.rs` 7 (`json_i32`),
  `message_poll.rs` 6, `message_media.rs` 6, `message_content.rs` 6,
  `chat.rs` 6, `message_link_preview.rs` 5, `message_reactions.rs` 4,
  `message_location.rs` 4, `message_audience.rs` 4, `communities.rs` 4,
  `message_audio_video.rs` 16, and 1-3 each in `message_sponsored`,
  `keyboards`, `chat_list`, `users`, `secret_chat`, `payments`,
  `message_checklist`, `envelope_types`, `bots`, `auth`, `service_action`,
  `saved`, `message_thread`, `chat_events`, `chat_drafts`, `chat_action_bar`.
  Casts that were already clamped, float-to-int casts (saturating in Rust) and
  `usize`-to-`i32` casts of local lengths were left alone.
- Arithmetic overflow (panics in debug builds) in
  `MessageSelfDestruct::remaining_secs` / `MessageAutoDelete::remaining_secs`
  (`envelope/message.rs`): `expires_in_ms` comes from a TDLib double and can
  saturate to `i64::MAX`, so `remaining_ms + 999` and the elapsed
  subtraction now use `saturating_*` / `try_from`.
- Checked and left: `requests/contacts.rs` `lines[prev]` / `line[1..]`
  (guarded by `!lines.is_empty()` and an ASCII space/tab prefix, and it parses a
  local `.vcf`, not TDLib JSON); constant-divisor `/` and `%`.

Verified: `cargo check`, new unit tests in
`envelope/json_helpers.rs` (`hardening_tests`: saturation, wrongly typed
media payloads, overflow-sized timers), existing envelope tests unchanged.

## R8 - message length limit

tdesktop (read only): `ApiWrap::sendMessage` cuts text over
`PremiumLimits::messageLengthCurrent()` into several messages with
`TextUtilities::CutPart` (break preference: paragraph, line, sentence end,
clause end, space, word separator, in the second half of the window; entities
are not split mid-span when a seam exists). Editing instead refuses
(`lng_edit_limit_reached`) and `CharactersLimitLabel` shows a red `-N` only
while the text is over the limit (nothing while "nearing" it).

Changes:

- `Session::message_text_length_max` (default 4096) follows the TDLib option
  `message_text_length_max` (Premium raises it); floored at 1.
- `src/text_split.rs`: `split_markup_text` (CutPart-style seams, never inside
  a formatting span when a clean seam exists, hard cut otherwise, never inside
  a surrogate pair), `units_over_limit`. Lengths are UTF-16 units of the
  parsed text (never below the scalar count, so safe whichever unit the server
  counts; tdesktop counts `QString::size()` the same way).
- Composer submit: a text-only send over the limit becomes several
  `ComposerSnapshot`s sent in order; the reply rides the first part only. The
  composer shows a muted "Will be sent as N messages" line.
- Editing a text message over the limit: `ConnectSendError::TextTooLong`,
  the edit stays open, status note "message too long (max N characters,
  remove M)", and the red `-N` counter (tdesktop style) above the input.
- Known limit: one formatting span longer than the limit has no clean seam, so
  the hard cut leaves literal markers on both halves (tdesktop splits such
  `pre`/`quote` entities; ours do not).

Verified: unit tests for the splitter, the option, the edit gate
(`driver_edit_snapshot_rejects_overlong_text`). The counter and submit path
were compiled and type-checked but not exercised in a live window.

## R6 - history window cap

tdesktop keeps a bounded run of messages around the viewport and re-requests
the rest on scroll. Quill's `HistoryState::upsert` only grew.

`src/state/history_trim.rs`: after a page lands, a window above
`HISTORY_WINDOW_CAP` (1500) is trimmed to `HISTORY_WINDOW_TRIM_TO` (1200) at
the end opposite the page:

- older page landed: drop the newest messages, set `has_newer`, remember the
  dropped tail in `latest_seen` (so `refresh_history_has_newer` keeps it set),
  clear `newer_failed`;
- newer page landed: drop the oldest messages, clear `loaded_complete`.

Safety rules: page-driven only (live `updateNewMessage` appends never trim,
so rows under a reader at the oldest end cannot vanish); no trim while the
doomed run holds a pending or failed send (retried on a later page);
tombstones, `viewed`, `unread_anchor`, `latest_seen` untouched.
`window_epoch` is intentionally not bumped by a trim: it makes the UI
re-anchor to the bottom / unread divider, which would yank the reader. Every
real window replacement (`reset_window`, jump to latest) still bumps it.

UI (`conversation.rs`): `row_window_shift` recognises a pure slide of the row
list and the scroller removes exactly the dropped rows (`splice` at the
tail and/or front, then `prepend` / `append`) so the visible anchor shifts
with the rows; anything else keeps the previous full-splice path.

Verified: reducer tests driving real `messages` pages (scroll back past the
cap -> newest trimmed and `has_newer`; page forward -> tail refetched, oldest
trimmed and `loaded_complete` cleared; reaching message 5000 clears
`has_newer`; jump to latest resets the window and bumps the epoch),
`HistoryState` unit tests (pending protection, bookkeeping, epoch), and
`row_window_shift` tests (tail trim, front trim, albums, rejects). Not
verified: the GPUI scroller behaviour on a real 1500-row chat.

## Q11 - autostart

`src/autostart.rs`: the Linux `.desktop` and macOS plist are now written
atomically (sibling temp file, `sync_all`, rename; temp removed on failure).
The `Exec=` value follows the Desktop Entry quoting rules (spaces, `$`, `"`,
backslash, `%%`), the plist path is XML-escaped. Tests cover quoting, escaping,
atomic replace without leftovers, a failed rename keeping the previous file and
cleaning the temp, and an unwritable parent mapping to `AutostartError::Io`.
Windows registry: the key/value/quoting test stays platform-independent; the
real-registry round-trip test already runs on the Windows CI job. The
documented fallback: failures surface in the Settings toggle and leave the
entry unchanged; no second mechanism (it could not be cleaned up reliably).
Not verified here: Windows (no Windows toolchain in this environment).
