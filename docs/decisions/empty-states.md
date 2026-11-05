# Empty, loading and failed states

## What Telegram Desktop does

`Dialogs::InnerWidget::refreshEmpty` (dialogs/dialogs_inner_widget.cpp)
centers a picture and one line in an empty chat list. An account with no
chats gets "You have no conversations yet." and a New Message button that
opens the contacts. An empty folder gets "Edit folder". Loading shows text.
The connecting widget (`Window::ConnectionState`) spins while it reconnects.

## What Quill does now

- `src/ui/empty_state.rs` (`EmptyState`) is the one look for a list or panel
  with nothing to show: a Lucide icon in a soft round tile, or a spinner while
  it loads, then a title, muted hint lines and an optional kit `Button`. The
  emoji pictures (💬 📁 🔕 📦 ⏳ ⚠ and the shared media tab glyphs) are gone,
  so the empty states match the rest of the chrome on every platform.
- Chat list:
  - No chats: a New Message button, which opens Contacts.
  - Empty folder: an Edit folder button.
  - Unread with nothing unread: a double check.
  - Archive: the archive icon.
  - Community: a spinner while it loads.
- Shared media: each tab's empty state uses its own icon. Loading spins. A
  failed load shows a warning icon, the error and a kit Retry button instead
  of an accent text link. A search that finds nothing shows a search icon.
  Profile media tiles without a thumbnail use the same tab icons.
- The connecting strip under the title bar shows a small spinner instead of
  a static amber dot.

## Verified

- Demos, light and dark, before and after:
  - `ready-chat-list-empty` with `QUILL_DEMO_CHAT_LIST_EMPTY` set to none,
    folder, unread, archive or community.
  - `ready-shared-media` with `QUILL_DEMO_SHARED_MEDIA_STATE` set to empty,
    loading or failed.
- Not checked against a live account. The connecting strip was not captured.
