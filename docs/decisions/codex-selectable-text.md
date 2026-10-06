# Selectable message text (tdesktop)

tdesktop (`history_inner_widget.cpp`) lets you drag-select text inside a
message and copy it with ⌘C. Copy takes the selection (`_selectedText` of
`_selectedTextItem`). The selection never leaves its message: dragging
onto another message switches to selecting whole messages
(`_dragSelFrom`/`_dragSelTo`), which is a separate mode. Quill's message
text wasn't selectable at all.

**Element.** `SelectableRichText` (`ui/selectable_text.rs`) is message
text that takes part in gpui-base's window text selection, with the root
`TextSelectionLayer` mounted:
- it lays out the styled run (bold, links, spoilers…), paints its own
  selection quads, and keeps link/spoiler clicks (a press that doesn't
  drag);
- ⌘C copies the selection, minus the em-space padding reserved for the
  time footer;
- paragraphs are ordered by message id, then paragraph, then caption.

**Staying inside one message.** gpui-base scopes can't express this:
gpui-kit's `Root` resets the active scope to the dialog layer on every
render. Each paragraph instead records its message. When the selection's
anchor belongs to another message, the paragraph projects no runs, so it
neither highlights nor copies. Dragging past a message selects to its edge
and no further, as in tdesktop. Whole-message drag selection is not
implemented yet.

**Two fixes needed for drags to work.**
- *Register in paint, not prepaint.* The history list can prepaint a row
  more than once and roll back the earlier passes. A registration left
  over from a rolled-back pass carried stale bounds, and the drag cursor
  flipped to the wrong paragraph.
- *Selection viewport.* Drag auto-scroll measures the pointer against the
  hitbox's content mask, and a bubble's clip made that mask the bubble.
  Dragging out of the bubble scrolled the history. `SelectionViewport`
  wraps the history and hands its bounds to the registrations instead.

Verified live: partial selection and copy within a message, a drag across
messages stopping at the first message's end, and upward drags without
scroll jumps.
