# Thermal pressure forces power saving

## What tdesktop does

Nothing. `Core::Application` combines `base::BatterySaving::value()` with
`ignoreBatterySavingValue()` and calls `PowerSaving::SetForceAll(saving && !ignore)`
(`core/application.cpp`). A search of `Telegram/SourceFiles/core` finds no thermal
input. This is a Quill addition built on the same force path.

## What changed

- GPUI 0.3.9 exposes `App::thermal_state()` and `App::on_thermal_state_change()`.
  macOS reports `NSProcessInfo.thermalState` (`gpui-pre-apple/src/thermal.rs`) and
  notifies on change. The Linux and Windows platforms return `Nominal` and never
  notify, so this only has an effect on macOS.
- `src/battery_saving.rs`: a `Thermal` enum, a latched "hot" flag, and
  `forcing_for(saving, thermal_hot, ignore)`, which is
  `(saver_on || thermal_hot) && !ignore`.
- `src/ui/battery_watch.rs`: reads the state once at startup, then follows
  `on_thermal_state_change`. No polling. When the forced answer changes it updates
  GPUI reduced motion and redraws, as the battery path does.
- `src/ui/appearance_power.rs`: the dimmed "forced" state of the switches uses the
  same predicate.

## Decisions

- Conservative: only Serious and Critical force saving. Fair is the system asking
  for discretionary work to be reduced, which is too common to change the whole UI.
- Hysteresis: enter at Serious or Critical, leave only at Nominal. Fair holds the
  previous answer, so a machine oscillating across the Serious/Fair boundary does
  not flap animations on and off. (`thermal_hot_next`, pure.)
- Existing switch governs it: "Save Power on Low Battery" off means neither the
  battery saver nor thermal pressure forces anything. No new setting, no new copy.
  The switch is shown when the platform reports a battery saver, which is true on
  macOS, the only platform with a thermal signal.
- Thermal forcing works even when the platform gives no battery answer.

## Verification

Unit tests cover the full battery x thermal x ignore table and the latch
transitions. Real thermal changes on macOS could not be exercised in the Linux
cloud container; the macOS path in GPUI was read, not run.
