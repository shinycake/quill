# One update button, no update switches

Owner, 2026-10-23: the in-app update was annoying. A found update showed a banner with no install button, installing lived in Settings behind three switches ("Check for updates on launch", "Install updates automatically", "Install beta versions"), and a downloaded update had no visible way to restart.

## What Quill does now

- It checks GitHub for the latest stable release at launch and every four hours while it runs.
- A newer release that this copy can install downloads right away, without a click (tdesktop's default, `cAutoUpdate` on).
- One button follows the update. It is derived from `UpdateState` on every render (`updater::UpdateControl::of`):
  Check for updates, then Update Quill (only when the download has not started, for example after it failed), then the download with progress ("Downloading update 19.0 / 48.0 MB…"), then Restart to update.
- While there is an update to act on, the button also sits at the bottom of the chat list, like tdesktop's "Update Telegram" (`Dialogs::Widget::checkUpdateStatus`). Settings shows it under the status line.
- Quill never restarts by itself. A downloaded update also installs at the next start (`update_install::apply_on_start`).
- A copy that a package manager owns, or a source build, only shows that a release exists and why it cannot install it. A copy on a disk image or in a read-only folder links the release page.

## What was removed

- All three switches and their stored files (`auto_update.json`, `update_channel.json`, the `check_updates_on_launch` appearance field). Old files are ignored.
- The beta channel (`interpret_release_list` for updates). What's new still reads the releases list.
- The top banner for an available or ready update. The banner stays only for "Updated to Quill X" with its notes.
