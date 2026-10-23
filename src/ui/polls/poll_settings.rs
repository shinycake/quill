//! The settings section of the poll box, in the order of tdesktop's
//! `CreatePollBox`: Show Who Voted, Allow Multiple Answers, Allow Adding
//! Options, Allow Revoting, Shuffle Options, Set Correct Answer, then (in
//! channels) Restrict to Subscribers and Limit by Country, then Limit
//! Duration with its "Poll ends" row and "Hide results".

use super::*;
use crate::ui::dialogs::poll::PollDialog;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use quill::lang::{keys, tr};
use quill::poll::{POLL_PERIOD_PRESETS, PollClose, poll_close_max};

impl PollDialog {
    /// Whether the "Set Correct Answer" switch can be moved: a bot's
    /// request button may have fixed the type.
    pub(crate) fn quiz_locked(&self) -> bool {
        self.lock != QuizLock::Free
    }

    /// "Allow Adding Options" is off and locked for quizzes and for
    /// anonymous polls (`updateAddOptionsLocked`).
    pub(crate) fn add_options_locked(&self) -> bool {
        self.in_channel || self.is_quiz || self.is_anonymous
    }

    /// "Set Correct Answer" (`quiz`): a quiz has fixed options and no
    /// revoting.
    pub(crate) fn set_quiz(&mut self, on: bool) {
        if self.quiz_locked() {
            return;
        }
        self.is_quiz = on;
        if on {
            self.allow_adding_options = false;
            self.allows_revoting = false;
        } else {
            self.quiz_correct_rows.clear();
        }
    }

    /// "Allow Multiple Answers": with it off a quiz keeps one right answer.
    pub(crate) fn set_multiple(&mut self, on: bool) {
        self.allows_multiple_answers = on;
        if !on {
            self.quiz_correct_rows.truncate(1);
        }
    }

    /// "Show Who Voted": a poll that names voters can take new options.
    pub(crate) fn set_show_who_voted(&mut self, on: bool) {
        self.is_anonymous = !on;
        if !on {
            self.allow_adding_options = false;
        }
    }

    pub(crate) fn set_add_options(&mut self, on: bool) {
        if !self.add_options_locked() {
            self.allow_adding_options = on;
        }
    }

    /// Mark or unmark option `row` as a right answer.
    pub(crate) fn set_correct_row(&mut self, row: usize, on: bool) {
        if !on {
            self.quiz_correct_rows.retain(|marked| *marked != row);
        } else if self.allows_multiple_answers {
            if !self.quiz_correct_rows.contains(&row) {
                self.quiz_correct_rows.push(row);
            }
        } else {
            self.quiz_correct_rows = vec![row];
        }
    }

    /// A row left the list: the marks after it move up with their options.
    pub(crate) fn forget_correct_row(&mut self, removed: usize) {
        self.quiz_correct_rows = self
            .quiz_correct_rows
            .iter()
            .filter(|marked| **marked != removed)
            .map(|marked| if *marked > removed { marked - 1 } else { *marked })
            .collect();
    }
}

impl QuillApp {
    /// One switch row (tdesktop's `AddPollToggleButton`): title and about
    /// line on the left, the switch on the right.
    fn poll_switch_row(
        &self,
        id: &'static str,
        title: &'static str,
        about: &'static str,
        on: bool,
        locked: bool,
        set: fn(&mut PollDialog, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(title))
                    .child(div().text_xs().text_color(muted).child(about)),
            )
            .child(
                Switch::new(id)
                    .checked(on)
                    .disabled(locked)
                    .accessibility_label(title)
                    .on_click(cx.listener(move |this, &next: &bool, _, cx| {
                        if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
                            set(dialog, next);
                        }
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The whole settings section, under its "Settings" title.
    pub(in crate::ui) fn poll_settings(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.composer_ui.poll_dialog.as_ref()?;
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::POLLS_CREATE_SETTINGS)),
            );
        if !dialog.in_channel {
            section = section.child(self.poll_switch_row(
                "poll-toggle-who-voted",
                tr(keys::POLLS_CREATE_SHOW_WHO_VOTED),
                tr(keys::POLLS_CREATE_SHOW_WHO_VOTED_ABOUT),
                !dialog.is_anonymous,
                false,
                |dialog, on| dialog.set_show_who_voted(on),
                cx,
            ));
        }
        section = section.child(self.poll_switch_row(
            "poll-toggle-multiple",
            tr(keys::POLLS_CREATE_ALLOW_MULTIPLE_ANSWERS),
            tr(keys::POLLS_CREATE_ALLOW_MULTIPLE_ANSWERS_ABOUT),
            dialog.allows_multiple_answers,
            false,
            |dialog, on| dialog.set_multiple(on),
            cx,
        ));
        if !dialog.in_channel {
            section = section.child(self.poll_switch_row(
                "poll-toggle-add-options",
                tr(keys::POLLS_CREATE_ALLOW_ADDING_OPTIONS),
                tr(keys::POLLS_CREATE_ALLOW_ADDING_OPTIONS_ABOUT),
                dialog.allow_adding_options,
                dialog.add_options_locked(),
                |dialog, on| dialog.set_add_options(on),
                cx,
            ));
        }
        section = section
            .child(self.poll_switch_row(
                "poll-toggle-revoting",
                tr(keys::POLLS_CREATE_ALLOW_REVOTING),
                tr(keys::POLLS_CREATE_ALLOW_REVOTING_ABOUT),
                dialog.allows_revoting,
                false,
                |dialog, on| dialog.allows_revoting = on,
                cx,
            ))
            .child(self.poll_switch_row(
                "poll-toggle-shuffle",
                tr(keys::POLLS_CREATE_SHUFFLE_OPTIONS),
                tr(keys::POLLS_CREATE_SHUFFLE_OPTIONS_ABOUT),
                dialog.shuffle_options,
                false,
                |dialog, on| dialog.shuffle_options = on,
                cx,
            ))
            .child(self.poll_switch_row(
                "poll-toggle-quiz",
                tr(keys::POLLS_CREATE_SET_CORRECT_ANSWER),
                if dialog.allows_multiple_answers {
                    tr(keys::POLLS_CREATE_SET_CORRECT_ANSWER_ABOUT_MULTI)
                } else {
                    tr(keys::POLLS_CREATE_SET_CORRECT_ANSWER_ABOUT)
                },
                dialog.is_quiz,
                dialog.quiz_locked(),
                |dialog, on| dialog.set_quiz(on),
                cx,
            ));
        if dialog.in_channel {
            section = section
                .child(self.poll_switch_row(
                    "poll-toggle-subscribers",
                    tr(keys::POLLS_CREATE_RESTRICT_TO_SUBSCRIBERS),
                    tr(keys::POLLS_CREATE_RESTRICT_TO_SUBSCRIBERS_ABOUT),
                    dialog.members_only,
                    false,
                    |dialog, on| dialog.members_only = on,
                    cx,
                ))
                .child(self.poll_country_limit(cx));
        }
        section = section.child(self.poll_switch_row(
            "poll-toggle-duration",
            tr(keys::POLLS_CREATE_LIMIT_DURATION),
            tr(keys::POLLS_CREATE_LIMIT_DURATION_ABOUT),
            dialog.limit_duration,
            false,
            |dialog, on| dialog.limit_duration = on,
            cx,
        ));
        if dialog.limit_duration {
            section = section
                .child(self.poll_ends_row(cx))
                .child(self.poll_switch_row(
                    "poll-toggle-hide-results",
                    tr(keys::POLLS_CREATE_HIDE_RESULTS),
                    tr(keys::POLLS_CREATE_HIDE_RESULTS_ABOUT),
                    dialog.hide_results_until_closes,
                    false,
                    |dialog, on| dialog.hide_results_until_closes = on,
                    cx,
                ));
        }
        Some(section.into_any_element())
    }

