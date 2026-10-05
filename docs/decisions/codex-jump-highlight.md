# codex/jump-highlight — row states as tints, not outlines

- The search/reply jump target, rows selected for forwarding, and failed sends used to get a
  2 px/1 px border plus horizontal padding around the whole row. The row shifted when the state
  flipped, and the outline boxed in the empty space beside the bubble.
- They now tint the row background, with no border or padding change: primary at 12% (jump
  target), the theme selection color (forward selection), danger at 8% (failed send).
