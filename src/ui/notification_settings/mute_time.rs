//! The custom mute duration: tdesktop `Ui::ChooseTimeWidget`
//! (`ui/boxes/choose_time.cpp`, used by `MuteBox`), three number fields for
//! days (0-31), hours (0-23) and minutes (0-59), each captioned with the
//! unit and its count. The confirm button reads "Unmute" while all three are
//! zero.

use super::*;
use gpui_kit::component::input::{InputState, MaskPattern, NumberInput};
use quill::lang::{keys, trn};
use quill::mute_menu::{CustomMute, MAX_DAYS, MAX_HOURS, MAX_MINUTES};

/// The three fields of the custom duration.
pub(in crate::ui) struct MuteTimeInputs {
    days: Entity<InputState>,
    hours: Entity<InputState>,
    minutes: Entity<InputState>,
}

fn field(
    start: u32,
    max: u32,
    window: &mut Window,
    cx: &mut Context<QuillApp>,
) -> Entity<InputState> {
    let input = cx.new(|cx| {
        InputState::new(window, cx)
            .mask_pattern(MaskPattern::Number {
                separator: None,
                fraction: None,
            })
            .min(0.0)
            .max(f64::from(max))
            .default_value(start.to_string())
    });
    // Stepping with the buttons or arrows changes the text silently; any
    // change of the field redraws the panel so the button and captions follow.
    cx.observe(&input, |_, _, cx| cx.notify()).detach();
    input
}

impl MuteTimeInputs {
    fn value(&self, cx: &App) -> CustomMute {
        let read = |input: &Entity<InputState>| input.read(cx).value().trim().parse().unwrap_or(0);
        CustomMute {
            days: read(&self.days),
            hours: read(&self.hours),
            minutes: read(&self.minutes),
        }
        .clamped()
    }
}

impl QuillApp {
    /// Make the fields once a window can be reached. Openers of the Mute
    /// panel have no window in hand, so this runs after the current update.
    pub(in crate::ui) fn queue_mute_time_inputs(&mut self, cx: &mut Context<Self>) {
        if self.notify.mute_time.is_some() {
            return;
        }
        let Some(handle) = self.windows.main else {
            return;
        };
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = app.update(cx, |this, cx| this.reset_mute_time_inputs(window, cx));
            });
        });
    }

    /// Put the fields on the default 8 hours (creating them if needed).
    pub(in crate::ui) fn reset_mute_time_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = CustomMute::default();
        match &self.notify.mute_time {
            Some(inputs) => {
                for (input, value) in [
                    (&inputs.days, start.days),
                    (&inputs.hours, start.hours),
                    (&inputs.minutes, start.minutes),
                ] {
                    input.update(cx, |state, cx| {
                        state.set_value(value.to_string(), window, cx);
                    });
                }
            }
            None => {
                self.notify.mute_time = Some(MuteTimeInputs {
                    days: field(start.days, MAX_DAYS, window, cx),
                    hours: field(start.hours, MAX_HOURS, window, cx),
                    minutes: field(start.minutes, MAX_MINUTES, window, cx),
                });
            }
        }
        cx.notify();
    }

    /// The duration the fields show.
    pub(in crate::ui) fn mute_time_value(&self, cx: &App) -> Option<CustomMute> {
        self.notify
            .mute_time
            .as_ref()
            .map(|inputs| inputs.value(cx))
    }

    /// The three fields with their unit captions.
    pub(in crate::ui) fn mute_time_fields(&self, cx: &App) -> Option<AnyElement> {
        let inputs = self.notify.mute_time.as_ref()?;
        let value = inputs.value(cx);
        let column = |id: &'static str, input: &Entity<InputState>, caption: String| {
            div()
                .id(id)
                .test_support()
                .flex()
                .flex_col()
                .gap_1()
                .w(px(120.))
                .child(
                    NumberInput::new(input)
                        .small()
                        .accessibility_label(caption.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_center()
                        .text_color(cx.theme().muted_foreground)
                        .child(caption),
                )
        };
        Some(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(column(
                    "mute-days",
                    &inputs.days,
                    trn(keys::DAYS, value.days as usize),
                ))
                .child(column(
                    "mute-hours",
                    &inputs.hours,
                    trn(keys::HOURS, value.hours as usize),
                ))
                .child(column(
                    "mute-minutes",
                    &inputs.minutes,
                    trn(keys::MINUTES, value.minutes as usize),
                ))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
impl MuteTimeInputs {
    /// Type `days`, `hours` and `minutes` into the fields.
    pub(in crate::ui) fn set_for_test(
        &self,
        days: &str,
        hours: &str,
        minutes: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (input, text) in [
            (&self.days, days),
            (&self.hours, hours),
            (&self.minutes, minutes),
        ] {
            input.update(cx, |state, cx| {
                state.set_value(text.to_string(), window, cx)
            });
        }
    }
}
