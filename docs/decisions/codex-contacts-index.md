# Contacts section index (`chatlist-contacts-index`)

## What tdesktop does

`PeerListBox` owns a `PeerListSectionIndex` (`boxes/peer_list_section_index.cpp`). It is shown only when the list has at least two section letters (`refreshSectionIndex`). Rows get their section from `ContactsBoxController::applySectionHeaders`: the first character of the chat-list name sort key, upper-cased if it is a letter, otherwise `#`. The only caller of `setSectionHeadersShown(true)` is `PrepareContactsBox`, the Contacts box. The add-members and new-group pickers use plain peer lists with no headers and no index, so tdesktop has no index there.

## What Quill does

The Contacts tab already had the full behaviour from `codex-chatlist-global.md`: name-sorted sections with `#`, the letter bar, click and drag to jump through the real scroll handle, and the visible-letters highlight. The logic is the pure module `src/contacts_index.rs`.

Quill matches tdesktop by leaving the add-members and new-group pickers without an index. They are short, capped lists inside a dialog; adding a bar there would go beyond tdesktop.

This change only closes the remaining gap: it adds unit tests that pin the grouping rule for Cyrillic, Arabic and accented Latin names, and for digits and symbols falling into `#`. It also declares the parity item.

## Verification

`gate.sh`, `check-hotspots.sh origin/main` and `check-file-size.sh`. The earlier visual capture (`ready-chatlist-contacts-index`) covers the bar; nothing visual changed.
