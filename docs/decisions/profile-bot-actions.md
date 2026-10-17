# Bot and channel action rows in the info panel

## tdesktop

`info/profile/info_profile_actions.cpp`:

- `addBotCommandActions` (:3006-3071): Bot Help and Bot Settings show only when
  the bot lists `/help` or `/settings`, and send that command to the bot's
  chat. Bot Privacy Policy opens `privacy_policy_url`, else sends `/privacy`
  when the bot has it, else opens the default policy page.
- `addReportAction` (:3075), `addBlockAction` (:3090-3141): for a bot the block
  row reads "Stop and block bot" and blocks at once; a blocked bot shows
  "Restart bot", which unblocks, starts the bot and shows its chat.
- `fillUserActions` (:3181-3199): bot rows, a skip, Report, then Block.
- `fillChannelActions` (:3202-3210): Join Channel, Leave channel, and Report
  unless the viewer created the channel. Megagroups get none of these.

## Quill before

The user panel always showed the person wording for Block and had no
Help, Settings, Privacy or Report rows. A channel the viewer had not joined
had no Join row in the info panel, and no Report row.

## What changed

- `quill::bot_profile` holds the pure rules (command lookup without case,
  privacy target order) with unit tests.
- `ui/profile_panels/bot_rows.rs` renders the rows with the existing flows:
  `open_message_report` (the chat header menu's Report), `join_channel`,
  `unblock_peer` plus the bot start, and the composer send path.
- Block row wording and one-tap behaviour for bots in `user_info_panel.rs`.
- Order: bot rows and Report follow Quill's existing bot actions; Join comes
  before the channel's management section (which holds Leave) and Report after.

## Known gaps

A bot with no listed private chat: Help, Settings and `/privacy` open the chat
first and need a second tap (tdesktop sends to a history it can create
silently). Report on such a bot opens the chat instead.

## Verification

Demos `ready-info-bot` and `ready-info-channel-left` in light and dark.
