//! Stop Poll and Close Secret Chat confirmations as boxes
//! over the window, the way tdesktop asks (`Ui::MakeConfirmBox`). They used to be red banners squeezed in above
//! the composer. Enter confirms and Escape cancels, as in tdesktop.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_base::Disableable;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;

pub(super) type Step = fn(&mut QuillApp, &mut Context<QuillApp>);

/// What a confirm box asks: its title, the line under it and the button.
pub(super) struct Ask {
    pub(super) title: SharedString,
    pub(super) text: SharedString,
    pub(super) confirm: SharedString,
}

/// The shared shape: title, text (plus `extra`), Cancel and a red confirm
/// button; Enter confirms, Escape and ✕ cancel. The open flag clears in
/// `confirm` or `cancel`, and the shell closes the box from it.
pub(super) fn confirm_box(
    app: &Entity<QuillApp>,
    shell: &Entity<QuillShell>,
    dialog: Dialog,
    kind: DialogKind,
    ask: Ask,
    steps: (Step, Step),
    cx: &mut App,
) -> Dialog {
    confirm_box_sending(app, shell, dialog, kind, ask, steps, false, cx)
}

/// [`confirm_box`] with the confirmed request out: the red button shows
/// its spinner and waits for the answer; Cancel stays live.
#[allow(clippy::too_many_arguments)]
pub(super) fn confirm_box_sending(
    app: &Entity<QuillApp>,
    shell: &Entity<QuillShell>,
    dialog: Dialog,
    kind: DialogKind,
    ask: Ask,
    (cancel, confirm): (Step, Step),
    sending: bool,
    cx: &mut App,
) -> Dialog {
    let on_close = QuillShell::on_close_kind(app, shell, kind, move |this, _, cx| cancel(this, cx));
    let footer = app.update(cx, |_, cx| {
        div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("confirm-box-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| cancel(this, cx))),
            )
            .child(
                Button::new("confirm-box-ok")
                    .label(ask.confirm)
                    .danger()
                    .loading(sending)
                    .disabled(sending)
                    .on_click(cx.listener(move |this, _, _, cx| confirm(this, cx))),
            )
    });
    let ok_app = app.clone();
    let text = ask.text;
    dialog
        .overlay(true)
        .w(px(BOX_WIDTH))
        .title(crate::ui::shell::dialog_title(ask.title))
        // Enter confirms; returning false leaves the closing to the shell,
        // so `on_close` (which cancels) never runs after a confirm.
        .on_ok(move |_, _, cx| {
            ok_app.update(cx, confirm);
            false
        })
        .content(move |content, _, _| {
            content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_sm().child(text.clone())),
            )
        })
        .footer(footer)
        .on_close(on_close)
}

impl QuillApp {
    /// TGX `StopPollWarn` / `StopQuizWarn`: nobody can vote afterwards.
    fn build_stop_poll_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let is_quiz = app
            .read(cx)
            .chat_ui
            .pending_stop_poll
            .is_some_and(|(_, _, is_quiz)| is_quiz);
        let ask = if is_quiz {
            Ask {
                title: tr(keys::Q_MEDIA_STOP_THIS_QUIZ).into(),
                text: tr(keys::Q_MEDIA_IF_YOU_STOP_THIS_QUIZ).into(),
                confirm: tr(keys::Q_MEDIA_STOP_QUIZ).into(),
            }
        } else {
            Ask {
                title: tr(keys::Q_MEDIA_STOP_THIS_POLL).into(),
                text: tr(keys::POLLS_STOP_WARNING).into(),
                confirm: tr(keys::POLLS_STOP).into(),
            }
        };
        confirm_box(
            app,
            shell,
            dialog,
            DialogKind::StopPollConfirm,
            ask,
            (Self::cancel_stop_poll, Self::confirm_stop_poll),
            cx,
        )
    }

    /// TGX `DeleteSecretChat{Pending,Closed,}Confirm`, keyed on the secret
    /// chat's state.
    fn build_close_secret_chat_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let (title, text, confirm) = app.read(cx).secret_close_copy();
        let ask = Ask {
            title: title.into(),
            text: text.into(),
            confirm: confirm.into(),
        };
        confirm_box(
            app,
            shell,
            dialog,
            DialogKind::CloseSecretChat,
            ask,
            (
                Self::cancel_close_secret_chat,
                Self::confirm_close_secret_chat,
            ),
            cx,
        )
    }
}

crate::ui::shell::register_dialogs! {
    StopPollConfirm => DialogSpec::new(
        2944,
        |app| app.chat_ui.pending_stop_poll.is_some(),
        QuillApp::build_stop_poll_dialog,
    )
    .per_window()
    .close(|app, _, cx| app.cancel_stop_poll(cx)),
    CloseSecretChat => DialogSpec::new(
        2945,
        |app| app.chat_list.pending_close_secret_chat.is_some(),
        QuillApp::build_close_secret_chat_dialog,
    )
    .close(|app, _, cx| app.cancel_close_secret_chat(cx)),
}
