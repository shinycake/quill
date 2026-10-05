# codex/contacts-presence — online state in the contacts list

- `with_presence_dot` (chat_row.rs) is the shared avatar + green "online" dot. The chat list
  (#357) now uses it, and so do contacts rows (10 px dot on 32 px avatars).
- Online contacts' status line ("online") uses the accent color; other statuses stay muted.
