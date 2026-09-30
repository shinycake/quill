//! scheduled-messages dialog.

use super::app::QuillApp;
use super::message_text::format_unix_date_time;
use super::shell::DialogKind;
use super::shell::QuillShell;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::ComposerEdit;
use quill::telegram::envelope::{MessageSchedulingState, ParsedMessage, effective_content};
use std::cell::RefCell;
use std::rc::Rc;
/// M1: short label for a schedule delay ("1h", "8h", "2d").
pub(super) fn format_schedule_delay(secs: i64) -> String {
    if secs % 86400 == 0 {
        format!("{}d", secs / 86400)
    } else if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}m", secs / 60)
    }
}

/// M1: label for a scheduled message's planned send time
/// (`messageSchedulingState`, schema 1.8.67 lines 5902/5905).
pub(super) fn scheduled_message_label(message: &ParsedMessage) -> String {
    match message.scheduling_state {
        Some(MessageSchedulingState::SendAtDate { send_date }) => {
            format!("scheduled for {}", format_unix_date_time(send_date as i64))
        }
        Some(MessageSchedulingState::SendWhenOnline) => "scheduled for when online".to_string(),
        None => "scheduled".to_string(),
    }
}

impl QuillApp {
    /// kit Phase 2 (redo): scheduled messages hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via
    /// `on_close`.
    pub(super) fn build_scheduled_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Scheduled, |this, _, cx| {
                this.scheduled_dialog_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).title("Scheduled messages");
            let body = this.scheduled_dialog_body(cx);
            dialog
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): list body extracted from the old
    /// `scheduled_dialog` — kept pure (no custom scrim/panel).
    pub(super) fn scheduled_dialog_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let messages: Vec<ParsedMessage> = self
            .session()
            .map(|session| session.scheduled_messages.clone())
            .unwrap_or_default();
        let mut list = div()
            .id("scheduled-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(360.))
            .overflow_y_scroll();
        if messages.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No scheduled messages."),
            );
        }
        for message in messages {
            let id = message.id;
            let preview = effective_content(&message.content, message.ephemeral.as_ref()).preview();
            let label = scheduled_message_label(&message);
            // M1: scheduled sends are the user's own — editing routes
            // through the same composer edit flow with `scheduled: true`
            // so `edit_snapshot` validates against the scheduled list.
            let edit = ComposerEdit::from_own_content(
                message.chat_id,
                message.id,
                true,
                false,
                &message.content,
            )
            .map(|mut edit| {
                edit.scheduled = true;
                edit
            });
            let mut row = div()
                .id(("scheduled-row", id.0 as u64))
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(cx.theme().sidebar)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_sm().child(preview))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(label),
                        ),
                );
            if let Some(edit) = edit {
                row = row.child(
                    Button::new(format!("scheduled-edit-{}", id.0))
                        .label("Edit")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.scheduled_dialog_open = false;
                            this.begin_edit(edit.clone(), window, cx);
                            this.close_kit_dialog_if_done(DialogKind::Scheduled, window, cx);
                        })),
                );
            }
            list = list.child(
                row.child(
                    Button::new(format!("scheduled-delete-{}", id.0))
                        .label("Delete")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.delete_scheduled_message(id, cx);
                        })),
                ),
            );
        }
        list.into_any_element()
    }
}
