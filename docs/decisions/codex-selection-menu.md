# Message menu over a text selection (tdesktop)

tdesktop's message menu (`history_view_context_menu.cpp`) changes when it
opens over selected text:
- `AddReplyToMessageAction` labels the first row "Quote & Reply" and
  replies with the selection as the quote;
- "Copy Selected Text" replaces the whole-message "Copy Text";
- tdesktop has no separate quote dialog. Quoting part of a message *is*
  selecting it.

Quill had a "Reply with Quote" item that opened a dialog for trimming the
message text by hand. That was a stand-in, because message text couldn't
be selected; now it can (codex-selectable-text). So:
- `open_message_menu` records the window selection when it lies in the
  clicked message (`selected_message_text`);
- with a selection, Reply becomes "Quote & Reply"
  (`begin_quote_reply`: the quote's UTF-16 offset is found in the full
  text, as before), and Copy becomes "Copy Selected Text";
- the quote dialog and its menu item are removed;
- Copy Text no longer shows a "copied to clipboard" toast (tdesktop shows
  none for copying text either).

Not covered: tdesktop's "Translate Selected Text", and copy restriction
for protected chats. Quill doesn't track `has_protected_content` yet.

Verified live: selection → right-click → both items; Copy Selected Text
copies the selection; Quote & Reply shows "Reply to Mom" quoting it.
