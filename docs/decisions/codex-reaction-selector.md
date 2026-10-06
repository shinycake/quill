# Full reaction selector (Telegram Desktop parity)

Owner: "not all options appear, no search etc. I want it exactly the
same." Quill's expanded reactions were a fixed grid of the available
emoji plus recent custom emoji: no search, no custom packs.

## Telegram Desktop
`HistoryView::Reactions::Selector`, when the chat allows custom emoji
(`customAllowed`), expands into an `EmojiListWidget` in `FullReactions`
mode:
- a search field (emoji keyword search),
- one section with every available reaction (top, recent, popular),
- every installed custom emoji pack,
- a footer of pack icons.
Without custom emoji it stays the plain reaction grid (`RecentReactions`).
Non-Premium accounts see the packs; choosing one prompts for Premium.

## Decision
- The composer's emoji panel gains a reaction mode (`ReactionTarget`):
  the same virtualized sections, footer and animation, built by
  `build_reaction_rows` (available reactions, then the custom packs).
- The search filters standard emoji by catalog keyword and custom emoji
  by their associated emoji; packs share `push_custom_packs` with the
  composer panel.
- In reaction mode the panel opens where the message menu was (window
  overlay with a click-away backdrop), titled "Reactions", with its own
  search field (`reaction_search_input`), cleared and focused on open.
- A cell click toggles the reaction and closes. Premium gating for custom
  emoji stays in `toggle_reaction`.
- The chevron opens the selector when custom emoji are allowed, else the
  inline grid, as in tdesktop.

Verified live: the selector opens from the menu chevron; "fire" filters
reactions and pack emoji; a custom-emoji reaction was added to and
removed from a Saved Messages sticker.
