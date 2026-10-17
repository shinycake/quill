# Translate on the profile bio

## What tdesktop does

`info/profile/info_profile_actions.cpp`, `setupAboutContextMenu` (line ~1465), used for a user's Bio and a chat's About (`addInfoLine` at ~1711 and ~1875):

- Over a link to a peer id it only offers Copy ID.
- A selection (or a label without extra lines) uses the label's own menu: Copy Selected Text, or Copy Text.
- Then (~1518) Translate Selected Text over a selection, Translate otherwise, unless `Ui::SkipTranslate(text)` (`boxes/translate_box.cpp:491`): empty text, the "Show Translate button" setting off, no letters in the first 100 characters, or the text recognised as a "Do Not Translate" language. The text tested is the selection when there is one.

## What Quill did

Group and channel descriptions already had Copy Text and Translate (no selection variants). A user's bio row only had "Copy Bio".

## What changed

- `quill::translate::about_menu` holds the plan: which copy item and which translate item show, given the text, the selection and `SkipTranslate`. Unit tested.
- `about_text_menu` (profile_panels/about_text.rs) builds the menu from the plan and is shared by the description block and the bio row. It reads the window's text selection (keyed to the profile's chat) so the Selected variants appear.
- The bio row draws its text through the selectable rich text, so it can be selected, and its context menu is the shared one (Copy Text, Translate).
- Verify tooling: `r:x,y` right-click step for demo scripts; demo `ready-info-user-bio`.

Deviation: tdesktop's Copy ID over a peer-id link has no Quill counterpart (no such links). Bios have no entities in Quill's state yet, so links in a bio are not drawn.

## Verified

Unit tests for each condition; GPUI test for the bio plan and the setting; captures of the open menu in light and dark.
