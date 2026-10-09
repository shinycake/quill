## Translate bar server flag, channel auto-translate, reply and bot keyboards (waves B18 + B12)

### tdesktop reference

- `history/view/history_view_translate_tracker.cpp` (`setup`): the bar and the translated chat are tracked when `translateChatEnabled` is on and the account has Premium **or** the channel has `AutoTranslation`. The tracker also takes the user's own messages when the peer has auto-translation (`item->out() && !autoTranslation()` is the only skip).
- `history/view/history_view_translate_bar.cpp`: the label is "Translate to X"; once translated it reads "Show Original", or "View Original (X)" for an auto-translated channel (`lng_translate_return_original`). The menu "Hide" is `messages.togglePeerTranslations`, the server flag TDLib mirrors as `chat.is_translatable`.
- `core/core_settings.h`: `_translateChatEnabled = true` by default. The settings switch is locked without Premium, but the stored value still gates auto-translated channels.
- `boxes/peers/edit_peer_info_box.cpp`: "Auto-translate messages" switch on channels (`lng_edit_autotranslate`), saved with the other info edits; locked until the channel has enough boosts.
- `history/history_widget.cpp`: the field's keyboard button toggles the bot keyboard (`_botKeyboardShow` / `_botKeyboardHide`, `_botKeyboardHiddenId`); a new keyboard message shows it again. `setupFastButtonMode` (line ~3832) is an experimental option (`kOptionFastButtonsMode`, off by default) with a per-bot switch in the bot's profile: with an empty field, keys 1-9 press the first nine inline buttons of the last message.
- `boxes/peers/choose_peer_box.cpp`: request buttons. Users and chats are chosen from a list filtered by the button's restrictions (bot or not, forum, has username, created by me, admin rights) and confirmed with `lng_request_peer_confirm` ("Are you sure you want to send {chat} to {bot}?", Send). `lng_bot_share_phone` ("Do you want to share your phone number with this bot?", Share) confirms the phone button.
- `data/stickers`/`FieldAutocomplete`: typing a lone "@" in an empty field lists the recent inline bots (`getRecentInlineBots`).

### What changed in Quill

**B18 translate bar**
1. Hide / Undo in the bar menu now send `toggleChatIsTranslatable` (it exists in TDLib 1.8.67; the earlier doc said it did not). The flag is applied at once and restored if TDLib refuses (`RequestRollback::ChatIsTranslatable`); `updateChatIsTranslatable` stays the source of truth. The local hidden-chats list is only used by the offline demo.
2. `supergroup.has_automatic_translation` is parsed (`updateSupergroup`, `getSupergroup`) and kept in `TranslateState::auto_translate_supergroups`. `translate::tracking_enabled` is tdesktop's rule (`translate_chats && (premium || automatic)`); the bar model, the translated rows and the pump follow it. Auto-translated channels also translate the user's own posts, and the bar says "View Original (Spanish)" once translated.
3. "Auto-translate messages" switch in the channel's Manage section, for creators and admins with `can_change_info` (`toggleSupergroupHasAutomaticTranslation`, optimistic, rolled back on a TDLib error such as missing boosts; tdesktop locks the switch below the required boost level, Quill lets TDLib decide).
4. `translate_chats` ("Translate Entire Chats") defaults to on, like `_translateChatEnabled`. The switch still shows off and locked without Premium; the stored value is what lets auto-translated channels work for free accounts.
5. Already in place from batch 7: Show Translate Button, Do Not Translate list, Translate To chooser, Don't translate X.

**B12 reply and bot keyboards**
1. `updateChatReplyMarkup` is parsed and stored per chat (`ReplyKeyboardState`). `chat.reply_markup_message_id` is read from `updateNewChat`; when the open chat names a keyboard message that is not in the loaded history and TDLib has not reported one, `getMessage` fetches it. This schema has no `getChatReplyMarkup`; `getMessage` is what tdesktop-style clients use. `Session::custom_keyboard_for_chat` prefers TDLib's answer and falls back to scanning loaded messages (the demo and old behavior).
2. Composer keyboard button (Keyboard / Keyboard-off icon, next to the scheduled button) shows or hides the panel. The state is per keyboard message, so a new keyboard shows again.
3. Request buttons keep their restrictions (`RequestUsersSpec`, `RequestChatSpec`). The phone button opens tdesktop's "Share" confirmation and calls `sharePhoneNumber` with the bot. Users and chat buttons open a dialog listing chats that pass `request_share::user_candidate` / `chat_candidate` (kind, bot flag, forum, username, created by me; secret chats never), then the send confirmation, then `shareUsersWithBot` / `shareChatWithBot` with `keyboardButtonSourceMessage`. Multi-user requests (`max_quantity > 1`) get checkmarks and a Next button.
4. A lone "@" in an empty composer fetches `getRecentInlineBots` once per session and lists them first in the mention menu; picking one inserts `@username ` and starts inline mode.

### Not done

- Fast inline-button keys (1-9): experimental in tdesktop and off by default, with a per-bot toggle; skipped. 
- `keyboardButtonTypeRequestUsers` Premium restriction and the chat admin-rights restrictions are not pre-filtered (Quill does not track other users' Premium status or the bot's admin rights); TDLib rejects an invalid share and the status line says so. `only_check` is not used. `request_name/username/photo` need nothing extra from the user side.
- `keyboardButtonTypeRequestManagedBot`, location and poll requests stay disabled.
- The sponsored-messages toggle of the README line "Auto-translate channel and sponsored-messages toggles" (`toggleSupergroupCanHaveSponsoredMessages`); only auto-translate is done, so `parity:admin-auto-translate` is not declared.

### Verified

- Unit and recorded-JSON tests: `translate::tracking_enabled` / `bar_label_for`, defaults; `toggleChatIsTranslatable` request shape, optimistic flag and rollback on error; `has_automatic_translation` parse, `can_change_info` gate, request shape and rollback; request-button parsing with restrictions; `updateChatReplyMarkup` parse (message and null); keyboard outside the loaded window, dismissal, removal; `getMessage` fetch for `reply_markup_message_id` and its answer; `shareUsersWithBot` / `shareChatWithBot` shapes; `getRecentInlineBots` once and cached; `request_share` filters and confirmation text.
- Demo captures (`ready-reply-keyboard` views keyboard, hidden, phone, users, confirm, chat; `ready-translate` view `auto` with a Spanish channel).

### Check live

A bot with a reply keyboard that is older than the loaded history (open the chat, the keyboard shows without scrolling); the keyboard button; a bot with request-users / request-chat / request-phone buttons (BotFather-made test bot); type "@" in an empty message after using an inline bot; hide the translate bar and look at another client (the flag syncs); a channel you administer with boosts for the auto-translate switch; a non-Premium account in an auto-translated channel.
