# Selection bar actions, drag/range selection, pin boxes

## tdesktop
- `PinMessageBox` (`boxes/pin_messages_box.cpp`): confirm "Pin this message
  in the group?" / "Would you like to pin this message?" / the older-message
  variant. Private chats (not Saved Messages) get "Also pin for {user}",
  unchecked, which sets `pm_oneside` when off. Basic groups and megagroups
  get "Notify all members", checked, which sets `silent` when off; not shown
  for channels or when pinning a message older than the newest pin.
- Unpin asks `lng_pinned_unpin_sure` (or `_many_sure` with a count).
  Unpin all asks `lng_pinned_unpin_all_sure`; hiding the bar asks
  `lng_pinned_hide_all_sure` (both already matched in Quill).
- History selection supports drag across rows and Shift+click ranges.

## Changed
- `src/selection_pin.rs`: pure rules (`pin_choice`, `request_flags`,
  question wording, `range_between`), unit tested.
- `src/ui/pin_box.rs`: the message menu's Pin opens the box and sends
  `pinChatMessage` with `disable_notification` / `only_for_self` from the
  checkbox (was hard-coded false/false); Unpin confirms. New driver method
  `pin_chat_message_with`.
- Selection bar: "Copy as Text" (same text as Cmd/Ctrl+C), "Save" (when
  media is selected: pick a folder, copy downloaded files with unique names,
  starts downloads first if needed), "Unpin" (when all selected are pinned
  and the user may pin; confirms with the count). Forward, Delete (with
  "Delete for everyone" where all are yours) and Report already existed.
- Selection rows: press toggles and starts a drag that gives the same state
  to rows the pointer crosses; Shift+click selects the range from the last
  clicked row among loaded messages.

## Not done
- Starting a selection by dragging from outside selection mode (rows have
  no overlay until a message is selected).
- Download / Send Now / Reschedule selected.

## Verified
Gate (fmt, clippy, core + UI tests) OK. Demo capture `ready-select-mode`
shows the new "Copy as Text" button. Pin box, Save, Unpin, drag and
Shift-range were not exercised interactively (no live account, no pinned
fixtures); the flag mapping and range logic are unit tested.
