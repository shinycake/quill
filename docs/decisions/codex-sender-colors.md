# Group sender colors; message actions beside the bubble

`user.accent_color_id` (schema 1.8.67, line 2383) is now parsed into `ParsedUser`. Group message sender names render semibold in the sender's Telegram name color: the seven built-ins (red, orange, violet, green, cyan, blue, pink), with separate light and dark values for legibility. Server palette ids (7+) fold onto the built-ins. Mapping them through `updateAccentColors.built_in_accent_color_id` is a follow-up. Chat senders (anonymous admins, channels posting in groups) get a color derived from the chat id. Rows carry the sender as `SenderLabel { name, accent }`.

The hover-revealed message actions button sat inside the bubble's top-right corner and covered the time on one-line messages. It now sits just outside the bubble's side edge (right of incoming, left of outgoing). It uses the kit's bubble-anchored reaction region, restyled to a bare container, because that slot isn't clipped by the bubble surface. Wrapping the bubble in an extra row broke the kit's end alignment for outgoing messages, so that approach was dropped.

The slow-mode demo fixture now defines its two group members (names and accent ids).
