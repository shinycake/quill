# Message menu and reply bar parity (tdesktop)

Compared live on the same message in both apps.

**Menu.** tdesktop's message menu (`history_view_context_menu.cpp`):
- left-aligned rows, each with an icon;
- order Reply, Edit, Pin, Copy Text, Copy Link, Forward, Delete, Select;
- the reaction strip floats as its own pill above the menu.

Quill had centered text buttons in its own order, with the strip inside
the panel. Now:
- items carry an icon and a position and are sorted into tdesktop's
  order; Delete is drawn in the danger color;
- labels follow tdesktop ("Copy Text", "Copy Message Link", "Reply with
  Quote", "View Comments", "Stop Poll", "Resend");
- the strip renders in a separate rounded pill above the menu.

Quill keeps "Reply with Quote" in the menu: tdesktop reaches it through a
text selection, which Quill doesn't have yet.

**Reply bar.** tdesktop titles it "Reply to {sender}" and renders the
replied message (custom emoji included). Quill said "Reply" over plain
text with box glyphs. The bar now names the sender and renders text
previews through the chat-list preview line with custom emoji images
(`custom_emoji_images`).

Verified live.
