//! The poll creation box over the window (tdesktop `CreatePollBox`, a box
//! of `boxWideWidth`). It used to be a panel pushed in above the composer,
//! squeezed by the history and framed by an accent border. Unsent input
//! asks first: Cancel, Escape or a click outside open a "Discard this
//! poll?" box, as tdesktop's `CreatePollBox` asks before it closes.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// tdesktop `boxWideWidth`.
const WIDE_WIDTH: f32 = 420.;
/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;

impl QuillApp {
    fn poll_box_title(&self) -> &'static str {
        let quiz = self
            .composer_ui
            .poll_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.is_quiz);
        if quiz {
            tr(keys::Q_MEDIA_NEW_QUIZ)
        } else {
            tr(keys::POLLS_CREATE_TITLE)
        }
    }

    /// Escape inside the box: a country or deadline step goes back to the
    /// poll, the poll itself asks before it drops what was typed.
    fn poll_box_escape(&mut self, cx: &mut Context<Self>) {
        if self.country_box_open() {
            self.close_open_country_boxes(cx);
        } else if self.poll_deadline_box_open() {
            self.close_poll_deadline_box(cx);
        } else {
            self.request_close_poll_dialog(cx);
        }
    }

    fn build_poll_create_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let title = app.read(cx).poll_box_title();
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PollCreate, |this, _, cx| {
                this.request_close_poll_dialog(cx);
            });
        let body_app = app.clone();
        let footer = app.update(cx, |this, cx| this.poll_box_footer(cx));
        let dialog = match footer {
            Some(footer) => dialog.footer(footer),
            None => dialog,
        };
        dialog
            .overlay(true)
            .w(px(WIDE_WIDTH))
            .title(crate::ui::shell::dialog_title(title))
            // Escape is the box's own (`poll_box_escape`): it steps back
            // out of the country and deadline pickers first.
            .keyboard(false)
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, window, cx| {
                    let escape_app = body_app.clone();
                    let body = body_app.update(cx, |this, cx| {
                        let question = this.composer_ui.poll_dialog.as_mut().and_then(|dialog| {
                            std::mem::take(&mut dialog.focus_question)
                                .then(|| dialog.question_input.clone())
                        });
                        crate::ui::dialog_focus::focus_after_mount(question, window, cx);
                        this.poll_dialog_panel(cx)
                    });
                    content.child(
                        div()
                            .on_key_down(move |event, _, cx| {
                                if event.keystroke.key == "escape" {
                                    cx.stop_propagation();
                                    escape_app.update(cx, |this, cx| this.poll_box_escape(cx));
                                }
                            })
                            .children(body),
                    )
                },
            ))
            .on_close(on_close)
    }

    fn build_poll_discard_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::PollDiscard, keep_editing);
        app.update(cx, |_, cx| {
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("poll-discard-no")
                        .label(tr(keys::Q_MEDIA_KEEP_EDITING))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            keep_editing(this, window, cx);
                            this.close_kit_dialog_if_done(DialogKind::PollDiscard, window, cx);
                        })),
                )
                .child(
                    Button::new("poll-discard-yes")
                        .label(tr(keys::Q_MEDIA_DISCARD))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_poll_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::PollDiscard, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .w(px(BOX_WIDTH))
                .content(|content, _, _| {
                    content.child(div().text_sm().child(tr(keys::Q_MEDIA_DISCARD_THIS_POLL)))
                })
                .footer(footer)
                .on_close(on_close)
        })
    }
}

fn keep_editing(this: &mut QuillApp, _: &mut Window, cx: &mut Context<QuillApp>) {
    if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
        dialog.confirming_discard = false;
    }
    cx.notify();
}

crate::ui::shell::register_dialogs! {
    PollDiscard => DialogSpec::new(
        2935,
        |app| app.composer_ui.poll_dialog.as_ref().is_some_and(|d| d.confirming_discard),
        QuillApp::build_poll_discard_dialog,
    )
    .close(|app, _, cx| {
            app.composer_ui.poll_dialog = None;
            cx.notify();
        }),
    PollCreate => DialogSpec::new(
        2936,
        |app| app.composer_ui.poll_dialog.is_some(),
        QuillApp::build_poll_create_dialog,
    )
    .close(|app, _, cx| {
            app.composer_ui.poll_dialog = None;
            cx.notify();
        }),
}
