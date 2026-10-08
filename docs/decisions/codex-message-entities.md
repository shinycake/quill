## Message text entities and links (gap-audit batch 1)

- **Scope:** every interactive TDLib `textEntityType` Quill used to drop, hidden-link confirmation, link context menu and hover feedback. Reference: tdesktop `core/click_handler_types.cpp`, lib_ui `basic_click_handlers.cpp`, `history_view_context_menu.cpp`.
- **Parsed and clickable** (`TextEntityKind` / `LinkTarget`, carried on `TextRun::link`):
  - `@mention` resolves through the existing deep-link flow (`https://t.me/<name>`); mention-name opens the profile.
  - `#hashtag` / `$cashtag` search: in-chat for groups and channels, global from a private chat (tdesktop `SearchByHashtag`).
  - `/command` is sent to the chat, adding `@bot` in a group when the sender is a bot (`Bot::WrapCommandInChat`). Sending is direct, as in tdesktop; nothing is inserted into the composer.
  - Email opens `mailto:` after a strict address check (`platform::mailto_url`).
  - Phone, bank card and date-time open a one-row copy menu (tdesktop shows a richer menu, see below).
  - Media timestamp seeks the message's own voice/audio/video, else the replied-to one. Voice/audio set `playback_positions`; video sets `pending_viewer_seek` consumed by the viewer.
  - Bare `example.com` URL entities open over `https://` (they were not openable before).
  - Inline code copies on click; `pre` blocks show the language and a hover copy button.
- **Hidden-link confirmation** (`link_policy`): a `textUrl` whose label is not the address asks "Open this link?", except for Telegram's own hosts (`telegram.org`, `telegra.ph`, `graph.org`, `fragment.com`, `telesco.pe`); `t.me`-family hidden links still ask. Any URL whose domain mixes alphabets asks and highlights the odd characters; invisible ones are spelled `<U+200B>`. Script detection is a coarse block table, not Qt's `QChar::script`.
- **Right-click on a link** adds "Open Link" (web) and the copy entry (Copy Link / Username / Hashtag / Email Address / Phone Number / Card Number) to the normal message menu. The text element records the press in the capture phase; the menu picks it up when opened at the same point.
- **Hover:** links underline while hovered (painted by `SelectableRichText`, so the bidi layout is untouched); a hidden `textUrl` and phone numbers show a tooltip after 450 ms.
- **RTL/selection:** ranges map through the same layout as before; selection and copy are unchanged.
- **Not done (left for later):** tdesktop's phone menu actions "Add to contacts" and "open chat" (`resolvePhone`), bank-card info lookup (`getBankCardInfo`), date-time "Add to calendar" / "Set reminder", `mentionName` tooltip with the user's real name, and tooltip for links inside the custom-emoji-only path. Chat-list previews ignore the new entities.
- **Capture note:** `--screenshot-demo ready-text-entities` renders light in both theme settings (renderer limitation); the fixture message now carries every entity type plus an RTL line.
