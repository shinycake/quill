# Bot command menu clicks (and other composer popups)

Clicking a command in the bot menu just closed the menu. The composer's
`Blur` event closes the command, mention and inline-result popups, and
pressing a row moved focus off the composer. So the popup was gone
before the click completed.

Telegram Desktop sends a clicked bot command right away (Tab or Enter
insert it). Now:
- command rows act on mouse-down: `send_command_menu_index` drops the
  `/`-token, keeps any other draft text, and sends `/command`;
- mention rows and inline-bot results also pick on mouse-down;
- Enter and Tab keep inserting the highlighted command.

Verified on the bot command-menu demo: clicking "/help" sends it and the
menu closes. Not exercised in a live bot chat, since that would send a
message to a third party.
