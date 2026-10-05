# codex/chat-nav-keys — next / previous chat shortcuts

- Adds `NextChat` / `PrevChat` actions: Alt+↓ / Alt+↑ and Ctrl+Tab / Ctrl+Shift+Tab
  (Telegram Desktop's chords), rebindable like the other shortcuts (`next-chat`, `prev-chat`).
- `step_open_chat` walks the visible chat list (current folder / archive / search filter),
  skipping headers, and wraps at the ends. With no chat open it starts at the first or last row.
- The shortcuts reference table lists the new chords so the settings overlay and the
  resolved defaults stay in sync (test count 31 → 35).
