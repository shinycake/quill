//! "Date" formatting in the composer (Telegram Desktop's `editMarkdownDate`,
//! lib_ui input_field.cpp, and its box in chat_helpers/message_field.cpp):
//! Formatting > Date or Cmd/Ctrl+Shift+D turns the selected text into a
//! date every reader sees in their own time zone. A new date starts at
//! midday today; a date under the caret opens at its own time. The box
//! is titled "Choose date and time" and saves with "Save"; any date from
//! 1970 to 2037 is allowed (`min = 1`, `max = 2114380800`).
//!
//! In the field the date is a link to `tg://time?unix=N`
//! (`quill::composer::date_link`), sent as `textEntityTypeDateTime`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::{accent, bg_canvas};
use super::composer_field::field_doc;
use super::scheduled::{TimePicker, picked_unix};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::date_picker::{DatePicker, DatePickerState};
use gpui_kit::component::time_field::{HourCycle, TimePrecision};
use gpui_kit::component::*;
use gpui_kit::{TestSupportExt, *};
use quill::composer::{FormatAction, apply_format_markup, date_link, date_link_time};
use quill::composer_doc::ComposerTag;
use quill::lang::{keys, tr};
use quill::local_time::{civil_local, now_unix};
use std::ops::Range;

super::screenshot_demo::register_demos![
    super::screenshot_demo::DemoSpec::chat_list("ready-composer-date")
        .setup(QuillApp::demo_composer_date),
];

/// The latest date the box takes (tdesktop `.max`).
const MAX_DATE: i64 = 2_114_380_800;

/// The date box above the composer.
pub(super) struct ComposerDateDialog {
    /// The text that becomes the date when the box opened.
    range: Range<usize>,
    /// That text, to notice a draft that changed meanwhile.
    selected: String,
    pub(super) picker: TimePicker,
}

/// Midday today, local time: where a new date starts (tdesktop opens the
/// calendar on today and takes the chosen day at 12:00).
fn midday_today(now: i64) -> i64 {
    let civil = civil_local(now);
    // Minutes only: the picker drops seconds.
    let since_midnight = i64::from(civil.hour) * 3600 + i64::from(civil.minute) * 60;
    now - now.rem_euclid(60) - since_midnight + 12 * 3600
}

impl QuillApp {
    /// The date under the composer's caret or selection: its range and
    /// time.
    pub(super) fn composer_date_at_caret(&self, cx: &App) -> Option<(Range<usize>, i32)> {
        let state = self.composer.read(cx);
        let selection = state.selected_range();
        field_doc(state)
            .spans
            .into_iter()
            .find_map(|span| match &span.tag {
                ComposerTag::Link(url)
                    if span.range.start <= selection.start && selection.end <= span.range.end =>
                {
                    date_link_time(url).map(|time| (span.range.clone(), time))
                }
                _ => None,
            })
    }

    /// Whether Formatting > Date can run: something selected, or a date
    /// under the caret (tdesktop disables it otherwise).
    pub(super) fn composer_date_enabled(&self, cx: &App) -> bool {
        !self.composer.read(cx).selected_range().is_empty()
            || self.composer_date_at_caret(cx).is_some()
    }

    /// Formatting > Date / Cmd/Ctrl+Shift+D.
    pub(super) fn open_composer_date_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        let (range, existing) = match self.composer_date_at_caret(cx) {
            Some((range, time)) if !self.composer_ui.rich_editor_open => (range, Some(time)),
            _ => (self.composer.read(cx).selected_range(), None),
        };
        let Some(selected) = text.get(range.clone()).filter(|s| !s.is_empty()) else {
            return;
        };
        let selected = selected.to_string();
        let initial = existing.map_or_else(|| midday_today(now_unix()), i64::from);
        let state = cx.new(|cx| {
            DatePickerState::new(window, cx)
                .time_precision(TimePrecision::Minute)
                .hour_cycle(HourCycle::H23)
        });
        let picker = TimePicker::open(state, initial, window, cx);
        self.composer_ui.date_dialog = Some(ComposerDateDialog {
            range,
            selected,
            picker,
        });
        cx.notify();
    }

    pub(super) fn close_composer_date_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composer_ui.date_dialog.take().is_some() {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    /// "Save": the remembered text becomes the picked date.
    fn apply_composer_date(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.composer_ui.date_dialog.as_ref() else {
            return;
        };
        let Some(time) = picked_unix(&dialog.picker, cx)
            .filter(|time| (1..=MAX_DATE).contains(time))
            .and_then(|time| i32::try_from(time).ok())
        else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_PICK_A_DATE_AND_TIME)));
            cx.notify();
            return;
        };
        let range = dialog.range.clone();
        let selected = dialog.selected.clone();
        self.close_composer_date_dialog(window, cx);
        let text = self.composer.read(cx).value().to_string();
        if text.get(range.clone()) != Some(selected.as_str()) {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_THE_TEXT_CHANGED_SELECT_IT,
            )));
            cx.notify();
            return;
        }
        let url = date_link(time);
        if self.composer_ui.rich_editor_open {
            let (new_text, selection) = apply_format_markup(&text, range, &FormatAction::Link(url));
            self.composer.update(cx, |input, cx| {
                input.set_value(&new_text, window, cx);
                input.set_selected_range(selection, cx);
                input.focus(window, cx);
            });
            cx.notify();
            return;
        }
        self.composer.update(cx, |input, cx| {
            input.set_selected_range(range.clone(), cx);
        });
        self.toggle_composer_tag(ComposerTag::Link(url), window, cx);
        self.composer.update(cx, |input, cx| {
            input.set_selected_range(range.end..range.end, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// `ready-composer-date`: "tonight" selected and the box open.
    fn demo_composer_date(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value(tr(keys::Q_CHAT_SEE_YOU_TONIGHT), window, cx);
            input.set_selected_range(8..15, cx);
        });
        self.open_composer_date_dialog(window, cx);
    }

    /// The box, above the composer input row.
    pub(super) fn composer_date_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.composer_ui.date_dialog.as_ref()?;
        self.composer_date_dialog_panel_body(cx)
    }

    /// `composer_date_dialog_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn composer_date_dialog_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.composer_ui.date_dialog.as_ref()?;
        Some(
            div()
                .id("composer-date-dialog")
                .test_support()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(accent())
                .bg(bg_canvas())
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(keys::FORMATTING_DATE_TITLE)),
                )
                .child(
                    DatePicker::new(&dialog.picker.state)
                        .placeholder(tr(keys::Q_CHAT_PICK_A_DATE_AND_TIME_2)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("composer-date-save")
                                .label(tr(keys::SETTINGS_SAVE))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_composer_date(window, cx)
                                })),
                        )
                        .child(
                            Button::new("composer-date-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_composer_date_dialog(window, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "composer_date_tests.rs"]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::midday_today;

    #[test]
    fn a_new_date_starts_at_midday() {
        let now = quill::local_time::now_unix();
        let civil = quill::local_time::civil_local(midday_today(now));
        // 11 or 13 on a day the clocks change.
        assert!((11..=13).contains(&civil.hour), "{}", civil.hour);
        assert_eq!(civil.minute, 0);
    }
}
