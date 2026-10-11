# Owned bots: edit screen and "Create Bot" (codex/owned-bots)

Parity item `parity:bots-owned-manage`, "Owned bots management and create a
bot".

## What tdesktop does

- There is no "My Bots" list. tdesktop never asks for the bots a person
  owns (no owned-bots call anywhere in `SourceFiles`), and no menu or
  settings entry lists them. TDLib has `getOwnedBots`, but tdesktop does not
  use it, so Quill does not add a list either.
- A bot with `botInfo->canEditInformation` (`userTypeBot.can_be_edited`) gets
  the group-style edit box (`boxes/peers/edit_peer_info_box.cpp`, `_isBot`
  branches): photo (`photos.uploadProfilePhoto` with `bot`), name
  (`bots.setBotInfo` with `f_name`) and "about" (`bots.setBotInfo` with
  `f_about`, which TDLib calls the short description). The manage section
  adds "Edit Intro", "Edit Commands" and "Change Bot Settings", each of which
  resolves @BotFather and starts it with `<username>-intro`,
  `<username>-commands` or `<username>` (`toggleBotManager`), then shows the
  BotFather chat. Usernames, balances, the affiliate program and "Verify
  Accounts" (done in `codex-bot-verification.md`) sit in the same section.
- "Create a bot" is not a menu item. It is reached from
  `tg://newbot?manager=&username=&name=` (`core/deep_links/deep_links_new.cpp`,
  TDLib `internalLinkTypeRequestManagedBot`): resolve the manager bot, refuse
  with "{bot} doesn't have Bot Management Mode enabled." unless it has
  `can_manage_bots`, else `CreateManagedBotBox`. The box
  (`boxes/peers/create_managed_bot_box.cpp`) has a name field and a username
  field with the ending "bot" appended for you (no more than 32 characters,
  at least 5, only letters, digits and underscores), a check of the username
  after a pause, and `bots.createBot`. On success tdesktop opens the new bot's
  chat and shows "{name} created!". `BOT_CREATE_LIMIT_EXCEEDED` shows the
  limit with a link to BotFather's delete flow, and for non-Premium accounts a
  Premium upsell.

## What changed

- `src/owned_bots.rs` (new): the rules and request bodies. Username ending
  and length rules, the BotFather start parameters, which fields changed,
  and JSON for `getBotName`, `getBotInfoShortDescription`, `setBotName`,
  `setBotInfoShortDescription`, `setBotProfilePhoto`, `checkBotUsername` and
  `createBot`.
- `ParsedUser` keeps `userTypeBot.can_be_edited` and `can_manage_bots`.
- Bots domain: new request purposes, `src/state/domains/bots/owned.rs` with
  the edit texts and the create-dialog state, and the answers wired through
  the users (`user`, `checkChatUsernameResult*`), common (`text`) and `ok`
  paths. A late username verdict is dropped by generation.
- `src/connect/owned_bots.rs`: driver calls for the above.
- Deep link: `internalLinkTypeRequestManagedBot` becomes
  `DeepLinkAction::CreateManagedBot`, resolved like the other bot links.
- UI: `ui/bot_edit_dialog.rs` (an "Edit bot" row on the profile of a bot you
  own, with name, about, photo and the three BotFather rows) and
  `ui/bot_create_dialog.rs` (the Create dialog).
- Hotspot: none beyond `mod` lines.

## Differences from tdesktop

- The BotFather rows open the chat with the start parameter pending, so the
  person presses Start. tdesktop sends it at once. Quill never sends a message
  without a press.
- The about text limit is 255 like tdesktop's field; the server decides.
- A failed `createBot` shows the error code and a hint to delete a bot with
  @BotFather. Quill does not keep the server's message, so it cannot tell the
  limit error from others, and there is no Premium upsell.
- The edit screen has no usernames, balance, affiliate or community rows.
  Those are separate parity items.
- The `requestManagedBot` keyboard button is still unsupported (as before).

## How verified

- Unit tests: username rules and request JSON (`owned_bots::tests`), driver
  tests for loading, saving only changed fields, a refused save, the photo
  request, stale username verdicts, `createBot` success and failure, and the
  bot type flags (`connect::tests::owned_bots`), and the link route
  (`deep_link_types::web_app_link_tests`).
- `gate.sh` passes.
- Not verified: the dialogs on screen and everything against a live account.
  No bot was edited or created, and nothing was sent to @BotFather.
