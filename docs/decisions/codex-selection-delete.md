# Delete a message selection (tdesktop)

tdesktop's selection bar has "Delete N". Its `DeleteMessagesBox` asks
"Do you want to delete N messages?" (or "…this message?") and, when
every message can be deleted for everyone, offers "Delete for everyone"
or "Also delete for {name}" in private chats, checked by default.

Now:
- the selection bar has Delete. A gpui-kit alert asks tdesktop's question
  and, when every selected message is yours and the chat isn't Saved
  Messages, shows the checkbox (on by default);
- `ConnectDriver::delete_selected` sends one `deleteMessages` for the
  selection. Every message must be loaded and deletable; `revoke` is
  honored only when they're all yours (driver test);
- confirming clears the selection.

Per-message revoke limits (`can_be_deleted_for_all_users`) aren't known
without `getMessageProperties`. Own messages are treated as revocable,
and TDLib rejects the ones that aren't.

Verified live up to the dialog: the dialog shows and Cancel leaves
everything in place. Nothing was deleted.
