# Delete dissolve no longer rebuilds the rows every frame

tdesktop (`ThanosEffect`) animates the removed message's row in place. Quill
did that by changing the history rows key every frame, so every row was built
again each frame of the dissolve.

What changed:
- The rows key carries each dissolving message's phase (waiting, in rows, dust,
  done), not the clock. The row reads its progress at render.
- Reading the dissolves is fail closed: if their cell is borrowed, the key gets
  a sentinel, so the rows are built whole and the splice stands down.
- While a gap closes, that one row is remeasured each frame
  (`remeasure_items`), so the rows below follow it up instead of jumping at the
  dust phase. Nothing else is rebuilt.
- The key and the build judge the phases at one instant per round of rows work.

Verified: splice tests (key stable across frames, one rebuild at the phase
change, borrow conflict -> sentinel and no splice); demo stills across the
collapse show the rows below moving up in steps (47 px, then 66 px) rather than
at the end.
