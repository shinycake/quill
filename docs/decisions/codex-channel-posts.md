# codex/channel-posts — channel post footer and subscriber bar

- Channel posts (and signed messages) put the author signature, the view count (eye icon,
  compact "12.4K") and the time/receipt on one footer row (`message_footer_meta`). They used
  to sit on two separate lines with a "👁" emoji. Views and signature still mark the bubble tail
  as non-empty, so the footer keeps its own row and never overlaps text that reserved space
  for an inline time.
- The channel bar for non-members is a full-width primary "Join channel" (no helper sentence).
  Subscribers get a full-width "Mute"/"Unmute" toggle (`apply_chat_mute`). Leaving moves to the
  info panel's existing "Leave channel" row, which asks for confirmation instead of leaving in
  one click. Admin, creator, banned and unknown states keep their notes, now centered.
- `Eye` is embedded.
