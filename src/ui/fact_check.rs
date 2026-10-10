//! "Add Fact Check" / "Edit Fact Check" from the message menu (Telegram
//! Desktop `EditFactcheckBox`, `Data::Factchecks::save`): a dialog with the
//! text field; an empty text removes the fact check.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use std::cell::RefCell;
use std::rc::Rc;

/// Telegram Desktop falls back to this when the server sends no
/// `fact_check_length_max`.
pub(super) const FACT_CHECK_LENGTH_MAX: usize = 1024;

pub(super) struct FactCheckDialog {
    pub(super) chat_id: ChatId,
    pub(super) message_id: MessageId,
    /// The fact check being replaced ("" when adding).
    pub(super) existing: String,
    pub(super) input: Entity<TextareaState>,
}

impl QuillApp {
    /// Open the dialog for a message (`existing` is its current fact check).
    pub(super) fn open_fact_check(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        existing: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Fact check")
                .auto_grow(3, 8)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| {
            input.set_value(existing.clone(), window, cx)
        });
        self.fact_check_dialog = Some(FactCheckDialog {
            chat_id,
            message_id,
            existing,
            input,
        });
        cx.notify();
    }

    fn close_fact_check(&mut self, cx: &mut Context<Self>) {
        self.fact_check_dialog = None;
        cx.notify();
    }

    fn submit_fact_check(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.fact_check_dialog.take() else {
            return;
        };
        let text: String = dialog
            .input
            .read(cx)
            .value()
            .trim()
            .chars()
            .take(FACT_CHECK_LENGTH_MAX)
            .collect();
        if text == dialog.existing.trim() {
            cx.notify();
            return;
        }
        match self.live.as_mut() {
            Some(live) => {
                self.status_note =
                    match live
                        .driver
                        .set_fact_check(dialog.chat_id, dialog.message_id, &text)
                    {
                        Ok(_) if text.is_empty() => "removing the fact check…".into(),
                        Ok(_) => "saving the fact check…".into(),
                        Err(_) => "could not save the fact check".into(),
                    };
            }
            None => {
                if let Some(message) = self
                    .demo_session
                    .as_mut()
                    .and_then(|session| session.histories.get_mut(&dialog.chat_id.0))
                    .and_then(|history| history.messages.get_mut(&dialog.message_id.0))
                {
                    message.extras.fact_check = text.clone();
                }
                self.status_note = "fact check saved (demo)".into();
            }
        }
        cx.notify();
    }

    pub(super) fn build_fact_check_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FactCheck, |this, _, cx| {
                this.close_fact_check(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.fact_check_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let adding = state.existing.is_empty();
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Add a note that gives readers context. Leave it empty to remove the fact check.",
                        ),
                )
                .child(Textarea::new(&state.input).aria_label("Fact check"))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("fact-check-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_fact_check(cx);
                                    this.close_kit_dialog_if_done(DialogKind::FactCheck, window, cx);
                                })),
                        )
                        .child(
                            Button::new("fact-check-save")
                                .label("Save")
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_fact_check(cx);
                                    this.close_kit_dialog_if_done(DialogKind::FactCheck, window, cx);
                                })),
                        ),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if adding {
                    "Add Fact Check"
                } else {
                    "Edit Fact Check"
                }))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    FactCheck => DialogSpec::new(
        5400,
        |app| app.fact_check_dialog.is_some(),
        QuillApp::build_fact_check_dialog,
    ),
}
