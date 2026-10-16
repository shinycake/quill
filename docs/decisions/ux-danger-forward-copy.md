# Danger wash button, forward panel actions, call status copy

Branch `ux-danger-forward-copy` (quality-sweep items 36 and 40).

## What tdesktop does

- Sessions box: "Terminate all other sessions" is a plain attention-red text action (`attentionButtonFg`), no pink fill.
- `ShareBox::createButtons`: the Send button exists only while a chat is ticked (`selectedChanged` rebuilds the buttons); with nothing ticked the box shows Cancel only. Enter in the search field calls `Inner::selectActive`, which toggles the first (or highlighted) chat, so a single filtered result is ticked with one Enter. Ctrl/Cmd+Enter submits (`setSubmittedCallback`).
- Call status lines (`lng_call_status_*`) are language strings.

## What changed

1. Danger wash button (`kit_colors::ensure_danger_aa`). The kit's own themes draw the danger button as a pale wash whose label is `button.danger.foreground`, which falls back to `danger`. The earlier fix corrected `danger` but left the label on the kit's old red (239,68,68) and tinted the wash against it. Now, when the label equals the danger red (wash style), the label follows the corrected red and the idle, hover and pressed washes move away from it until each pair reaches 4.5:1. Palette themes (filled button, active-text label) keep the previous path. Test `danger_wash_button_reaches_aa` covers light and dark. The button stays a kit `danger()` button; the pink wash is the kit's look, and Quill's tdesktop-style text-only version is not drawn by hand.
2. Forward panel (`share_box_ui.rs`, `search_fields.rs`). Quill showed Send / Send without sound / Schedule as disabled kit buttons when nothing was ticked (2.5:1 in dark). Like tdesktop they now exist only while a chat is ticked. Enter in the search field toggles the first match (it used to send when something was ticked, which tdesktop does not do); Ctrl/Cmd+Enter sends. Test `forward_tests::enter_toggles_the_first_match_and_does_not_send`.
3. Copy. The "Forward to…" title already went through `tr()` (`qll_ops_forward_to`), so that half of item 40 was already done. The call status strings "requesting…", "ringing…", "waiting…" and "connecting…" are now `lng_call_status_*` keys in `lang/en/ops.strings` with the single "…" character. `scripts/check-lang.py` passes.

## Measured

| where | before | after |
|---|---|---|
| "Terminate all other sessions" label on wash, light | 2.42 (demo capture, `fix-ux-contrast`) | at least 4.5 by test (button is below the fold of the sessions capture, so no pixel measurement) |
| forward Schedule / Send, dark | 2.69 / 2.52 (disabled) | not drawn with nothing ticked; ticked they are enabled kit buttons |

Captures: `/Volumes/Idan1TB/quill-work/scratch/quality-sweep/fix-ux-leftovers/after-*.png` (sessions and forward, light and dark). The forward capture shows the panel with no Send row.

## Unverified

The terminate-all button was not pixel-measured after the change (needs a scrolled sessions capture); the guarantee is the unit test on the same colour code. The ticked-state forward buttons were not captured.