    /// The furthest a poll may close from now, in seconds.
    pub(in crate::ui) fn poll_close_max_secs(&self) -> i64 {
        poll_close_max(
            self.session()
                .and_then(|session| session.payments.poll_open_period_max),
        )
    }

    /// "Poll ends": the moment the poll closes, opening the preset menu
    /// (and "Custom") like tdesktop's popup menu.
    fn poll_ends_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let close = self
            .composer_ui
            .poll_dialog
            .as_ref()
            .map(|dialog| dialog.close)
            .unwrap_or(PollClose::Period(quill::poll::POLL_DEFAULT_PERIOD_SECS));
        let ends = crate::ui::message_text::format_unix_date_time(
            close.ends_at(quill::local_time::now_unix()),
        );
        let max = self.poll_close_max_secs();
        let owner = cx.entity().downgrade();
        Button::new("poll-ends")
            .outline()
            .w_full()
            .h(px(36.))
            .px_3()
            .justify_start()
            .accessibility_label(tr(keys::POLLS_CREATE_POLL_ENDS))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().child(tr(keys::POLLS_CREATE_POLL_ENDS)))
                    .child(div().text_color(muted).child(ends)),
            )
            .dropdown_menu(move |mut menu, _, _| {
                for secs in POLL_PERIOD_PRESETS.into_iter().filter(|secs| *secs <= max) {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(quill::emoji_status_selector::format_mute_for(
                            secs as i32,
                        ))
                        .checked(close == PollClose::Period(secs))
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
                                    dialog.close = PollClose::Period(secs);
                                }
                                cx.notify();
                            });
                        }),
                    );
                }
                let owner = owner.clone();
                menu.item(
                    PopupMenuItem::new(tr(keys::POLLS_CREATE_DURATION_CUSTOM))
                        .checked(matches!(close, PollClose::Date(_)))
                        .on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.open_poll_deadline_box(window, cx);
                            });
                        }),
                )
            })
            .into_any_element()
    }

    /// tdesktop's "Limit by Country" switch (broadcast channels) and, while
    /// it is on, the "Allowed Countries" row under it. Turning it off
    /// forgets the chosen countries.
    fn poll_country_limit(&self, cx: &mut Context<Self>) -> AnyElement {
        let dialog = self.composer_ui.poll_dialog.as_ref();
        let on = dialog.is_some_and(|dialog| dialog.limit_by_country);
        let codes = dialog.map(|dialog| dialog.country_codes.clone());
        let mut block = div().flex().flex_col().gap_2().child(self.poll_switch_row(
            "poll-toggle-country",
            tr(keys::POLLS_CREATE_LIMIT_BY_COUNTRY),
            tr(keys::POLLS_CREATE_LIMIT_BY_COUNTRY_ABOUT),
            on,
            false,
            |dialog, on| {
                dialog.limit_by_country = on;
                if !on {
                    dialog.country_codes.clear();
                    dialog.country_box = None;
                }
            },
            cx,
        ));
        if on {
            block = block.child(self.country_field_button(
                CountryHost::Poll,
                tr(keys::POLLS_CREATE_ALLOWED_COUNTRIES),
                self.country_summary(&codes.unwrap_or_default()),
                cx,
            ));
        }
        block.into_any_element()
    }
}
