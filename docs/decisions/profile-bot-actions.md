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

## Bots without a chat

Opening a bot's panel runs `createPrivateChat` quietly (`ensure_private_chat`,
purpose `EnsurePrivateChat`: the chat joins the session but is not opened), so
Help, Settings, `/privacy`, Report and Restart work on the first tap, as in
tdesktop. Report is drawn in red right before Block, and both Report rows show
only when TDLib says `can_be_reported`. Join shows for any non-member status
(left or banned). Sending reuses `send_command_text` (extracted from the
entity-link command send) and `open_user_chat`.

Not covered: tdesktop's support-account and verify-codes exclusions (Quill has
no such flags).

## Verification

Demos `ready-info-bot` and `ready-info-channel-left` in light and dark, and
the UI tests in `bot_rows_tests.rs` (row order, Join on an unjoined channel).
