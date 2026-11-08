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
use super::composer_field::field_doc;
use super::scheduled::{TimePicker, picked_unix};
use crate::ui::status_note::StatusNote;
use crate::ui::wall_clock::WallClock;
use gpui_kit::component::button::*;
use gpui_kit::component::date_picker::{DatePicker, DatePickerState};
use gpui_kit::component::time_field::{HourCycle, TimePrecision};
use gpui_kit::{TestSupportExt, *};
use quill::composer::{FormatAction, apply_format_markup, date_link, date_link_time};
use quill::composer_doc::ComposerTag;
use quill::lang::{keys, tr};
use std::ops::Range;

super::screenshot_demo::register_demos![
    super::screenshot_demo::DemoSpec::chat_list("ready-composer-date")
        .setup(QuillApp::demo_composer_date),
];

/// The latest date the box takes (tdesktop `.max`).
const MAX_DATE: i64 = 2_114_380_800;

/// The date box (a kit dialog, `composer_boxes.rs`).
pub(super) struct ComposerDateDialog {
    /// The text that becomes the date when the box opened.
    range: Range<usize>,
    /// That text, to notice a draft that changed meanwhile.
    selected: String,
    pub(super) picker: TimePicker,
}

/// Midday today, local time: where a new date starts (tdesktop opens the
/// calendar on today and takes the chosen day at 12:00).
fn midday_today(clock: &WallClock) -> i64 {
    let now = clock.now();
    let civil = clock.civil(now);
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
            Some((range, time)) if !self.chat_ui.rich_editor_open => (range, Some(time)),
            _ => (self.composer.read(cx).selected_range(), None),
        };
        let Some(selected) = text.get(range.clone()).filter(|s| !s.is_empty()) else {
            return;
        };
        let selected = selected.to_string();
        let initial = existing.map_or_else(|| midday_today(&WallClock::of(cx)), i64::from);
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
        let Some(time) = picked_unix(&dialog.picker, 0, cx)
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
        if self.chat_ui.rich_editor_open {
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
        // `QUILL_DEMO_COMPOSER_BOX=link` opens the Add Link box instead,
        // `code` the Code Language box over a fenced block, `contact` the
        // contact chooser.
        match std::env::var("QUILL_DEMO_COMPOSER_BOX").as_deref() {
            Ok("link") => self.open_composer_link_dialog(window, cx),
            Ok("code") => {
                self.chat_ui.rich_editor_open = true;
                self.composer.update(cx, |input, cx| {
                    input.set_value("```rust\nfn main() {}\n```", window, cx);
                    input.set_selected_range(12..12, cx);
                });
                self.open_code_language_dialog(window, cx);
            }
            Ok("contact") => {
                if let Some(session) = self.demo_session.as_mut() {
                    demo_contacts(session, &self.demo_ui.sink, &self.demo_ui.seq);
                }
                self.open_share_contact_panel(window, cx);
            }
            _ => self.open_composer_date_dialog(window, cx),
        }
    }

    /// The date box's body: the date and time picker.
    pub(super) fn composer_date_dialog_panel(&self, _: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.composer_ui.date_dialog.as_ref()?;
        Some(
            div()
                .id("composer-date-dialog")
                .test_support()
                .child(
                    DatePicker::new(&dialog.picker.state)
                        .placeholder(tr(keys::Q_CHAT_PICK_A_DATE_AND_TIME_2)),
                )
                .into_any_element(),
        )
    }

    /// Cancel and Save under the date box.
    pub(super) fn composer_date_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("composer-date-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_composer_date_dialog(window, cx);
                    })),
            )
            .child(
                Button::new("composer-date-save")
                    .label(tr(keys::SETTINGS_SAVE))
                    .primary()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.apply_composer_date(window, cx)),
                    ),
            )
            .into_any_element()
    }
}

/// Three address-book contacts with phone numbers for the contact chooser.
pub(super) fn demo_contacts(
    session: &mut quill::state::Session,
    sink: &std::sync::Arc<quill::diagnostics::MemorySink>,
    seq: &std::sync::atomic::AtomicU64,
) {
    use quill::diagnostics::DiagnosticSink;
    let sink: std::sync::Arc<dyn DiagnosticSink> = sink.clone();
    seq.store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
    let people = [
        (41, "Mira", "Stone"),
        (42, "Theo", "Lark"),
        (43, "Ines", "Moreau"),
    ];
    for (id, first, last) in people {
        let json = format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","phone_number":"155501000{id}","is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        );
        if let Some(owned) = quill::telegram::client::copy_and_parse(&json, seq, &sink) {
            session.apply(owned);
        }
    }
    session.users_state.contacts = Some(people.iter().map(|(id, _, _)| *id).collect());
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "composer_date_tests.rs"]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::midday_today;
    use crate::ui::wall_clock::WallClock;

    #[test]
    fn a_new_date_starts_at_midday() {
        let clock = WallClock::chicago_at(WallClock::CHICAGO_FALL_BACK - 4 * 3600 + 7);
        let midday = midday_today(&clock);
        // 12:00 CDT on 2026-10-31.
        assert_eq!(midday, WallClock::CHICAGO_FALL_BACK - 14 * 3600);
        let civil = clock.civil(midday);
        assert_eq!((civil.day, civil.hour, civil.minute), (31, 12, 0));
    }
}
