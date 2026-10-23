# sliders-clear-history: step sliders and one Clear history box

Parity ids: `settings-mute-custom-slider`, `settings-autodelete-custom-slider`, `data-size-sliders`, `chatlist-clear-history-box`.

## tdesktop
- Custom mute (`ChooseTimeBox`): days, hours and minutes fields; the total is the mute time, zero unmutes.
- Auto-delete (`AutoDeleteSettingsBox`): a picker over a fixed list of periods, plus Disable and Save.
- Data and Storage: `AddSizeLimitSlider` (auto-download size caps) and local storage `createLimitsSlider` pick from fixed step lists with a pseudo-discrete slider. The values are `TotalSizeLimit`, `TimeLimit` and `SizeLimitByIndex`.
- Clear history (`DeleteMessagesBox` with `_wipeHistoryJustClear`) is one menu item. It asks once. A "delete for the other side" checkbox shows only where the server allows it; saved messages, channels and the for-me-only case have none.

## What changed
- `src/limit_steps.rs`: the three step lists with index/value conversion. `src/clear_history.rs`: what the box shows for each chat kind. Both are core, tested without a window.
- `src/ui/index_sliders.rs`: kit `Slider` over the step index with `accessibility_label`, used by the network editor (one cap slider and switch per photo, video and file), the local storage limits (applied on release) and the auto-delete picker.
- Auto-delete picker (`ttl_picker.rs`) is a slider whose first step is Off, with Cancel and Save (Save on Off disables); secret chats keep their presets. The stale doc comment there is fixed.
- Custom mute is three number inputs (days, hours, minutes) and one Mute/Unmute button (`mute_time.rs`).
- Clear history is one chat-menu item (`chat-menu-clear`) and one box (`clear_history_box.rs`) with a revoke checkbox, using the same `Checkbox` pattern as message delete. Every split place (row menu, header menu, group confirm) now routes there; `GroupConfirmAction::ClearHistory` is gone.
- The "{} downloads" title moved to the `qll_ops_network_downloads` lang key (`lang/en/limits.strings`).

## Differences from tdesktop
- The auto-delete picker is a slider, not tdesktop's drum picker.
- The local size limit has one extra last step, "No limit".
- There is no media-limit slider; TDLib has no such option.
- When only the "for everyone" variant is allowed, the box clears for everyone without a checkbox.
- Auto-download keeps a switch and slider per media type; tdesktop shares sliders across types.
- The mute minute step is 1 and there is no wheel.

## Verified
- Core tests for the step lists, mute maths, clear-history plans; driver tests that `deleteChatHistory` carries `revoke` false or true.
- GPUI tests per control: cap sliders, storage sliders apply on release, TTL save, save on Off, cancel, mute fields and apply, one-item clear menu with and without the checkbox.
- `gate.sh` green. Captures (light and dark) of the network editor and mute fields, light of the TTL slider, read back.
- Not captured: the local storage sliders and the Clear history box (the demo chats do not allow clearing), and nothing ran against a live account. Both are covered by the GPUI tests.
