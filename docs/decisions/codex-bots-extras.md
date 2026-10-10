# Bots extras: fast buttons, add a bot to a chat, share a game

Cluster `bots`. Four items landed in full; five were left alone on purpose (see the end).

## Fast buttons (`bots-fast-buttons`)

In tdesktop: an experimental switch ("Fast buttons mode" in a bot's profile, `FastButtonsBots`) stores a set of bot ids. For those bots, while the field is empty, the keys 1 to 9 without modifiers press the n-th inline button of the history's last message (`HistoryWidget::setupFastButtonMode`, `ReplyKeyboard::getLinkByIndex`). Buttons count left to right, top to bottom. The same message draws the digit on each button (`ReplyKeyboard::hasFastButtonMode`).

In Quill before: inline buttons could only be clicked.

Now:
- `src/fast_buttons.rs` keeps the set in `fast_buttons.json` under the app data root and holds the pure rules: which key means which index, how a flat index maps to a row and column (empty rows add nothing), and which buttons get a digit.
- `Session::fast_button_target` picks the newest loaded message of a bot chat when it carries an inline keyboard and the loaded window reaches the end of the chat.
- The bot's profile panel has a "Fast buttons mode" switch. When it is on, the last message shows digits in the corner of its first nine buttons.
- A keystroke interceptor (the same mechanism as Up-to-edit) presses the button when the composer is focused and empty, no edit or attachment is pending, and the key has no modifier. Buttons that do nothing (disabled, unknown) are skipped, so the key falls through to the field.
- Clicking and the key share one `activate_inline_button`, so every button type behaves the same either way.

## Add a bot to a group or channel (`bots-add-to-group`, `profile-add-bot-to-group`)

In tdesktop: a bot's profile shows "Add to Group", "Add to Channel" or "Add to Group or Channel" (`InviteToChatButton`) with a line saying what the bot can manage. It opens a picker (`AddBotToGroupBoxController`). Chats where the person can add admins come first, then groups where members can be added. An admin target opens the rights editor pre-filled with the rights the bot asks for (`botInfo.default_group/channel_administrator_rights`); a member target asks "Add the bot to «group»?". The `?startgroup`, `?startgroup=<token>&admin=...` and `?startchannel&admin=...` links open the same picker with a narrower scope, and a start token is sent as the bot's start message.

In Quill before: the links answered "isn't supported", and there was no bot action in the profile.

Now:
- `src/bot_invite.rs` holds the rules: label and about line, the three scopes, which chats qualify and how the bot joins each one (admin with rights, or member), the order of rows, and which rights apply to a group or a channel.
- `ParsedUser::can_join_groups` and `BotInfo::{group,channel}_admin_rights` are parsed (an all-false rights block counts as "none").
- The profile panel of a bot has the action and the about line. The dialog lists the chats, then shows the rights as checkboxes (only the ones that make sense for that chat type) or a plain confirmation. Admins are added with `setChatMemberStatus` through the existing `promote_chat_member`; members through `addChatMember(s)`. A start token goes out as `sendBotStartMessage` instead of a plain add, or right after the promotion.
- `internalLinkTypeBotStartInGroup` and `internalLinkTypeBotAddToChannel` now route to the picker. The bot is resolved with `searchPublicChat`, its chat stays closed, and the picker opens over the current view.

Differences:
- tdesktop lists member rows for a bot that cannot join groups when the scope is "all". Quill leaves them out, since Telegram would refuse.
- Channels never list member rows, in both.
- The replies bot, verify-codes bot and support accounts are not special-cased; they do not accept being added, so the action does not appear for them in practice.

## Share a game (`bots-share-game`)

In tdesktop: `t.me/<bot>?game=<name>` opens a chat picker (`ShowShareGameBox`) without Saved Messages, and without channels; picking a chat asks "Share the game with {user}?" (or the group form) and sends the game. The share box of a game message offers to copy `t.me/<bot>?game=<name>`.

In Quill before: a bot's panel could send a game it had seen, and `internalLinkTypeGame` answered "not supported".

Now:
- `src/game_share.rs` has the link, the destination filter and the confirmation wording.
- The game link resolves the bot, then opens the picker and the confirmation over the current view. Sending uses the existing `send_game_message` (`inputMessageGame`), then opens the chat.
- A game message's menu has "Copy game link".

## Not done

- `bots-miniapp-inline`: blocked, GPUI has no web view.
- `bots-allow-write` and `profile-bot-open-app`: both live in the mini app flow. The consent checkbox is part of the web view launch, and "Open App" starts that same web view. Opening the URL in the browser would not be what tdesktop does.
- `bots-apps-tab`: popular apps come from `getGrossingWebAppBots`, but tapping one launches a mini app, so the tab would list things that cannot open.
- `bots-verification-badge`: tdesktop draws the verifying organization's custom emoji next to the name and shows a description row; "verify via bot" is a separate verifier-bot flow (`setMessageSenderBotVerification`). Neither half is built.
- `bots-owned-manage`: tdesktop has no owned-bots list. It only creates a managed bot from a `newbot` link, which needs the manager-bot flow; `getOwnedBots` and `createBot` exist in the schema but there is nothing in tdesktop to follow.

## Verification

- Unit tests: key to index mapping, flat index to row and column with empty rows, digit badges (never past nine), `fast_button_target` in the reducer, scopes and rows of the add-bot picker (admin rows first, channels admin-only, link rights replacing defaults, bots that cannot join), the rights mask for groups and channels, the labels and about lines, bot-info rights parsing, `can_join_groups` parsing, the three link types routing to their actions, `searchPublicChat` requests for the new actions, the game link, the destination filter and the confirmation text.
- Demo captures with `--screenshot-demo ready-bot-extras` (`QUILL_DEMO_BOTEXTRAS=fast|add|add-rights|add-member|share-game`), all with injected English fixtures.
- Not verified against a live account: adding a bot or sending a game would act on real chats.
