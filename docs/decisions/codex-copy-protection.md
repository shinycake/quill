# Copy protection (tdesktop)

TDLib marks chats whose content can't be saved, forwarded or copied with
`chat.has_protected_content` (schema 1.8.67, line 3598; changes arrive as
`updateChatHasProtectedContent`, line 10582). tdesktop
(`HistoryInner::hasCopyRestriction`, `!peer->allowsForwarding()`) then:
- leaves out copy and forward actions;
- refuses ⌘C on selected text with a toast: "Sorry, copying from this
  group/channel is disabled by admins." or "…from this chat is
  restricted.";
- hides Save and Share in the media viewer.

Quill didn't track the flag. With message text now selectable, ⌘C would
copy out of protected chats. Now:
- the session keeps `protected_chats` from the chat and the update;
- ⌘C on a selection in a protected chat shows tdesktop's message instead
  of copying (`refuse_protected_copy`);
- the message menu drops Copy and Forward without waiting for TDLib's
  per-message properties;
- the viewer hides Share and Save, and their handlers refuse too.

Per-message restrictions (TDLib `can_be_saved` / tdesktop
`forbidsForward`, e.g. self-destructing media) are still not handled
beyond what `getMessageProperties` gates in the menu.

Covered by a state test (chat flag + update); copy in an unprotected
chat verified live.
