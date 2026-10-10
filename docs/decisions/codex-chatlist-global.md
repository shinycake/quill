# Chat list: contacts, calls, stories menu, suggestions

## What tdesktop does
- `boxes/peer_list_controllers.cpp` (`PrepareContactsBox`, `ContactsBoxController`): the contacts box sorts by last seen by default. A top button switches to name order, which adds a section header per first letter (a letter, or `#`). The box has a search field.
- `boxes/peer_list_section_index.cpp`: an index bar on the right edge. It appears with two or more sections, drops letters when the column is too short, scrolls to a section while the pointer is held, and magnifies the letters near the pointer.
- `calls/calls_box_controller.cpp` (`ClearCallsBox`): "Clear All" in the calls box opens a confirm with a "Delete for everyone" checkbox.
- `dialogs/ui/dialogs_stories_content.cpp` (`FillSourceMenu`): right-clicking a story shows Send Message / Open Group / Open Channel, View profile / group info / channel info, and Hide Stories or Unhide Stories.
- `dialogs/dialogs_top_bar_suggestion.cpp`, `dialogs/suggestions/*`: one suggestion on top of the chat list, with a dismiss cross. Birthdays, "Add your birthday", "Add your photo", the phone check, the password check and Premium offers.
- `window/window_main_menu.cpp`: My Profile, New Group, New Channel, Contacts, Calls, Saved Messages, Settings, Night Mode.

## What was already there
The contacts list (flat, name order), the Calls list, the stories strip, the main menu dropdown, the password check card in Settings > Privacy, the "Date of Birth" privacy row and typed global search with chat type, content and date filters.

## What changed
- `src/contacts_index.rs`, `src/ui/chatlist_global.rs`, `src/ui/contacts.rs`: the Contacts tab has a search field (word-prefix match), a sort button (last seen is the default, name adds section headers) and an "Invite friends" button that copies an invitation text. The index bar is a port of tdesktop's: slot thinning, nearest-slot scrubbing, the fisheye scale and the re-centred magnified column. List items have fixed heights so the bar can place sections without measuring. The easing animation of the magnifier is not ported; the scale follows the pointer directly.
- `src/chatlist_calls.rs`, `src/ui/calls_clear.rs`, `src/connect/calls.rs`: "Clear all" on the Calls list opens the confirm box. It sends `deleteAllCallMessages(revoke)`. The cached list empties when TDLib answers `ok` and stays after a refusal. Demo sessions have no driver and only empty their own fixture.
- `src/stories_menu.rs`, `src/ui/stories_menu_ui.rs`: the right-click menu of a strip tile. Hide/Unhide uses `setChatActiveStoriesList`. "Mute stories" is an addition (tdesktop has no such entry); it flips the per-chat `mute_stories` setting. Your own tile only offers "Send Message".
- `src/chatlist_suggestions.rs`, `src/ui/suggestions_block.rs`: the suggestions card above the folder tabs. Sources are `updateSuggestedActions` and the new `updateContactCloseBirthdays` parser. Priority: phone check, password check, today's birthdays, add birthday, add photo, Premium. Dismissing sends `hideSuggestedAction` or `hideContactCloseBirthdays`; the card goes at once and returns if TDLib refuses. The Premium text is generic because TDLib sends no discount figure.
- `src/ui/suggestions_block.rs` (`birthday_contacts_block`): Settings > Contacts lists contacts with a birthday yesterday, today or tomorrow, which is all TDLib reports. The privacy row for the date of birth already existed.
- `src/main_menu.rs`, `src/ui/navigation.rs`: the main menu gains My Profile, Contacts, Calls, Accounts and a Night Mode switch. The switch flips between the light and dark theme and turns the auto-night schedule off so the choice sticks.

## Not done
- Search tabs on the empty query (Channels, Apps, Posts and the media tabs): the typed search already has the filters, but the empty-query panel needs paging and new lists. `chatlist-search-tabs` is not claimed.
- Main menu: "Set Emoji Status", "My Stories" and the "my groups and channels" submenu. `chatlist-main-menu` is not claimed. The menu is a kit dropdown that a capture cannot open, so its new items were checked by build and tests only.
- The index bar on the add-members picker. `chatlist-contacts-index` covers contacts only, so it is not claimed.
- "Invite friends" has no tdesktop counterpart. It copies the text the mobile clients share.

## How verified
Unit tests cover the ordering, sections, search, bar layout, scrubbing and fisheye maths, the suggestion priority and copy, the stories menu, the Night Mode rule and the request shapes. Driver tests cover Clear calls (sent, kept until `ok`, kept after an error) and suggestion hide with rollback. Demo captures, viewed: `ready-chatlist-contacts-index`, `ready-chatlist-calls-clear`, `ready-chatlist-stories-menu`, `ready-chatlist-suggestions`, `ready-chatlist-suggestions-phone`, `ready-chatlist-birthdays`. No call history was cleared and no account was touched; everything ran on injected fixtures.
