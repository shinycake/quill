# Composer gaps 2: Enter sends a command, anonymous and inline-bot field hints

## Enter on the "/" menu sends the command (`bots-command-menu-enter-sends`)

tdesktop (`chat_helpers/field_autocomplete.cpp`): Enter calls
`chooseSelected(ByEnter)` (line 1066), which fires `botCommandChosen` with
`/command` plus `@username` when the list shows commands of several bots
(`commandsWithUsername`, line 1615). The handler (line 2038) inserts the tag
only for Tab; otherwise it sends the command at once and sets the field to the
text after the caret (`getTextWithTagsPart(cursor)`). Enter is ignored while
the mouse chose the row (`_mouseSelection`, line 1573). Quill's rows have no
hover selection, so that case cannot arise.

Quill used to insert the command on Enter as well as on Tab. Enter now takes
the same path as a click on a row (`send_command_menu_index`): it sends
`/cmd` or `/cmd@bot`, removes the `/` token and keeps what followed the caret.
Tab is unchanged (insert, then a space). The "Edit Quick Replies" row keeps
opening its dialog.

## Anonymous admin placeholder (`composer-placeholder-anon-inline`)

tdesktop (`history_view_compose_controls.cpp` `updateFieldPlaceholder`, line
3243): in a non-broadcast channel (supergroup) where `adminRights() &
Anonymous`, the placeholder is `lng_send_anonymous_ph`. Priority: editing,
Stars price, forum topic (Quill has no topic placeholder yet), broadcast
(silent or not), anonymous admin, plain.

`ParsedChatMember.is_anonymous` is `rights.is_anonymous` for an administrator
and `chatMemberStatusCreator.is_anonymous` for the owner; `accept_own_chat_member`
stores it on the chat, and `PlaceholderFacts.anonymous_admin` feeds the
existing priority list in `src/composer/placeholder.rs`.

## Inline bot placeholder

`updateFieldPlaceholder` (line 3247): when not editing and an inline bot with a
placeholder is resolved for the typed `@bot`, the field shows
`inlinePlaceholder` after the first `username.size() + 2` symbols
(`InputField::placeholderSkipWidth`, lib_ui `input_field.cpp:2993`, which adds
a space width while the separator is not typed yet). It disappears once the
text is longer than that. The bot must be a bot with a non-empty placeholder
(`ParseInlineBotQuery`, `message_field.cpp:911`).

TDLib: `userTypeBot.inline_query_placeholder`, parsed into
`ParsedUser.inline_query_placeholder`. The gpui-kit input only draws a
placeholder for an empty field, so the hint is a canvas over the field (the
same technique as the spell-check underlines) placed after the last typed
character. `inline_bot_hint` in `src/composer/placeholder.rs` holds the rule.

## Verified

Unit tests (`placeholder.rs`, `composer_hint_tests.rs`), GPUI UI tests
(`group_commands_tests.rs`, `bars_tests.rs`) and light/dark captures of the
`ready-composer-bars` demo (`QUILL_DEMO_BARS=anonymous|inline`).
