## Slice: msg-blockquote-expandable (2026-09-30)

- **Scope:** Expandable block quotes — long block quotes collapse with an
  expand affordance (authoring covered by `parity:msg-quote-block`).
- **What was built:**
  - Envelope parses `textEntityTypeBlockQuote` /
    `textEntityTypeExpandableBlockQuote` into
    `TextEntityKind::{BlockQuote, ExpandableBlockQuote}` (previously dropped
    silently).
  - Quote groups render as one `quote_block` (accent bar, same visual
    language as `reply_quote_strip`).
  - Quotes longer than `QUOTE_COLLAPSE_LINES` (3, per TDLib docs —
    `schema/td_api.tl:5770` "collapsed by default to 3 lines") collapse with
    a kit ghost Button "Show more"/"Show less" toggling in place.
  - Quotes ≤ 3 lines render fully with no affordance; nested styles are
    preserved on the visible portion.
  - Expansion state reuses the spoiler `revealed` set (documented;
    high-bit quote keys keep spoiler visibility independent).
  - README box `parity:msg-blockquote-expandable` declared via
    `parity-fragments/parity-msg-blockquote-expandable.txt` (merge pipeline
    checks the box after merge).
- **Key decisions (ponytail):**
  - Collapse threshold is 3 visible lines — TDLib documents
    `textEntityTypeExpandableBlockQuote` as "collapsed by default to 3
    lines with the ability to show full text"; long plain
    `textEntityTypeBlockQuote`s collapse the same way so both render
    identically.
  - Expansion state reuses the spoiler `revealed` set + key scheme
    (chat, message, first-run index, caption?) instead of threading a
    second set through every history call site — same tap-to-reveal
    interaction pattern as spoilers.
  - Quote styling reuses the `reply_quote_strip` bar (`border_l_2` +
    `accent()`); the affordance is a kit ghost `Button`
    ("Show more"/"Show less"), same as "Load full message".
- **Out of this slice:** sender-authored
  `textEntityTypeExpandableBlockQuote` from the composer (composer only
  emits `textEntityTypeBlockQuote`).
