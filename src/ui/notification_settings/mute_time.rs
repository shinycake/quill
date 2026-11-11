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
    /// The window the fields were made in (`None`: the main window): their
    /// focus and blur subscriptions belong to it, so another window's box
    /// makes its own.
    window: Option<ChatId>,
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
    /// Put the fields on the default once a window can be reached. Every
    /// opening of the Mute panel starts there, not at the last duration
    /// typed. Openers have no window in hand, so this runs after the
    /// current update.
    pub(in crate::ui) fn queue_mute_time_inputs(&mut self, cx: &mut Context<Self>) {
        // The window that asked for the box draws it, so the fields are
        // made with that window: the main window when none asked, or when
        // the asking window is gone.
        let asked = self.asking_window_or_last_press().and_then(|chat| {
            self.windows
                .open_windows()
                .find_map(|(open, handle)| (open == chat).then_some((Some(chat), handle)))
        });
        let Some((scope, handle)) =
            asked.or_else(|| self.main_window.handle.map(|handle| (None, handle)))
        else {
            return;
        };
        let app = cx.entity().downgrade();
        let screen = self.screen_epoch();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = app.update(cx, |this, cx| {
                    this.in_screen_scope(screen, cx, |this, cx| {
                        this.in_window_scope(scope, cx, |this, cx| {
                            this.reset_mute_time_inputs(window, cx)
                        });
                    });
                });
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
        let scope = self.windows.scope;
        match &self.notify.mute_time {
            Some(inputs) if inputs.window == scope => {
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
            _ => {
                self.notify.mute_time = Some(MuteTimeInputs {
                    window: scope,
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
