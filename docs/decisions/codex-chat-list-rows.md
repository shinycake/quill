# Chat list rows and sidebar chrome

Rows now carry what a user scans for: the last message's local time (or a weekday or date via `local_time::chat_list_stamp`), an outgoing receipt (✓ sent / ✓✓ read against `last_read_outbox_message_id`), and the unread counter at the row's trailing edge instead of overlapping the avatar. Muted chats use a neutral counter; "marked as unread" chats get a dot. Pinned chats with nothing unread show a pin glyph. Muted, secret and forum markers are small glyphs or an outlined tag instead of filled pills. Avatars are 46px and rows are 64px (88px with folder tags).

The core gains `ChatSummary::last_message` (id, date, direction), set from `updateChatLastMessage`, and `last_message_receipt()`.

Sidebar: the always-on "Chats · N" caption only appears in search ("Recent" / "Search results") and while connecting. The empty story tray no longer renders a large "Create a story" button; "New story" moved to the main menu. The window title is "Quill" (no "— chats (demo)"), and the developer "Load older messages" title-bar button is gone. History pages automatically and the keyboard shortcut remains.

Out of scope: horizontally scrolling folder tabs, avatar colors keyed by Telegram's peer color (the kit hashes initials).
