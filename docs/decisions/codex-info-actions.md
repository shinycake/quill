# codex/info-actions — Group/Channel info panel actions

- Every management row now has a leading icon: edit title/description/photo,
  members/subscribers, permissions, invite link, username, broadcast upgrade, topics, welcome
  message, leave and delete.
- On/off settings (approve new members, sign messages, show authors, aggressive anti-spam) show
  a trailing kit `Switch` reflecting the state instead of a "✓ " text prefix. The row stays the
  click target and carries `aria-selected`.
- The welcome message row shows "On" when one is configured.
- Leave and Delete are destructive (danger color). Delete reads "Delete channel" for channels.
- The public username entry is an action row like its neighbors (was a ghost button).
- Twelve Lucide icons are embedded.
