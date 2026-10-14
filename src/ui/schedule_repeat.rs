//! The "Repeat" choice of the schedule picker (tdesktop
//! `Ui::ChooseRepeatPeriod` in `ui/boxes/choose_date_time.cpp`, added to the
//! box by `ScheduleBox`): a button under the date and time that reads
//! "Repeat: Never" and opens a menu of periods. Without Premium the button
//! shows a lock and tells how to get the feature instead of opening it.
//!
//! TDLib takes `repeat_period` only on `sendMessage` and on a
//! `forwardMessages` of one message, so the row is not offered when an
//! existing scheduled message is rescheduled (`editMessageSchedulingState`
//! has no repeat) or when several messages are forwarded.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::scheduled::{ScheduleTarget, using_test_dc};
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::schedule::{repeat_choice_for, repeat_choices};

impl QuillApp {
    /// Whether the picker for `target` offers a repeat period.
    pub(super) fn repeat_offered(&self, target: ScheduleTarget) -> bool {
        match target {
            ScheduleTarget::Composer => true,
            ScheduleTarget::Share => self
                .share
                .pending_forward
                .as_ref()
                .is_some_and(|draft| draft.count() == 1),
            ScheduleTarget::Reschedule(_) | ScheduleTarget::RescheduleSelected => false,
        }
    }

    /// The choice counts only for Premium (`session->premium()`).
    pub(super) fn repeat_allowed(&self, target: ScheduleTarget) -> bool {
        self.repeat_offered(target) && self.session().is_some_and(|s| s.is_premium())
    }

    /// The "Repeat: Never" button of the open picker.
    pub(super) fn schedule_repeat_row(
        &self,
        repeat: i32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.composer_ui.schedule_picker.as_ref()?;
        if !self.repeat_offered(picker.target) {
            return None;
        }
        let premium = self.session().is_some_and(|s| s.is_premium());
        let test_dc = using_test_dc();
        let shown = repeat_choice_for(if premium { repeat } else { 0 }, test_dc);
        let owner = cx.entity().downgrade();
        let button = Button::new("schedule-repeat")
            .label(format!("Repeat: {}", shown.label))
            .icon(if premium {
                gpui_kit::assets::IconName::ChevronDown
            } else {
                gpui_kit::assets::IconName::Lock
            })
            .ghost()
            .small()
            .accessibility_label(format!("Repeat: {}", shown.label));
        let button = if premium {
            button
                .dropdown_menu(move |mut menu, _, _| {
                    for choice in repeat_choices(test_dc) {
                        let owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(choice.label)
                                .checked(choice.period == shown.period)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        if let Some(picker) =
                                            this.composer_ui.schedule_picker.as_mut()
                                        {
                                            picker.repeat = choice.period;
                                        }
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                })
                .into_any_element()
        } else {
            // `showPremiumPromo`: the lock explains instead of opening.
            button
                .on_click(cx.listener(|this, _, _, cx| {
                    this.connection.status_note =
                        "Subscribe to Telegram Premium to schedule repeating messages.".into();
                    cx.notify();
                }))
                .into_any_element()
        };
        Some(
            div()
                .id("schedule-repeat-row")
                .flex()
                .justify_center()
                .child(button)
                .into_any_element(),
        )
    }

    /// The "Schedule" shortcut sends the composer's message once its time
    /// is confirmed, as tdesktop's schedule box does.
    pub(super) fn send_composer_after_schedule(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        if text.trim().is_empty()
            && self.composer_ui.pending_attachments.is_empty()
            && !self.forward_bar_here()
        {
            return;
        }
        let markup = self.composer_markup(cx);
        self.submit_composer(markup, window, cx);
    }
}
