# Double-click quick action setting

## What tdesktop does
- `Core::Settings::chatQuickAction` holds `DoubleClickQuickAction` (Reply by default, React, None). Settings > Chat shows Reply and React radios (`settings_chat.cpp`).
- `ListWidget::mouseDoubleClickEvent` runs it when no link is active, nothing is selected and the message is regular: Reply starts a reply, React toggles the favorite reaction.
- The reply and reaction buttons beside a hovered bubble (`cornerReply`, `cornerReaction`) were already implemented in Quill (`src/corner_buttons.rs`, settings switches), so nothing changed there.

## What changed
- `src/quick_action.rs`: `QuickAction` enum (Reply default, React, None) with pure `effect(sent)` dispatch and unit tests.
- `MediaPrefs.quick_action` persists it in `media_prefs.json` (serde default, older files load as Reply).
- `src/ui/quick_action_ui.rs`: settings radio group (Reply, React with the quick reaction) and `double_click_action`.
- Message row double-click now calls `double_click_action` instead of always reacting. Quill previously always reacted; the default now matches tdesktop (Reply).
- Like tdesktop, None is a stored value with no radio.

## Verification
- gate.sh: GATE OK. Unit tests cover the default, dispatch, index mapping and serde round trip, plus the prefs file round trip with the new field.
- Not verified: a GPUI integration test for the double-click and a visual check.
