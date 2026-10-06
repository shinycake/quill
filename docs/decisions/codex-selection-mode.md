# Message selection mode (tdesktop)

In tdesktop (`HistoryInner`), once messages are selected:
- a click anywhere on a message row toggles it, and links and media
  don't fire;
- each row shows a check circle;
- ⌘C copies the selection (`HistorySelectedItemsText`): one message as
  its text, several as `[date time] Name: text` lines, oldest first.

Quill's "Select" started a selection with a "N messages selected · Clear
· Forward" bar, but rows ignored clicks, showed no checks, and ⌘C copied
nothing. Now:
- while a selection exists in the open chat, every single-message row
  carries an occluding overlay. It toggles the row and draws the check
  circle (filled accent with a check when selected);
- ⌘C, with no text selection, copies the selected messages in tdesktop's
  format. Media without text reads as its kind ("Video", "Sticker"). The
  author comes from `Session::message_author_name`, and protected chats
  refuse;
- album rows don't get the overlay yet.

Deleting a multi-message selection (tdesktop's "Delete N") needs a
multi-message confirmation and is left for a follow-up.

Verified live in Saved Messages: toggling rows, the checks, and the
copied text.
