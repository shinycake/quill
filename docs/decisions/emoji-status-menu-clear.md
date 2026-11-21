# Clearing the emoji status from a custom emoji's menu

Owner ask (2026-11-21): add a way to clear the emoji status. The status
selector already removes it with its first (star) cell, but that is the only
way and easy to miss.

## What Telegram Desktop does

`menu/menu_emoji_status.cpp` `EmojiStatusMenu::AddSetAsStatusAction`, used by
the emoji panel's right-click menu (`EmojiListWidget::fillRecentMenu`) and an
emoji pack's box (`StickerSetBox::Inner::contextMenuEvent`):

- Premium accounts only; a custom emoji gets "Set Status", or "Remove Status"
  when it is the status now.
- The change shows a 5 s toast, "Emoji status set. Undo" or "Emoji status
  removed. Undo"; Undo puts back the old status and its end time.

## In Quill

- `quill::emoji_status_selector`: `menu_status_action` (which item),
  `status_undo` / `undo_change` (what Undo puts back; a timed status that ran
  out meanwhile is not restored).
- `ParsedUser::emoji_status_until` keeps `emoji_status.expiration_date`
  (`None`: no status), so Undo restores the end time. A collectible status
  has no custom emoji id; its toast carries no Undo.
- The toast click is a `NoteClick` on the status note (it replaced the
  `reveal` path option). A newer Undo toast takes the place of an older one,
  so a stale Undo cannot revert the newer change.
- `ui/media_panel/status_menu.rs` adds the item to the emoji panel's custom
  emoji cells (compose mode) and to an emoji pack box's cells.

Demo: `ready-emoji-status-menu` (right-click a recently used custom emoji).
