//! Methods moved out of `polls.rs` to keep files under 1000 lines.

use super::*;
use gpui_kit::component::switch::Switch;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Slice B3: the poll dialog's toggle row. Quiz mode forces
    /// single-answer and no revoting, mirroring Telegram X's
    /// `CreatePollController` quiz toggle.
    pub(in crate::ui) fn poll_toggle_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (is_quiz, anonymous, multiple, revoting, shuffle) = match &self.composer_ui.poll_dialog
        {
            Some(dialog) => (
                dialog.is_quiz,
                dialog.is_anonymous,
                dialog.allows_multiple_answers,
                dialog.allows_revoting,
                dialog.shuffle_options,
            ),
            None => (false, true, false, true, false),
        };
        let (add_options, hide_results, subscribers, has_deadline) =
            match &self.composer_ui.poll_dialog {
                Some(dialog) => (
                    dialog.allow_adding_options,
                    dialog.hide_results_until_closes,
                    dialog.members_only,
                    dialog.deadline.is_some(),
                ),
                None => (false, false, false, false),
            };
        // tdesktop offers "Restrict to Subscribers" in broadcast channels only.
        let in_channel = self
            .session()
            .and_then(|s| s.open_chat.and_then(|id| s.chats.get(&id.0)))
            .is_some_and(|chat| chat.is_channel());
        // Phase 6: kit Checkbox (was: ghost buttons with ☑/☐ labels).
        // Controlled: the requested value is written, not flipped.
        let checkbox =
            |id: &'static str, label: &'static str, on: bool, set: fn(&mut PollDialog, bool)| {
                Checkbox::new(id)
                    .checked(on)
                    .label(label)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
                            set(dialog, on);
                        }
                        cx.notify();
                    }))
            };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(checkbox(
                        "poll-toggle-quiz",
                        tr(keys::POLLS_CREATE_QUIZ_MODE),
                        is_quiz,
                        |dialog, on| {
                            dialog.is_quiz = on;
                            if on {
                                dialog.allows_multiple_answers = false;
                                dialog.allows_revoting = false;
                            }
                        },
                    ))
                    .child(checkbox(
                        "poll-toggle-anonymous",
                        tr(keys::POLLS_CREATE_ANONYMOUS),
                        anonymous,
                        |dialog, on| dialog.is_anonymous = on,
                    ))
                    .child(checkbox(
                        "poll-toggle-multiple",
                        tr(keys::POLLS_CREATE_MULTIPLE_CHOICE),
                        multiple,
                        |dialog, on| {
                            // Quizzes are single-answer; the toggle is inert in quiz mode.
                            if !dialog.is_quiz {
                                dialog.allows_multiple_answers = on;
                            }
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(checkbox(
                        "poll-toggle-revoting",
                        tr(keys::POLLS_CREATE_ALLOW_REVOTING),
                        revoting,
                        |dialog, on| {
                            // Quizzes force revoting off; the toggle is inert in quiz mode.
                            if !dialog.is_quiz {
                                dialog.allows_revoting = on;
                            }
                        },
                    ))
                    .child(checkbox(
                        "poll-toggle-shuffle",
                        tr(keys::POLLS_CREATE_SHUFFLE_OPTIONS),
                        shuffle,
                        |dialog, on| dialog.shuffle_options = on,
                    )),
            )
            // B15: tdesktop's remaining creation switches.
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(checkbox(
                        "poll-toggle-add-options",
                        tr(keys::POLLS_CREATE_ALLOW_ADDING_OPTIONS),
                        add_options && !is_quiz,
                        |dialog, on| {
                            // Quizzes have fixed options.
                            if !dialog.is_quiz {
                                dialog.allow_adding_options = on;
                            }
                        },
                    ))
                    .child(checkbox(
                        "poll-toggle-hide-results",
                        tr(keys::Q_MEDIA_HIDE_RESULTS_UNTIL_CLOSED),
                        hide_results,
                        |dialog, on| dialog.hide_results_until_closes = on,
                    ))
                    .when(in_channel, |this| {
                        this.child(checkbox(
                            "poll-toggle-subscribers",
                            tr(keys::Q_MEDIA_SUBSCRIBERS_ONLY),
                            subscribers,
                            |dialog, on| dialog.members_only = on,
                        ))
                    }),
            )
            .when(in_channel, |this| this.child(self.poll_country_limit(cx)))
            .child(
                Checkbox::new("poll-toggle-deadline")
                    .checked(has_deadline)
                    .label(tr(keys::Q_MEDIA_CLOSE_AT_A_SET_DATE))
                    .on_click(cx.listener(|this, &on: &bool, window, cx| {
                        if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
                            dialog.deadline = on.then(|| poll_deadline_picker(window, cx));
                        }
                        cx.notify();
                    })),
            )
    }

    /// tdesktop's "Limit by Country" switch (broadcast channels) and, while
    /// it is on, the "Allowed Countries" row under it. Turning it off
    /// forgets the chosen countries.
    fn poll_country_limit(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog = self.composer_ui.poll_dialog.as_ref();
        let on = dialog.is_some_and(|dialog| dialog.limit_by_country);
        let codes = dialog.map(|dialog| dialog.country_codes.clone());
        let muted = cx.theme().muted_foreground;
        let mut block = div().flex().flex_col().gap_2().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_sm()
                                .child(tr(keys::POLLS_CREATE_LIMIT_BY_COUNTRY)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(tr(keys::POLLS_CREATE_LIMIT_BY_COUNTRY_ABOUT)),
                        ),
                )
                .child(
                    Switch::new("poll-toggle-country")
                        .checked(on)
                        .accessibility_label(tr(keys::POLLS_CREATE_LIMIT_BY_COUNTRY))
                        .on_click(cx.listener(|this, &next: &bool, _, cx| {
                            if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
                                dialog.limit_by_country = next;
                                if !next {
                                    dialog.country_codes.clear();
                                    dialog.country_box = None;
                                }
                            }
                            cx.notify();
                        })),
                ),
        );
        if on {
            block = block.child(self.country_field_button(
                CountryHost::Poll,
                tr(keys::POLLS_CREATE_ALLOWED_COUNTRIES),
                self.country_summary(&codes.unwrap_or_default()),
                cx,
            ));
        }
        block
    }
}
