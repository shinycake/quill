# Power saving: reactions flag and the system battery saver

## What tdesktop does

`ui/power_saving.h` has eleven flags. `settings/settings_power_saving.cpp`
lists them in five groups (Animated Stickers, Animated Emoji, Animations in
Chats, then Calls and Interface animations alone). Core wires the system
battery saver (`base::BatterySaving`: macOS Low Power Mode, the Windows
battery saver, the Linux power-saver profile) into
`PowerSaving::SetForceAll(saving && !ignoreBatterySaving)`. Where the
platform gives no answer the box has no "Save Power on Low Battery" switch.
While the saver forces everything on, the box is dimmed and a click shows
"Please turn off Save Power on Low Battery to change these settings."
`EmojiListWidget::powerSavingFlag` picks `kEmojiReactions` instead of
`kEmojiPanel` when the panel is the reaction selector. Dice are not
sticker autoplay and are never gated.

## Audit of Quill against the flag list

Already honored before this change: StickersPanel (`sticker_image`),
StickersChat and EmojiChat (`history_animated`, composer and chat-list
custom emoji, Premium sticker effects), EmojiPanel, EmojiStatus, ChatBackground
(wallpaper rotation), ChatSpoiler (`spoiler_fx`), ChatEffects (send fly,
delete dust, Premium sticker effects), Calls (call panels), Animations
(GPUI reduced motion, also fed by the OS preference).

## What changed

- New `EmojiReactions` flag (bit 4, as in tdesktop) and its row "Autoplay in
  reactions menu". The reaction selector (the emoji panel in reaction mode)
  gates on it; the normal emoji panel keeps `EmojiPanel`.
- Dice landing animations no longer follow the sticker-in-chat switch.
- `src/battery_saving.rs`: the system saver signal and the force-all logic.
  macOS: `NSProcessInfo.isLowPowerModeEnabled`. Windows:
  `GetSystemPowerStatus` `SystemStatusFlag`. Linux: the
  `org.freedesktop.portal.PowerProfileMonitor` `power-saver-enabled`
  property (what GIO's `GPowerProfileMonitor` reads), then
  `net.hadess.PowerProfiles` `ActiveProfile`, both through `gdbus`. No
  answer means no switch and nothing forced.
- `src/ui/battery_watch.rs` asks every five seconds off the UI thread and,
  on a change, sets force-all, GPUI reduced motion and redraws. tdesktop is
  notified by the platform; polling is the cross-platform equivalent with no
  extra dependency.
- The Appearance section got tdesktop's subtitle, the "Save Power on Low
  Battery" switch with its explanation (only where a signal exists), and the
  dimmed state with the explaining note while the saver forces all flags.
  `ignore_battery_saving` is stored with the appearance prefs.

## Remaining differences

- tdesktop applies the box on Save; Quill's Appearance switches apply
  immediately, as every other Appearance control does.
- The reaction strip in the message menu shows stills in Quill, so only the
  expanded selector has something to gate.
- The Linux signal was written from memory of tdesktop's `lib_base` (not in
  the reference checkout) and matched to GIO's documented behavior.

## Verified

- Unit tests: bit values, grouping, per-flag independence, the global gate
  and force-all, the force rule, `gdbus` output parsing.
- GPUI test: with the saver on, picking a sticker arms no flight; with the
  saver ignored the flight arms again.
- `gate.sh` prints GATE OK.
- Not verified: the Linux and Windows probes on real hardware, and no
  screenshot of the section was taken.
