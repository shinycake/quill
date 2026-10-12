# Stars and boost updates, and files sent to Quill from outside

Items: `parity:updates-stars-boosts`, `parity:platform-sendto-menu`.

## Stars balance and boost updates

What tdesktop does: the Stars balance lives in one place (`Data::Credits`),
and every screen that shows it subscribes to that value, so a balance change
shows everywhere at once. A channel's boost level and counts are fetched
(`premium.getBoostsStatus`) when a boost screen opens and again after boosting.

TDLib: `updateOwnedStarCount` is the balance update. `updateChatBoost` is
marked "for bots only" in `schema/td_api.tl` (1.8.68): a user session never
receives it, so there is nothing to handle. The user-side signal for a boost
change is `supergroup.boost_level` inside `updateSupergroup`.

What changed:

- `updateOwnedStarCount` was already applied to `payments.hub.balance`, which
  every Stars screen reads on each draw (checkout, paid reactions, gifts, paid
  messages, Premium hub). One copy was stale: the Subscriptions list kept the
  balance from its own `getStarSubscriptions` answer. It now follows the
  update.
- `updateSupergroup` now carries `boost_level`. The session remembers the last
  level per supergroup. When it moves, the cached boost status of that chat
  (level, boost count) is dropped and the driver asks `getChatBoostStatus`
  again, so an open boost box, profile boost row, appearance dialog or giveaway
  screen shows the new level. The detailed status stays on screen until the
  answer replaces it. The first level seen only starts the record, and an
  unchanged level sends nothing.

Verified: reducer tests (`src/state/tests/boost_level.rs`), a driver test with
the recording sender (`src/connect/tests/boost_level_refresh.rs`: the request
goes out only when the level moves), the envelope parse test.

## Files sent to Quill from outside; the Windows "Send to" entry

What tdesktop does:

- `Core::Launcher` takes every argument that is not a flag, and everything
  after `--`, as a start URL. `Application::checkStartUrls` opens `tg:` links
  and collects local files; `MainWidget::handleStartFiles` shows the
  "Forward to…" recipient box. Picking a chat runs `sendPaths`: a chat that
  takes no photos, videos, music or files gets the toast "Sorry, no way to
  send media here :(" and the box stays; otherwise the chat opens and the
  send-files dialog appears. Nothing is sent without that dialog.
- The same path runs for a file-open event from the OS (`QEvent::FileOpen`)
  and for files forwarded by a second launch (single instance).
- On Windows only (`psSendToMenu`, `ManageAppLink`), an Advanced settings
  checkbox "Place Telegram in "Send to" menu" saves `<app>.lnk` in
  `FOLDERID_SendTo` (arguments `--`, working directory, description, app user
  model id). Unchecking deletes it. macOS and Linux have nothing: `psSendToMenu`
  is empty there, so Quill adds nothing for them (no share extension).

What changed:

- `src/start_files.rs`: argument parsing (flags skipped until `--`, links via
  `deep_link_inbox::sanitize_link`, files resolved against the working
  directory, `file://` URLs decoded, 8192-byte cap per argument like tdesktop),
  the single-instance hand-off (`--send-file=<absolute path>` per file, checked
  again by the owner) and the inbox the UI drains.
- `src/single_instance.rs`: the per-launch limits grew (32 to 256 arguments,
  64 KiB to 1 MiB) so a big selection in Explorer is not cut off.
- `src/main.rs`: launch files are queued, a second launch forwards its files,
  and a file URL from the OS URL-open callback is queued too.
- `src/ui/start_files_ui.rs`: the "Forward to…" chooser (searchable, like the
  recipient box), the refusal toast with the tdesktop wording, then the chat
  opens with the files in its composer. Quill's counterpart of the send-files
  dialog is the composer's attachment strip, where the user confirms; nothing
  is sent by this code. The chooser waits for sign-in and for the local
  passcode to be unlocked, as `checkStartUrls` does.
- `src/send_to_menu.rs`: the Windows shortcut (`Quill.lnk` in the Send to
  folder, `--`, working directory, description, user model id) and a switch in
  Appearance > General, shown on Windows only. The switch reads its state from
  the shortcut file, so it cannot disagree with Explorer, and writes on a
  background thread.

Differences from tdesktop, all deliberate:

- tdesktop remembers the checkbox in its settings file; Quill reads the
  shortcut itself. Same visible behavior.
- The composer's attachment strip takes at most ten files at a time
  (`ComposerAttachment::append_dropped_files`), the same limit as a drop; the
  status note says so. tdesktop's dialog splits more files into several albums.
- No cleanup on uninstall: the repository has no Windows installer or uninstall
  step to hook (tdesktop removes the link in `-cleanup`). Turning the switch off
  removes the shortcut.

Verified: unit tests for the argument parsing, the forwarding round trip, the
inbox hand-off and the shortcut file state (all platforms); GPUI UI tests for
the chooser (opens from files, refuses a chat without file rights, opens a
chat with the files attached); `cargo check --target x86_64-pc-windows-msvc`
of the core library type-checks the Windows shortcut code. Not run: creating
the real shortcut and launching from Explorer (no Windows machine here).
