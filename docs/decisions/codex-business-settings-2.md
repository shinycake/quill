# Telegram Business settings (part 2: start page, links, chatbots, ads)

Completes `parity:business-away-greeting` after part 1
(`codex-business-settings.md`, PR #687). This PR adds the start page,
links to chat, chatbots and the hub's "Ads in Channels" switch. It also
removes two deviations from part 1: screens now save when they are left,
and the hub uses tdesktop's icons.

## What tdesktop does

- `settings/business/settings_chat_intro.cpp` (Custom Intro, title "Start
  Page"): a live preview of the empty-chat intro, "Enter Title" (32
  characters by default) and "Enter Message" (70) with length limits,
  "Choose Sticker" (a sticker panel; "Random" plus a random hello sticker
  while none is chosen), the about text and "Reset to Default".
- `history/view/history_view_about_view.cpp`: an empty chat with a user
  who set a start page shows its title, message and sticker (clicking the
  sticker sends it) under "{name} added the message below for all empty
  chats. How?". "How?" opens the start page settings, or Premium without
  it.
- `settings/business/settings_chat_links.cpp`: "Create a Link to Chat"
  (hidden at the limit of 100 links), rows with the title (or the link
  without `https://`), the message and "N clicks" / "no clicks". A row
  opens a menu with Copy, Share, Get QR Code, Edit and Delete (confirmed:
  "Are you sure you want to delete this chat link?"). The box has the
  message (with formatting and an emoji panel) and "Link Name
  (optional)". The footer offers `t.me/username` and `t.me/+phone`, which
  copy on click.
- `settings/business/settings_chatbots.cpp` (title "Chat Automation"): a
  username field (`@name`, `name` or a `t.me` link) looks bots up after
  400 ms: the exact username, then the account's own and global search
  results, at most three bots that support Business ("Chatbot not
  found.", "This bot doesn't support Telegram Business yet."). "Add"
  connects a bot ("{bot} now manages your account."). The recipients
  selector also offers exclusions in "Only Selected Chats" (chats only).
  The permission groups come from `ChatbotsPermissionsLabels`, with "Read
  Messages" always on. Turning on gift, star or username rights shows a
  warning box. "Remove Bot" disconnects. Leaving with found but not added
  bots asks "No Bot Added". Chatbots is the one hub row that works
  without Premium.
- `settings/sections/settings_business.cpp`: for Premium accounts the hub
  ends with "Ads in Channels" and the "Do Not Hide Ads" switch.
- Every section saves in its destructor, so leaving it (back, closing
  settings) saves.

## What changed

- `src/business_start_page.rs`, `src/business_chat_links.rs` and
  `src/business_bots.rs`: parsing (`businessStartPage`,
  `businessChatLink(s)`, `businessConnectedBotInfo`, `businessBotRights`,
  bot recipients with `excluded_chat_ids`), request bodies
  (`setBusinessStartPage`, `get/create/edit/deleteBusinessChatLink`,
  `get/set/deleteBusinessConnectedBot`, `searchPublicChat`,
  `searchChatsOnServer` and `searchPublicChats` with the bot filter),
  tdesktop's username parsing, bot merging, rights groups and warnings.
  A link's message keeps its formatting through composer markup.
- `src/business_settings.rs`: `toggleHasSponsoredMessagesEnabled`; the
  start page joins `BusinessSettingKind`.
- `BusinessInfo` carries the start page; `ParsedUser` gains
  `can_connect_to_business`; `UserProfileExtras` gains
  `sponsored_enabled` (`has_sponsored_messages_enabled`).
- Settings domain: `BusinessExtraState` (links, connected bot, lookup,
  the TDLib limits `business_start_page_title_length_max`,
  `business_start_page_message_length_max` and
  `business_chat_link_count_max`), six purposes and three payloads. The
  lookup answers are routed by purpose from the existing `chat` and
  `chats` handlers. A missing bot is TDLib's 404.
- `src/connect/business_extra.rs`: driver methods.
- `src/ui/business/`: `start_page.rs`, `chat_links.rs`, `chatbots.rs`,
  `intro_view.rs` and `state.rs`. Picking a start page sticker reuses the
  sticker panel (a click is routed in `send_sticker_pick`). The empty
  private chat shows a set start page before the plain greeting
  (`greeting_intro`). The QR code reuses `render_qr_image`.
- Save on leave: Greeting, Away, Business Hours, Location, Start Page and
  Chat Automation save on Back, Close, Esc, the close button and the
  backdrop. Boxes keep their buttons, as in tdesktop (the weekday editor,
  the chats picker and the link box have Cancel and Save or Done).
- `src/main.rs`: Zap, MapPin, Link and MessageSquareHeart are added to the
  bundled icons for the hub rows.

## Same as tdesktop

- The hub's eight rows in fallback order and their wording, the Premium
  gate (except Chatbots), and the ads switch for Premium accounts.
- Start page limits, defaults, preview, random sticker and reset.
- Link list wording, limit, delete confirmation, QR code, and copying the
  plain links.
- Bot lookup order and limit, the "not found" / "not supported" lines,
  rights groups, "Read Messages" always on, the warnings, "Remove Bot" and
  "No Bot Added".
- Nothing is sent when a screen did not change.

## Differences and gaps

- A greeting or away message switched on without messages in its shortcut
  is not sent; tdesktop keeps the switch locally without saving. Quill
  shows "The greeting message stays off until it has messages." on the
  hub instead of losing the choice silently.
- An away custom range that ends before it starts blocks leaving the
  screen. tdesktop's pickers cannot produce such a range.
- If Esc or the close button closes the dialog while a screen has a
  problem, the problem goes to the status line, because the dialog is
  already closed.
- The link row's menu has no Share item: Quill has no share box for
  plain links yet. The link box has no emoji panel; formatting typed as
  markup (`**bold**`) is kept.
- tdesktop empties the included chats when a bot's exclusions are saved
  in "Only Selected Chats", which makes exclusions in that mode
  impossible. Quill keeps the included chats; TDLib documents
  `excluded_chat_ids` for exactly this case.
- The start page sticker panel is Quill's sticker panel (installed,
  recent, favorites, trending, search), not tdesktop's ChatIntro tabbed
  selector.
- Confirmations ("No Bot Added", delete link, warnings) are inline boxes
  in the dialog, not separate modal boxes.
- From part 1, still true: time fields instead of drum pickers (the kit
  has none) and no map picker.

## Verified

- Unit tests: `business_start_page`, `business_chat_links` (parsing,
  formatting round trip, requests, footer links), `business_bots`
  (parsing, request round trip, recipients for bots, picker rules,
  rights and warnings, username parsing, merging, lookup requests).
- Driver tests `connect::tests::business_extra`: links load, create, edit
  and delete; no bot is a 404; a bot save and refetch; the three-part
  lookup with an error; the start page limits from `updateOption`; the
  ads switch.
- 12 GPUI UI tests in `src/ui/business/tests.rs` (`--features
  demo-capture`): every screen renders, the Premium gate with Chatbots
  open, quick replies above the dialog, the limit, picking chats, the day
  editor, save on leave in demo mode, away checks, the start page sticker
  route and title limit, the link editor, the bot lookup with "No Bot
  Added", the warning and chats-only exclusions, and the start page in an
  empty chat.
- Visual check of the hub, start page, links, link editor, QR code and
  chatbots with a temporary demo (not committed).
- gate.sh, `scripts/check-hotspots.sh origin/main`,
  `scripts/check-file-size.sh`.
- Not run against a live account: these screens change real business
  settings and links.
