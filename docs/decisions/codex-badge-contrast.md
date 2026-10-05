# codex/badge-contrast — readable timer badges on outgoing bubbles

- The self-destruct ("⏱ view once") and auto-delete badges used warning orange everywhere,
  which is unreadable on the accent-filled outgoing bubble. Outgoing badges now inherit the
  bubble's text color at 80% opacity, like the time footer. Incoming badges keep warning orange.
- On media-led bubbles the badges get the caption's horizontal padding instead of sitting
  against the bubble edge.
