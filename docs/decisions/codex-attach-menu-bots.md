# Attachment menu bots in the attach menu

Item: `parity:composer-attach-bots`.

## What tdesktop does

`inline_bots/bot_attach_web_view.cpp` (`AttachWebView`):

- `requestBots` loads the list; `ParseAttachBot` keeps a bot only if it is a
  bot that supports the attachment menu, and saves its `default_static`
  icon (`ResolveIcon`). `PeerMatchesTypes` decides per chat: private chat
  with the bot itself (`SameBot`), another bot, a user, a group, a channel.
- A bot that is not added yet (`inactive`) goes through `confirmAddToMenu`:
  the bot's permission text, a checkbox when the bot wants to message the
  person, and the mini-app terms box (`FillDisclaimerBox`) until the person
  agreed once. `toggleBotIsAddedToAttachmentMenu` follows, with a toast.
- Removing happens from the open mini app's menu ("Remove from menu"), after
  a confirmation box (`lng_bot_remove_from_menu_sure`), then a toast.
- Opening from the attach menu calls `openWebApp` with the chat as context.

## What Quill already had

The list (`updateAttachmentMenuBots`), the chat-kind filter, opening with the
chat context (`openWebApp`, empty url), the `internalLinkTypeAttachmentMenuBot`
deep link and the add box with the write-access checkbox. This change fills
the remaining gaps.

## What changed

- Icons: `AttachmentMenuBot.icon` keeps `default_icon`. The reducer tracks the
  file (`state/session_attach_icons.rs`), the driver downloads the icons of
  the bots shown in the menu (`connect/attach_bot_icons.rs`, one line in the
  ingest), and the paperclip menu draws the downloaded SVG
  (`ui/attach_bots.rs`, read once and cached). Without a usable icon the bot
  glyph stays.
- The chat filter moved into `ui/attach_bots.rs` as a pure function so it can
  be tested.
- Add box: shows the mini-app terms paragraph and link until the person
  agreed once (the same trust set as the first-open box, so accepting here
  also covers opening), and uses tdesktop's permission wording.
- Remove from menu: asks first (`WebAppConfirm::RemoveFromMenu`), like
  tdesktop; the toast already existed.

Not done on purpose: the side menu variant (`show_in_side_menu`), Quill has
no main-menu bot entries.

## Verification

- Unit tests: parse of `default_icon`; chat-kind filtering (user, group,
  channel, self, the bot's own chat, not-added and hidden bots); SVG
  detection; driver test that only the attach-menu bot's icon is downloaded
  and resolves to its path once `updateFile` lands; `openWebApp` and
  `toggleBotIsAddedToAttachmentMenu` request JSON.
- `gate.sh`, `check-hotspots.sh`, `check-file-size.sh`.
- No real bot was added, removed or opened.
